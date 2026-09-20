#![deny(missing_docs)]
//! First-class Rust library for generating, serializing, resampling, rendering,
//! and transcoding audio waveforms.
//!
//! The crate is designed around reusable domain types instead of command-line
//! flags. The companion CLI lives in `audiowaveform-cli` and translates its
//! argument model into these library APIs.
//!
//! # Features
//!
//! No features are enabled by default. PCM/raw waveform generation, serialization,
//! and resampling are always available. Enable `format-mp3`, `format-m4a`, or another
//! `format-*` bundle for decoding, or `all-formats` for every supported input format.
//! Each format enables the shared `decode` plumbing. `render` enables PNG rendering;
//! `wav-output` enables PCM16 WAV writing independently of input decoding.
//! Opus and HE-AAC are not supported, even with `all-formats` enabled.
//!
//! # Examples
//!
//! Generate waveform data from an audio file:
//!
//! ```no_run
//! # #[cfg(feature = "format-mp3")]
//! # {
//! use audiowaveform::{GenerateOptions, Waveform, generate_waveform_from_path};
//!
//! let waveform = generate_waveform_from_path("input.mp3", &GenerateOptions::default())?;
//! waveform.save_to_path("output.dat", None)?;
//! # }
//! # Ok::<(), audiowaveform::Error>(())
//! ```
//!
//! Render a PNG from an existing waveform file:
//!
//! ```no_run
//! # #[cfg(feature = "render")]
//! # {
//! use audiowaveform::{RenderOptions, Waveform, render_waveform_to_path};
//!
//! let waveform = Waveform::load_from_path("input.dat", None)?;
//! render_waveform_to_path(&waveform, &RenderOptions::default(), "output.png")?;
//! # }
//! # Ok::<(), audiowaveform::Error>(())
//! ```

mod audio;
mod color;
mod error;
mod format;
#[cfg(feature = "render")]
mod render;
#[cfg(feature = "wav-output")]
mod wav;
mod waveform;

pub use audio::{
    GenerateOptions, PcmAudio, RawAudioConfig, RawSampleFormat, ScaleSpec, decode_raw_audio_reader,
    generate_waveform_from_pcm, generate_waveform_from_raw_reader,
};
#[cfg(feature = "decode")]
pub use audio::{
    decode_audio_from_path, decode_audio_from_reader, generate_waveform_from_path,
    generate_waveform_from_reader,
};
pub use color::{Color, ColorScheme, WaveformColors};
pub use error::Error;
pub use format::{AudioFormat, WaveformFormat};
#[cfg(feature = "render")]
pub use image::RgbaImage;
#[cfg(feature = "render")]
pub use render::{
    BarStyle, RenderOptions, RenderStyle, render_waveform, render_waveform_to_path,
    write_waveform_png,
};
#[cfg(feature = "wav-output")]
pub use wav::write_pcm_as_wav;
#[cfg(all(feature = "decode", feature = "wav-output"))]
pub use wav::{transcode_audio_path_to_wav_path, transcode_audio_reader_to_wav_writer};
pub use waveform::{AmplitudeScale, Waveform, WaveformPoint};
