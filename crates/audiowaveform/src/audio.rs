use crate::{Error, Waveform};

#[cfg(feature = "decode")]
mod decode;
mod options;
mod pcm;
mod peaks;
#[cfg(feature = "decode")]
mod probe;
mod raw;

#[cfg(all(feature = "decode", feature = "wav-output"))]
pub(crate) use decode::decode_audio_reader;
#[cfg(feature = "decode")]
pub use decode::{
    decode_audio_from_path, decode_audio_from_reader, generate_waveform_from_path,
    generate_waveform_from_reader,
};
pub use options::{GenerateOptions, ScaleSpec};
pub use pcm::PcmAudio;
pub use raw::{
    RawAudioConfig, RawSampleFormat, decode_raw_audio_reader, generate_waveform_from_raw_reader,
};

use peaks::PeakAccumulator;

/// Generates a waveform from in-memory PCM samples.
pub fn generate_waveform_from_pcm(
    pcm: &PcmAudio,
    options: &GenerateOptions,
) -> Result<Waveform, Error> {
    options.validate()?;
    let mut peaks = PeakAccumulator::new(
        pcm.sample_rate(),
        pcm.channels(),
        pcm.frame_count(),
        options,
    )?;
    peaks.push(pcm.samples())?;
    peaks.finish(options)
}

#[cfg(test)]
mod tests {
    use super::{GenerateOptions, PcmAudio, ScaleSpec, generate_waveform_from_pcm};
    use crate::{AmplitudeScale, WaveformPoint};

    #[test]
    fn generates_waveforms_from_pcm_for_mixed_and_split_channels() {
        let pcm = PcmAudio::new(48_000, 2, vec![100, 300, 200, 400, -100, -300, -200, -400])
            .expect("pcm");

        let mixed = generate_waveform_from_pcm(
            &pcm,
            &GenerateOptions {
                scale: ScaleSpec::SamplesPerPixel(2),
                split_channels: false,
                amplitude_scale: None,
            },
        )
        .expect("mixed waveform");
        assert_eq!(mixed.channels(), 1);
        assert_eq!(
            mixed.point(0, 0).expect("first point"),
            WaveformPoint { min: 200, max: 300 }
        );
        assert_eq!(
            mixed.point(0, 1).expect("second point"),
            WaveformPoint {
                min: -300,
                max: -200,
            }
        );

        let split = generate_waveform_from_pcm(
            &pcm,
            &GenerateOptions {
                scale: ScaleSpec::SamplesPerPixel(2),
                split_channels: true,
                amplitude_scale: Some(AmplitudeScale::Fixed(2.0)),
            },
        )
        .expect("split waveform");
        assert_eq!(split.channels(), 2);
        assert_eq!(
            split.point(0, 0).expect("left point"),
            WaveformPoint { min: 200, max: 400 }
        );
        assert_eq!(
            split.point(1, 1).expect("right point"),
            WaveformPoint {
                min: -800,
                max: -600,
            }
        );
    }
}
