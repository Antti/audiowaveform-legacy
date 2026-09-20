use crate::{Error, GenerateOptions, ScaleSpec, Waveform};

use super::flush_frame;

pub(super) fn needs_frame_count(scale: ScaleSpec) -> bool {
    matches!(
        scale,
        ScaleSpec::Points(_)
            | ScaleSpec::FitWidth {
                time_range: None,
                ..
            }
    )
}

/// Retains only the current bucket's extrema and the completed waveform points.
pub(super) struct PeakAccumulator {
    waveform: Waveform,
    channels: usize,
    mins: Vec<i16>,
    maxs: Vec<i16>,
    frames_seen: usize,
    bucket_end: usize,
    pending: bool,
    exact: Option<(usize, u32)>,
}

impl PeakAccumulator {
    pub(super) fn new(
        sample_rate: u32,
        channels: u16,
        frames: usize,
        options: &GenerateOptions,
    ) -> Result<Self, Error> {
        if channels == 0 {
            return Err(Error::invalid_data("Invalid number of input channels"));
        }
        let scale = options.scale.resolve(sample_rate, frames)?;
        let output_channels = if options.split_channels { channels } else { 1 };
        let waveform = Waveform::new(sample_rate, scale, output_channels)?;
        let exact = match options.scale {
            ScaleSpec::Points(points) => Some((frames, points)),
            _ => None,
        };
        Ok(Self {
            waveform,
            channels: usize::from(channels),
            mins: vec![i16::MAX; usize::from(output_channels)],
            maxs: vec![i16::MIN; usize::from(output_channels)],
            frames_seen: 0,
            bucket_end: exact.map_or(scale as usize, |(frames, points)| {
                (frames / points as usize).max(1)
            }),
            pending: false,
            exact,
        })
    }

    pub(super) fn push(&mut self, samples: &[i16]) -> Result<(), Error> {
        if !samples.len().is_multiple_of(self.channels) {
            return Err(Error::invalid_data("Incomplete interleaved audio frame"));
        }
        for frame in samples.chunks_exact(self.channels) {
            if self
                .exact
                .is_some_and(|(frames, _)| self.frames_seen >= frames)
            {
                return Err(Error::invalid_data(
                    "Audio frame count changed between decoding passes",
                ));
            }
            // A very short clip can place the same source frame in several buckets.
            loop {
                if self.waveform.channels() == 1 {
                    let sample = (frame.iter().map(|&value| i64::from(value)).sum::<i64>()
                        / self.channels as i64) as i16;
                    self.mins[0] = self.mins[0].min(sample);
                    self.maxs[0] = self.maxs[0].max(sample);
                } else {
                    for (channel, &sample) in frame.iter().enumerate() {
                        self.mins[channel] = self.mins[channel].min(sample);
                        self.maxs[channel] = self.maxs[channel].max(sample);
                    }
                }
                self.pending = true;
                if self.frames_seen + 1 < self.bucket_end {
                    break;
                }
                self.flush()?;
                if let Some((frames, points)) = self.exact {
                    let index = self.waveform.len();
                    if index == points as usize {
                        break;
                    }
                    let start = (index as u128 * frames as u128 / u128::from(points)) as usize;
                    self.bucket_end = (((index as u128 + 1) * frames as u128 / u128::from(points))
                        as usize)
                        .max(start + 1);
                    if start == self.frames_seen {
                        continue;
                    }
                } else {
                    self.bucket_end = self
                        .bucket_end
                        .saturating_add(self.waveform.samples_per_pixel() as usize);
                }
                break;
            }
            self.frames_seen = self
                .frames_seen
                .checked_add(1)
                .ok_or_else(|| Error::invalid_data("Audio frame count is too large"))?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        flush_frame(&mut self.waveform, &self.mins, &self.maxs)?;
        self.mins.fill(i16::MAX);
        self.maxs.fill(i16::MIN);
        self.pending = false;
        Ok(())
    }

    pub(super) fn finish(mut self, options: &GenerateOptions) -> Result<Waveform, Error> {
        if let Some((frames, _)) = self.exact {
            if self.frames_seen != frames {
                return Err(Error::invalid_data(
                    "Audio frame count changed between decoding passes",
                ));
            }
            self.waveform.set_source_frames(frames as u64)?;
        } else if self.pending {
            self.flush()?;
        }
        match options.amplitude_scale {
            Some(scale) => self.waveform.scale_amplitude(scale),
            None => Ok(self.waveform),
        }
    }
}
