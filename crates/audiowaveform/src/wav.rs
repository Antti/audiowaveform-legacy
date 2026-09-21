use std::fs::File;
use std::io::Write;
#[cfg(feature = "decode")]
use std::io::{Read, Seek};
use std::path::Path;

use byteorder::{LittleEndian, WriteBytesExt};

#[cfg(feature = "decode")]
use crate::AudioFormat;
#[cfg(feature = "decode")]
use crate::audio::decode_audio_reader;
use crate::{Error, PcmAudio};

/// Writes interleaved PCM audio as a 16-bit WAV stream.
pub fn write_pcm_as_wav<W: Write>(pcm: &PcmAudio, mut writer: W) -> Result<(), Error> {
    let header = validated_wav_header(pcm)?;
    writer.write_all(b"RIFF")?;
    writer.write_u32::<LittleEndian>(header.riff_size)?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_u32::<LittleEndian>(if header.extensible { 40 } else { 16 })?;
    writer.write_u16::<LittleEndian>(if header.extensible { 0xfffe } else { 1 })?;
    writer.write_u16::<LittleEndian>(pcm.channels())?;
    writer.write_u32::<LittleEndian>(pcm.sample_rate())?;
    writer.write_u32::<LittleEndian>(header.byte_rate)?;
    writer.write_u16::<LittleEndian>(header.block_align)?;
    writer.write_u16::<LittleEndian>(16)?;
    if header.extensible {
        writer.write_u16::<LittleEndian>(22)?;
        writer.write_u16::<LittleEndian>(16)?;
        writer.write_u32::<LittleEndian>(pcm.channel_mask().unwrap_or(0))?;
        // KSDATAFORMAT_SUBTYPE_PCM, in little-endian GUID byte order.
        writer.write_all(&[
            1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
        ])?;
    }
    writer.write_all(b"data")?;
    writer.write_u32::<LittleEndian>(header.data_size)?;
    // Bounded staging also avoids one write call per sample, without duplicating PCM.
    let mut bytes = [0; 8192];
    for samples in pcm.samples().chunks(bytes.len() / 2) {
        for (sample, output) in samples.iter().zip(bytes.as_chunks_mut::<2>().0.iter_mut()) {
            output.copy_from_slice(&sample.to_le_bytes());
        }
        writer.write_all(&bytes[..samples.len() * 2])?;
    }
    Ok(())
}

/// Writes PCM audio to a 16-bit WAV file after validating its header limits.
/// Invalid metadata leaves an existing destination unchanged.
pub fn write_pcm_to_wav_path(pcm: &PcmAudio, path: impl AsRef<Path>) -> Result<(), Error> {
    validated_wav_header(pcm)?;
    write_pcm_as_wav(pcm, File::create(path)?)
}

struct WavHeader {
    extensible: bool,
    block_align: u16,
    byte_rate: u32,
    data_size: u32,
    riff_size: u32,
}

fn validated_wav_header(pcm: &PcmAudio) -> Result<WavHeader, Error> {
    let default_mask = (pcm.channels() <= 2).then(|| (1_u32 << pcm.channels()) - 1);
    let extensible = pcm.channels() > 2 || pcm.channel_mask() != default_mask;
    validate_header(
        pcm.sample_rate(),
        pcm.channels(),
        pcm.samples().len(),
        extensible,
    )
}

fn validate_header(
    sample_rate: u32,
    channels: u16,
    samples: usize,
    extensible: bool,
) -> Result<WavHeader, Error> {
    let block_align = channels.checked_mul(2).ok_or_else(|| {
        Error::invalid_argument(
            "channels",
            "Channel count exceeds the WAV block alignment limit",
        )
    })?;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| {
            Error::invalid_argument(
                "sample rate",
                "Sample rate and channel count exceed the WAV byte rate limit",
            )
        })?;
    let data_size = u32::try_from(samples)
        .ok()
        .and_then(|samples| samples.checked_mul(2))
        .ok_or_else(|| {
            Error::invalid_argument("samples", "Audio exceeds the WAV RIFF size limit")
        })?;
    let riff_size = data_size
        .checked_add(if extensible { 60 } else { 36 })
        .ok_or_else(|| {
            Error::invalid_argument("samples", "Audio exceeds the WAV RIFF size limit")
        })?;
    Ok(WavHeader {
        extensible,
        block_align,
        byte_rate,
        data_size,
        riff_size,
    })
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
            assert!(
                super::validate_header(48_000, channels, maximum_samples, channels > 2).is_ok()
            );
            assert!(
                super::validate_header(48_000, channels, maximum_samples + 1, channels > 2)
                    .is_err()
            );
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
