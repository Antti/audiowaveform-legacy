use crate::Error;

/// A decoded interleaved PCM audio buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct PcmAudio {
    sample_rate: u32,
    channels: u16,
    pub(super) channel_mask: Option<u32>,
    samples: Vec<i16>,
}

impl PcmAudio {
    /// Creates a PCM buffer from interleaved 16-bit samples.
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<i16>) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::invalid_argument(
                "sample rate",
                "Invalid input sample rate: must be greater than zero",
            ));
        }
        if channels == 0 {
            return Err(Error::invalid_argument(
                "channels",
                "Invalid number of input channels: must be greater than zero",
            ));
        }
        if !samples.len().is_multiple_of(usize::from(channels)) {
            return Err(Error::invalid_argument(
                "samples",
                "Interleaved PCM sample count must be divisible by the channel count",
            ));
        }
        Ok(Self {
            sample_rate,
            channels,
            channel_mask: (channels <= 18).then(|| (1_u32 << channels) - 1),
            samples,
        })
    }

    /// Returns the source sample rate in Hz.
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the channel count.
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Returns the speaker mask in WAV bit order, when the layout is known.
    /// Constructed PCM uses the lowest `channels` speaker bits for up to 18 channels.
    pub const fn channel_mask(&self) -> Option<u32> {
        self.channel_mask
    }

    /// Sets a WAV speaker layout without changing the interleaved sample order.
    pub fn with_channel_mask(mut self, mask: u32) -> Result<Self, Error> {
        if mask & !0x3ffff != 0 || mask.count_ones() != u32::from(self.channels) {
            return Err(Error::invalid_argument(
                "channel mask",
                "Speaker mask must contain one supported WAV position per channel",
            ));
        }
        self.channel_mask = Some(mask);
        Ok(self)
    }

    /// Returns the interleaved PCM samples.
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    /// Returns the number of audio frames.
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Returns the duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        self.frame_count() as f64 / self.sample_rate as f64
    }
}
pub(super) fn clamp_float_to_i16(value: f64) -> i16 {
    value.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::PcmAudio;

    #[test]
    fn validates_pcm_audio_construction() {
        let pcm = PcmAudio::new(48_000, 2, vec![1, 2, 3, 4]).expect("pcm");
        assert_eq!(pcm.frame_count(), 2);
        assert_eq!(pcm.duration_seconds(), 2.0 / 48_000.0);

        let error = PcmAudio::new(0, 1, vec![1]).expect_err("invalid sample rate");
        assert_eq!(
            error.to_string(),
            "Invalid input sample rate: must be greater than zero"
        );

        let error = PcmAudio::new(48_000, 0, vec![1]).expect_err("invalid channels");
        assert_eq!(
            error.to_string(),
            "Invalid number of input channels: must be greater than zero"
        );

        let error = PcmAudio::new(48_000, 2, vec![1, 2, 3]).expect_err("unaligned samples");
        assert_eq!(
            error.to_string(),
            "Interleaved PCM sample count must be divisible by the channel count"
        );
    }
}
