# audiowaveform for Ruby

The `audiowaveform` gem provides native Ruby bindings for the Rust
`audiowaveform` library. It generates waveform data in process without invoking
the command-line program. Audio decoding runs without Ruby's global VM lock, so
other Ruby threads can continue while a waveform is generated.

## Installation

Add the gem to your bundle:

```ruby
gem "audiowaveform", "~> 0.1"
```

Releases include precompiled gems for CRuby 3.2, 3.3, 3.4, and 4.0 on these platforms:

| OS | Architectures | Gem platforms |
| --- | --- | --- |
| Linux (glibc) | x86-64, ARM64 | `x86_64-linux-gnu`, `aarch64-linux-gnu` |
| Linux (musl/Alpine) | x86-64, ARM64 | `x86_64-linux-musl`, `aarch64-linux-musl` |
| macOS | Intel, Apple Silicon | `x86_64-darwin`, `arm64-darwin` |
| Windows (UCRT) | x86-64 | `x64-mingw-ucrt` |

Precompiled gems do not require Rust, FFmpeg, or compilation during installation.
They contain a separate extension for each supported Ruby minor version.
Other Ruby versions and platforms fall back to the source gem, which needs Rust
and Ruby development headers. Installing directly from GitHub also builds from
source. JRuby and TruffleRuby are not supported.

## Usage

Generate waveform data from an audio file:

```ruby
require "audiowaveform"

waveform = AudioWaveform.generate(
  "recording.mp3",
  samples_per_pixel: 256,
  split_channels: false
)

waveform.sample_rate       # => 44_100
waveform.samples_per_pixel # => 256
waveform.channels          # => 1
waveform.length            # => number of waveform points per channel
waveform.data              # => interleaved [min, max, ...] samples
```

`AudioWaveform.generate` accepts these keyword arguments:

| Keyword | Default | Description |
| --- | --- | --- |
| `samples_per_pixel` | `256` | Number of source samples represented by each waveform point. Must be at least 2. |
| `pixels_per_second` | none | Time-based scale. Cannot be combined with `samples_per_pixel`. |
| `split_channels` | `false` | Preserve separate audio channels instead of mixing them down. |
| `amplitude_scale` | none | Non-negative numeric multiplier, or `:auto` to normalize automatically. |

The returned `AudioWaveform::Waveform` exposes:

| Method | Result |
| --- | --- |
| `sample_rate` | Source sample rate in hertz. |
| `samples_per_pixel` | Source samples represented by each waveform point. |
| `channels` | Number of waveform channels. |
| `storage_bits` / `bits` | Internal sample resolution, either 8 or 16. |
| `length` / `size` | Number of waveform points per channel. |
| `empty?` | Whether the waveform contains no points. |
| `duration` / `duration_seconds` | Approximate duration in seconds. |
| `data` | Interleaved minimum and maximum sample values. |
| `point(index, channel: 0)` | `[minimum, maximum]` pair for one point and channel. |

Save the generated waveform as binary DAT, JSON, or text:

```ruby
waveform.save("recording.dat", bits: 8)
waveform.save("recording.json", bits: 16)
waveform.save("recording.waveform", format: :txt)

json = waveform.to_json(bits: 8)
binary = waveform.to_dat(bits: 16)
text = waveform.to_txt(bits: 8)
```

`bits` must be 8 or 16. `save` infers the format from a `.dat`, `.json`, or
`.txt` extension unless `format:` is provided explicitly. Invalid generation
or serialization options raise `ArgumentError`; an out-of-range `point` raises
`IndexError`; decoding and file I/O failures raise `AudioWaveform::Error`.

Use `pixels_per_second` instead of `samples_per_pixel` when a time-based scale
is more convenient:

```ruby
waveform = AudioWaveform.generate("recording.flac", pixels_per_second: 100)
```

Amplitude can be scaled with a numeric multiplier or normalized automatically:

```ruby
AudioWaveform.generate("quiet.wav", amplitude_scale: 1.5)
AudioWaveform.generate("quiet.wav", amplitude_scale: :auto)
```

Both source and precompiled gems enable the Rust library's `all-formats` feature:
AAC-LC/ADTS, AAC-LC and ALAC in M4A/MP4, MP1/MP2/MP3, WAV/W64 (PCM and ADPCM),
FLAC, Ogg (Vorbis and FLAC), AIFF, CAF (PCM and ALAC), and supported audio tracks
in Matroska/WebM. No FFmpeg installation is required for decoding.

AAC-LC supports mono and stereo. HE-AAC and Opus remain unsupported, including
Opus inside Ogg/WebM/MP4. Raw PCM input is not currently exposed by the gem.
The filename extension identifies the container; an enabled container can still
contain an unsupported codec. Such files raise `AudioWaveform::Error`.
AAC/MP4 waveform duration can include encoder delay and padding; gapless
trimming is not currently supported.

## Development

From the repository root:

```sh
bundle install
bundle exec rake
bundle exec rake build
```

Gem packages are written to the repository root's `pkg/` directory. To build a
native gem for the current Ruby and platform, use `bundle exec rake native gem`.
For the same multi-version build used in CI, install Docker and run:

```sh
bundle exec rb-sys-dock --platform x86_64-linux --ruby-versions 3.2,3.3,3.4,4.0 --build
```

The Linux build targets `x86_64-linux` and `aarch64-linux` produce explicitly
tagged `-linux-gnu` gems, so RubyGems can distinguish glibc from musl.
The rb_sys version pinned in the root Gemfile selects the cross-compilation image.

## Releasing to RubyGems

The gem has its own semantic version, independent of the Rust library. Releases
use `ruby-vX.Y.Z` tags and [the Ruby Gem Release workflow](../../.github/workflows/ruby-release.yml).
Each release runs the Ruby tests, builds a source gem and seven platform gems,
installs and exercises every native gem on all four supported Ruby versions, and
publishes the validated artifacts to RubyGems and a GitHub release.

### One-time setup

1. Create a GitHub environment named `release` in `Antti/audiowaveform`, allowing
   deployments only from tags matching `ruby-v*`.
2. Sign in to the RubyGems account that will own `audiowaveform`, with MFA enabled.
   For the first release, create a **pending trusted publisher** from your profile.
   For an existing gem you own, use its **Trusted publishers** page.
3. Configure these exact values:

   | Field | Value |
   | --- | --- |
   | Gem name (pending publisher) | `audiowaveform` |
   | Repository owner | `Antti` |
   | Repository name | `audiowaveform` |
   | Workflow filename | `ruby-release.yml` |
   | Environment | `release` |

   Leave the optional reusable-workflow repository fields empty: publishing
   happens directly in `ruby-release.yml`. No RubyGems API-key secret is needed.
   Pending publishers expire after 12 hours, so create one shortly before the
   first release; recreate it if it expires before the first successful push.
   See the [RubyGems Trusted Publishing guide](https://guides.rubygems.org/trusted-publishing/).

### Each release

1. Update `bindings/ruby/lib/audiowaveform/version.rb` and the [Ruby changelog](CHANGELOG.md).
   Refresh the local Bundler lockfile with `bundle install` after changing the version.
2. Run `bundle exec rake` and `bundle exec rake build`, then commit and merge the
   release preparation. The source gem is `pkg/audiowaveform-X.Y.Z.gem`.
3. Optionally run **Ruby Gem Release** manually on that commit. Manual runs build
   and test all packages without publishing, including when run against a tag.
4. Tag the release commit and push the tag to the project repository:

   ```sh
   git tag -a ruby-vX.Y.Z -m "Release Ruby gem X.Y.Z"
   git push antti ruby-vX.Y.Z
   ```

   Replace `X.Y.Z` with the gem version, and `antti` with your remote name if different.
   The workflow rejects a tag that does not match the Ruby version file.
5. Check that the release workflow succeeds and all eight artifacts are visible
   on RubyGems and the GitHub release.

If publishing fails partway through, rerun the failed job using the same artifacts.
It skips previously uploaded gems only when their SHA-256 checksums match. A
different artifact under the same version/platform is rejected; release a new
version instead. Do not move an existing release tag.
