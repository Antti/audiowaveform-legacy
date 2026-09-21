//! Guard Symphonia's WAV mask repair before it can change the channel layout.
//! Register at the preferred tier so validation also applies without a format
//! hint, after metadata tags, and beyond the stream start. Symphonia 0.6.1 fixes
//! the ADTS estimator offset/zero-step bugs guarded here for 0.5.

use std::sync::OnceLock;

use symphonia::core::formats::probe::Probe;

pub(super) fn get_probe() -> &'static Probe {
    static PROBE: OnceLock<Probe> = OnceLock::new();
    PROBE.get_or_init(|| {
        let mut probe = Probe::default();
        #[cfg(feature = "format-wav")]
        probe.register_format_at_tier::<GuardedWavReader<'_>>(
            symphonia::core::common::Tier::Preferred,
        );
        symphonia::default::register_enabled_formats(&mut probe);
        probe
    })
}

#[cfg(feature = "format-wav")]
use wav::GuardedWavReader;

#[cfg(feature = "format-wav")]
mod wav {
    use symphonia::core::errors::Result;
    use symphonia::core::formats::probe::{ProbeFormatData, ProbeableFormat, Score, Scoreable};
    use symphonia::core::formats::{
        FormatInfo, FormatOptions, FormatReader, MediaInfo, SeekMode, SeekTo, SeekedTo, Track,
    };
    use symphonia::core::io::{MediaSourceStream, ScopedStream};
    use symphonia::core::meta::Metadata;
    use symphonia::core::packet::Packet;
    use symphonia::default::formats::WavReader;

    pub(super) struct GuardedWavReader<'s>(WavReader<'s>);

    impl Scoreable for GuardedWavReader<'_> {
        fn score(source: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
            WavReader::score(source)
        }
    }

    impl ProbeableFormat<'_> for GuardedWavReader<'_> {
        fn try_probe_new(
            mut source: MediaSourceStream<'_>,
            options: FormatOptions,
        ) -> Result<Box<dyn FormatReader + '_>> {
            super::validate_wave_layout(&mut source)?;
            Ok(Box::new(GuardedWavReader(WavReader::try_new(
                source, options,
            )?)))
        }

        fn probe_data() -> &'static [ProbeFormatData] {
            WavReader::probe_data()
        }
    }

    impl FormatReader for GuardedWavReader<'_> {
        fn format_info(&self) -> &FormatInfo {
            self.0.format_info()
        }
        fn media_info(&self) -> &MediaInfo {
            self.0.media_info()
        }
        fn metadata(&mut self) -> Metadata<'_> {
            self.0.metadata()
        }
        fn seek(&mut self, mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
            self.0.seek(mode, to)
        }
        fn tracks(&self) -> &[Track] {
            self.0.tracks()
        }
        fn next_packet(&mut self) -> Result<Option<Packet>> {
            self.0.next_packet()
        }
        fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
        where
            Self: 's,
        {
            Box::new(self.0).into_inner()
        }
    }
}

#[cfg(feature = "format-wav")]
fn validate_wave_layout(
    source: &mut symphonia::core::io::MediaSourceStream<'_>,
) -> symphonia::core::errors::Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    use symphonia::core::errors::decode_error;
    use symphonia::core::io::{ReadBytes, SeekBuffered};

    let start = source.pos();
    let mut header = [0; 12];
    source.read_exact(&mut header)?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return decode_error("wav: invalid RIFF/WAVE header");
    }
    let riff_len = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let end = start.checked_add(8 + u64::from(riff_len)).ok_or(
        symphonia::core::errors::Error::DecodeError("wav: invalid RIFF size"),
    )?;
    // Inspect only headers. Chunk contents and sample data remain in the reader.
    while source.pos().saturating_add(8) <= end {
        let mut chunk = [0; 8];
        source.read_exact(&mut chunk)?;
        let len = u32::from_le_bytes(chunk[4..].try_into().unwrap());
        let next = source
            .pos()
            .checked_add(u64::from(len) + u64::from(len % 2))
            .ok_or(symphonia::core::errors::Error::DecodeError(
                "wav: invalid chunk size",
            ))?;
        if &chunk[..4] == b"data" {
            break;
        }
        if next > end {
            return decode_error("wav: chunk exceeds RIFF size");
        }
        if &chunk[..4] == b"fmt " {
            if len < 16 {
                return decode_error("wav: incomplete format chunk");
            }
            let mut fmt = [0; 16];
            source.read_exact(&mut fmt)?;
            let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
            if !(1..=18).contains(&channels) {
                return decode_error("wav: unsupported channel count (expected 1..=18)");
            }
            if u16::from_le_bytes([fmt[0], fmt[1]]) == 0xfffe {
                if len < 40 {
                    return decode_error("wav: incomplete extensible format chunk");
                }
                let mut extension = [0; 8];
                source.read_exact(&mut extension)?;
                let mask = u32::from_le_bytes(extension[4..].try_into().unwrap());
                // A zero mask leaves the assignment unspecified. Symphonia can
                // infer the lowest channel bits safely for counts up to 18.
                if mask != 0 && (mask & !0x3ffff != 0 || mask.count_ones() != u32::from(channels)) {
                    return decode_error(
                        "wav: channel mask does not match supported speaker layout",
                    );
                }
            }
        }
        let remaining = next - source.pos();
        if remaining <= 4096 {
            source.ignore_bytes(remaining)?;
        } else {
            source.seek(SeekFrom::Start(next))?;
        }
    }
    if source.pos() - start >= isize::MAX as u64 || source.seek_buffered(start) != start {
        source.seek(SeekFrom::Start(start))?;
    }
    Ok(())
}
