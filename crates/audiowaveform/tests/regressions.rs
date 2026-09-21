//! Regression coverage for the library audit: allocation, file safety,
//! normalization, numeric boundaries, codec claims, and rendering.
use audiowaveform::{AmplitudeScale, Error, ScaleSpec, Waveform, WaveformFormat};

#[test]
fn dat_header_does_not_reserve_space_for_missing_samples() {
    for bits in [0_u32, 1] {
        let header: Vec<u8> = [2_u32, bits, 48_000, 256, u32::MAX, 24]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        match Waveform::load_from_reader(header.as_slice(), WaveformFormat::Dat) {
            Ok(waveform) => {
                assert!(waveform.is_empty());
                assert_eq!(waveform.allocated_bytes(), 0);
            }
            // On 32-bit platforms the declared sample count is unrepresentable.
            Err(Error::InvalidData { .. }) if usize::BITS == 32 => {}
            other => panic!("unexpected result: {other:?}"),
        }
    }
}

#[test]
fn automatic_normalization_preserves_peak_ratios_and_handles_silence() {
    for (input, expected) in [
        ([-100, 10_000], [-327, 32767]),
        ([-10_000, 100], [-32767, 327]),
        ([0, 100], [0, 32767]),
        ([-100, 0], [-32767, 0]),
        ([10, 100], [3276, 32767]),
        ([-100, -10], [-32767, -3276]),
        ([0, 0], [0, 0]),
    ] {
        let wave = Waveform::from_interleaved_samples(48_000, 256, 1, input.to_vec(), 16).unwrap();
        let scaled = wave.scale_amplitude(AmplitudeScale::Auto).unwrap();
        assert_eq!(scaled.interleaved_samples(), &expected, "input={input:?}");
    }
}

#[test]
fn fit_width_rejects_overflow_in_scale_and_time_range() {
    for end in [4_294_967_298.0, f64::MAX] {
        let scale = ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: Some((0.0, end)),
        };
        assert!(scale.resolve(1, 0).is_err());
    }
    #[cfg(target_pointer_width = "64")]
    assert!(
        ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: None
        }
        .resolve(48_000, 4_294_967_298)
        .is_err()
    );
    assert_eq!(
        ScaleSpec::FitWidth {
            width_pixels: 1,
            time_range: Some((0.0, f64::from(u32::MAX)))
        }
        .resolve(1, 0)
        .unwrap(),
        u32::MAX
    );
}

#[cfg(feature = "format-wav")]
mod decoding {
    use super::*;
    use audiowaveform::{
        AudioFormat, RawAudioConfig, RawSampleFormat, decode_audio_from_path,
        decode_audio_from_reader, decode_raw_audio_reader,
    };

    #[test]
    fn wave64_is_explicitly_unsupported() {
        assert_eq!(AudioFormat::from_extension("W64"), None);
        // Format rejection happens before trying to open even a missing file.
        assert!(matches!(decode_audio_from_path("unsupported.w64"),
            Err(Error::UnsupportedFormat { format }) if format == "w64"));
    }

    #[test]
    fn signed_integer_pcm_is_quantized_the_same_with_or_without_a_wav_container() {
        for (bits, little, big, values) in [
            (
                24_u16,
                RawSampleFormat::S24Le,
                RawSampleFormat::S24Be,
                vec![-8388608_i32, -257, -256, -1, 0, 255, 256, 8388607],
            ),
            (
                32,
                RawSampleFormat::S32Le,
                RawSampleFormat::S32Be,
                vec![i32::MIN, -65537, -65536, -1, 0, 65535, 65536, i32::MAX],
            ),
        ] {
            let width = usize::from(bits / 8);
            let payload: Vec<_> = values
                .iter()
                .flat_map(|n| n.to_le_bytes()[..width].to_vec())
                .collect();
            let reversed: Vec<_> = values
                .iter()
                .flat_map(|n| n.to_be_bytes()[4 - width..].to_vec())
                .collect();
            let mut wav = std::io::Cursor::new(Vec::new());
            {
                let mut writer = hound::WavWriter::new(
                    &mut wav,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 48_000,
                        bits_per_sample: bits,
                        sample_format: hound::SampleFormat::Int,
                    },
                )
                .unwrap();
                for value in values {
                    writer.write_sample(value).unwrap();
                }
                writer.finalize().unwrap();
            }
            wav.set_position(0);
            let container = decode_audio_from_reader(wav, Some(AudioFormat::Wav)).unwrap();
            for (format, bytes) in [(little, payload), (big, reversed)] {
                let raw = decode_raw_audio_reader(
                    bytes.as_slice(),
                    &RawAudioConfig::new(48_000, 1, format).unwrap(),
                )
                .unwrap();
                assert_eq!(raw.samples(), container.samples(), "{format:?}");
            }
        }
    }
}

#[cfg(feature = "render")]
mod rendering {
    use super::*;
    use audiowaveform::{
        BarStyle, RenderOptions, RenderStyle, render_waveform, render_waveform_to_path,
        write_waveform_png,
    };
    use std::io::{self, Write};

    fn waveform() -> Waveform {
        Waveform::from_interleaved_samples(48_000, 64, 1, vec![-32768, 32767], 16).unwrap()
    }

    struct FailingWriter {
        on_flush: bool,
    }
    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.on_flush && bytes == b"IEND" {
                return Err(io::Error::other("IEND failure"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.on_flush {
                Err(io::Error::other("flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn png_final_chunk_and_flush_errors_are_reported() {
        for on_flush in [false, true] {
            let error = write_waveform_png(
                &waveform(),
                &RenderOptions::default(),
                FailingWriter { on_flush },
            )
            .unwrap_err();
            assert!(error.to_string().contains(if on_flush {
                "flush failure"
            } else {
                "IEND failure"
            }));
        }
    }

    #[test]
    fn invalid_render_options_preserve_existing_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.png");
        for options in [
            RenderOptions {
                width: 0,
                ..Default::default()
            },
            RenderOptions {
                amplitude_scale: AmplitudeScale::Fixed(f64::NAN),
                ..Default::default()
            },
        ] {
            std::fs::write(&path, b"keep me").unwrap();
            assert!(render_waveform_to_path(&waveform(), &options, &path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        }
    }

    #[test]
    fn invalid_coordinates_return_errors_without_large_allocations() {
        for options in [
            RenderOptions {
                width: u32::MAX,
                height: 1,
                ..Default::default()
            },
            RenderOptions {
                width: 1,
                height: u32::MAX,
                ..Default::default()
            },
            RenderOptions {
                width: 1,
                height: 3,
                start_time: f64::MAX,
                ..Default::default()
            },
            RenderOptions {
                width: 1,
                height: 3,
                style: RenderStyle::Bars {
                    width: u32::MAX,
                    gap: 1,
                    style: BarStyle::Square,
                },
                ..Default::default()
            },
        ] {
            assert!(render_waveform(&waveform(), &options).is_err());
        }
    }

    #[test]
    fn tall_images_map_full_amplitude_without_integer_overflow() {
        for style in [
            RenderStyle::Normal,
            RenderStyle::Bars {
                width: 1,
                gap: 0,
                style: BarStyle::Square,
            },
        ] {
            let options = RenderOptions {
                width: 1,
                height: 32_769,
                axis_labels: false,
                style,
                ..Default::default()
            };
            let image = render_waveform(&waveform(), &options).unwrap();
            let color = options.colors.waveform[0];
            for y in [0, 16_384, 32_768] {
                assert_eq!(
                    image.get_pixel(0, y).0,
                    [color.red, color.green, color.blue, color.alpha]
                );
            }
        }
    }

    #[test]
    fn oversized_bars_only_draw_visible_pixels() {
        for style in [BarStyle::Square, BarStyle::Rounded] {
            let options = RenderOptions {
                width: 1,
                height: 3,
                axis_labels: false,
                style: RenderStyle::Bars {
                    width: 40_000,
                    gap: 0,
                    style,
                },
                ..Default::default()
            };
            let image = render_waveform(&waveform(), &options).unwrap();
            let color = options.colors.waveform[0];
            assert_eq!(
                image.get_pixel(0, 1).0,
                [color.red, color.green, color.blue, color.alpha]
            );
        }
    }

    #[test]
    fn waveform_drawing_preserves_vertical_borders() {
        let source = Waveform::from_interleaved_samples(
            48_000,
            64,
            1,
            vec![-32768, 32767, -32768, 32767, -32768, 32767],
            16,
        )
        .unwrap();
        let options = RenderOptions {
            width: 3,
            height: 40,
            ..Default::default()
        };
        let image = render_waveform(&source, &options).unwrap();
        let border = options.colors.border;
        for x in [0, 2] {
            assert_eq!(
                image.get_pixel(x, 20).0,
                [border.red, border.green, border.blue, border.alpha]
            );
        }
        assert_ne!(image.get_pixel(1, 20), image.get_pixel(0, 20));
    }
}

#[cfg(feature = "wav-output")]
mod wav_output {
    use audiowaveform::{PcmAudio, write_pcm_as_wav, write_pcm_to_wav_path};

    #[test]
    fn unrepresentable_wav_metadata_is_rejected_before_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.wav");
        for pcm in [
            PcmAudio::new(1 << 31, 1, vec![]).unwrap(),
            PcmAudio::new(48_000, 32_768, vec![]).unwrap(),
        ] {
            let mut output = b"keep me".to_vec();
            assert!(write_pcm_as_wav(&pcm, &mut output).is_err());
            assert_eq!(output, b"keep me");
            std::fs::write(&path, b"keep me").unwrap();
            assert!(write_pcm_to_wav_path(&pcm, &path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        }
    }

    #[cfg(feature = "format-wav")]
    #[test]
    fn failed_transcode_preserves_destination_and_same_file_transcode_succeeds() {
        use audiowaveform::{decode_audio_from_path, transcode_audio_path_to_wav_path};
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.wav");
        let output = directory.path().join("output.wav");
        std::fs::write(&input, b"invalid audio").unwrap();
        std::fs::write(&output, b"keep me").unwrap();
        assert!(transcode_audio_path_to_wav_path(&input, &output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"keep me");

        let pcm = PcmAudio::new(48_000, 1, vec![256, -512, 1024]).unwrap();
        write_pcm_as_wav(&pcm, std::fs::File::create(&input).unwrap()).unwrap();
        transcode_audio_path_to_wav_path(&input, &input).unwrap();
        assert_eq!(decode_audio_from_path(&input).unwrap(), pcm);

        let alias = directory.path().join("alias.wav");
        std::fs::hard_link(&input, &alias).unwrap();
        transcode_audio_path_to_wav_path(&input, &alias).unwrap();
        assert_eq!(decode_audio_from_path(&input).unwrap(), pcm);
        #[cfg(unix)]
        {
            let link = directory.path().join("symlink.wav");
            std::os::unix::fs::symlink(&input, &link).unwrap();
            transcode_audio_path_to_wav_path(&input, &link).unwrap();
            assert_eq!(decode_audio_from_path(&input).unwrap(), pcm);
        }
    }
}
