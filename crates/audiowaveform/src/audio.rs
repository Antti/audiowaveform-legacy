#[cfg(feature = "decode")]
use std::fs::File;
#[cfg(feature = "decode")]
use std::io::SeekFrom;
use std::io::{Read, Seek};
#[cfg(feature = "decode")]
use std::path::Path;

#[cfg(feature = "decode")]
use symphonia::core::audio::{AudioBufferRef, SampleBuffer, Signal};
#[cfg(feature = "decode")]
use symphonia::core::codecs::{CODEC_TYPE_ALAC, DecoderOptions};
#[cfg(feature = "decode")]
use symphonia::core::errors::Error as SymphoniaError;
#[cfg(feature = "decode")]
use symphonia::core::formats::FormatOptions;
#[cfg(feature = "decode")]
use symphonia::core::io::{MediaSource, MediaSourceStream};
#[cfg(feature = "decode")]
use symphonia::core::meta::MetadataOptions;
#[cfg(feature = "decode")]
use symphonia::core::probe::Hint;
#[cfg(feature = "decode")]
use symphonia::default::get_codecs;

#[cfg(feature = "decode")]
use crate::AudioFormat;
use crate::{AmplitudeScale, Error, Waveform};

mod peaks;
#[cfg(feature = "decode")]
mod probe;
use peaks::{PeakAccumulator, needs_frame_count};

#[cfg(feature = "decode")]
struct ReadSeekMediaSource<R> {
    inner: R,
    byte_len: Option<u64>,
}

#[cfg(feature = "decode")]
impl<R> ReadSeekMediaSource<R> {
    fn new(inner: R, byte_len: Option<u64>) -> Self {
        Self { inner, byte_len }
    }
}

#[cfg(feature = "decode")]
impl<R: Read> Read for ReadSeekMediaSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            match self.inner.read(buf) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

#[cfg(feature = "decode")]
impl<R: Seek> Seek for ReadSeekMediaSource<R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[cfg(feature = "decode")]
impl<R: Read + Seek + Send + Sync> MediaSource for ReadSeekMediaSource<R> {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

/// A decoded interleaved PCM audio buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct PcmAudio {
    sample_rate: u32,
    channels: u16,
    channel_mask: Option<u32>,
    samples: Vec<i16>,
}

impl PcmAudio {
    /// Creates a PCM buffer from interleaved 16-bit samples.
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<i16>) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::invalid_argument(
                "sample rate",
                "Invalid input sample rate: must be greater than zero",
            ));
        }
        if channels == 0 {
            return Err(Error::invalid_argument(
                "channels",
                "Invalid number of input channels: must be greater than zero",
            ));
        }
        if !samples.len().is_multiple_of(usize::from(channels)) {
            return Err(Error::invalid_argument(
                "samples",
                "Interleaved PCM sample count must be divisible by the channel count",
            ));
        }
        Ok(Self {
            sample_rate,
            channels,
            channel_mask: (channels <= 18).then(|| (1_u32 << channels) - 1),
            samples,
        })
    }

    /// Returns the source sample rate in Hz.
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the channel count.
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Returns the speaker mask in WAV bit order, when the layout is known.
    /// Constructed PCM uses the lowest `channels` speaker bits for up to 18 channels.
    pub const fn channel_mask(&self) -> Option<u32> {
        self.channel_mask
    }

    /// Sets a WAV speaker layout without changing the interleaved sample order.
    pub fn with_channel_mask(mut self, mask: u32) -> Result<Self, Error> {
        if mask & !0x3ffff != 0 || mask.count_ones() != u32::from(self.channels) {
            return Err(Error::invalid_argument(
                "channel mask",
                "Speaker mask must contain one supported WAV position per channel",
            ));
        }
        self.channel_mask = Some(mask);
        Ok(self)
    }

    /// Returns the interleaved PCM samples.
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    /// Returns the number of audio frames.
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Returns the duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        self.frame_count() as f64 / self.sample_rate as f64
    }
}

/// Waveform scale selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleSpec {
    /// Generate exactly this many points per channel from the decoded PCM.
    /// Empty input stays empty; when there are fewer frames than points, samples repeat.
    /// This scale is only supported for generation, not waveform resampling.
    Points(u32),
    /// Use a fixed number of source samples per waveform point.
    SamplesPerPixel(u32),
    /// Derive samples per waveform point from a target number of rendered pixels per second.
    PixelsPerSecond(u32),
    /// Fit a duration or full clip into the provided width.
    FitWidth {
        /// Output width in pixels.
        width_pixels: u32,
        /// Optional `(start_time, end_time)` range in seconds.
        time_range: Option<(f64, f64)>,
    },
}

impl ScaleSpec {
    /// Checks scale arguments that do not depend on source metadata.
    fn validate(self) -> Result<(), Error> {
        match self {
            Self::Points(0) => {
                return Err(Error::invalid_argument(
                    "points",
                    "Invalid points: must be greater than zero",
                ));
            }
            Self::SamplesPerPixel(0 | 1) => {
                return Err(Error::invalid_argument("zoom", "Invalid zoom: minimum 2"));
            }
            Self::PixelsPerSecond(0) => {
                return Err(Error::invalid_argument(
                    "pixels per second",
                    "Invalid pixels per second: must be greater than zero",
                ));
            }
            Self::FitWidth {
                width_pixels,
                time_range,
            } => {
                if width_pixels == 0 {
                    return Err(Error::invalid_argument(
                        "image width",
                        "Invalid image width: minimum 1",
                    ));
                }
                if let Some((start, end)) = time_range {
                    if !start.is_finite() || start < 0.0 {
                        return Err(Error::invalid_argument(
                            "start time",
                            "Invalid start time: minimum 0",
                        ));
                    }
                    if !end.is_finite() || end < start {
                        return Err(Error::invalid_argument(
                            "end time",
                            format!("Invalid end time, must be greater than {start}"),
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Resolves the scale to a concrete number of samples per waveform point.
    /// For `Points`, returns a nominal integer scale, rounded down with a minimum of 2.
    pub fn resolve(self, sample_rate: u32, frame_count: usize) -> Result<u32, Error> {
        self.resolve_frames(sample_rate, frame_count as u64)
    }

    pub(crate) fn resolve_frames(self, sample_rate: u32, frame_count: u64) -> Result<u32, Error> {
        self.validate()?;
        let resolved = match self {
            Self::Points(points) => u32::try_from((frame_count / u64::from(points)).max(2))
                .map_err(|_| {
                    Error::invalid_argument("points", "Too many source frames per point")
                })?,
            Self::SamplesPerPixel(value) => value,
            Self::PixelsPerSecond(value) => sample_rate / value,
            Self::FitWidth {
                width_pixels,
                time_range,
            } => {
                let frames = if let Some((start, end)) = time_range {
                    let frames = (end - start) * f64::from(sample_rate);
                    if !frames.is_finite() || frames >= u64::MAX as f64 {
                        return Err(Error::invalid_argument(
                            "time range",
                            "Time range contains too many source frames",
                        ));
                    }
                    frames as u64
                } else {
                    frame_count
                };
                u32::try_from(frames / u64::from(width_pixels)).map_err(|_| {
                    Error::invalid_argument("image width", "Too many source frames per pixel")
                })?
            }
        };

        if resolved < 2 {
            return Err(Error::invalid_argument("zoom", "Invalid zoom: minimum 2"));
        }

        Ok(resolved)
    }
}

/// Waveform generation settings.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerateOptions {
    /// Scale selection.
    pub scale: ScaleSpec,
    /// Whether to keep each source channel separate in the waveform output.
    pub split_channels: bool,
    /// Optional post-generation amplitude scaling.
    pub amplitude_scale: Option<AmplitudeScale>,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            scale: ScaleSpec::SamplesPerPixel(256),
            split_channels: false,
            amplitude_scale: None,
        }
    }
}

impl GenerateOptions {
    // Validate before decoding, counting, or temporary-file I/O. Keep the
    // generation API's end-time diagnostic and reject empty ranges before I/O;
    // direct scale resolution rejects an empty range as a sub-minimum zoom.
    fn validate(&self) -> Result<(), Error> {
        self.scale.validate().map_err(|error| match error {
            Error::InvalidArgument {
                name: "end time", ..
            } => invalid_generation_end_time(),
            other => other,
        })?;
        if matches!(self.scale, ScaleSpec::FitWidth { time_range: Some((start, end)), .. } if start == end)
        {
            return Err(invalid_generation_end_time());
        }
        if let Some(scale) = self.amplitude_scale {
            scale.validate()?;
        }
        Ok(())
    }
}

fn invalid_generation_end_time() -> Error {
    Error::invalid_argument(
        "end time",
        "Invalid end time: must be finite and greater than the start time",
    )
}

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

/// Generates a waveform from in-memory PCM samples.
pub fn generate_waveform_from_pcm(
    pcm: &PcmAudio,
    options: &GenerateOptions,
) -> Result<Waveform, Error> {
    options.validate()?;
    let mut peaks =
        PeakAccumulator::new(pcm.sample_rate, pcm.channels, pcm.frame_count(), options)?;
    peaks.push(&pcm.samples)?;
    peaks.finish(options)
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

/// Generates a waveform from an audio file path using Symphonia.
#[cfg(feature = "decode")]
pub fn generate_waveform_from_path(
    path: impl AsRef<Path>,
    options: &GenerateOptions,
) -> Result<Waveform, Error> {
    options.validate()?;
    let path = path.as_ref();
    let format = AudioFormat::from_path(path).ok_or_else(|| Error::UnsupportedFormat {
        format: path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
    })?;
    let file = File::open(path)?;
    generate_waveform_from_reader(file, Some(format), options)
}

/// Decodes an audio file path into interleaved 16-bit PCM using Symphonia.
#[cfg(feature = "decode")]
pub fn decode_audio_from_path(path: impl AsRef<Path>) -> Result<PcmAudio, Error> {
    let path = path.as_ref();
    let format = AudioFormat::from_path(path).ok_or_else(|| Error::UnsupportedFormat {
        format: path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
    })?;
    let file = File::open(path)?;
    decode_audio_from_reader(file, Some(format))
}

/// Generates a waveform from an arbitrary seekable audio reader using Symphonia.
/// Aggregates decoded packets without retaining the full PCM recording. Exact
/// point counts and full-clip `FitWidth` decode twice: first to count frames,
/// then to aggregate. Other scales decode once. The reader must remain unchanged
/// between passes. Memory includes decoder/container state and the waveform output.
#[cfg(feature = "decode")]
pub fn generate_waveform_from_reader<R: Read + Seek + Send + Sync + 'static>(
    reader: R,
    format_hint: Option<AudioFormat>,
    options: &GenerateOptions,
) -> Result<Waveform, Error> {
    options.validate()?;
    let mut reader = reader;
    let count_frames = needs_frame_count(options.scale);
    let start = if count_frames {
        reader.stream_position()?
    } else {
        0
    };
    let mut source = media_source_stream(reader)?;
    let expected = if count_frames {
        let (info, returned_source) =
            decode_audio_stream(source, format_hint, true, |_, _, _| Ok(()))?;
        source = returned_source;
        source.seek(SeekFrom::Start(start))?;
        Some(info)
    } else {
        None
    };

    let mut peaks = None;
    let (info, _) = decode_audio_stream(
        source,
        format_hint,
        false,
        |sample_rate, channels, samples| {
            if peaks.is_none() {
                peaks = Some(PeakAccumulator::new(
                    sample_rate,
                    channels,
                    expected.map_or(0, |info| info.frames),
                    options,
                )?);
            }
            peaks
                .as_mut()
                .expect("initialized accumulator")
                .push(samples)
        },
    )?;
    if expected.is_some_and(|expected| expected != info) {
        return Err(Error::invalid_data(
            "Audio stream changed between decoding passes",
        ));
    }
    let peaks = match peaks {
        Some(peaks) => peaks,
        None => PeakAccumulator::new(info.sample_rate, info.channels, info.frames, options)?,
    };
    peaks.finish(options)
}

/// Decodes an arbitrary seekable audio reader into interleaved 16-bit PCM using Symphonia.
/// This explicitly retains the entire decoded recording in memory. Use
/// `generate_waveform_from_reader` when only waveform peaks are needed.
#[cfg(feature = "decode")]
pub fn decode_audio_from_reader<R: Read + Seek + Send + Sync + 'static>(
    reader: R,
    format_hint: Option<AudioFormat>,
) -> Result<PcmAudio, Error> {
    decode_audio_reader(reader, format_hint)
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

fn clamp_float_to_i16(value: f64) -> i16 {
    value.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

#[cfg(feature = "decode")]
pub(crate) fn decode_audio_reader<R: Read + Seek + Send + Sync + 'static>(
    reader: R,
    format_hint: Option<AudioFormat>,
) -> Result<PcmAudio, Error> {
    let mut samples = Vec::new();
    let (info, _) = decode_audio_stream(
        media_source_stream(reader)?,
        format_hint,
        false,
        |_, _, block| {
            samples.extend_from_slice(block);
            Ok(())
        },
    )?;
    let mut pcm = PcmAudio::new(info.sample_rate, info.channels, samples)?;
    pcm.channel_mask = info.channel_mask.filter(|mask| mask & !0x3ffff == 0);
    Ok(pcm)
}

#[cfg(feature = "decode")]
fn media_source_stream<R: Read + Seek + Send + Sync + 'static>(
    mut reader: R,
) -> Result<MediaSourceStream, Error> {
    let position = reader.stream_position()?;
    let byte_len = reader.seek(SeekFrom::End(0)).ok();
    reader.seek(SeekFrom::Start(position))?;
    let mut source = MediaSourceStream::new(
        Box::new(ReadSeekMediaSource::new(reader, byte_len)),
        Default::default(),
    );
    if position != 0 {
        source.seek(SeekFrom::Start(position))?;
    }
    Ok(source)
}

#[cfg(feature = "decode")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedInfo {
    sample_rate: u32,
    channels: u16,
    channel_mask: Option<u32>,
    frames: usize,
}

/// Decode one packet at a time. Returning the underlying stream lets a counting
/// pass restart probing from the beginning, even for formats without demuxer seeking.
#[cfg(feature = "decode")]
fn decode_audio_stream(
    source: MediaSourceStream,
    format_hint: Option<AudioFormat>,
    count_only: bool,
    mut visit: impl FnMut(u32, u16, &[i16]) -> Result<(), Error>,
) -> Result<(DecodedInfo, MediaSourceStream), Error> {
    let mut hint = Hint::new();
    if let Some(format) = format_hint {
        format.ensure_enabled()?;
        hint.with_extension(format.as_str());
    }
    let probed = probe::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .into_iter()
        .chain(format.tracks())
        .find(|track| get_codecs().get_codec(track.codec_params.codec).is_some())
        .ok_or_else(|| Error::UnsupportedFormat {
            format: "no supported audio track in this build".into(),
        })?;
    let track_id = track.id;
    let mut codec_params = track.codec_params.clone();
    if codec_params.codec == CODEC_TYPE_ALAC {
        // CAF can wrap ALAC configuration in legacy frma/alac atoms; the decoder wants the payload.
        const WRAPPER: &[u8] = b"\x00\x00\x00\x0cfrmaalac\x00\x00\x00\x24alac\x00\x00\x00\x00";
        if let Some(cookie) = &codec_params.extra_data
            && cookie.len() == WRAPPER.len() + 24
            && cookie.starts_with(WRAPPER)
        {
            codec_params.extra_data = Some(cookie[WRAPPER.len()..].into());
        }
    }
    let mut sample_rate = codec_params.sample_rate;
    let mut channels = codec_params
        .channels
        .map(|channels| channels.count() as u16);
    let mut channel_mask = codec_params.channels.map(|channels| channels.bits());
    let mut delay_remaining = codec_params.delay.unwrap_or(0) as usize;
    let mut decoder = get_codecs().make(&codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();
    let mut conversion: Option<SampleBuffer<i16>> = None;
    let mut saw_frames = false;
    let mut frames = 0_usize;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };

        let spec = decoded.spec();
        let decoded_channels = spec.channels.count() as u16;
        if spec.rate == 0 || decoded_channels == 0 {
            return Err(Error::invalid_data(
                "Invalid decoded audio sample rate or channel count",
            ));
        }
        if saw_frames
            && (sample_rate != Some(spec.rate) || channel_mask != Some(spec.channels.bits()))
        {
            return Err(Error::invalid_data(
                "Audio stream changes sample rate or channel layout",
            ));
        }
        // MP4 can leave channel metadata in codec configuration rather than the container track.
        sample_rate = Some(spec.rate);
        channels = Some(decoded_channels);
        channel_mask = Some(spec.channels.bits());

        saw_frames |= decoded.frames() > 0;
        let block_frames = decoded.frames();
        let skip = delay_remaining.min(block_frames);
        delay_remaining -= skip;
        frames = frames
            .checked_add(block_frames - skip)
            .ok_or_else(|| Error::invalid_data("Audio frame count is too large"))?;
        if count_only || skip == block_frames {
            continue;
        }
        samples.clear();
        let interleaved = match decoded {
            AudioBufferRef::F32(buffer) => {
                extend_interleaved_f32_samples(buffer.as_ref(), &mut samples);
                samples.as_slice()
            }
            AudioBufferRef::F64(buffer) => {
                extend_interleaved_f64_samples(buffer.as_ref(), &mut samples);
                samples.as_slice()
            }
            _ => {
                let spec = *decoded.spec();
                let required = decoded.capacity() * spec.channels.count();
                if conversion
                    .as_ref()
                    .is_none_or(|buffer| buffer.capacity() < required)
                {
                    conversion = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
                }
                let sample_buffer = conversion.as_mut().expect("conversion buffer initialized");
                sample_buffer.copy_interleaved_ref(decoded);
                sample_buffer.samples()
            }
        };
        let samples = &interleaved[skip * usize::from(decoded_channels)..];
        if !samples.is_empty() {
            visit(
                sample_rate.expect("decoded sample rate"),
                decoded_channels,
                samples,
            )?;
        }
    }

    let sample_rate = sample_rate.ok_or(Error::MissingMetadata {
        name: "sample_rate",
    })?;
    let channels = channels.ok_or(Error::MissingMetadata { name: "channels" })?;
    if sample_rate == 0 || channels == 0 {
        return Err(Error::invalid_data(
            "Invalid decoded audio sample rate or channel count",
        ));
    }
    Ok((
        DecodedInfo {
            sample_rate,
            channels,
            channel_mask,
            frames,
        },
        format.into_inner(),
    ))
}

#[cfg(feature = "decode")]
fn extend_interleaved_f32_samples(
    buffer: &symphonia::core::audio::AudioBuffer<f32>,
    samples: &mut Vec<i16>,
) {
    let channels = buffer.spec().channels.count();
    for frame in 0..buffer.frames() {
        for channel in 0..channels {
            let sample = buffer.chan(channel)[frame];
            samples.push(clamp_float_to_i16(f64::from(sample) * f64::from(i16::MAX)));
        }
    }
}

#[cfg(feature = "decode")]
fn extend_interleaved_f64_samples(
    buffer: &symphonia::core::audio::AudioBuffer<f64>,
    samples: &mut Vec<i16>,
) {
    let channels = buffer.spec().channels.count();
    for frame in 0..buffer.frames() {
        for channel in 0..channels {
            let sample = buffer.chan(channel)[frame];
            samples.push(clamp_float_to_i16(sample * f64::from(i16::MAX)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        GenerateOptions, PcmAudio, RawAudioConfig, RawSampleFormat, ScaleSpec,
        generate_waveform_from_pcm, parse_raw_audio, parse_raw_sample,
    };
    use crate::{AmplitudeScale, WaveformPoint};

    #[test]
    fn validates_pcm_audio_construction() {
        let pcm = PcmAudio::new(48_000, 2, vec![1, 2, 3, 4]).expect("pcm");
        assert_eq!(pcm.frame_count(), 2);
        assert_eq!(pcm.duration_seconds(), 2.0 / 48_000.0);

        let error = PcmAudio::new(0, 1, vec![1]).expect_err("invalid sample rate");
        assert_eq!(
            error.to_string(),
            "Invalid input sample rate: must be greater than zero"
        );

        let error = PcmAudio::new(48_000, 0, vec![1]).expect_err("invalid channels");
        assert_eq!(
            error.to_string(),
            "Invalid number of input channels: must be greater than zero"
        );

        let error = PcmAudio::new(48_000, 2, vec![1, 2, 3]).expect_err("unaligned samples");
        assert_eq!(
            error.to_string(),
            "Interleaved PCM sample count must be divisible by the channel count"
        );
    }

    #[test]
    fn resolves_scale_specifications_and_rejects_invalid_values() {
        assert_eq!(
            ScaleSpec::SamplesPerPixel(64)
                .resolve(48_000, 96_000)
                .expect("samples per pixel"),
            64
        );
        assert_eq!(
            ScaleSpec::PixelsPerSecond(100)
                .resolve(48_000, 96_000)
                .expect("pixels per second"),
            480
        );
        assert_eq!(
            ScaleSpec::FitWidth {
                width_pixels: 400,
                time_range: Some((0.0, 4.0)),
            }
            .resolve(48_000, 0)
            .expect("fit width"),
            480
        );

        let error = ScaleSpec::PixelsPerSecond(0)
            .resolve(48_000, 0)
            .expect_err("invalid pixels per second");
        assert_eq!(
            error.to_string(),
            "Invalid pixels per second: must be greater than zero"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 0,
            time_range: None,
        }
        .resolve(48_000, 96_000)
        .expect_err("invalid width");
        assert_eq!(error.to_string(), "Invalid image width: minimum 1");

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((5.0, 4.0)),
        }
        .resolve(48_000, 96_000)
        .expect_err("invalid range");
        assert_eq!(
            error.to_string(),
            "Invalid end time, must be greater than 5"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((f64::INFINITY, 10.0)),
        }
        .resolve(48_000, 96_000)
        .expect_err("non-finite start time");
        assert_eq!(error.to_string(), "Invalid start time: minimum 0");

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((0.0, f64::INFINITY)),
        }
        .resolve(48_000, 96_000)
        .expect_err("non-finite end time");
        assert_eq!(
            error.to_string(),
            "Invalid end time, must be greater than 0"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 100_000,
            time_range: None,
        }
        .resolve(48_000, 96_000)
        .expect_err("zoom too small");
        assert_eq!(error.to_string(), "Invalid zoom: minimum 2");
    }

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

    #[test]
    fn generates_waveforms_from_pcm_for_mixed_and_split_channels() {
        let pcm = PcmAudio::new(48_000, 2, vec![100, 300, 200, 400, -100, -300, -200, -400])
            .expect("pcm");

        let mixed = generate_waveform_from_pcm(
            &pcm,
            &GenerateOptions {
                scale: ScaleSpec::SamplesPerPixel(2),
                split_channels: false,
                amplitude_scale: None,
            },
        )
        .expect("mixed waveform");
        assert_eq!(mixed.channels(), 1);
        assert_eq!(
            mixed.point(0, 0).expect("first point"),
            WaveformPoint { min: 200, max: 300 }
        );
        assert_eq!(
            mixed.point(0, 1).expect("second point"),
            WaveformPoint {
                min: -300,
                max: -200,
            }
        );

        let split = generate_waveform_from_pcm(
            &pcm,
            &GenerateOptions {
                scale: ScaleSpec::SamplesPerPixel(2),
                split_channels: true,
                amplitude_scale: Some(AmplitudeScale::Fixed(2.0)),
            },
        )
        .expect("split waveform");
        assert_eq!(split.channels(), 2);
        assert_eq!(
            split.point(0, 0).expect("left point"),
            WaveformPoint { min: 200, max: 400 }
        );
        assert_eq!(
            split.point(1, 1).expect("right point"),
            WaveformPoint {
                min: -800,
                max: -600,
            }
        );
    }
}
