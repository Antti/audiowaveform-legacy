# Audiowaveform Version History

This repository is a Rust rewrite of the original BBC `audiowaveform`
project. Historical entries below are inherited from that original project and
are kept here as release history context.

## Rust 0.1.0 (2026-09-21)

First Rust library and CLI release, versioned independently from the historical
C++ releases below. The Rust API is still evolving; `0.x` minor releases may
contain breaking changes.

- Update Rust dependencies, including Symphonia 0.6.1, PNG 0.18.1, and rb-sys
  0.9.130. Preserve waveform quantization and leading-delay handling, reuse one
  PCM conversion buffer, and retain metadata-aware format detection.
- Decode ID3-prefixed ADTS/AAC with Symphonia's corrected duration estimator,
  and reject unsupported WAV channel counts and inconsistent masks
  before demuxing. Retry interrupted encoded reads.
- Retain known speaker layouts in decoded PCM and WAV transcoding, including
  side-surround and nonstandard mono/stereo positions. Write WAV samples in
  bounded blocks instead of buffering another copy of the entire output.
- Correct exact-point render offsets at point boundaries and the clip end.
- Enforce signed DAT sample-rate/scale limits before reading or writing; correct
  the documented version-2 payload offsets.
- Apply CLI amplitude scaling to waveform conversion/resampling and enable TXT
  output for audio generation and resampling. Accept spaced `--compression -1`.
- Validate generation options before input I/O, and use wide frame coordinates
  for resampling on 32-bit platforms.
- Remove per-point heap allocations, reuse owned waveform storage for amplitude
  scaling, skip PCM conversion in counting passes, and reuse integer conversion
  buffers between decoded packets. Preallocate waveform output when its size is
  known.
- Grow DAT sample storage only as payload is read instead of reserving memory
  from an untrusted length header; check length arithmetic for overflow.
- Preserve existing output files when WAV decoding or PNG option validation
  fails in the library and CLI, and allow WAV transcoding when input and output
  refer to the same file. Add `write_pcm_to_wav_path` for validated PCM output.
- Propagate PNG final-chunk and flush failures to the caller.
- Normalize asymmetric signals using the largest absolute peak without clipping
  the opposite peak. Silence remains unchanged.
- Check WAV header limits, fit-width scales, and render coordinates before
  arithmetic can overflow; use wider intermediates for tall images.
- Clip bar drawing loops to the canvas and constrain rounded corners to their
  rectangle. Draw waveform borders after the waveform so they remain visible.
  PNG fixtures reflect the corrected borders; interior pixels are unchanged.
- Match raw signed 24/32-bit PCM quantization to container decoding.
- Report Wave64 (`.w64`) as unsupported and remove incorrect support claims.
- Generate waveform peaks incrementally without buffering the complete decoded
  recording. Exact point counts use a counting pass followed by aggregation.
- Stream raw PCM pipes at fixed scales and use temporary files when input must
  be replayed, including encoded CLI stdin and named pipes.
- Add exact-count PCM waveform generation with `ScaleSpec::Points`, preserving
  decoded duration in JSON and using fractional point spacing when rendering.
- Expose direct 8-bit or 16-bit waveform values with `Waveform::data`.
- Render exact-point JSON directly in the CLI and reject incompatible DAT exports
  before creating or truncating the output file.
- Make the Rust library minimal by default, with opt-in `format-*`, `render`,
  and `wav-output` features and an `all-formats` input bundle.
- Enable every supported input format, PNG rendering, and WAV output by default
  in the CLI; allow custom builds with `--no-default-features`.
- Add AAC-LC/ADTS, MP4/M4A (including ALAC), AIFF, CAF, MPEG layers I/II,
  Matroska/WebM audio, Ogg FLAC, and WAV ADPCM input support.
- Select a supported audio track in multi-track containers and report disabled
  capabilities with their required Cargo feature.

## v1.10.3 (2025-08-20)

 * Fixed CMakeLists.txt to work with Boost 1.89.0 and later versions

## v1.10.2 (2025-04-18)

 * The `--bar-gap` option can now be set to zero.

## v1.10.1 (2024-01-26)

 * Fixed `--input-filename` and `--output-filename` options to accept
   filenames containing spaces

## v1.10.0 (2024-01-23)

 * Added support for raw audio file input

## v1.9.1 (2023-10-01)

 * Updated documentation and command line `--help` ouptut
 * Fixed CMakeLists.txt for compatibility with older CMake versions

## v1.9.0 (2023-09-30)

 * Added support for input JSON format waveform data files
 * The `--waveform-colors` option now allows you to set different colors
   for each audio channel

## v1.8.1 (2023-07-08)

 * Fixed rounded bar style rendering to show bars during silent
   periods of audio

## v1.8.0 (2023-05-31)

 * Added support for rendering waveforms as vertical bars. Use the
   `--waveform-style bars` option, and the `--bar-style`,
   `--bar-width`, and `--bar-gap` options to customize the image

## v1.7.1 (2023-03-19)

 * Fixed waveform image generation when audio is read from a socket
   and the `--zoom auto` option is used to automatically fit the
   waveform to a given image width

## v1.7.0 (2022-12-10)

 * Fixed waveform image generation when audio is piped to stdin
   and the `--zoom auto` option is used to automatically fit the
   waveform to a given image width

## v1.6.0 (2022-02-18)

 * Added support for Opus audio
 * Fixed crash when reading malformed .dat files

## v1.5.1 (2021-07-31)

 * Fixed buffer overflow error in MP3 decoder

## v1.5.0 (2021-07-07)

 * Added `--quiet` option, to disable progress and information messages
 * Increased channel limit to 24
 * Removed sample rate, zoom, and start time limits when generating
   waveform images

## v1.4.2 (2020-05-03)

 * Enable `--amplitude-scale` option when generating waveform data

## v1.4.1 (2020-01-28)

 * Enable conversion from FLAC or Ogg Vorbis audio to WAV format

## v1.4.0 (2019-11-03)

 * Added `--input-format` and `--output-format` options, to enable
   reading input from stdin and writing output to stdout

## v1.3.3 (2018-12-11)

 * Increase channel limit to eight

## v1.3.2 (2018-11-23)

 * Fixed version 2 binary data format

## v1.3.1 (2018-11-21)

 * Added `--split-channels` option to produce multi-channel output
   files
 * Enabled static linking

## v1.2.2 (2018-06-23)

 * Fixed MinGW build
 * Updated Ubuntu package details

## v1.2.1 (2018-05-25)

 * Added support for Ogg Vorbis audio

## v1.2.0 (2018-04-16)

 * Added `--zoom` `auto` option to automatically fit the waveform to a
   given image width when generating PNG images

## v1.1.0 (2017-02-02)

 * Skip information frames in MP3 files, to correct for initial
   offset delay

## v1.0.12 (2016-12-20)

 * Added `--amplitude-scale` option to control ampltiude scaling
   when generating PNG images
 * Skip ID3 tags in MP3 files

## v1.0.11 (2016-04-22)

 * Added `--compression` command line option to set PNG compression
   level
 * Removed examples to shorten `--help` output

## v1.0.10 (2015-03-27)

 * Corrected handling of floating point format WAV files

## v1.0.9 (2014-11-12)

 * Added `--pixels-per-second` option as alternative way of setting the
   zoom level

## v1.0.8 (2014-10-22)

 * Corrected use of `--bits` option when converting waveform data files
 * Allow image colours to include transparency

## v1.0.7 (2014-09-22)

 * Added support for FLAC audio

## v1.0.6 (2014-04-01)

 * Added command-line options to set image colours

## v1.0.5 (2014-03-21)

 * Allow creation of JSON waveform data files from input audio files

## v1.0.4 (2014-03-10)

 * Added `--end` option to set time range when rendering images and
   `--no-axis-labels` option to control axis label rendering

## v1.0.3 (2013-12-11)

 * Allow creation of PNG images directly from input audio files

## v1.0.2 (2013-12-11)

 * Added support for mono input audio files

## v1.0.1 (2013-10-14)

 * Initial public release
