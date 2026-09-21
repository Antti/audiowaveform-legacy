use std::io::{self, Read};

use audiowaveform::{
    AmplitudeScale, Error, GenerateOptions, PcmAudio, RawAudioConfig, RawSampleFormat, ScaleSpec,
    generate_waveform_from_pcm, generate_waveform_from_raw_reader,
};

/// Models a pipe with unaligned short reads and one interrupted system call.
struct ShortReads<'a> {
    remaining: &'a [u8],
    chunk_size: usize,
    interrupted: bool,
}

impl Read for ShortReads<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let count = self.chunk_size.min(output.len()).min(self.remaining.len());
        output[..count].copy_from_slice(&self.remaining[..count]);
        self.remaining = &self.remaining[count..];
        Ok(count)
    }
}

#[test]
fn raw_pipes_preserve_peaks_across_unaligned_reads_and_bucket_boundaries() {
    let samples: Vec<i16> = (0..40_010)
        .map(|index| (index * 1009 % 65_536 - 32_768) as i16)
        .collect();
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let pcm = PcmAudio::new(48_000, 2, samples).unwrap();
    let config = RawAudioConfig::new(48_000, 2, RawSampleFormat::S16Le).unwrap();
    for scale in [
        ScaleSpec::SamplesPerPixel(311),
        ScaleSpec::PixelsPerSecond(100),
        ScaleSpec::Points(110),
        ScaleSpec::FitWidth {
            width_pixels: 110,
            time_range: None,
        },
        ScaleSpec::FitWidth {
            width_pixels: 110,
            time_range: Some((0.0, 0.25)),
        },
    ] {
        for split_channels in [false, true] {
            let options = GenerateOptions {
                scale,
                split_channels,
                amplitude_scale: Some(AmplitudeScale::Fixed(0.75)),
            };
            let expected = generate_waveform_from_pcm(&pcm, &options).unwrap();
            for chunk_size in [1, 7, 32_767] {
                let reader = ShortReads {
                    remaining: &bytes,
                    chunk_size,
                    interrupted: false,
                };
                let streamed =
                    generate_waveform_from_raw_reader(reader, &config, &options).unwrap();
                assert_eq!(
                    streamed, expected,
                    "{scale:?}, split={split_channels}, chunk={chunk_size}"
                );
            }
        }
    }
}

#[test]
fn raw_pipes_handle_empty_tiny_and_incomplete_frames() {
    let config = RawAudioConfig::new(48_000, 2, RawSampleFormat::S16Le).unwrap();
    for samples in [
        vec![],
        vec![-512_i16, 256],
        vec![1, 2, -32768, 32767, 100, -200],
    ] {
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let pcm = PcmAudio::new(48_000, 2, samples).unwrap();
        for scale in [ScaleSpec::Points(110), ScaleSpec::SamplesPerPixel(2)] {
            let options = GenerateOptions {
                scale,
                ..Default::default()
            };
            let reader = ShortReads {
                remaining: &bytes,
                chunk_size: 1,
                interrupted: false,
            };
            assert_eq!(
                generate_waveform_from_raw_reader(reader, &config, &options).unwrap(),
                generate_waveform_from_pcm(&pcm, &options).unwrap()
            );
            for malformed in [&[0_u8][..], &[0, 0][..], &[0, 0, 0][..]] {
                let error =
                    generate_waveform_from_raw_reader(malformed, &config, &options).unwrap_err();
                if malformed.len() == 2 {
                    assert!(matches!(
                        error,
                        Error::InvalidArgument {
                            name: "samples",
                            ..
                        }
                    ));
                } else {
                    assert!(matches!(error, Error::InvalidData { .. }));
                }
            }
        }
    }
}

#[test]
fn raw_streaming_propagates_midstream_io_errors() {
    struct FailingReader(bool);
    impl Read for FailingReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.0 {
                return Err(io::ErrorKind::Other.into());
            }
            self.0 = true;
            output[..4].fill(0);
            Ok(4)
        }
    }
    let config = RawAudioConfig::new(48_000, 1, RawSampleFormat::S16Le).unwrap();
    for scale in [ScaleSpec::Points(110), ScaleSpec::SamplesPerPixel(2)] {
        let options = GenerateOptions {
            scale,
            ..Default::default()
        };
        assert!(
            generate_waveform_from_raw_reader(FailingReader(false), &config, &options).is_err()
        );
    }
}

#[cfg(feature = "format-wav")]
mod encoded {
    use std::io::{Cursor, Seek, SeekFrom};

    use super::*;
    use audiowaveform::{AudioFormat, generate_waveform_from_reader};

    fn wav(frames: u32, rate: u32, channels: u16) -> Vec<u8> {
        let size = frames * u32::from(channels) * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.resize(44 + size as usize, 0);
        bytes
    }

    enum Replay {
        FailSeek,
        FailRead,
        Replace(Vec<u8>),
    }

    struct ChangingReader {
        inner: Cursor<Vec<u8>>,
        has_read: bool,
        fail_read: bool,
        replay: Option<Replay>,
    }

    impl ChangingReader {
        fn new(replay: Replay) -> Self {
            Self {
                inner: Cursor::new(wav(1000, 48_000, 2)),
                has_read: false,
                fail_read: false,
                replay: Some(replay),
            }
        }
    }

    impl Read for ChangingReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.fail_read && self.inner.position() >= 512 {
                return Err(io::Error::other("replay read failed"));
            }
            self.has_read = true;
            let count = output.len().min(512);
            self.inner.read(&mut output[..count])
        }
    }

    impl Seek for ChangingReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            if position == SeekFrom::Start(0) && self.has_read {
                match self.replay.take() {
                    Some(Replay::FailSeek) => return Err(io::Error::other("rewind failed")),
                    Some(Replay::FailRead) => self.fail_read = true,
                    Some(Replay::Replace(bytes)) => self.inner = Cursor::new(bytes),
                    None => {}
                }
            }
            self.inner.seek(position)
        }
    }

    #[test]
    fn replay_rejects_changed_frame_counts_and_metadata() {
        for scale in [
            ScaleSpec::Points(110),
            ScaleSpec::FitWidth {
                width_pixels: 110,
                time_range: None,
            },
        ] {
            let options = GenerateOptions {
                scale,
                ..Default::default()
            };
            for (frames, rate, channels) in [
                (999, 48_000, 2),
                (1001, 48_000, 2),
                (1000, 44_100, 2),
                (1000, 48_000, 1),
            ] {
                let reader = ChangingReader::new(Replay::Replace(wav(frames, rate, channels)));
                let error = generate_waveform_from_reader(reader, Some(AudioFormat::Wav), &options)
                    .unwrap_err();
                assert!(matches!(error, Error::InvalidData { .. }), "{error}");
                assert!(
                    error
                        .to_string()
                        .contains("changed between decoding passes")
                );
            }
        }
    }

    #[test]
    fn replay_propagates_io_failures_but_fixed_scales_need_no_rewind() {
        for (replay, message) in [
            (Replay::FailSeek, "rewind failed"),
            (Replay::FailRead, "replay read failed"),
        ] {
            let options = GenerateOptions {
                scale: ScaleSpec::Points(110),
                ..Default::default()
            };
            let error = generate_waveform_from_reader(
                ChangingReader::new(replay),
                Some(AudioFormat::Wav),
                &options,
            )
            .unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
        let waveform = generate_waveform_from_reader(
            ChangingReader::new(Replay::FailSeek),
            Some(AudioFormat::Wav),
            &GenerateOptions {
                scale: ScaleSpec::SamplesPerPixel(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(waveform.len(), 100);
    }
}
