# Ruby Gem Version History

## Unreleased

- Enable all supported Rust input formats in source and precompiled builds,
  including AAC-LC/M4A, ALAC, AIFF, CAF, MPEG layers I/II, Matroska/WebM audio,
  Ogg FLAC, and WAV ADPCM. Opus and HE-AAC remain unsupported.

## 0.1.0 - 2026-09-20

### Added

- Generate waveform data directly from WAV, MP3, FLAC, and Ogg/Vorbis files.
- Read waveform metadata and points, then serialize data as DAT, JSON, or text.
- Install a source gem from GitHub releases or directly from the repository.

### Changed

- Release Ruby's global VM lock during generation, serialization, and file writes.
- Point installation instructions and package metadata at `Antti/audiowaveform`.

### Fixed

- Build, install, and test the native extension across supported Linux, macOS,
  and Windows environments.
