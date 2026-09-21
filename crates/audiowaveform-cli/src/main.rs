use std::fs::File;
use std::io::{self, Cursor, Read};
use std::process::ExitCode;

#[cfg(all(feature = "decode", feature = "wav-output"))]
use audiowaveform::decode_audio_from_reader;
#[cfg(feature = "decode")]
use audiowaveform::generate_waveform_from_reader;
use audiowaveform::{
    Error, GenerateOptions, RawAudioConfig, Waveform, WaveformFormat,
    generate_waveform_from_raw_reader,
};
#[cfg(feature = "wav-output")]
use audiowaveform::{decode_raw_audio_reader, write_pcm_as_wav, write_pcm_to_wav_path};
#[cfg(feature = "render")]
use audiowaveform::{render_waveform_to_path, write_waveform_png};
use clap::{CommandFactory, Parser};

mod args;
mod options;

use args::{Cli, CliFormat};
use options::Operation;
#[cfg(any(feature = "render", feature = "wav-output"))]
use options::Request;

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.help {
        let mut command = Cli::command();
        if command.print_help().is_ok() {
            println!();
        }
        return ExitCode::SUCCESS;
    }
    if cli.version {
        println!("AudioWaveform v{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let request = options::resolve(cli)?;
    match request.operation {
        Operation::Generate(format) => {
            let waveform = generate_waveform_from_input(
                request.input_filename.as_deref(),
                request.input_format,
                request.raw_config.as_ref(),
                &GenerateOptions {
                    scale: request.scale,
                    split_channels: request.split_channels,
                    amplitude_scale: Some(request.amplitude),
                },
            )?;
            write_waveform_output(
                &waveform,
                request.output_filename.as_deref(),
                format,
                Some(request.bits.unwrap_or(16)),
            )?;
        }
        Operation::Convert(format) => {
            let waveform =
                load_waveform_input(request.input_filename.as_deref(), request.input_format)?;
            let waveform = if request.has_resample {
                waveform.resample(request.scale).map_err(stringify_error)?
            } else {
                waveform
            };
            let waveform = waveform
                .into_scaled_amplitude(request.amplitude)
                .map_err(stringify_error)?;
            write_waveform_output(
                &waveform,
                request.output_filename.as_deref(),
                format,
                request.bits,
            )?;
        }
        #[cfg(feature = "wav-output")]
        Operation::Transcode => transcode(&request)?,
        #[cfg(feature = "render")]
        Operation::Render => render(&request)?,
    }
    if !request.quiet {
        eprintln!("Done");
    }
    Ok(())
}

#[cfg(feature = "wav-output")]
fn transcode(request: &Request) -> Result<(), String> {
    let bytes = read_input_bytes(request.input_filename.as_deref())?;
    let pcm = match request.input_format {
        CliFormat::Raw => decode_raw_audio_reader(
            Cursor::new(bytes),
            request.raw_config.as_ref().expect("validated raw config"),
        )
        .map_err(stringify_error)?,
        _ => {
            #[cfg(feature = "decode")]
            {
                decode_audio_from_reader(Cursor::new(bytes), request.input_format.as_audio_format())
                    .map_err(stringify_error)?
            }
            #[cfg(not(feature = "decode"))]
            return Err(stringify_error(Error::FeatureDisabled {
                feature: "decode",
            }));
        }
    };
    if is_stdio_filename(request.output_filename.as_deref()) {
        write_pcm_as_wav(&pcm, io::stdout().lock())
    } else {
        write_pcm_to_wav_path(
            &pcm,
            request.output_filename.as_deref().expect("checked stdio"),
        )
    }
    .map_err(stringify_error)
}

#[cfg(feature = "render")]
fn render(request: &Request) -> Result<(), String> {
    let waveform = if request.input_format.is_audio_input() {
        generate_waveform_from_input(
            request.input_filename.as_deref(),
            request.input_format,
            request.raw_config.as_ref(),
            &GenerateOptions {
                scale: request.scale,
                split_channels: request.split_channels,
                amplitude_scale: None,
            },
        )?
    } else {
        let waveform =
            load_waveform_input(request.input_filename.as_deref(), request.input_format)?;
        if waveform.source_frames().is_some() && !request.has_resample {
            waveform
        } else {
            waveform.resample(request.scale).map_err(stringify_error)?
        }
    };
    if is_stdio_filename(request.output_filename.as_deref()) {
        write_waveform_png(&waveform, &request.render, io::stdout().lock())
    } else {
        render_waveform_to_path(
            &waveform,
            &request.render,
            request.output_filename.as_deref().expect("checked stdio"),
        )
    }
    .map_err(stringify_error)
}

fn generate_waveform_from_input(
    filename: Option<&str>,
    format: CliFormat,
    raw: Option<&RawAudioConfig>,
    options: &GenerateOptions,
) -> Result<Waveform, String> {
    if format == CliFormat::Raw {
        let mut reader: Box<dyn Read> = if is_stdio_filename(filename) {
            Box::new(io::stdin().lock())
        } else {
            Box::new(
                File::open(filename.expect("checked stdio")).map_err(|error| error.to_string())?,
            )
        };
        return generate_waveform_from_raw_reader(
            &mut reader,
            raw.expect("validated raw config"),
            options,
        )
        .map_err(stringify_error);
    }
    #[cfg(feature = "decode")]
    {
        use std::io::Seek;
        let file = if is_stdio_filename(filename) {
            spool_audio_input(&mut io::stdin().lock())?
        } else {
            let mut file =
                File::open(filename.expect("checked stdio")).map_err(|error| error.to_string())?;
            if file.stream_position().is_ok() {
                file
            } else {
                // A filename can also refer to a FIFO or process substitution.
                spool_audio_input(&mut file)?
            }
        };
        generate_waveform_from_reader(file, format.as_audio_format(), options)
            .map_err(stringify_error)
    }
    #[cfg(not(feature = "decode"))]
    Err(stringify_error(Error::FeatureDisabled {
        feature: "decode",
    }))
}

#[cfg(feature = "decode")]
fn spool_audio_input(reader: &mut impl Read) -> Result<File, String> {
    // Container probing and exact-count generation need a seekable input.
    // Keep non-seekable encoded input on disk instead of accumulating it in a Vec.
    use std::io::Seek;
    let mut spool = tempfile::tempfile().map_err(|error| error.to_string())?;
    io::copy(reader, &mut spool).map_err(|error| error.to_string())?;
    spool.rewind().map_err(|error| error.to_string())?;
    Ok(spool)
}

fn load_waveform_input(filename: Option<&str>, format: CliFormat) -> Result<Waveform, String> {
    let bytes = read_input_bytes(filename)?;
    Waveform::load_from_reader(
        Cursor::new(bytes),
        format.as_waveform_format().expect("waveform input"),
    )
    .map_err(stringify_error)
}

fn read_input_bytes(filename: Option<&str>) -> Result<Vec<u8>, String> {
    if is_stdio_filename(filename) {
        let mut buffer = Vec::new();
        io::stdin()
            .lock()
            .read_to_end(&mut buffer)
            .map_err(|error| error.to_string())?;
        Ok(buffer)
    } else {
        std::fs::read(filename.expect("checked stdio")).map_err(|error| error.to_string())
    }
}

fn write_waveform_output(
    waveform: &Waveform,
    filename: Option<&str>,
    format: WaveformFormat,
    bits: Option<u8>,
) -> Result<(), String> {
    if is_stdio_filename(filename) {
        waveform.write_to_writer(io::stdout().lock(), format, bits)
    } else {
        waveform.write_to_path(filename.expect("checked stdio"), Some(format), bits)
    }
    .map_err(stringify_error)
}

fn is_stdio_filename(filename: Option<&str>) -> bool {
    filename.is_none() || filename == Some("-")
}

fn stringify_error(error: Error) -> String {
    error.to_string()
}
