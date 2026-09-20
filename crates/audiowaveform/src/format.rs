use std::path::Path;
use std::str::FromStr;

use crate::Error;

/// Supported audio container or source formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioFormat {
    /// AAC audio in an ADTS stream.
    Aac,
    /// AIFF or AIFF-C audio.
    Aiff,
    /// Core Audio Format audio.
    Caf,
    /// Audio in an ISO MP4 container, including M4A.
    Mp4,
    /// Audio in a Matroska or WebM container.
    Mkv,
    /// MPEG layer I audio.
    Mp1,
    /// MPEG layer II audio.
    Mp2,
    /// MP3 audio.
    Mp3,
    /// WAV audio.
    Wav,
    /// FLAC audio.
    Flac,
    /// Ogg Vorbis or FLAC audio.
    Ogg,
    /// Opus audio (recognized, but decoding is not supported).
    Opus,
    /// Headerless raw PCM or floating-point audio.
    Raw,
}

impl AudioFormat {
    /// Returns the canonical lowercase name for the format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Aiff => "aiff",
            Self::Caf => "caf",
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Mp1 => "mp1",
            Self::Mp2 => "mp2",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::Raw => "raw",
        }
    }

    /// Infers an audio format from a filesystem path extension.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let extension = path.as_ref().extension()?.to_str()?;
        Self::from_extension(extension)
    }

    /// Infers an audio format from a file extension string.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "aac" | "adts" => Some(Self::Aac),
            "aiff" | "aif" | "aifc" => Some(Self::Aiff),
            "caf" => Some(Self::Caf),
            "mp4" | "m4a" | "m4b" | "m4r" | "m4v" | "mov" => Some(Self::Mp4),
            "mkv" | "mka" | "webm" => Some(Self::Mkv),
            "mp1" => Some(Self::Mp1),
            "mp2" => Some(Self::Mp2),
            "mp3" => Some(Self::Mp3),
            "wav" | "w64" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "ogg" | "oga" => Some(Self::Ogg),
            "opus" => Some(Self::Opus),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }

    /// Checks whether this build includes the input format's decoding feature.
    ///
    /// Raw audio is always available through the raw PCM APIs. Opus is recognized
    /// only to report that it is unsupported, even with `all-formats` enabled.
    pub fn ensure_enabled(self) -> Result<(), Error> {
        let (enabled, feature) = match self {
            Self::Aac => (cfg!(feature = "format-aac"), "format-aac"),
            Self::Aiff => (cfg!(feature = "format-aiff"), "format-aiff"),
            Self::Caf => (cfg!(feature = "format-caf"), "format-caf"),
            Self::Mp4 => (cfg!(feature = "format-m4a"), "format-m4a"),
            Self::Mkv => (cfg!(feature = "format-mkv"), "format-mkv"),
            Self::Mp1 => (cfg!(feature = "format-mp1"), "format-mp1"),
            Self::Mp2 => (cfg!(feature = "format-mp2"), "format-mp2"),
            Self::Mp3 => (cfg!(feature = "format-mp3"), "format-mp3"),
            Self::Wav => (cfg!(feature = "format-wav"), "format-wav"),
            Self::Flac => (cfg!(feature = "format-flac"), "format-flac"),
            Self::Ogg => (cfg!(feature = "format-ogg"), "format-ogg"),
            Self::Raw => return Ok(()),
            Self::Opus => {
                return Err(Error::UnsupportedFormat {
                    format: "opus".into(),
                });
            }
        };
        if enabled {
            Ok(())
        } else {
            Err(Error::FeatureDisabled { feature })
        }
    }
}

impl FromStr for AudioFormat {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_extension(s).ok_or_else(|| Error::UnsupportedFormat {
            format: s.to_string(),
        })
    }
}

/// Supported serialized waveform data formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaveformFormat {
    /// Binary `.dat` waveform data.
    Dat,
    /// Compact JSON waveform data.
    Json,
    /// Plain text CSV-like waveform data.
    Txt,
}

impl WaveformFormat {
    /// Returns the canonical lowercase name for the format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dat => "dat",
            Self::Json => "json",
            Self::Txt => "txt",
        }
    }

    /// Infers a waveform data format from a filesystem path extension.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let extension = path.as_ref().extension()?.to_str()?;
        Self::from_extension(extension)
    }

    /// Infers a waveform data format from a file extension string.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "dat" => Some(Self::Dat),
            "json" => Some(Self::Json),
            "txt" => Some(Self::Txt),
            _ => None,
        }
    }
}

impl FromStr for WaveformFormat {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_extension(s).ok_or_else(|| Error::UnsupportedFormat {
            format: s.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioFormat, WaveformFormat};

    #[test]
    fn infers_audio_formats_from_extensions_and_paths() {
        assert_eq!(AudioFormat::from_extension("mp3"), Some(AudioFormat::Mp3));
        assert_eq!(AudioFormat::from_extension("w64"), Some(AudioFormat::Wav));
        assert_eq!(AudioFormat::from_extension("oga"), Some(AudioFormat::Ogg));
        assert_eq!(AudioFormat::from_path("clip.flac"), Some(AudioFormat::Flac));
        assert_eq!(AudioFormat::from_path("clip.opus"), Some(AudioFormat::Opus));
        assert_eq!(AudioFormat::from_extension("unknown"), None);
    }

    #[test]
    fn parses_audio_format_strings() {
        assert_eq!("wav".parse::<AudioFormat>().expect("wav"), AudioFormat::Wav);
        assert_eq!("oga".parse::<AudioFormat>().expect("oga"), AudioFormat::Ogg);

        let error = "unknown".parse::<AudioFormat>().expect_err("unsupported");
        assert_eq!(error.to_string(), "Unsupported format: unknown");
    }

    #[test]
    fn infers_and_parses_waveform_formats() {
        assert_eq!(
            WaveformFormat::from_extension("dat"),
            Some(WaveformFormat::Dat)
        );
        assert_eq!(
            WaveformFormat::from_path("waveform.json"),
            Some(WaveformFormat::Json)
        );
        assert_eq!(
            "txt".parse::<WaveformFormat>().expect("txt"),
            WaveformFormat::Txt
        );
        assert_eq!(WaveformFormat::from_extension("png"), None);

        let error = "csv".parse::<WaveformFormat>().expect_err("unsupported");
        assert_eq!(error.to_string(), "Unsupported format: csv");
    }
}
