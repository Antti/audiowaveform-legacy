# Ruby Gem Version History

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
