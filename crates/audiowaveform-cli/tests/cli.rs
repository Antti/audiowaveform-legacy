mod support;

use assert_cmd::Command;
#[cfg(feature = "format-wav")]
use audiowaveform::Waveform;
use predicates::prelude::*;

#[cfg(all(feature = "format-mp3", feature = "wav-output"))]
use self::support::assert_wav_file_matches_fixture;
#[cfg(any(
    feature = "format-wav",
    feature = "format-m4a",
    feature = "render",
    all(feature = "format-mp3", feature = "wav-output")
))]
use self::support::fixture_path;
#[cfg(any(
    feature = "format-wav",
    feature = "render",
    all(feature = "format-mp3", feature = "wav-output")
))]
use self::support::named_temp_file;
use self::support::read_fixture;
#[cfg(feature = "render")]
use self::support::{assert_png_bytes_match_fixture, assert_png_file_matches_fixture};

const EXACT_POINT_JSON: &str = r#"{"version":2,"channels":1,"sample_rate":48000,"samples_per_pixel":3,"bits":16,"length":3,"data":[-100,5,-200,100,-300,200],"source_frames":11}"#;

#[cfg(feature = "render")]
#[test]
fn invalid_render_coordinates_preserve_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.png");
    std::fs::write(&output, b"keep existing output").unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "--input-format", "json", "--start", "1e308", "-o"])
        .arg(&output)
        .write_stdin(EXACT_POINT_JSON)
        .assert()
        .failure()
        .stderr(predicate::str::contains("coordinate limit"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing output");
}

#[cfg(feature = "wav-output")]
#[test]
fn invalid_wav_header_metadata_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.wav");
    std::fs::write(&output, b"keep existing output").unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "raw",
            "--raw-format",
            "s16le",
            "--raw-samplerate",
            "2147483647",
            "--raw-channels",
            "2",
            "-o",
        ])
        .arg(&output)
        .write_stdin([0_u8; 4])
        .assert()
        .failure()
        .stderr(predicate::str::contains("WAV byte rate limit"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing output");
}

#[cfg(all(feature = "format-wav", feature = "wav-output"))]
#[test]
fn failed_wav_decode_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.wav");
    std::fs::write(&output, b"keep existing output").unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "--input-format", "wav", "-o"])
        .arg(&output)
        .write_stdin("invalid audio")
        .assert()
        .failure();
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing output");
}

#[test]
fn rejects_wave64_without_touching_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.dat");
    std::fs::write(&output, b"keep existing output").unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "-i", "unsupported.w64", "-o"])
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unsupported format: w64"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing output");

    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["--input-format", "w64", "--output-format", "dat"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'w64'"));
}

#[test]
fn rejects_unrepresentable_exact_point_dat_without_truncating_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("existing.dat");
    std::fs::write(&output, b"keep existing output").unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "--input-format", "json", "-o"])
        .arg(&output)
        .write_stdin(EXACT_POINT_JSON)
        .assert()
        .failure()
        .stderr(predicate::str::contains("DAT cannot represent"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep existing output");

    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "--input-format", "json", "--output-format", "dat"])
        .write_stdin(EXACT_POINT_JSON)
        .assert()
        .failure()
        .stdout("");
}

#[cfg(feature = "render")]
#[test]
fn renders_exact_point_json_without_requesting_resampling() {
    let output = Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "json",
            "--output-format",
            "png",
            "-w",
            "3",
            "-h",
            "40",
            "--no-axis-labels",
        ])
        .write_stdin(EXACT_POINT_JSON)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let image = image::load_from_memory(&output).unwrap();
    assert_eq!((image.width(), image.height()), (3, 40));
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "json",
            "--output-format",
            "png",
            "--zoom",
            "256",
        ])
        .write_stdin(EXACT_POINT_JSON)
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "Exact point counts require generation from audio",
        ));
}

#[test]
fn prints_help_and_version() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: audiowaveform [OPTIONS]"))
        .stdout(predicate::str::contains(
            "Generate waveform data and images from audio",
        ));

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("AudioWaveform v"));
}

#[test]
fn accepts_spaced_default_compression_level() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args(["--compression", "-1", "--help"])
        .assert()
        .success();
}

#[test]
fn requires_input_and_output_configuration() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .assert()
        .failure()
        .stderr("Error: Must specify either input filename or input format\n");

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args(["--input-format", "wav"])
        .assert()
        .failure()
        .stderr("Error: Must specify either output filename or output format\n");
}

#[test]
fn rejects_invalid_enum_values_via_clap() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args(["--colors", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'test'"))
        .stderr(predicate::str::contains("possible values"));
}

#[cfg(feature = "render")]
#[test]
fn rejects_non_finite_numeric_values() {
    let input = fixture_path("test_file_stereo_8bit_64spp_wav.dat");
    let input = input.to_str().expect("utf8");

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-q",
            "-i",
            input,
            "--output-format",
            "png",
            "-z",
            "64",
            "--start",
            "inf",
        ])
        .assert()
        .failure()
        .stderr("Invalid start time: minimum 0\n");

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-q",
            "-i",
            input,
            "--output-format",
            "png",
            "-z",
            "64",
            "--amplitude-scale",
            "NaN",
        ])
        .assert()
        .failure()
        .stderr("Error: Invalid amplitude scale: must be a positive number\n");
}

#[test]
fn rejects_raw_channel_counts_that_do_not_fit_the_library_type() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-q",
            "--input-format",
            "raw",
            "--output-format",
            "wav",
            "--raw-samplerate",
            "48000",
            "--raw-channels",
            "65537",
            "--raw-format",
            "s16le",
        ])
        .assert()
        .failure()
        .stderr("Invalid number of input channels: maximum 65535\n");
}

#[cfg(feature = "format-wav")]
#[test]
fn generates_dat_output_to_file_and_stdout() {
    let output = named_temp_file(".dat");
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_stereo.wav").to_str().expect("utf8"),
            "-o",
            output.path().to_str().expect("utf8"),
            "-b",
            "8",
            "-z",
            "64",
        ])
        .assert()
        .success()
        .stderr("Done\n");
    assert_eq!(
        std::fs::read(output.path()).expect("read output"),
        read_fixture("test_file_stereo_8bit_64spp_wav.dat")
    );

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "--input-format",
            "wav",
            "--output-format",
            "dat",
            "-b",
            "8",
            "-z",
            "64",
        ])
        .write_stdin(read_fixture("test_file_stereo.wav"))
        .assert()
        .success()
        .stdout(read_fixture("test_file_stereo_8bit_64spp_wav.dat"))
        .stderr("Done\n");
}

#[cfg(feature = "format-wav")]
#[test]
fn generates_json_and_text_outputs_to_stdout() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "--input-format",
            "wav",
            "--output-format",
            "json",
            "-b",
            "8",
            "-z",
            "64",
        ])
        .write_stdin(read_fixture("test_file_stereo.wav"))
        .assert()
        .success()
        .stdout(read_fixture("test_file_stereo_8bit_64spp_wav.json"))
        .stderr("Done\n");

    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_stereo_8bit_64spp_wav.dat")
                .to_str()
                .expect("utf8"),
            "--output-format",
            "txt",
        ])
        .assert()
        .success()
        .stdout(read_fixture("test_file_stereo_8bit_64spp_wav.txt"))
        .stderr("Done\n");
}

#[cfg(feature = "format-wav")]
#[test]
fn applies_fixed_amplitude_scaling_to_waveform_data_output() {
    let unscaled_output = named_temp_file(".json");
    let scaled_output = named_temp_file(".json");

    for (output, amplitude_scale) in [(&unscaled_output, "1.0"), (&scaled_output, "2.0")] {
        Command::cargo_bin("audiowaveform")
            .expect("binary")
            .args([
                "-q",
                "-i",
                fixture_path("test_file_stereo.wav").to_str().expect("utf8"),
                "-o",
                output.path().to_str().expect("utf8"),
                "-z",
                "64",
                "--amplitude-scale",
                amplitude_scale,
            ])
            .assert()
            .success();
    }

    let unscaled = Waveform::load_from_path(unscaled_output.path(), None).expect("unscaled");
    let scaled = Waveform::load_from_path(scaled_output.path(), None).expect("scaled");
    let expected = unscaled
        .interleaved_samples()
        .iter()
        .map(|value| (i32::from(*value) * 2).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
        .collect::<Vec<_>>();

    assert_eq!(scaled.interleaved_samples(), expected);
}

#[cfg(feature = "render")]
#[test]
fn generates_png_output_to_file_and_stdout() {
    let output = named_temp_file(".png");
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_stereo_8bit_64spp_wav.dat")
                .to_str()
                .expect("utf8"),
            "-o",
            output.path().to_str().expect("utf8"),
            "-z",
            "128",
        ])
        .assert()
        .success()
        .stderr("Done\n");
    assert_png_file_matches_fixture(output.path(), "test_file_stereo_dat_128spp.png");

    let output = Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_stereo_8bit_64spp_wav.dat")
                .to_str()
                .expect("utf8"),
            "--output-format",
            "png",
            "-z",
            "128",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_png_bytes_match_fixture(&output, "test_file_stereo_dat_128spp.png");
}

#[cfg(all(feature = "format-mp3", feature = "wav-output"))]
#[test]
fn transcodes_audio_to_wav_output() {
    let output = named_temp_file(".wav");
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_mono.mp3").to_str().expect("utf8"),
            "-o",
            output.path().to_str().expect("utf8"),
            "--output-format",
            "wav",
        ])
        .assert()
        .success()
        .stderr("Done\n");

    assert_wav_file_matches_fixture(output.path(), "test_file_mono_converted.wav", 1);
}

#[cfg(feature = "format-wav")]
#[test]
fn quiet_mode_suppresses_done_output() {
    let output = named_temp_file(".dat");
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-q",
            "-i",
            fixture_path("test_file_stereo.wav").to_str().expect("utf8"),
            "-o",
            output.path().to_str().expect("utf8"),
            "-b",
            "8",
            "-z",
            "64",
        ])
        .assert()
        .success()
        .stderr("");
}

#[cfg(feature = "format-wav")]
#[test]
fn rejects_unsupported_output_combinations() {
    Command::cargo_bin("audiowaveform")
        .expect("binary")
        .args([
            "-i",
            fixture_path("test_file_stereo.wav").to_str().expect("utf8"),
            "--output-format",
            "mp3",
        ])
        .assert()
        .failure()
        .stderr("Can't generate mp3 format output from wav format input\n");
}

#[test]
fn converts_waveform_data_and_raw_pcm_without_optional_features() {
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(["-q", "--input-format", "dat", "--output-format", "txt"])
        .write_stdin(read_fixture("test_file_stereo_8bit_64spp_wav.dat"))
        .assert()
        .success()
        .stdout(read_fixture("test_file_stereo_8bit_64spp_wav.txt"));

    let output = Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "raw",
            "--raw-samplerate",
            "16000",
            "--raw-channels",
            "1",
            "--raw-format",
            "s16le",
            "--output-format",
            "dat",
            "-b",
            "8",
            "-z",
            "64",
        ])
        .write_stdin(read_fixture("test_file_mono.raw"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let waveform = audiowaveform::Waveform::load_from_reader(
        std::io::Cursor::new(output),
        audiowaveform::WaveformFormat::Dat,
    )
    .unwrap();
    assert_eq!(waveform.sample_rate(), 16_000);
    assert_eq!(waveform.channels(), 1);
    assert_eq!(waveform.storage_bits(), 8);
    assert_eq!(
        waveform.len(),
        read_fixture("test_file_mono.raw").len().div_ceil(2 * 64)
    );
}

#[test]
fn reports_disabled_formats_before_creating_output_files() {
    for (format, enabled, feature) in [
        ("mp3", cfg!(feature = "format-mp3"), "format-mp3"),
        ("m4a", cfg!(feature = "format-m4a"), "format-m4a"),
        ("wav", cfg!(feature = "format-wav"), "format-wav"),
        ("webm", cfg!(feature = "format-mkv"), "format-mkv"),
    ] {
        if enabled {
            continue;
        }
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output.json");
        Command::cargo_bin("audiowaveform")
            .unwrap()
            .args([
                "-q",
                "--input-format",
                format,
                "-o",
                output.to_str().unwrap(),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "enable the `{feature}` Cargo feature"
            )));
        assert!(!output.exists());
    }
}

#[test]
fn reports_disabled_output_features() {
    for (format, enabled, feature) in [
        ("png", cfg!(feature = "render"), "render"),
        ("wav", cfg!(feature = "wav-output"), "wav-output"),
    ] {
        if enabled {
            continue;
        }
        Command::cargo_bin("audiowaveform")
            .unwrap()
            .args(["--input-format", "dat", "--output-format", format])
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "enable the `{feature}` Cargo feature"
            )));
    }
}

#[cfg(feature = "format-m4a")]
#[test]
fn generates_waveforms_from_m4a_paths_and_mp4_stdin() {
    for fixture in [
        "formats/stereo.m4a",
        "formats/alac.m4a",
        "formats/video-first.mp4",
    ] {
        let path_output = Command::cargo_bin("audiowaveform")
            .unwrap()
            .args([
                "-q",
                "-i",
                fixture_path(fixture).to_str().unwrap(),
                "--output-format",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(String::from_utf8_lossy(&path_output).contains("48000"));
        Command::cargo_bin("audiowaveform")
            .unwrap()
            .args(["-q", "--input-format", "m4a", "--output-format", "json"])
            .write_stdin(read_fixture(fixture))
            .assert()
            .success()
            .stdout(path_output);
    }
}

#[cfg(all(unix, feature = "format-m4a"))]
#[test]
fn reads_encoded_pipes_passed_as_filenames() {
    let bytes = read_fixture("formats/stereo.m4a");
    let options = ["-q", "--input-format", "m4a", "--output-format", "json"];
    let expected = Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(options)
        .write_stdin(bytes.clone())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    // /dev/stdin is an opened pipe here, exercising the same path as a FIFO
    // or shell process substitution without an external mkfifo dependency.
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args(options)
        .args(["-i", "/dev/stdin"])
        .write_stdin(bytes)
        .assert()
        .success()
        .stdout(expected);
}

#[test]
fn waveform_conversion_and_resampling_apply_amplitude_for_every_output_format() {
    let input = r#"{"version":2,"channels":1,"sample_rate":48000,"samples_per_pixel":2,"bits":16,"length":2,"data":[-100,100,-200,200]}"#;
    for format in ["dat", "json", "txt"] {
        for resample in [false, true] {
            for amplitude in ["2", "auto"] {
                let mut command = Command::cargo_bin("audiowaveform").unwrap();
                command.args([
                    "-q",
                    "--input-format",
                    "json",
                    "--output-format",
                    format,
                    "--amplitude-scale",
                    amplitude,
                ]);
                if resample {
                    command.args(["--zoom", "4"]);
                }
                let output = command
                    .write_stdin(input)
                    .assert()
                    .success()
                    .get_output()
                    .stdout
                    .clone();
                let expected = match (resample, amplitude) {
                    (false, "2") => vec![-200, 200, -400, 400],
                    (false, _) => vec![-16383, 16383, -32767, 32767],
                    (true, "2") => vec![-400, 400],
                    (true, _) => vec![-32767, 32767],
                };
                if format == "txt" {
                    let values: Vec<i16> = std::str::from_utf8(&output)
                        .unwrap()
                        .split([',', '\n'])
                        .filter(|s| !s.is_empty())
                        .map(|s| s.parse().unwrap())
                        .collect();
                    assert_eq!(values, expected);
                } else {
                    let wave = audiowaveform::Waveform::load_from_reader(
                        output.as_slice(),
                        format.parse().unwrap(),
                    )
                    .unwrap();
                    assert_eq!(wave.interleaved_samples(), &expected);
                }
            }
        }
    }
}

#[test]
fn raw_audio_can_be_written_as_text_peaks() {
    let input: Vec<_> = [-100_i16, 100, -200, 200]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "raw",
            "--raw-format",
            "s16le",
            "--raw-samplerate",
            "48000",
            "--raw-channels",
            "1",
            "--zoom",
            "2",
            "--output-format",
            "txt",
        ])
        .write_stdin(input)
        .assert()
        .success()
        .stdout("-100,100\n-200,200\n");
}

#[cfg(feature = "format-wav")]
#[test]
fn encoded_audio_can_be_written_as_text_peaks() {
    let mut input = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut input,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for sample in [-100_i16, 100, -200, 200] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    Command::cargo_bin("audiowaveform")
        .unwrap()
        .args([
            "-q",
            "--input-format",
            "wav",
            "--zoom",
            "2",
            "--output-format",
            "txt",
        ])
        .write_stdin(input.into_inner())
        .assert()
        .success()
        .stdout("-100,100\n-200,200\n");
}
