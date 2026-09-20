# Ruby Gem Version History

## 0.2.0 - 2026-09-20

### Added

- Add `AudioWaveform.generate(..., points: 110)` for an exact point count without
  duration metadata. Preserve decoded duration, repeat samples for very short
  clips, and leave empty clips empty.
- Add `waveform.data(bits: 8)` with the same values as 8-bit JSON output, without
  serialization. Calling `data` without arguments still returns 16-bit values.
- Preserve exact-point timing in JSON via `source_frames`; reject DAT export
  when its integer scale cannot represent that timing.

## 0.1.0 - 2026-09-20

### Added

- Publish source and precompiled gems through RubyGems Trusted Publishing on
  `ruby-vX.Y.Z` tag pushes. Manual workflow runs build and test without publishing.
- Build native gems for CRuby 3.2–4.0 on Linux glibc/musl (x86-64 and ARM64),
  macOS (Intel and Apple Silicon), and Windows UCRT (x86-64).
- Load the extension matching the running Ruby version and verify installed
  platform gems before publishing. Native gems have no build-time dependencies.
- Enable all supported Rust input formats in source and precompiled builds,
  including AAC-LC/M4A, ALAC, AIFF, CAF, MPEG layers I/II, Matroska/WebM audio,
  Ogg FLAC, and WAV ADPCM. Opus and HE-AAC remain unsupported.

- Generate waveform data directly from WAV, MP3, FLAC, and Ogg/Vorbis files.
- Read waveform metadata and points, then serialize data as DAT, JSON, or text.
- Install a source gem from GitHub releases or directly from the repository.

### Changed

- Release Ruby's global VM lock during generation, serialization, and file writes.
- Point installation instructions and package metadata at `Antti/audiowaveform`.

### Fixed

- Build, install, and test the native extension across supported Linux, macOS,
  and Windows environments.
