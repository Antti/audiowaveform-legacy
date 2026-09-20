use audiowaveform::{
    AmplitudeScale, GenerateOptions, PcmAudio, ScaleSpec, Waveform, WaveformFormat,
    generate_waveform_from_pcm,
};

fn generate(samples: Vec<i16>, channels: u16, points: u32, split_channels: bool) -> Waveform {
    generate_waveform_from_pcm(
        &PcmAudio::new(48_000, channels, samples).unwrap(),
        &GenerateOptions {
            scale: ScaleSpec::Points(points),
            split_channels,
            amplitude_scale: None,
        },
    )
    .unwrap()
}

#[test]
fn exact_points_partition_all_samples_and_preserve_peaks() {
    // 11 frames into 3 buckets: [0..3], [3..7], [7..11]. Both boundary peaks survive.
    let samples = vec![5, -100, 2, 100, 4, 5, -200, 200, 8, 9, -300];
    let waveform = generate(samples.clone(), 1, 3, false);
    assert_eq!(waveform.len(), 3);
    assert_eq!(waveform.data(16).unwrap(), [-100, 5, -200, 100, -300, 200]);
    assert_eq!(waveform.source_frames(), Some(11));
    assert_eq!(waveform.samples_per_pixel(), 3);
    assert_eq!(waveform.samples_per_point(), 11.0 / 3.0);
    assert_eq!(waveform.duration_seconds(), 11.0 / 48_000.0);
    assert_eq!(
        generate(samples, 1, 1, false).data(16).unwrap(),
        [-300, 200]
    );
}

#[test]
fn target_count_is_exact_for_non_divisible_clip_lengths() {
    let waveform = generate(vec![512; 12_000], 1, 110, false);
    assert_eq!(waveform.len(), 110);
    assert_eq!(waveform.data(8).unwrap(), vec![2; 220]);
    assert_eq!(waveform.duration_seconds(), 0.25);
}

#[test]
fn short_clips_repeat_samples_without_silence_or_losing_the_last_frame() {
    let waveform = generate(vec![-20, 40], 1, 5, false);
    assert_eq!(
        waveform.data(16).unwrap(),
        [-20, -20, -20, -20, -20, -20, 40, 40, 40, 40]
    );
    assert_eq!(waveform.duration_seconds(), 2.0 / 48_000.0);
    assert_eq!(waveform.samples_per_point(), 0.4);
    assert_eq!(
        generate(vec![-10], 1, 110, false).data(16).unwrap(),
        vec![-10; 220]
    );
}

#[test]
fn exact_points_support_mixing_splitting_and_amplitude_scaling() {
    let samples = vec![100, -300, -200, 400, 300, -100];
    let mixed = generate(samples.clone(), 2, 2, false);
    assert_eq!(mixed.data(16).unwrap(), [-100, -100, 100, 100]);
    let split = generate(samples, 2, 2, true);
    assert_eq!(split.channels(), 2);
    assert_eq!(
        split.data(16).unwrap(),
        [100, 100, -300, -300, -200, 300, -100, 400]
    );
    let scaled = split.scale_amplitude(AmplitudeScale::Fixed(2.0)).unwrap();
    assert_eq!(
        scaled.data(16).unwrap(),
        [200, 200, -600, -600, -400, 600, -200, 800]
    );
    assert_eq!(scaled.duration_seconds(), split.duration_seconds());
}

#[test]
fn empty_input_stays_empty_and_zero_points_are_rejected() {
    let empty = generate(Vec::new(), 1, 110, false);
    assert!(empty.is_empty());
    assert_eq!(empty.duration_seconds(), 0.0);
    assert!(empty.data(8).unwrap().is_empty());
    for samples in [vec![], vec![0, 0]] {
        let error = generate_waveform_from_pcm(
            &PcmAudio::new(48_000, 1, samples).unwrap(),
            &GenerateOptions {
                scale: ScaleSpec::Points(0),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Invalid points: must be greater than zero"
        );
    }
}

#[test]
fn json_round_trips_exact_timing_including_sub_sample_buckets_and_empty_audio() {
    for waveform in [
        generate(vec![256; 11], 1, 3, false),
        generate(vec![-256; 2], 1, 5, false),
        generate(vec![], 1, 110, false),
    ] {
        for bits in [8, 16] {
            let mut json = Vec::new();
            waveform
                .write_to_writer(&mut json, WaveformFormat::Json, Some(bits))
                .unwrap();
            let loaded = Waveform::load_from_reader(json.as_slice(), WaveformFormat::Json).unwrap();
            assert_eq!(loaded.source_frames(), waveform.source_frames());
            assert_eq!(loaded.duration_seconds(), waveform.duration_seconds());
            assert_eq!(loaded.interleaved_samples(), waveform.interleaved_samples());
        }
    }
    for (frames, length, spp) in [(0, 1, 2), (10, 0, 2), (10, 1, 2)] {
        let json = format!(
            r#"{{"version":2,"channels":1,"sample_rate":48000,"samples_per_pixel":{spp},"bits":16,"length":{length},"data":{},"source_frames":{frames}}}"#,
            if length == 0 { "[]" } else { "[0,0]" }
        );
        assert!(Waveform::load_from_reader(json.as_bytes(), WaveformFormat::Json).is_err());
    }
}

#[test]
fn dat_only_accepts_exact_points_when_the_timing_is_representable() {
    let waveform = generate(vec![0; 11], 1, 3, false);
    let mut bytes = Vec::new();
    assert!(
        waveform
            .write_to_writer(&mut bytes, WaveformFormat::Dat, None)
            .is_err()
    );
    assert!(bytes.is_empty());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing.dat");
    std::fs::write(&path, b"keep existing output").unwrap();
    assert!(waveform.save_to_path(&path, None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep existing output");
    let waveform = generate(vec![256; 12], 1, 3, false);
    waveform
        .write_to_writer(&mut bytes, WaveformFormat::Dat, None)
        .unwrap();
    let loaded = Waveform::load_from_reader(bytes.as_slice(), WaveformFormat::Dat).unwrap();
    assert_eq!(loaded.duration_seconds(), waveform.duration_seconds());
    assert_eq!(loaded.interleaved_samples(), waveform.interleaved_samples());
}

#[test]
fn fixed_scale_operations_cannot_silently_discard_exact_timing() {
    let mut waveform = generate(vec![0; 11], 1, 3, false);
    assert!(waveform.resample(ScaleSpec::SamplesPerPixel(8)).is_err());
    assert!(
        Waveform::new(48_000, 2, 1)
            .unwrap()
            .resample(ScaleSpec::Points(5))
            .is_err()
    );
    assert!(
        waveform
            .push_frame(&[audiowaveform::WaveformPoint { min: 0, max: 1 }])
            .is_err()
    );
}

#[test]
fn direct_bit_conversion_matches_serialization_for_every_i16_value() {
    let samples: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    let waveform = Waveform::from_interleaved_samples(48_000, 2, 1, samples.clone(), 16).unwrap();
    for bits in [8, 16] {
        let mut json = Vec::new();
        waveform
            .write_to_writer(&mut json, WaveformFormat::Json, Some(bits))
            .unwrap();
        let serialized: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(
            serde_json::to_value(waveform.data(bits).unwrap()).unwrap(),
            serialized["data"]
        );
    }
    assert_eq!(waveform.interleaved_samples(), samples);
    assert_eq!(waveform.storage_bits(), 16);
    assert!(waveform.data(12).is_err());
    let values = Waveform::from_interleaved_samples(
        48_000,
        2,
        1,
        vec![-32768, 32767, -257, 257, -255, 255],
        16,
    )
    .unwrap();
    assert_eq!(values.data(8).unwrap(), [-128, 127, -1, 1, 0, 0]);
}
