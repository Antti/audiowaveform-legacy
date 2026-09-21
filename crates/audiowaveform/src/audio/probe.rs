//! Guards around Symphonia 0.5's optional ADTS estimator and WAV mask repair.
//! Register before the default readers so guards also apply without a format hint,
//! after metadata tags, and when a container is found beyond the stream start.

use std::sync::OnceLock;

use symphonia::core::probe::Probe;

pub(super) fn get_probe() -> &'static Probe {
    static PROBE: OnceLock<Probe> = OnceLock::new();
    PROBE.get_or_init(|| {
        let mut probe = Probe::default();
        register_guards(&mut probe);
        symphonia::default::register_enabled_formats(&mut probe);
        probe
    })
}

fn register_guards(_probe: &mut Probe) {
    #[cfg(any(feature = "format-aac", feature = "format-m4a", feature = "format-mkv"))]
    {
        use std::io::{Seek, SeekFrom};
        use symphonia::core::formats::FormatReader;
        use symphonia::core::io::{MediaSourceStream, ReadBytes};
        use symphonia::core::probe::{Instantiate, QueryDescriptor};
        use symphonia::default::formats::AdtsReader;

        for descriptor in AdtsReader::query() {
            let mut descriptor = *descriptor;
            descriptor.inst = Instantiate::Format(|source, options| {
                // An ID3 prefix can make the upstream estimator subtract the
                // start offset twice, underflow or call step_by(0). Unknown byte
                // length disables only the estimate; we count decoded frames.
                let position = source.pos();
                let mut source = MediaSourceStream::new(
                    Box::new(super::decode::ReadSeekMediaSource::new(source, None)),
                    Default::default(),
                );
                source.seek(SeekFrom::Start(position))?;
                Ok(Box::new(AdtsReader::try_new(source, options)?))
            });
            _probe.register(&descriptor);
        }
    }
    #[cfg(feature = "format-wav")]
    {
        use symphonia::core::formats::FormatReader;
        use symphonia::core::probe::{Instantiate, QueryDescriptor};
        use symphonia::default::formats::WavReader;

        for descriptor in WavReader::query() {
            let mut descriptor = *descriptor;
            descriptor.inst = Instantiate::Format(|mut source, options| {
                validate_wave_layout(&mut source)?;
                Ok(Box::new(WavReader::try_new(source, options)?))
            });
            _probe.register(&descriptor);
        }
    }
}

#[cfg(feature = "format-wav")]
fn validate_wave_layout(
    source: &mut symphonia::core::io::MediaSourceStream,
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
