use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, AudioBuffer, GenericAudioBufferRef};

use crate::audio::pcm::clamp_float_to_i16;

/// Packet conversion storage reused for the lifetime of a decoding pass.
#[derive(Default)]
pub(super) struct SampleConverter {
    samples: Vec<i16>,
}

impl SampleConverter {
    pub(super) fn interleaved(&mut self, decoded: GenericAudioBufferRef<'_>) -> &[i16] {
        self.samples.clear();
        match decoded {
            GenericAudioBufferRef::F32(buffer) => {
                extend_float_samples(buffer, &mut self.samples);
            }
            GenericAudioBufferRef::F64(buffer) => {
                extend_float_samples(buffer, &mut self.samples);
            }
            _ => {
                decoded.copy_to_vec_interleaved(&mut self.samples);
            }
        }
        &self.samples
    }
}

fn extend_float_samples<S: Sample + Into<f64>>(buffer: &AudioBuffer<S>, samples: &mut Vec<i16>) {
    let channels = buffer.num_planes();
    for frame in 0..buffer.frames() {
        for channel in 0..channels {
            // Keep the established float quantization rather than the integer
            // sample conversion, including truncation toward zero.
            let sample: f64 = buffer.plane(channel).expect("valid channel")[frame].into();
            samples.push(clamp_float_to_i16(sample * f64::from(i16::MAX)));
        }
    }
}
