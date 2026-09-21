# audiowaveform

[![CI](https://github.com/Antti/audiowaveform/actions/workflows/rust.yml/badge.svg)](https://github.com/Antti/audiowaveform/actions/workflows/rust.yml)

`audiowaveform` is a Rust library and CLI for generating waveform data from audio,
serializing waveform files, rendering PNG waveform images, and transcoding audio
to PCM16 WAV.

This repository is the canonical home of the Rust rewrite:
[github.com/Antti/audiowaveform](https://github.com/Antti/audiowaveform).

It is a Rust rewrite of the original BBC `audiowaveform` project:
[github.com/bbc/audiowaveform](https://github.com/bbc/audiowaveform).
The original project was created by Chris Needham and contributors at BBC
Research & Development.

The repository's main workspace remains Rust-only:

- `crates/audiowaveform`: reusable library crate
- `crates/audiowaveform-cli`: thin `audiowaveform` command-line wrapper

Ruby applications can use the native extension in `bindings/ruby` to generate
waveforms in process through the same library crate.

![Example Waveform](doc/example.png "Example Waveform")

## Features

- Decode AAC-LC, ALAC, MP1/MP2/MP3, WAV, FLAC, Ogg, AIFF, CAF, and audio in MP4/Matroska/WebM containers
- Generate `.dat`, `.json`, and `.txt` waveform files
- Render PNG waveform images in pure Rust
- Transcode decoded audio or raw PCM input to PCM16 WAV
- Use path-based, stream-based, or in-memory APIs from the library crate

Wave64 (`.w64`), Opus, and HE-AAC are unsupported by the current decoder. AAC-LC
supports mono and stereo. Enabling a container such as WebM does not add unsupported codecs.
AAC/MP4 decoding does not apply gapless trimming, so decoded audio and waveform
duration can include encoder delay and padding.

## Quick Start

Build the workspace:

```sh
cargo build --workspace
```

Run the CLI:

```sh
cargo run -p audiowaveform-cli -- -i fixtures/test_file_stereo.wav -o output.dat
```

Install the CLI locally:

```sh
cargo install --path crates/audiowaveform-cli
```

Run the test suite:

```sh
cargo test --workspace
```

## Library Usage

The Rust library has **no default features**. PCM/raw waveform generation,
waveform serialization, and resampling are always available. Enable input formats
and output capabilities explicitly:

```toml
[dependencies]
audiowaveform = { version = "1.10.3", features = ["format-mp3", "format-m4a"] }
```

| Cargo feature | Capability |
| --- | --- |
| `format-aac` | AAC-LC in ADTS (`.aac`, `.adts`) |
| `format-aiff` | PCM in AIFF/AIFF-C (`.aiff`, `.aif`, `.aifc`) |
| `format-caf` | PCM and ALAC in CAF |
| `format-flac` | FLAC |
| `format-m4a` / `format-mp4` | AAC-LC, ALAC, MP3, and PCM in MP4/M4A/MOV containers |
| `format-mkv` / `format-webm` | Supported audio codecs in Matroska/WebM (`.mkv`, `.mka`, `.webm`); no Opus |
| `format-mp1`, `format-mp2`, `format-mp3` | MPEG audio layers I, II, and III respectively |
| `format-ogg` | Vorbis and FLAC in Ogg (`.ogg`, `.oga`) |
| `format-wav` | PCM and ADPCM in WAV |
| `all-formats` | All input format bundles above |
| `render` | PNG rendering |
| `wav-output` | PCM16 WAV writing |

`format-mp4` aliases `format-m4a`; `format-webm` aliases `format-mkv`.
Format features enable the shared `decode` plumbing automatically. `decode`
alone does not enable any codecs or containers. `all-formats` does not enable
PNG rendering or WAV writing. Cargo features are additive: another dependency
can enable additional features in a shared build.

Generate waveform data from an audio file:

```rust,no_run
use audiowaveform::{GenerateOptions, WaveformFormat, generate_waveform_from_path};

fn main() -> Result<(), audiowaveform::Error> {
    let waveform = generate_waveform_from_path("input.mp3", &GenerateOptions::default())?;
    waveform.save_to_path("output.dat", Some(WaveformFormat::Dat))?;
    Ok(())
}
```

Render a PNG from a stored waveform (requires `render`):

```rust,no_run
use audiowaveform::{RenderOptions, Waveform, render_waveform_to_path};

fn main() -> Result<(), audiowaveform::Error> {
    let waveform = Waveform::load_from_path("input.dat", None)?;
    render_waveform_to_path(&waveform, &RenderOptions::default(), "output.png")?;
    Ok(())
}
```

Generate waveform data from in-memory PCM:

```rust
use audiowaveform::{GenerateOptions, PcmAudio, generate_waveform_from_pcm};

fn main() -> Result<(), audiowaveform::Error> {
    let pcm = PcmAudio::new(48_000, 1, vec![0_i16; 48_000])?;
    let waveform = generate_waveform_from_pcm(&pcm, &GenerateOptions::default())?;
    assert!(!waveform.is_empty());
    Ok(())
}
```

Additional examples live in `crates/audiowaveform/examples`.

File output APIs validate before opening the destination: `render_waveform_to_path`
preserves existing PNG files when rendering options are invalid, and
`write_pcm_to_wav_path` (requires `wav-output`) rejects unrepresentable WAV headers
before writing. `transcode_audio_path_to_wav_path` also finishes decoding first,
allowing input and output to name the same file.

`AmplitudeScale::Auto` preserves relative amplitudes, maps the largest absolute
peak to 32767, and leaves silence unchanged.

Use `ScaleSpec::Points(110)` in `GenerateOptions::scale` to generate exactly 110
min/max pairs per channel from nonempty audio. Generation counts decoded PCM
frames, so duration metadata is not required. Empty audio stays empty; very short
clips repeat samples. `waveform.data(8)?` returns signed 8-bit values in a `Vec<i16>`
using the same conversion as 8-bit serialization, without a JSON round trip.
`data(16)` returns a copy of the original values.

For exact point counts, `duration_seconds()` retains the decoded duration and
`samples_per_point()` supplies the fractional scale. `samples_per_pixel()` is
only a nominal integer scale. JSON preserves exact timing via `source_frames`;
DAT export rejects scales it cannot represent. Exact-point waveforms must be
regenerated from audio rather than appended to or resampled.

Waveform generation aggregates decoded blocks without retaining the full PCM
recording. Fixed scales use one decoding pass; exact point counts and full-clip
`FitWidth` use two passes to count actual frames and then aggregate peaks. This
keeps PCM working memory bounded even when duration metadata is absent. Total
memory also includes decoder/container state and the output waveform. Explicit
`decode_audio_*` APIs still return an entire in-memory PCM buffer.

Raw readers accept pipes: fixed scales stream directly, while scales needing a
total frame count spool input to a temporary file. CLI waveform generation reads
audio files directly and spools encoded stdin or named pipes to a temporary file
for seeking. Seekable input must remain unchanged between decoding passes.

## Ruby Usage

Install the `audiowaveform` gem from RubyGems and generate waveform
data without invoking the command-line program:

```ruby
gem "audiowaveform", "~> 0.1"
```

```ruby
require "audiowaveform"

waveform = AudioWaveform.generate("input.mp3", samples_per_pixel: 256)
waveform.save("output.dat", bits: 8)
```

Generate a fixed-size array for a waveform display:

```ruby
waveform = AudioWaveform.generate("recording.m4a", points: 110)
peaks = waveform.data(bits: 8) # 110 min/max pairs, 220 integers
```

See [`bindings/ruby/README.md`](bindings/ruby/README.md) for the full Ruby API
and development instructions.

## CLI Usage

The CLI defaults to `all-formats`, `render`, and `wav-output`. To build a smaller
CLI, disable defaults and enable only the features you need:

```sh
cargo build -p audiowaveform-cli --no-default-features --features format-mp3,format-m4a
```

Add `render` or `wav-output` for those outputs. With no features, the CLI can
still process raw PCM and convert/resample waveform data. Requests for omitted
formats or outputs report the required Cargo feature.

Generate `.dat` waveform data:

```sh
audiowaveform -i input.wav -o output.dat -z 128 -b 8
```

Render a PNG from waveform data:

```sh
audiowaveform -i input.dat -o output.png -w 1000 -h 200
```

Generate JSON waveform data from compressed audio:

```sh
audiowaveform -i input.flac -o output.json --pixels-per-second 50
```

Convert raw PCM to WAV:

```sh
audiowaveform -i input.raw -o output.wav --input-format raw --raw-samplerate 48000 --raw-channels 2 --raw-format s16le
```

See all options with:

```sh
audiowaveform --help
```

## Supported Formats

Audio input:

- `aac` and `adts` (AAC-LC)
- `mp1`, `mp2`, and `mp3`
- `mp4`, `m4a`, `m4b`, `m4r`, `m4v`, and `mov` (supported audio tracks only)
- `mkv`, `mka`, and `webm` (supported audio tracks only; no Opus)
- `aiff`, `aif`, and `aifc`
- `caf`
- `wav`
- `flac`
- `ogg` and `oga`
- `raw`

Waveform input:

- `dat`
- `json`

Waveform output:

- `dat`
- `json`
- `txt`

Image output:

- `png`

Audio output:

- `wav`

## Documentation

- Waveform file formats: [doc/DataFormat.md](doc/DataFormat.md)
- CLI man page: [doc/audiowaveform.1](doc/audiowaveform.1)
- Waveform format man page: [doc/audiowaveform.5](doc/audiowaveform.5)
- Project release history: [CHANGELOG.md](CHANGELOG.md)
- Ruby gem release history: [bindings/ruby/CHANGELOG.md](bindings/ruby/CHANGELOG.md)

Generate local API docs with:

```sh
cargo doc -p audiowaveform --all-features --no-deps
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for workflow, documentation, and testing expectations.

## License

`audiowaveform` is released under the GPL-3.0-or-later license. See [COPYING](COPYING).
