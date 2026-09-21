use std::path::Path;

#[cfg(feature = "render")]
use audiowaveform::BarStyle;
use audiowaveform::{AudioFormat, ColorScheme, RawSampleFormat, WaveformFormat};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, ValueEnum};

const CLI_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .usage(AnsiColor::Cyan.on_default().bold().underline())
    .literal(AnsiColor::Blue.on_default().bold())
    .placeholder(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().bold())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Magenta.on_default().bold())
    .context(AnsiColor::BrightBlack.on_default().dimmed())
    .context_value(AnsiColor::Yellow.on_default().italic());

#[derive(Debug, Parser)]
#[command(
    name = "audiowaveform",
    disable_version_flag = true,
    disable_help_flag = true,
    styles = CLI_STYLES
)]
/// Generate waveform data and images from audio.
pub(super) struct Cli {
    /// Show help information.
    #[arg(long = "help")]
    pub(super) help: bool,

    /// Show version information.
    #[arg(short = 'v', long = "version")]
    pub(super) version: bool,

    /// Disable progress and information messages.
    #[arg(short = 'q', long = "quiet")]
    pub(super) quiet: bool,

    /// Read input from a file or `-` for stdin.
    #[arg(short = 'i', long = "input-filename")]
    pub(super) input_filename: Option<String>,

    /// Write output to a file or `-` for stdout.
    #[arg(short = 'o', long = "output-filename")]
    pub(super) output_filename: Option<String>,

    /// Preserve channels instead of mixing to mono.
    #[arg(long = "split-channels")]
    pub(super) split_channels: bool,

    /// Override input format detection.
    #[arg(long = "input-format", value_enum)]
    pub(super) input_format: Option<CliFormat>,

    /// Override output format detection.
    #[arg(long = "output-format", value_enum)]
    pub(super) output_format: Option<CliFormat>,

    /// Use a fixed number of samples per pixel or `auto`.
    #[arg(short = 'z', long = "zoom")]
    pub(super) zoom: Option<String>,

    /// Set zoom using pixels per second.
    #[arg(long = "pixels-per-second")]
    pub(super) pixels_per_second: Option<i32>,

    /// Set waveform output bit depth.
    #[arg(short = 'b', long = "bits")]
    pub(super) bits: Option<i32>,

    /// Start rendering at a time offset in seconds.
    #[arg(short = 's', long = "start", default_value_t = 0.0)]
    pub(super) start: f64,

    /// Fit the output width to the given end time.
    #[arg(short = 'e', long = "end")]
    pub(super) end: Option<f64>,

    /// Set image width in pixels.
    #[arg(short = 'w', long = "width", default_value_t = 800)]
    pub(super) width: i32,

    /// Set image height in pixels.
    #[arg(short = 'h', long = "height", default_value_t = 250)]
    pub(super) height: i32,

    /// Choose a built-in color scheme.
    #[arg(short = 'c', long = "colors", value_enum, default_value = "audacity")]
    pub(super) color_scheme: CliColorScheme,

    /// Override the border color using `rrggbb[aa]`.
    #[arg(long = "border-color")]
    pub(super) border_color: Option<String>,

    /// Override the background color using `rrggbb[aa]`.
    #[arg(long = "background-color")]
    pub(super) background_color: Option<String>,

    /// Set one or more waveform colors using `rrggbb[aa]`.
    #[arg(long = "waveform-color")]
    pub(super) waveform_color: Option<String>,

    /// Render as lines or grouped bars.
    #[arg(long = "waveform-style", value_enum, default_value = "normal")]
    pub(super) waveform_style: CliWaveformStyle,

    /// Set bar width in pixels.
    #[arg(long = "bar-width", default_value_t = 8)]
    pub(super) bar_width: i32,

    /// Set gap between bars in pixels.
    #[arg(long = "bar-gap", default_value_t = 4)]
    pub(super) bar_gap: i32,

    /// Set the bar end-cap style.
    #[arg(long = "bar-style", value_enum, default_value = "square")]
    pub(super) bar_style: CliBarStyle,

    /// Override the axis label color using `rrggbb[aa]`.
    #[arg(long = "axis-label-color")]
    pub(super) axis_label_color: Option<String>,

    /// Hide time axis labels.
    #[arg(long = "no-axis-labels")]
    pub(super) no_axis_labels: bool,

    /// Show time axis labels.
    #[arg(long = "with-axis-labels")]
    pub(super) with_axis_labels: bool,

    /// Scale amplitude or use `auto`.
    #[arg(long = "amplitude-scale", default_value = "1.0")]
    pub(super) amplitude_scale: String,

    /// Set PNG compression level from `-1` to `9`.
    #[arg(long = "compression", default_value_t = -1, allow_negative_numbers = true)]
    pub(super) compression: i32,

    /// Set raw input sample rate in Hz.
    #[arg(long = "raw-samplerate")]
    pub(super) raw_sample_rate: Option<i32>,

    /// Set raw input channel count.
    #[arg(long = "raw-channels")]
    pub(super) raw_channels: Option<i32>,

    /// Set raw input sample format.
    #[arg(long = "raw-format", value_enum)]
    pub(super) raw_format: Option<CliRawSampleFormat>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliFormat {
    #[value(alias = "adts")]
    Aac,
    #[value(aliases = ["aif", "aifc"])]
    Aiff,
    Caf,
    #[value(aliases = ["m4a", "m4b", "m4r", "m4v", "mov"])]
    Mp4,
    #[value(aliases = ["mka", "webm"])]
    Mkv,
    Mp1,
    Mp2,
    Mp3,
    Wav,
    Flac,
    #[value(alias = "oga")]
    Ogg,
    Opus,
    Raw,
    Dat,
    Json,
    Txt,
    Png,
}

impl CliFormat {
    pub(super) fn from_path(path: &str) -> Result<Self, String> {
        let extension = Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("Unknown file format: {path}"))?;
        match extension.to_ascii_lowercase().as_str() {
            "aac" | "adts" => Ok(Self::Aac),
            "aiff" | "aif" | "aifc" => Ok(Self::Aiff),
            "caf" => Ok(Self::Caf),
            "mp4" | "m4a" | "m4b" | "m4r" | "m4v" | "mov" => Ok(Self::Mp4),
            "mkv" | "mka" | "webm" => Ok(Self::Mkv),
            "mp1" => Ok(Self::Mp1),
            "mp2" => Ok(Self::Mp2),
            "mp3" => Ok(Self::Mp3),
            "wav" => Ok(Self::Wav),
            "w64" => Err("Unsupported format: w64".to_string()),
            "flac" => Ok(Self::Flac),
            "ogg" | "oga" => Ok(Self::Ogg),
            "opus" => Ok(Self::Opus),
            "raw" => Ok(Self::Raw),
            "dat" => Ok(Self::Dat),
            "json" => Ok(Self::Json),
            "txt" => Ok(Self::Txt),
            "png" => Ok(Self::Png),
            _ => Err(format!("Unknown file format: {path}")),
        }
    }

    pub(super) fn is_audio_input(self) -> bool {
        self.as_audio_format().is_some()
    }

    pub(super) fn is_waveform_input(self) -> bool {
        matches!(self, Self::Dat | Self::Json)
    }

    pub(super) fn as_audio_format(self) -> Option<AudioFormat> {
        match self {
            Self::Aac => Some(AudioFormat::Aac),
            Self::Aiff => Some(AudioFormat::Aiff),
            Self::Caf => Some(AudioFormat::Caf),
            Self::Mp4 => Some(AudioFormat::Mp4),
            Self::Mkv => Some(AudioFormat::Mkv),
            Self::Mp1 => Some(AudioFormat::Mp1),
            Self::Mp2 => Some(AudioFormat::Mp2),
            Self::Mp3 => Some(AudioFormat::Mp3),
            Self::Wav => Some(AudioFormat::Wav),
            Self::Flac => Some(AudioFormat::Flac),
            Self::Ogg => Some(AudioFormat::Ogg),
            Self::Opus => Some(AudioFormat::Opus),
            Self::Raw => Some(AudioFormat::Raw),
            _ => None,
        }
    }

    pub(super) fn as_waveform_format(self) -> Option<WaveformFormat> {
        match self {
            Self::Dat => Some(WaveformFormat::Dat),
            Self::Json => Some(WaveformFormat::Json),
            Self::Txt => Some(WaveformFormat::Txt),
            _ => None,
        }
    }

    pub(super) fn name(self) -> &'static str {
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
            Self::Dat => "dat",
            Self::Json => "json",
            Self::Txt => "txt",
            Self::Png => "png",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliColorScheme {
    Audacity,
    Audition,
}

impl CliColorScheme {
    pub(super) fn into_library(self) -> ColorScheme {
        match self {
            Self::Audacity => ColorScheme::Audacity,
            Self::Audition => ColorScheme::Audition,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliWaveformStyle {
    Normal,
    Bars,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliBarStyle {
    Square,
    Rounded,
}

#[cfg(feature = "render")]
impl CliBarStyle {
    pub(super) fn into_library(self) -> BarStyle {
        match self {
            Self::Square => BarStyle::Square,
            Self::Rounded => BarStyle::Rounded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliRawSampleFormat {
    #[value(name = "s8")]
    S8,
    #[value(name = "u8")]
    U8,
    #[value(name = "s16le")]
    S16Le,
    #[value(name = "s16be")]
    S16Be,
    #[value(name = "s24le")]
    S24Le,
    #[value(name = "s24be")]
    S24Be,
    #[value(name = "s32le")]
    S32Le,
    #[value(name = "s32be")]
    S32Be,
    #[value(name = "f32le")]
    F32Le,
    #[value(name = "f32be")]
    F32Be,
    #[value(name = "f64le")]
    F64Le,
    #[value(name = "f64be")]
    F64Be,
}

impl CliRawSampleFormat {
    pub(super) fn into_library(self) -> RawSampleFormat {
        match self {
            Self::S8 => RawSampleFormat::S8,
            Self::U8 => RawSampleFormat::U8,
            Self::S16Le => RawSampleFormat::S16Le,
            Self::S16Be => RawSampleFormat::S16Be,
            Self::S24Le => RawSampleFormat::S24Le,
            Self::S24Be => RawSampleFormat::S24Be,
            Self::S32Le => RawSampleFormat::S32Le,
            Self::S32Be => RawSampleFormat::S32Be,
            Self::F32Le => RawSampleFormat::F32Le,
            Self::F32Be => RawSampleFormat::F32Be,
            Self::F64Le => RawSampleFormat::F64Le,
            Self::F64Be => RawSampleFormat::F64Be,
        }
    }
}
