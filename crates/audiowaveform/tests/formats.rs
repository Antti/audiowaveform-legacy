#![cfg(feature = "decode")]

mod support;

use std::io::Cursor;

use audiowaveform::{
    AudioFormat, Error, GenerateOptions, decode_audio_from_path, generate_waveform_from_path,
    generate_waveform_from_reader,
};
use support::fixture_path;
#[cfg(feature = "format-m4a")]
use support::read_fixture;

#[test]
fn format_features_control_decoding() {
    for (fixture, enabled, feature, sample_rate, channels) in [
        (
            "formats/stereo.aac",
            cfg!(feature = "format-aac"),
            "format-aac",
            48_000,
            2,
        ),
        (
            "formats/stereo.m4a",
            cfg!(feature = "format-m4a"),
            "format-m4a",
            48_000,
            2,
        ),
        (
            "formats/mono.m4a",
            cfg!(feature = "format-m4a"),
            "format-m4a",
            44_100,
            1,
        ),
        (
            "formats/alac.m4a",
            cfg!(feature = "format-m4a"),
            "format-m4a",
            48_000,
            2,
        ),
        (
            "formats/fragmented.mp4",
            cfg!(feature = "format-m4a"),
            "format-m4a",
            48_000,
            2,
        ),
        (
            "formats/video-first.mp4",
            cfg!(feature = "format-m4a"),
            "format-m4a",
            48_000,
            2,
        ),
        (
            "formats/stereo.aiff",
            cfg!(feature = "format-aiff"),
            "format-aiff",
            48_000,
            2,
        ),
        (
            "formats/stereo.caf",
            cfg!(feature = "format-caf"),
            "format-caf",
            48_000,
            2,
        ),
        (
            "formats/pcm.caf",
            cfg!(feature = "format-caf"),
            "format-caf",
            48_000,
            2,
        ),
        (
            "formats/stereo.webm",
            cfg!(feature = "format-mkv"),
            "format-mkv",
            48_000,
            2,
        ),
        (
            "formats/stereo.mka",
            cfg!(feature = "format-mkv"),
            "format-mkv",
            48_000,
            2,
        ),
        (
            "formats/stereo.mp2",
            cfg!(feature = "format-mp2"),
            "format-mp2",
            48_000,
            2,
        ),
        (
            "formats/silence.mp1",
            cfg!(feature = "format-mp1"),
            "format-mp1",
            44_100,
            1,
        ),
        (
            "test_file_stereo.mp3",
            cfg!(feature = "format-mp3"),
            "format-mp3",
            16_000,
            2,
        ),
        (
            "test_file_stereo.flac",
            cfg!(feature = "format-flac"),
            "format-flac",
            16_000,
            2,
        ),
        (
            "formats/flac.ogg",
            cfg!(feature = "format-ogg"),
            "format-ogg",
            48_000,
            2,
        ),
        (
            "test_file_stereo.oga",
            cfg!(feature = "format-ogg"),
            "format-ogg",
            16_000,
            2,
        ),
        (
            "formats/stereo.wav",
            cfg!(feature = "format-wav"),
            "format-wav",
            48_000,
            2,
        ),
        (
            "formats/adpcm.wav",
            cfg!(feature = "format-wav"),
            "format-wav",
            48_000,
            2,
        ),
    ] {
        let result = decode_audio_from_path(fixture_path(fixture));
        if !enabled {
            assert!(
                matches!(result, Err(Error::FeatureDisabled { feature: actual }) if actual == feature),
                "{fixture}: {result:?}"
            );
            continue;
        }
        let pcm = result.unwrap_or_else(|error| panic!("{fixture}: {error}"));
        assert_eq!(pcm.sample_rate(), sample_rate, "{fixture}");
        assert_eq!(pcm.channels(), channels, "{fixture}");
        assert!(pcm.frame_count() > 0, "{fixture}");
        if fixture.starts_with("formats/") {
            assert!(
                (pcm.duration_seconds() - 0.25).abs() < 0.1,
                "{fixture}: {}",
                pcm.duration_seconds()
            );
        }
        if !fixture.ends_with("silence.mp1") {
            assert!(pcm.samples().iter().any(|sample| *sample != 0), "{fixture}");
        }
        let waveform =
            generate_waveform_from_path(fixture_path(fixture), &GenerateOptions::default())
                .expect("generate");
        assert!(!waveform.is_empty(), "{fixture}");
    }
}

#[cfg(feature = "format-m4a")]
#[test]
fn m4a_decoding_selects_audio_and_supports_probing_without_a_filename() {
    let options = GenerateOptions::default();
    let audio = generate_waveform_from_path(fixture_path("formats/stereo.m4a"), &options).unwrap();
    let video =
        generate_waveform_from_path(fixture_path("formats/video-first.mp4"), &options).unwrap();
    assert_eq!(audio, video);
    let probed = generate_waveform_from_reader(
        Cursor::new(read_fixture("formats/stereo.m4a")),
        None,
        &options,
    )
    .unwrap();
    assert_eq!(audio, probed);
    assert!(
        !generate_waveform_from_path(fixture_path("formats/short.m4a"), &options)
            .unwrap()
            .is_empty()
    );
    let error = decode_audio_from_path(fixture_path("formats/video-only.mp4")).unwrap_err();
    assert!(error.to_string().contains("no supported audio track"));
}

#[test]
fn corrupt_enabled_formats_return_errors() {
    for format in [
        AudioFormat::Aac,
        AudioFormat::Mp4,
        AudioFormat::Wav,
        AudioFormat::Mp3,
        AudioFormat::Mkv,
    ] {
        if format.ensure_enabled().is_ok() {
            let result = generate_waveform_from_reader(
                Cursor::new(b"not an audio file".to_vec()),
                Some(format),
                &GenerateOptions::default(),
            );
            assert!(result.is_err(), "{format:?}");
        }
    }
}
