use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::codecs::{CODEC_TYPE_ALAC, CodecParameters, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_codecs;

use super::peaks::{PeakAccumulator, needs_frame_count};
use super::probe;
use crate::{AudioFormat, Error, GenerateOptions, PcmAudio, Waveform};

mod convert;
use convert::SampleConverter;

pub(super) struct ReadSeekMediaSource<R> {
    inner: R,
    byte_len: Option<u64>,
}

impl<R> ReadSeekMediaSource<R> {
    pub(super) fn new(inner: R, byte_len: Option<u64>) -> Self {
        Self { inner, byte_len }
    }
}

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

impl<R: Seek> Seek for ReadSeekMediaSource<R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<R: Read + Seek + Send + Sync> MediaSource for ReadSeekMediaSource<R> {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

/// Generates a waveform from an audio file path using Symphonia.
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
        let (info, returned_source) = DecoderSession::new(source, format_hint)?.count_frames()?;
        source = returned_source;
        source.seek(SeekFrom::Start(start))?;
        Some(info)
    } else {
        None
    };

    let mut peaks = None;
    let (info, _) = DecoderSession::new(source, format_hint)?.visit_samples(
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
pub fn decode_audio_from_reader<R: Read + Seek + Send + Sync + 'static>(
    reader: R,
    format_hint: Option<AudioFormat>,
) -> Result<PcmAudio, Error> {
    decode_audio_reader(reader, format_hint)
}

pub(crate) fn decode_audio_reader<R: Read + Seek + Send + Sync + 'static>(
    reader: R,
    format_hint: Option<AudioFormat>,
) -> Result<PcmAudio, Error> {
    let mut samples = Vec::new();
    let (info, _) = DecoderSession::new(media_source_stream(reader)?, format_hint)?.visit_samples(
        |_, _, block| {
            samples.extend_from_slice(block);
            Ok(())
        },
    )?;
    let mut pcm = PcmAudio::new(info.sample_rate, info.channels, samples)?;
    pcm.channel_mask = info.channel_mask.filter(|mask| mask & !0x3ffff == 0);
    Ok(pcm)
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedInfo {
    sample_rate: u32,
    channels: u16,
    channel_mask: Option<u32>,
    frames: usize,
}

/// A selected track and decoder, shared by counting and sample-visiting passes.
struct DecoderSession {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    codec_params: CodecParameters,
}

enum DecodeMode<F> {
    CountFrames,
    VisitSamples(F),
}

impl DecoderSession {
    fn new(source: MediaSourceStream, format_hint: Option<AudioFormat>) -> Result<Self, Error> {
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
        let format = probed.format;
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
        normalize_codec_parameters(&mut codec_params);
        let decoder = get_codecs().make(&codec_params, &DecoderOptions::default())?;
        Ok(Self {
            format,
            decoder,
            track_id,
            codec_params,
        })
    }

    fn count_frames(self) -> Result<(DecodedInfo, MediaSourceStream), Error> {
        self.run::<fn(u32, u16, &[i16]) -> Result<(), Error>>(DecodeMode::CountFrames)
    }

    fn visit_samples(
        self,
        visit: impl FnMut(u32, u16, &[i16]) -> Result<(), Error>,
    ) -> Result<(DecodedInfo, MediaSourceStream), Error> {
        self.run(DecodeMode::VisitSamples(visit))
    }

    // Keep one packet loop and metadata/delay policy for both operations. Return
    // the underlying stream so counting can replay formats without demuxer seeking.
    fn run<F>(self, mut mode: DecodeMode<F>) -> Result<(DecodedInfo, MediaSourceStream), Error>
    where
        F: FnMut(u32, u16, &[i16]) -> Result<(), Error>,
    {
        let Self {
            mut format,
            mut decoder,
            track_id,
            codec_params,
        } = self;
        let mut sample_rate = codec_params.sample_rate;
        let mut channels = codec_params
            .channels
            .map(|channels| channels.count() as u16);
        let mut channel_mask = codec_params.channels.map(|channels| channels.bits());
        let mut delay_remaining = codec_params.delay.unwrap_or(0) as usize;

        let mut conversion = SampleConverter::default();
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
            let DecodeMode::VisitSamples(visit) = &mut mode else {
                continue;
            };
            if skip == block_frames {
                continue;
            }
            let interleaved = conversion.interleaved(decoded);
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
}

fn normalize_codec_parameters(codec_params: &mut CodecParameters) {
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
}
