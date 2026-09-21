#[cfg(feature = "render")]
use std::str::FromStr;

use audiowaveform::{AmplitudeScale, Error, RawAudioConfig, ScaleSpec, WaveformFormat};
#[cfg(feature = "render")]
use audiowaveform::{Color, RenderOptions, RenderStyle, WaveformColors};

#[cfg(feature = "render")]
use crate::args::CliWaveformStyle;
use crate::args::{Cli, CliFormat, CliRawSampleFormat};
use crate::stringify_error;

/// A supported operation selected after validating arguments and feature availability.
#[derive(Clone, Copy)]
pub(super) enum Operation {
    Generate(WaveformFormat),
    Convert(WaveformFormat),
    #[cfg(feature = "wav-output")]
    Transcode,
    #[cfg(feature = "render")]
    Render,
}

/// Resolved CLI options; constructing a request does not open input or output files.
pub(super) struct Request {
    pub(super) input_filename: Option<String>,
    pub(super) output_filename: Option<String>,
    pub(super) input_format: CliFormat,
    pub(super) operation: Operation,
    pub(super) bits: Option<u8>,
    pub(super) amplitude: AmplitudeScale,
    pub(super) scale: ScaleSpec,
    pub(super) raw_config: Option<RawAudioConfig>,
    pub(super) has_resample: bool,
    pub(super) split_channels: bool,
    pub(super) quiet: bool,
    #[cfg(feature = "render")]
    pub(super) render: RenderOptions,
}

pub(super) fn resolve(cli: Cli) -> Result<Request, String> {
    #[cfg(feature = "render")]
    if cli.height < 1 {
        return Err("Invalid image height: minimum 1".to_string());
    }
    let input_format = resolve_format(cli.input_filename.as_deref(), cli.input_format, true)?;
    let output_format = resolve_format(cli.output_filename.as_deref(), cli.output_format, false)?;
    let bits = resolve_bits(cli.bits)?;
    #[cfg(feature = "render")]
    let compression = resolve_compression(cli.compression)?;
    let amplitude = parse_amplitude_scale(&cli.amplitude_scale)?;
    #[cfg(feature = "render")]
    let colors = resolve_colors(&cli)?;
    #[cfg(feature = "render")]
    let render_style = resolve_render_style(&cli)?;
    let scale = resolve_scale(&cli)?;
    let raw_config = if input_format == CliFormat::Raw {
        Some(resolve_raw_audio_config(&cli)?)
    } else {
        None
    };
    let has_resample = cli.zoom.is_some() || cli.pixels_per_second.is_some() || cli.end.is_some();
    if let Some(format) = input_format.as_audio_format() {
        format.ensure_enabled().map_err(stringify_error)?;
    }
    if output_format == CliFormat::Png && !cfg!(feature = "render") {
        return Err(stringify_error(Error::FeatureDisabled {
            feature: "render",
        }));
    }
    if output_format == CliFormat::Wav && !cfg!(feature = "wav-output") {
        return Err(stringify_error(Error::FeatureDisabled {
            feature: "wav-output",
        }));
    }

    let operation = match output_format {
        #[cfg(feature = "wav-output")]
        CliFormat::Wav if input_format.is_audio_input() => Operation::Transcode,
        CliFormat::Dat | CliFormat::Json | CliFormat::Txt if input_format.is_audio_input() => {
            Operation::Generate(output_format.as_waveform_format().expect("waveform format"))
        }
        CliFormat::Dat | CliFormat::Json | CliFormat::Txt if input_format.is_waveform_input() => {
            Operation::Convert(output_format.as_waveform_format().expect("waveform format"))
        }
        #[cfg(feature = "render")]
        CliFormat::Png if input_format.is_audio_input() || input_format.is_waveform_input() => {
            Operation::Render
        }
        _ => {
            return Err(format!(
                "Can't generate {} format output from {} format input",
                output_format.name(),
                input_format.name()
            ));
        }
    };
    #[cfg(feature = "render")]
    let render = RenderOptions {
        width: cli.width as u32,
        height: cli.height as u32,
        start_time: cli.start,
        amplitude_scale: amplitude,
        axis_labels: cli.with_axis_labels || !cli.no_axis_labels,
        style: render_style,
        colors,
        png_compression_level: compression.map(|value| value as u8),
    };
    Ok(Request {
        input_filename: cli.input_filename,
        output_filename: cli.output_filename,
        input_format,
        operation,
        bits,
        amplitude,
        scale,
        raw_config,
        has_resample,
        split_channels: cli.split_channels,
        quiet: cli.quiet,
        #[cfg(feature = "render")]
        render,
    })
}

fn resolve_format(
    filename: Option<&str>,
    explicit: Option<CliFormat>,
    input: bool,
) -> Result<CliFormat, String> {
    if let Some(explicit) = explicit {
        return Ok(explicit);
    }
    if let Some(filename) = filename {
        return CliFormat::from_path(filename);
    }
    Err(if input {
        "Error: Must specify either input filename or input format".to_string()
    } else {
        "Error: Must specify either output filename or output format".to_string()
    })
}

fn resolve_bits(bits: Option<i32>) -> Result<Option<u8>, String> {
    match bits {
        Some(8 | 16) | None => Ok(bits.map(|bits| bits as u8)),
        Some(_) => Err("Error: Invalid bits: must be either 8 or 16".to_string()),
    }
}

#[cfg(feature = "render")]
fn resolve_compression(compression: i32) -> Result<Option<i32>, String> {
    if (-1..=9).contains(&compression) {
        Ok((compression >= 0).then_some(compression))
    } else {
        Err(
            "Error: Invalid compression level: must be from 0 (none) to 9 (best), or -1 (default)"
                .to_string(),
        )
    }
}

fn parse_amplitude_scale(value: &str) -> Result<AmplitudeScale, String> {
    if value == "auto" {
        return Ok(AmplitudeScale::Auto);
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "Error: Invalid amplitude scale: must be a number".to_string())?;
    if !parsed.is_finite() || parsed < 0.0 {
        Err("Error: Invalid amplitude scale: must be a positive number".to_string())
    } else {
        Ok(AmplitudeScale::Fixed(parsed))
    }
}

fn resolve_scale(cli: &Cli) -> Result<ScaleSpec, String> {
    if cli.zoom.is_some() && cli.end.is_some() {
        return Err("Specify either --end or --zoom but not both".to_string());
    }
    if cli.pixels_per_second.is_some() && cli.end.is_some() {
        return Err("Specify either --end or --pixels-per-second but not both".to_string());
    }
    if cli.zoom.is_some() && cli.pixels_per_second.is_some() {
        return Err("Specify either --zoom or --pixels-per-second but not both".to_string());
    }
    if cli.width < 1 {
        return Err("Invalid image width: minimum 1".to_string());
    }

    if let Some(end) = cli.end {
        return Ok(ScaleSpec::FitWidth {
            width_pixels: cli.width as u32,
            time_range: Some((cli.start, end)),
        });
    }
    if let Some(pixels_per_second) = cli.pixels_per_second {
        if pixels_per_second <= 0 {
            return Err("Invalid pixels per second: must be greater than zero".to_string());
        }
        return Ok(ScaleSpec::PixelsPerSecond(pixels_per_second as u32));
    }

    match cli.zoom.as_deref() {
        Some("auto") => Ok(ScaleSpec::FitWidth {
            width_pixels: cli.width as u32,
            time_range: None,
        }),
        Some(value) => {
            let zoom = value
                .parse::<u32>()
                .map_err(|_| "Error: Invalid zoom: must be a number or 'auto'".to_string())?;
            Ok(ScaleSpec::SamplesPerPixel(zoom))
        }
        None => Ok(ScaleSpec::SamplesPerPixel(256)),
    }
}

#[cfg(feature = "render")]
fn resolve_colors(cli: &Cli) -> Result<WaveformColors, String> {
    let mut colors = cli.color_scheme.into_library().palette();

    if let Some(value) = &cli.border_color {
        colors.border = Color::from_str(value).map_err(stringify_error)?;
    }
    if let Some(value) = &cli.background_color {
        colors.background = Color::from_str(value).map_err(stringify_error)?;
    }
    if let Some(value) = &cli.axis_label_color {
        colors.axis_label = Color::from_str(value).map_err(stringify_error)?;
    }
    if let Some(value) = &cli.waveform_color {
        let waveform = value
            .split(',')
            .map(|item| Color::from_str(item).map_err(stringify_error))
            .collect::<Result<Vec<_>, _>>()?;
        if !waveform.is_empty() {
            colors.waveform = waveform;
        }
    }

    Ok(colors)
}

#[cfg(feature = "render")]
fn resolve_render_style(cli: &Cli) -> Result<RenderStyle, String> {
    match cli.waveform_style {
        CliWaveformStyle::Normal => Ok(RenderStyle::Normal),
        CliWaveformStyle::Bars => {
            if cli.bar_width < 1 {
                return Err("Invalid bar width: minimum 1".to_string());
            }
            if cli.bar_gap < 0 {
                return Err("Invalid bar gap: minimum 0".to_string());
            }
            Ok(RenderStyle::Bars {
                width: cli.bar_width as u32,
                gap: cli.bar_gap as u32,
                style: cli.bar_style.into_library(),
            })
        }
    }
}

fn resolve_raw_audio_config(cli: &Cli) -> Result<RawAudioConfig, String> {
    let sample_rate = cli
        .raw_sample_rate
        .ok_or_else(|| "Error: Missing --raw-samplerate option".to_string())?;
    let channels = cli
        .raw_channels
        .ok_or_else(|| "Error: Missing --raw-channels option".to_string())?;
    let sample_format = cli
        .raw_format
        .map(CliRawSampleFormat::into_library)
        .ok_or_else(|| "Error: Missing --raw-format option".to_string())?;
    if sample_rate <= 0 {
        return Err("Invalid input sample rate: must be greater than zero".to_string());
    }
    if channels <= 0 {
        return Err("Invalid number of input channels: must be greater than zero".to_string());
    }
    let channels = u16::try_from(channels)
        .map_err(|_| "Invalid number of input channels: maximum 65535".to_string())?;

    RawAudioConfig::new(sample_rate as u32, channels, sample_format).map_err(stringify_error)
}
