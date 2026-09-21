use crate::{AmplitudeScale, Error};

/// Waveform scale selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleSpec {
    /// Generate exactly this many points per channel from the decoded PCM.
    /// Empty input stays empty; when there are fewer frames than points, samples repeat.
    /// This scale is only supported for generation, not waveform resampling.
    Points(u32),
    /// Use a fixed number of source samples per waveform point.
    SamplesPerPixel(u32),
    /// Derive samples per waveform point from a target number of rendered pixels per second.
    PixelsPerSecond(u32),
    /// Fit a duration or full clip into the provided width.
    FitWidth {
        /// Output width in pixels.
        width_pixels: u32,
        /// Optional `(start_time, end_time)` range in seconds.
        time_range: Option<(f64, f64)>,
    },
}

impl ScaleSpec {
    /// Checks scale arguments that do not depend on source metadata.
    fn validate(self) -> Result<(), Error> {
        match self {
            Self::Points(0) => {
                return Err(Error::invalid_argument(
                    "points",
                    "Invalid points: must be greater than zero",
                ));
            }
            Self::SamplesPerPixel(0 | 1) => {
                return Err(Error::invalid_argument("zoom", "Invalid zoom: minimum 2"));
            }
            Self::PixelsPerSecond(0) => {
                return Err(Error::invalid_argument(
                    "pixels per second",
                    "Invalid pixels per second: must be greater than zero",
                ));
            }
            Self::FitWidth {
                width_pixels,
                time_range,
            } => {
                if width_pixels == 0 {
                    return Err(Error::invalid_argument(
                        "image width",
                        "Invalid image width: minimum 1",
                    ));
                }
                if let Some((start, end)) = time_range {
                    if !start.is_finite() || start < 0.0 {
                        return Err(Error::invalid_argument(
                            "start time",
                            "Invalid start time: minimum 0",
                        ));
                    }
                    if !end.is_finite() || end < start {
                        return Err(Error::invalid_argument(
                            "end time",
                            format!("Invalid end time, must be greater than {start}"),
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Resolves the scale to a concrete number of samples per waveform point.
    /// For `Points`, returns a nominal integer scale, rounded down with a minimum of 2.
    pub fn resolve(self, sample_rate: u32, frame_count: usize) -> Result<u32, Error> {
        self.resolve_frames(sample_rate, frame_count as u64)
    }

    pub(crate) fn resolve_frames(self, sample_rate: u32, frame_count: u64) -> Result<u32, Error> {
        self.validate()?;
        let resolved = match self {
            Self::Points(points) => u32::try_from((frame_count / u64::from(points)).max(2))
                .map_err(|_| {
                    Error::invalid_argument("points", "Too many source frames per point")
                })?,
            Self::SamplesPerPixel(value) => value,
            Self::PixelsPerSecond(value) => sample_rate / value,
            Self::FitWidth {
                width_pixels,
                time_range,
            } => {
                let frames = if let Some((start, end)) = time_range {
                    let frames = (end - start) * f64::from(sample_rate);
                    if !frames.is_finite() || frames >= u64::MAX as f64 {
                        return Err(Error::invalid_argument(
                            "time range",
                            "Time range contains too many source frames",
                        ));
                    }
                    frames as u64
                } else {
                    frame_count
                };
                u32::try_from(frames / u64::from(width_pixels)).map_err(|_| {
                    Error::invalid_argument("image width", "Too many source frames per pixel")
                })?
            }
        };

        if resolved < 2 {
            return Err(Error::invalid_argument("zoom", "Invalid zoom: minimum 2"));
        }

        Ok(resolved)
    }
}

/// Waveform generation settings.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerateOptions {
    /// Scale selection.
    pub scale: ScaleSpec,
    /// Whether to keep each source channel separate in the waveform output.
    pub split_channels: bool,
    /// Optional post-generation amplitude scaling.
    pub amplitude_scale: Option<AmplitudeScale>,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            scale: ScaleSpec::SamplesPerPixel(256),
            split_channels: false,
            amplitude_scale: None,
        }
    }
}

impl GenerateOptions {
    // Validate before decoding, counting, or temporary-file I/O. Keep the
    // generation API's end-time diagnostic and reject empty ranges before I/O;
    // direct scale resolution rejects an empty range as a sub-minimum zoom.
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.scale.validate().map_err(|error| match error {
            Error::InvalidArgument {
                name: "end time", ..
            } => invalid_generation_end_time(),
            other => other,
        })?;
        if matches!(self.scale, ScaleSpec::FitWidth { time_range: Some((start, end)), .. } if start == end)
        {
            return Err(invalid_generation_end_time());
        }
        if let Some(scale) = self.amplitude_scale {
            scale.validate()?;
        }
        Ok(())
    }
}

fn invalid_generation_end_time() -> Error {
    Error::invalid_argument(
        "end time",
        "Invalid end time: must be finite and greater than the start time",
    )
}

#[cfg(test)]
mod tests {
    use super::ScaleSpec;

    #[test]
    fn resolves_scale_specifications_and_rejects_invalid_values() {
        assert_eq!(
            ScaleSpec::SamplesPerPixel(64)
                .resolve(48_000, 96_000)
                .expect("samples per pixel"),
            64
        );
        assert_eq!(
            ScaleSpec::PixelsPerSecond(100)
                .resolve(48_000, 96_000)
                .expect("pixels per second"),
            480
        );
        assert_eq!(
            ScaleSpec::FitWidth {
                width_pixels: 400,
                time_range: Some((0.0, 4.0)),
            }
            .resolve(48_000, 0)
            .expect("fit width"),
            480
        );

        let error = ScaleSpec::PixelsPerSecond(0)
            .resolve(48_000, 0)
            .expect_err("invalid pixels per second");
        assert_eq!(
            error.to_string(),
            "Invalid pixels per second: must be greater than zero"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 0,
            time_range: None,
        }
        .resolve(48_000, 96_000)
        .expect_err("invalid width");
        assert_eq!(error.to_string(), "Invalid image width: minimum 1");

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((5.0, 4.0)),
        }
        .resolve(48_000, 96_000)
        .expect_err("invalid range");
        assert_eq!(
            error.to_string(),
            "Invalid end time, must be greater than 5"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((f64::INFINITY, 10.0)),
        }
        .resolve(48_000, 96_000)
        .expect_err("non-finite start time");
        assert_eq!(error.to_string(), "Invalid start time: minimum 0");

        let error = ScaleSpec::FitWidth {
            width_pixels: 400,
            time_range: Some((0.0, f64::INFINITY)),
        }
        .resolve(48_000, 96_000)
        .expect_err("non-finite end time");
        assert_eq!(
            error.to_string(),
            "Invalid end time, must be greater than 0"
        );

        let error = ScaleSpec::FitWidth {
            width_pixels: 100_000,
            time_range: None,
        }
        .resolve(48_000, 96_000)
        .expect_err("zoom too small");
        assert_eq!(error.to_string(), "Invalid zoom: minimum 2");
    }
}
