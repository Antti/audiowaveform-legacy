use std::io::{Read, Seek};

use super::pcm::clamp_float_to_i16;
use super::peaks::{PeakAccumulator, needs_frame_count};
use crate::{Error, GenerateOptions, PcmAudio, Waveform};

/// Supported raw audio sample encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawSampleFormat {
    /// Signed 8-bit integer.
    S8,
    /// Unsigned 8-bit integer.
    U8,
    /// Signed little-endian 16-bit integer.
    S16Le,
    /// Signed big-endian 16-bit integer.
    S16Be,
    /// Signed little-endian 24-bit integer.
    S24Le,
    /// Signed big-endian 24-bit integer.
    S24Be,
    /// Signed little-endian 32-bit integer.
    S32Le,
    /// Signed big-endian 32-bit integer.
    S32Be,
    /// Little-endian 32-bit float.
    F32Le,
    /// Big-endian 32-bit float.
    F32Be,
    /// Little-endian 64-bit float.
    F64Le,
    /// Big-endian 64-bit float.
    F64Be,
}

impl RawSampleFormat {
    /// Returns the canonical CLI-friendly name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::S8 => "s8",
            Self::U8 => "u8",
            Self::S16Le => "s16le",
            Self::S16Be => "s16be",
            Self::S24Le => "s24le",
            Self::S24Be => "s24be",
            Self::S32Le => "s32le",
            Self::S32Be => "s32be",
            Self::F32Le => "f32le",
            Self::F32Be => "f32be",
            Self::F64Le => "f64le",
            Self::F64Be => "f64be",
        }
    }
}

impl std::str::FromStr for RawSampleFormat {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "s8" => Ok(Self::S8),
            "u8" => Ok(Self::U8),
            "s16le" => Ok(Self::S16Le),
            "s16be" => Ok(Self::S16Be),
            "s24le" => Ok(Self::S24Le),
            "s24be" => Ok(Self::S24Be),
            "s32le" => Ok(Self::S32Le),
            "s32be" => Ok(Self::S32Be),
            "f32le" => Ok(Self::F32Le),
            "f32be" => Ok(Self::F32Be),
            "f64le" => Ok(Self::F64Le),
            "f64be" => Ok(Self::F64Be),
            _ => Err(Error::UnsupportedFormat {
                format: s.to_string(),
            }),
        }
    }
}

/// Configuration for decoding raw audio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawAudioConfig {
    /// Source sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Raw sample encoding.
    pub sample_format: RawSampleFormat,
}

impl RawAudioConfig {
    /// Creates a raw audio configuration.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        sample_format: RawSampleFormat,
    ) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::invalid_argument(
                "raw sample rate",
                "Invalid input sample rate: must be greater than zero",
            ));
        }
        if channels == 0 {
            return Err(Error::invalid_argument(
                "raw channels",
                "Invalid number of input channels: must be greater than zero",
            ));
        }
        Ok(Self {
            sample_rate,
            channels,
            sample_format,
        })
    }
}

/// Generates a waveform from raw audio bytes.
/// Fixed scales consume bounded blocks directly. Scales requiring the total
/// frame count spool the input to a temporary file, then aggregate in a second pass.
pub fn generate_waveform_from_raw_reader<R: Read>(
    mut reader: R,
    config: &RawAudioConfig,
    options: &GenerateOptions,
) -> Result<Waveform, Error> {
    options.validate()?;
    RawAudioConfig::new(config.sample_rate, config.channels, config.sample_format)?;
    if needs_frame_count(options.scale) {
        // The public raw-reader API accepts pipes. Spool bytes to disk so an
        // unknown length never forces the entire PCM stream into memory.
        let mut spool = tempfile::tempfile()?;
        let bytes = std::io::copy(&mut reader, &mut spool)?;
        let frame_width = usize::from(config.channels) * raw_sample_width(config.sample_format);
        let frames = usize::try_from(bytes / frame_width as u64)
            .map_err(|_| Error::invalid_data("Audio frame count is too large"))?;
        spool.rewind()?;
        generate_raw_stream(&mut spool, config, options, frames)
    } else {
        generate_raw_stream(&mut reader, config, options, 0)
    }
}

/// Decodes raw audio bytes into interleaved 16-bit PCM.
/// This explicitly retains the entire decoded recording in memory.
pub fn decode_raw_audio_reader<R: Read>(
    mut reader: R,
    config: &RawAudioConfig,
) -> Result<PcmAudio, Error> {
    let mut samples = Vec::new();
    visit_raw_samples(&mut reader, config, |block| {
        samples.extend_from_slice(block);
        Ok(())
    })?;
    PcmAudio::new(config.sample_rate, config.channels, samples)
}
fn generate_raw_stream(
    reader: &mut impl Read,
    config: &RawAudioConfig,
    options: &GenerateOptions,
    frames: usize,
) -> Result<Waveform, Error> {
    let mut peaks = PeakAccumulator::new(config.sample_rate, config.channels, frames, options)?;
    visit_raw_samples(reader, config, |samples| peaks.push(samples))?;
    peaks.finish(options)
}

fn visit_raw_samples(
    reader: &mut impl Read,
    config: &RawAudioConfig,
    mut visit: impl FnMut(&[i16]) -> Result<(), Error>,
) -> Result<(), Error> {
    RawAudioConfig::new(config.sample_rate, config.channels, config.sample_format)?;
    let width = raw_sample_width(config.sample_format);
    let frame_width = width * usize::from(config.channels);
    let mut bytes = vec![0; frame_width * (16_384 / usize::from(config.channels)).max(1)];
    let mut samples = Vec::with_capacity(bytes.len() / width);
    let mut buffered = 0;
    loop {
        let count = match reader.read(&mut bytes[buffered..]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        let available = buffered + count;
        let complete = available / frame_width * frame_width;
        samples.clear();
        samples.extend(
            bytes[..complete]
                .chunks_exact(width)
                .map(|sample| parse_raw_sample(sample, config.sample_format)),
        );
        visit(&samples)?;
        bytes.copy_within(complete..available, 0);
        buffered = available - complete;
    }
    if !buffered.is_multiple_of(width) {
        return Err(Error::invalid_data(
            "Raw audio byte length is not aligned to the sample format",
        ));
    }
    if buffered != 0 {
        return Err(Error::invalid_argument(
            "samples",
            "Interleaved PCM sample count must be divisible by the channel count",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn parse_raw_audio(bytes: &[u8], config: &RawAudioConfig) -> Result<PcmAudio, Error> {
    decode_raw_audio_reader(bytes, config)
}

fn raw_sample_width(format: RawSampleFormat) -> usize {
    match format {
        RawSampleFormat::S8 | RawSampleFormat::U8 => 1,
        RawSampleFormat::S16Le | RawSampleFormat::S16Be => 2,
        RawSampleFormat::S24Le | RawSampleFormat::S24Be => 3,
        RawSampleFormat::S32Le
        | RawSampleFormat::S32Be
        | RawSampleFormat::F32Le
        | RawSampleFormat::F32Be => 4,
        RawSampleFormat::F64Le | RawSampleFormat::F64Be => 8,
    }
}

fn parse_raw_sample(bytes: &[u8], format: RawSampleFormat) -> i16 {
    match format {
        RawSampleFormat::S8 => i16::from(i8::from_ne_bytes([bytes[0]])) << 8,
        RawSampleFormat::U8 => (i16::from(bytes[0]) - 128) << 8,
        RawSampleFormat::S16Le => i16::from_le_bytes([bytes[0], bytes[1]]),
        RawSampleFormat::S16Be => i16::from_be_bytes([bytes[0], bytes[1]]),
        RawSampleFormat::S24Le => (sign_extend_24([bytes[0], bytes[1], bytes[2]]) >> 8) as i16,
        RawSampleFormat::S24Be => (sign_extend_24([bytes[2], bytes[1], bytes[0]]) >> 8) as i16,
        RawSampleFormat::S32Le => {
            (i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) >> 16) as i16
        }
        RawSampleFormat::S32Be => {
            (i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) >> 16) as i16
        }
        RawSampleFormat::F32Le => clamp_float_to_i16(
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
                * f64::from(i16::MAX),
        ),
        RawSampleFormat::F32Be => clamp_float_to_i16(
            f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
                * f64::from(i16::MAX),
        ),
        RawSampleFormat::F64Le => clamp_float_to_i16(
            f64::from_le_bytes(bytes.try_into().expect("checked width")) * f64::from(i16::MAX),
        ),
        RawSampleFormat::F64Be => clamp_float_to_i16(
            f64::from_be_bytes(bytes.try_into().expect("checked width")) * f64::from(i16::MAX),
        ),
    }
}

fn sign_extend_24(bytes: [u8; 3]) -> i32 {
    let sign = if bytes[2] & 0x80 != 0 { 0xFF } else { 0x00 };
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], sign])
}

#[cfg(test)]
mod tests {
    use super::{RawAudioConfig, RawSampleFormat, parse_raw_audio, parse_raw_sample};
    use std::str::FromStr;

    #[test]
    fn parses_raw_sample_formats_and_validates_raw_audio_config() {
        assert_eq!(
            RawSampleFormat::from_str("s16le").expect("raw sample format"),
            RawSampleFormat::S16Le
        );
        assert_eq!(
            RawSampleFormat::from_str("F64BE").expect("raw sample format"),
            RawSampleFormat::F64Be
        );
        let error = RawSampleFormat::from_str("pcm").expect_err("unsupported format");
        assert_eq!(error.to_string(), "Unsupported format: pcm");

        let config = RawAudioConfig::new(44_100, 2, RawSampleFormat::S16Le).expect("config");
        assert_eq!(config.sample_rate, 44_100);
        assert_eq!(config.channels, 2);

        let error =
            RawAudioConfig::new(0, 1, RawSampleFormat::S16Le).expect_err("invalid sample rate");
        assert_eq!(
            error.to_string(),
            "Invalid input sample rate: must be greater than zero"
        );

        let error =
            RawAudioConfig::new(44_100, 0, RawSampleFormat::S16Le).expect_err("invalid channels");
        assert_eq!(
            error.to_string(),
            "Invalid number of input channels: must be greater than zero"
        );
    }

    #[test]
    fn decodes_representative_raw_sample_formats() {
        let cases = [
            (RawSampleFormat::S8, vec![0x80], i16::MIN),
            (RawSampleFormat::U8, vec![0xff], 32_512),
            (RawSampleFormat::S16Le, vec![0x34, 0x12], 0x1234),
            (RawSampleFormat::S16Be, vec![0x12, 0x34], 0x1234),
            (RawSampleFormat::S24Le, vec![0x00, 0x00, 0x01], 256),
            (RawSampleFormat::S24Be, vec![0x01, 0x00, 0x00], 256),
            (RawSampleFormat::S32Le, vec![0x00, 0x00, 0x01, 0x00], 1),
            (RawSampleFormat::S32Be, vec![0x00, 0x01, 0x00, 0x00], 1),
            (
                RawSampleFormat::F32Le,
                1.0_f32.to_le_bytes().to_vec(),
                i16::MAX,
            ),
            (
                RawSampleFormat::F32Be,
                1.0_f32.to_be_bytes().to_vec(),
                i16::MAX,
            ),
            (
                RawSampleFormat::F64Le,
                1.0_f64.to_le_bytes().to_vec(),
                i16::MAX,
            ),
            (
                RawSampleFormat::F64Be,
                1.0_f64.to_be_bytes().to_vec(),
                i16::MAX,
            ),
        ];

        for (format, bytes, expected) in cases {
            assert_eq!(parse_raw_sample(&bytes, format), expected, "{format:?}");
        }
    }

    #[test]
    fn decodes_raw_audio_and_rejects_unaligned_buffers() {
        let config = RawAudioConfig::new(16_000, 1, RawSampleFormat::S16Le).expect("config");
        let pcm = parse_raw_audio(&[0x01, 0x00, 0xff, 0xff], &config).expect("parse raw");
        assert_eq!(pcm.samples(), &[1, -1]);

        let error = parse_raw_audio(&[0x01], &config).expect_err("unaligned bytes");
        assert_eq!(
            error.to_string(),
            "Raw audio byte length is not aligned to the sample format"
        );
    }
}
