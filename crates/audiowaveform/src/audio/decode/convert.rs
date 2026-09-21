use symphonia::core::audio::{AudioBuffer, AudioBufferRef, SampleBuffer, Signal};
use symphonia::core::sample::Sample;

use crate::audio::pcm::clamp_float_to_i16;

/// Packet conversion storage reused for the lifetime of a decoding pass.
#[derive(Default)]
pub(super) struct SampleConverter {
    floats: Vec<i16>,
    integers: Option<SampleBuffer<i16>>,
}

impl SampleConverter {
    pub(super) fn interleaved(&mut self, decoded: AudioBufferRef<'_>) -> &[i16] {
        self.floats.clear();
        match decoded {
            AudioBufferRef::F32(buffer) => {
                extend_float_samples(buffer.as_ref(), &mut self.floats);
                &self.floats
            }
            AudioBufferRef::F64(buffer) => {
                extend_float_samples(buffer.as_ref(), &mut self.floats);
                &self.floats
            }
            _ => {
                let spec = *decoded.spec();
                let required = decoded.capacity() * spec.channels.count();
                if self
                    .integers
                    .as_ref()
                    .is_none_or(|buffer| buffer.capacity() < required)
                {
                    self.integers = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
                }
                let buffer = self
                    .integers
                    .as_mut()
                    .expect("conversion buffer initialized");
                buffer.copy_interleaved_ref(decoded);
                buffer.samples()
            }
        }
    }
}

fn extend_float_samples<S: Sample + Into<f64>>(buffer: &AudioBuffer<S>, samples: &mut Vec<i16>) {
    let channels = buffer.spec().channels.count();
    for frame in 0..buffer.frames() {
        for channel in 0..channels {
            // Keep the established float quantization rather than the integer
            // SampleBuffer conversion, including truncation toward zero.
            let sample: f64 = buffer.chan(channel)[frame].into();
            samples.push(clamp_float_to_i16(sample * f64::from(i16::MAX)));
        }
    }
}
