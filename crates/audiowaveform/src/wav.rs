use std::fs::File;
use std::io::Write;
#[cfg(feature = "decode")]
use std::io::{Read, Seek};
use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

#[cfg(feature = "decode")]
use crate::AudioFormat;
#[cfg(feature = "decode")]
use crate::audio::decode_audio_reader;
use crate::{Error, PcmAudio};

/// Writes interleaved PCM audio as a 16-bit WAV stream.
pub fn write_pcm_as_wav<W: Write>(pcm: &PcmAudio, mut writer: W) -> Result<(), Error> {
    let spec = validated_wav_spec(pcm.sample_rate(), pcm.channels(), pcm.samples().len())?;
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut wav = WavWriter::new(cursor, spec)?;
        for sample in pcm.samples() {
            wav.write_sample(*sample)?;
        }
        wav.finalize()?;
    }
    writer.write_all(&bytes)?;
    Ok(())
}

/// Writes PCM audio to a 16-bit WAV file after validating its header limits.
/// Invalid metadata leaves an existing destination unchanged.
pub fn write_pcm_to_wav_path(pcm: &PcmAudio, path: impl AsRef<Path>) -> Result<(), Error> {
    validated_wav_spec(pcm.sample_rate(), pcm.channels(), pcm.samples().len())?;
    write_pcm_as_wav(pcm, File::create(path)?)
}

fn validated_wav_spec(sample_rate: u32, channels: u16, samples: usize) -> Result<WavSpec, Error> {
    let block_align = channels.checked_mul(2).ok_or_else(|| {
        Error::invalid_argument(
            "channels",
            "Channel count exceeds the WAV block alignment limit",
        )
    })?;
    sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| {
            Error::invalid_argument(
                "sample rate",
                "Sample rate and channel count exceed the WAV byte rate limit",
            )
        })?;
    // Hound uses a 68-byte extensible header for more than two channels,
    // otherwise a 44-byte PCM header. RIFF size excludes its first eight bytes.
    let header_size = if channels > 2 { 68_u32 } else { 44 };
    u32::try_from(samples)
        .ok()
        .and_then(|samples| samples.checked_mul(2))
        .and_then(|bytes| bytes.checked_add(header_size - 8))
        .ok_or_else(|| {
            Error::invalid_argument("samples", "Audio exceeds the WAV RIFF size limit")
        })?;
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    Ok(spec)
}

/// Decodes audio from a path and writes it as a WAV file.
#[cfg(feature = "decode")]
pub fn transcode_audio_path_to_wav_path(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<(), Error> {
    let input = input.as_ref();
    let format = AudioFormat::from_path(input).ok_or_else(|| Error::UnsupportedFormat {
        format: input
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
    })?;
    let file = File::open(input)?;
    // Finish reading before touching the destination, including when the paths
    // name the same file through a symlink or hard link.
    let pcm = decode_audio_reader(file, Some(format))?;
    write_pcm_to_wav_path(&pcm, output)
}

/// Decodes audio from a seekable reader and writes it as a WAV stream.
#[cfg(feature = "decode")]
pub fn transcode_audio_reader_to_wav_writer<R: Read + Seek + Send + Sync + 'static, W: Write>(
    reader: R,
    format_hint: Option<AudioFormat>,
    writer: W,
) -> Result<(), Error> {
    let pcm = decode_audio_reader(reader, format_hint)?;
    write_pcm_as_wav(&pcm, writer)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::write_pcm_as_wav;
    use crate::PcmAudio;

    #[test]
    fn checks_riff_size_limits_without_allocating_gigabytes() {
        for (channels, header) in [(1, 44_u32), (3, 68)] {
            let maximum_samples = (u32::MAX - (header - 8)) as usize / 2;
            assert!(super::validated_wav_spec(48_000, channels, maximum_samples).is_ok());
            assert!(super::validated_wav_spec(48_000, channels, maximum_samples + 1).is_err());
        }
    }

    #[test]
    fn writes_pcm_audio_as_valid_wav_bytes() {
        let pcm = PcmAudio::new(44_100, 1, vec![0, 1, -1, 2, -2]).expect("pcm");
        let mut wav = Vec::new();
        write_pcm_as_wav(&pcm, &mut wav).expect("write wav");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");

        let mut reader = hound::WavReader::new(Cursor::new(wav)).expect("read wav");
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 44_100);
        assert_eq!(
            reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .expect("samples"),
            vec![0, 1, -1, 2, -2]
        );
    }
}
