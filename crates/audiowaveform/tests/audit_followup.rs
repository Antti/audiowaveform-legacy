//! Regression cases from the second library and consumer audit.
use audiowaveform::*;
#[cfg(any(feature = "format-wav", feature = "format-aac"))]
use std::io::Cursor;
use std::io::{self, Read, Seek, SeekFrom};

#[cfg(feature = "format-wav")]
fn wav(channels: u16, frames: usize) -> Vec<u8> {
    // Use an independent encoder, including its mismatched high-channel masks.
    let mut output = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut output,
        hound::WavSpec {
            channels,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for sample in 0..usize::from(channels) * frames {
        writer.write_sample(sample as i16).unwrap();
    }
    writer.finalize().unwrap();
    output.into_inner()
}

#[cfg(feature = "format-wav")]
#[test]
fn interrupted_encoded_reads_are_retried() {
    struct Interrupted {
        inner: Cursor<Vec<u8>>,
        first: bool,
    }
    impl Read for Interrupted {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.first {
                self.first = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.inner.read(output)
        }
    }
    impl Seek for Interrupted {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            self.inner.seek(from)
        }
    }
    let pcm = decode_audio_from_reader(
        Interrupted {
            inner: Cursor::new(wav(1, 100)),
            first: true,
        },
        None,
    )
    .unwrap();
    assert_eq!(pcm.frame_count(), 100);
}

#[cfg(feature = "format-wav")]
#[test]
fn external_wav_channel_masks_are_checked_before_demuxing() {
    for channels in [18, 19, 24, 32, 49, 50] {
        for hint in [None, Some(AudioFormat::Wav)] {
            let result = decode_audio_from_reader(Cursor::new(wav(channels, 2)), hint);
            if channels <= 18 {
                let pcm = result.unwrap();
                assert_eq!((pcm.channels(), pcm.frame_count()), (channels, 2));
            } else {
                assert!(result.is_err(), "channels={channels}");
            }
        }
    }
    for mask in [1_u32, 0x80000000, 0xffffffff] {
        let mut input = wav(6, 2);
        input[40..44].copy_from_slice(&mask.to_le_bytes());
        assert!(decode_audio_from_reader(Cursor::new(input), None).is_err());
    }
}

#[cfg(feature = "format-wav")]
#[test]
fn wav_guard_handles_chunks_padding_prefixes_and_initial_offsets() {
    let original = wav(6, 2);
    let mut tagged = original[..12].to_vec();
    tagged.extend_from_slice(b"JUNK\x03\0\0\0abc\0");
    tagged.extend_from_slice(&original[12..]);
    let size = tagged.len() as u32 - 8;
    tagged[4..8].copy_from_slice(&size.to_le_bytes());
    let mut source = Cursor::new([vec![0; 37], tagged].concat());
    source.set_position(37);
    let options = GenerateOptions {
        scale: ScaleSpec::Points(3),
        ..Default::default()
    };
    let wave = generate_waveform_from_reader(source, None, &options).unwrap();
    assert_eq!((wave.len(), wave.source_frames()), (3, Some(2)));
}

#[test]
fn dat_signed_fields_are_validated_before_reading_and_writing() {
    for (rate, scale) in [(u32::MAX, 64), (48_000, u32::MAX)] {
        let header: Vec<_> = [1_u32, 0, rate, scale, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        assert!(Waveform::load_from_reader(header.as_slice(), WaveformFormat::Dat).is_err());
        let wave = Waveform::new(rate, scale, 1).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.dat");
        std::fs::write(&path, b"keep me").unwrap();
        assert!(
            wave.write_to_path(&path, Some(WaveformFormat::Dat), None)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        let mut bytes = Vec::new();
        assert!(
            wave.write_to_writer(&mut bytes, WaveformFormat::Dat, None)
                .is_err()
        );
        assert!(bytes.is_empty());
        wave.write_to_writer(&mut bytes, WaveformFormat::Json, None)
            .unwrap();
    }
    let wave = Waveform::new(i32::MAX as u32, i32::MAX as u32, 1).unwrap();
    let mut bytes = Vec::new();
    wave.write_to_writer(&mut bytes, WaveformFormat::Dat, None)
        .unwrap();
    assert_eq!(
        Waveform::load_from_reader(bytes.as_slice(), WaveformFormat::Dat).unwrap(),
        wave
    );
}

#[cfg(feature = "format-aac")]
#[test]
fn padded_id3_aac_decodes_with_and_without_a_hint() {
    let audio = include_bytes!("../../../fixtures/formats/stereo.aac");
    let expected = decode_audio_from_reader(Cursor::new(audio.as_slice()), None).unwrap();
    for tag_length in [audio.len() / 2, audio.len(), audio.len() * 2] {
        let payload_size = tag_length - 10;
        let mut bytes = b"ID3\x04\0\0".to_vec();
        bytes.extend_from_slice(&[
            ((payload_size >> 21) & 127) as u8,
            ((payload_size >> 14) & 127) as u8,
            ((payload_size >> 7) & 127) as u8,
            (payload_size & 127) as u8,
        ]);
        bytes.resize(tag_length, 0);
        bytes.extend_from_slice(audio);
        for hint in [None, Some(AudioFormat::Aac)] {
            let pcm = decode_audio_from_reader(Cursor::new(bytes.clone()), hint).unwrap();
            assert_eq!(pcm, expected);
            let options = GenerateOptions {
                scale: ScaleSpec::Points(110),
                ..Default::default()
            };
            assert_eq!(
                generate_waveform_from_reader(Cursor::new(bytes.clone()), hint, &options).unwrap(),
                generate_waveform_from_pcm(&expected, &options).unwrap()
            );
        }
    }
}

struct NoIo;
impl Read for NoIo {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("invalid options must be rejected before reading")
    }
}
impl Seek for NoIo {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        panic!("invalid options must be rejected before seeking")
    }
}

#[test]
fn invalid_generation_options_do_no_input_io() {
    let mut options: Vec<_> = [
        ScaleSpec::Points(0),
        ScaleSpec::SamplesPerPixel(1),
        ScaleSpec::PixelsPerSecond(0),
        ScaleSpec::FitWidth {
            width_pixels: 0,
            time_range: None,
        },
        ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: Some((f64::NAN, 1.0)),
        },
        ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: Some((2.0, 1.0)),
        },
        ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: Some((1.0, 1.0)),
        },
    ]
    .into_iter()
    .map(|scale| GenerateOptions {
        scale,
        ..Default::default()
    })
    .collect();
    options.extend(
        [f64::NAN, f64::INFINITY, -1.0]
            .into_iter()
            .map(|value| GenerateOptions {
                scale: ScaleSpec::Points(110),
                amplitude_scale: Some(AmplitudeScale::Fixed(value)),
                ..Default::default()
            }),
    );
    for options in options {
        assert!(
            generate_waveform_from_raw_reader(
                NoIo,
                &RawAudioConfig::new(48_000, 1, RawSampleFormat::S16Le).unwrap(),
                &options
            )
            .is_err()
        );
        #[cfg(feature = "decode")]
        assert!(generate_waveform_from_reader(NoIo, None, &options).is_err());
    }
}

#[test]
fn resampling_wide_frame_coordinates_is_safe_on_32_bit_targets() {
    let wave =
        Waveform::from_interleaved_samples(48_000, 2_000_000_000, 1, vec![-1, 1, -2, 2, -3, 3], 16)
            .unwrap();
    assert_eq!(
        wave.resample(ScaleSpec::SamplesPerPixel(2_000_000_001))
            .unwrap()
            .interleaved_samples(),
        &[-1, 1, -2, 2, -3, 3]
    );
    assert_eq!(
        wave.resample(ScaleSpec::FitWidth {
            width_pixels: 2,
            time_range: None
        })
        .unwrap()
        .interleaved_samples(),
        &[-1, 1, -3, 3]
    );
}

#[test]
fn consuming_amplitude_scaling_reuses_the_sample_allocation() {
    for scale in [
        AmplitudeScale::Fixed(1.0),
        AmplitudeScale::Fixed(2.0),
        AmplitudeScale::Auto,
    ] {
        let wave = Waveform::from_interleaved_samples(48_000, 64, 1, vec![-100, 200], 16).unwrap();
        let expected = wave.scale_amplitude(scale).unwrap();
        let allocation = wave.interleaved_samples().as_ptr();
        let scaled = wave.into_scaled_amplitude(scale).unwrap();
        assert_eq!(scaled, expected);
        assert_eq!(scaled.interleaved_samples().as_ptr(), allocation);
    }
}

#[cfg(feature = "render")]
#[test]
fn exact_point_tail_render_matches_cropping_and_end_is_empty() {
    for (sample_rate, frames, points, boundary) in [
        (16_000, 960_000, 110, 55),
        (44_100, 11025, 110, 55),
        (48_000, 480_000, 100, 25),
    ] {
        let split = boundary * frames / points;
        let pcm = PcmAudio::new(
            sample_rate,
            1,
            [vec![0; split], vec![16_384; frames - split]].concat(),
        )
        .unwrap();
        let wave = generate_waveform_from_pcm(
            &pcm,
            &GenerateOptions {
                scale: ScaleSpec::Points(points as u32),
                ..Default::default()
            },
        )
        .unwrap();
        for style in [
            RenderStyle::Normal,
            RenderStyle::Bars {
                width: 1,
                gap: 0,
                style: BarStyle::Square,
            },
        ] {
            let options = RenderOptions {
                width: points as u32,
                height: 40,
                axis_labels: false,
                style,
                ..Default::default()
            };
            let full = render_waveform(&wave, &options).unwrap();
            let start = boundary as f64 * frames as f64 / points as f64 / sample_rate as f64;
            let tail = render_waveform(
                &wave,
                &RenderOptions {
                    width: (points - boundary) as u32,
                    start_time: start,
                    ..options.clone()
                },
            )
            .unwrap();
            for x in 0..tail.width() {
                for y in 0..tail.height() {
                    assert_eq!(tail.get_pixel(x, y), full.get_pixel(x + boundary as u32, y));
                }
            }
            let end = render_waveform(
                &wave,
                &RenderOptions {
                    width: 1,
                    start_time: wave.duration_seconds(),
                    ..options.clone()
                },
            )
            .unwrap();
            let bg = options.colors.background;
            assert!(
                end.pixels()
                    .all(|p| p.0 == [bg.red, bg.green, bg.blue, bg.alpha])
            );
        }
    }
}

#[cfg(all(feature = "wav-output", feature = "format-wav"))]
#[test]
fn wav_transcoding_preserves_speaker_positions_and_sample_order() {
    for (channels, mask) in [(6, 0x60f_u32), (1, 4), (2, 0x600)] {
        let pcm = PcmAudio::new(
            48_000,
            channels,
            (0..channels * 3).map(|n| n as i16).collect(),
        )
        .unwrap()
        .with_channel_mask(mask)
        .unwrap();
        let mut bytes = Vec::new();
        write_pcm_as_wav(&pcm, &mut bytes).unwrap();
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), mask);
        let decoded = decode_audio_from_reader(Cursor::new(bytes.clone()), None).unwrap();
        assert_eq!(decoded, pcm);
        let mut output = Vec::new();
        transcode_audio_reader_to_wav_writer(Cursor::new(bytes), None, &mut output).unwrap();
        assert_eq!(u32::from_le_bytes(output[40..44].try_into().unwrap()), mask);
        assert_eq!(
            decode_audio_from_reader(Cursor::new(output), None).unwrap(),
            pcm
        );
    }
}
