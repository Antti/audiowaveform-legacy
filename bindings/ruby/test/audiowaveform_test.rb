# frozen_string_literal: true

require_relative "test_helper"

class AudioWaveformTest < Minitest::Test
  include AudioWaveformTestSupport

  def test_generates_waveform_data_from_audio
    waveform = AudioWaveform.generate(
      Pathname(fixture("test_file_stereo.wav")),
      samples_per_pixel: 128
    )

    assert_instance_of AudioWaveform::Waveform, waveform
    refute waveform.empty?
    assert_equal 16_000, waveform.sample_rate
    assert_equal 128, waveform.samples_per_pixel
    assert_equal 1, waveform.channels
    assert_equal 16, waveform.storage_bits
    assert_equal waveform.storage_bits, waveform.bits
    assert_operator waveform.length, :>, 0
    assert_equal waveform.length, waveform.size
    assert_equal waveform.length * 2, waveform.data.length
    assert_operator waveform.duration, :>, 0.0
    assert_equal waveform.duration, waveform.duration_seconds
  end

  def test_preserves_channels_when_requested
    waveform = AudioWaveform.generate(
      fixture("test_file_stereo.wav"),
      samples_per_pixel: 128,
      split_channels: true
    )

    assert_equal 2, waveform.channels
    assert_equal waveform.length * waveform.channels * 2, waveform.data.length
  end

  def test_generates_an_exact_point_count_from_decoded_frames
    waveform = AudioWaveform.generate(fixture("formats/stereo.wav"), points: 110)
    assert_equal 110, waveform.length
    assert_equal 220, waveform.data.length
    assert_equal 0.25, waveform.duration
    assert_equal 12_000, JSON.parse(waveform.to_json).fetch("source_frames")

    split = AudioWaveform.generate(fixture("formats/stereo.wav"), points: 110, split_channels: true)
    assert_equal 110, split.length
    assert_equal 440, split.data.length

    # Fragmented MP4 can omit track duration; generation uses decoded frames.
    fragmented = AudioWaveform.generate(fixture("formats/fragmented.mp4"), points: 110)
    assert_equal 110, fragmented.length
    assert_operator fragmented.duration, :>, 0

    scaled = AudioWaveform.generate(fixture("formats/stereo.wav"), points: 110, amplitude_scale: 0.5)
    assert_equal waveform.data.map { |value| (value * 0.5).truncate }, scaled.data
    assert_equal waveform.duration, scaled.duration
  end

  def test_point_counts_for_empty_and_very_short_audio
    Dir.mktmpdir do |directory|
      path = File.join(directory, "short.wav")
      [0, 1, 3].each do |frames|
        write_silence_wav(path, frame_count: frames)
        waveform = AudioWaveform.generate(path, points: 110)
        assert_equal(frames.zero? ? 0 : 110, waveform.length)
        assert_equal frames / 16_000.0, waveform.duration
      end
    end
  end

  def test_direct_bit_output_matches_json_and_leaves_internal_values_unchanged
    waveform = AudioWaveform.generate(fixture("test_file_stereo.wav"), points: 110, split_channels: true)
    original = waveform.data
    [8, 16].each do |bits|
      assert_equal JSON.parse(waveform.to_json(bits: bits)).fetch("data"), waveform.data(bits: bits)
    end
    assert waveform.data(bits: 8).all? { |value| value.between?(-128, 127) }
    assert_equal original, waveform.data
    assert_equal 16, waveform.bits
    copy = waveform.data(bits: 16)
    copy[0] = 123_456
    assert_equal original, waveform.data
    [nil, false, "8", 8.0, 0, 12, 256].each do |bits|
      assert_raises(ArgumentError) { waveform.data(bits: bits) }
    end
  end

  def test_point_count_validation_and_serialization
    path = fixture("formats/stereo.wav")
    [0, -1, 1.5, false, "110", 2**32].each do |points|
      assert_raises(ArgumentError) { AudioWaveform.generate(path, points: points) }
    end
    [{samples_per_pixel: 256}, {pixels_per_second: 100}].each do |scale|
      assert_raises(ArgumentError) { AudioWaveform.generate(path, points: 110, **scale) }
    end
    assert_equal AudioWaveform.generate(path).data, AudioWaveform.generate(path, points: nil).data
    assert_raises(ArgumentError) { AudioWaveform.generate(path, points: 110).to_dat }
    Dir.mktmpdir do |directory|
      output = File.join(directory, "existing.dat")
      File.write(output, "keep existing output")
      assert_raises(ArgumentError) { AudioWaveform.generate(path, points: 110).save(output) }
      assert_equal "keep existing output", File.read(output)
    end
    assert_kind_of String, AudioWaveform.generate(path, points: 100).to_dat
    assert_equal 110, AudioWaveform.generate(path, points: 110).to_txt.lines.length
  end

  def test_generates_from_each_documented_compressed_format
    %w[
      test_file_stereo.mp3 test_file_stereo.flac test_file_stereo.oga
      formats/stereo.aac formats/stereo.m4a formats/mono.m4a formats/alac.m4a
      formats/fragmented.mp4 formats/video-first.mp4 formats/stereo.aiff
      formats/stereo.caf formats/pcm.caf formats/stereo.webm formats/stereo.mka
      formats/stereo.mp2 formats/silence.mp1 formats/flac.ogg formats/adpcm.wav
    ].each do |filename|
      waveform = AudioWaveform.generate(fixture(filename))

      assert_operator waveform.length, :>, 0, filename
    end
  end

  def test_releases_the_gvl_for_generation_serialization_and_saving
    Dir.mktmpdir do |directory|
      path = File.join(directory, "long.wav")
      write_silence_wav(path, frame_count: 5_000_000)

      waveform = assert_other_ruby_thread_progresses do
        AudioWaveform.generate(path, samples_per_pixel: 2)
      end
      json = assert_other_ruby_thread_progresses { waveform.to_json(bits: 8) }
      output_path = File.join(directory, "long.json")
      assert_other_ruby_thread_progresses { waveform.save(output_path, bits: 8) }

      refute waveform.empty?
      assert json.start_with?("{")
      assert_operator File.size(output_path), :>, 0
    end
  end

  def test_empty_waveform_predicate
    Dir.mktmpdir do |directory|
      path = File.join(directory, "empty.wav")
      write_silence_wav(path, frame_count: 0)

      waveform = AudioWaveform.generate(path)

      assert waveform.empty?
      assert_equal 0, waveform.length
    end
  end

  def test_supports_pixels_per_second_and_amplitude_scaling
    waveform = AudioWaveform.generate(
      fixture("test_file_mono.wav"),
      pixels_per_second: 100,
      amplitude_scale: :auto
    )

    assert_equal 160, waveform.samples_per_pixel
    assert_operator waveform.data.map(&:abs).max, :>, 30_000

    scaled = AudioWaveform.generate(
      fixture("test_file_mono.wav"),
      pixels_per_second: 100,
      amplitude_scale: 0.5
    )
    assert_operator scaled.data.map(&:abs).max, :<, waveform.data.map(&:abs).max

    string_scaled = AudioWaveform.generate(
      fixture("test_file_mono.wav"),
      pixels_per_second: 100,
      amplitude_scale: "0.5"
    )
    assert_equal scaled.data, string_scaled.data

    assert_equal waveform.data, AudioWaveform.generate(
      fixture("test_file_mono.wav"),
      pixels_per_second: 100,
      amplitude_scale: "auto"
    ).data
  end

  def test_reads_individual_points
    waveform = AudioWaveform.generate(fixture("test_file_mono.wav"))

    assert_equal waveform.data.first(2), waveform.point(0)
    assert_raises(IndexError) { waveform.point(waveform.length) }
    assert_raises(IndexError) { waveform.point(0, channel: 1) }
    assert_raises(IndexError) { waveform.point(-1) }
    assert_raises(IndexError) { waveform.point(0, channel: -1) }
    assert_raises(IndexError) { waveform.point(0.5) }
    assert_raises(IndexError) { waveform.point(2**100) }
    assert_raises(IndexError) { waveform.point(0, channel: 65_536) }
    assert_raises(NoMethodError) { AudioWaveform::Waveform.new }
  end

  def test_serializes_dat_json_and_text
    waveform = AudioWaveform.generate(
      fixture("test_file_mono.wav"),
      samples_per_pixel: 128
    )

    dat = waveform.to_dat(bits: 8)
    json = JSON.parse(waveform.to_json(bits: 8))
    text = waveform.to_txt(bits: 16)

    assert_equal Encoding::ASCII_8BIT, dat.encoding
    assert_equal [1].pack("V"), dat.byteslice(0, 4)
    assert_equal 8, json.fetch("bits")
    assert_equal waveform.length, json.fetch("length")
    assert_equal waveform.length, JSON.parse(JSON.generate(waveform)).fetch("length")
    assert_equal 16, JSON.parse(waveform.to_json).fetch("bits")
    assert_match(/^-?\d+,-?\d+/, text)
  end

  def test_saves_waveform_and_returns_self
    waveform = AudioWaveform.generate(fixture("test_file_mono.wav"))

    Dir.mktmpdir do |directory|
      path = File.join(directory, "waveform.json")

      assert_same waveform, waveform.save(path, bits: 8)
      assert_equal 8, JSON.parse(File.read(path)).fetch("bits")

      extensionless_path = File.join(directory, "waveform")
      waveform.save(extensionless_path, format: :json)
      assert_equal waveform.length, JSON.parse(File.read(extensionless_path)).fetch("length")

      assert_raises(AudioWaveform::Error) do
        waveform.save(File.join(directory, "waveform.png"))
      end
      assert_raises(AudioWaveform::Error) do
        waveform.save(directory, format: :json)
      end
    end
  end

  def test_scale_values_have_consistent_bounds_and_error_types
    [:samples_per_pixel, :pixels_per_second, :points].each do |keyword|
      [false, true, 2**32, -1, 1.5, "110"].each do |value|
        # Validation must precede opening even a nonexistent path.
        assert_raises(ArgumentError) { AudioWaveform.generate("missing.wav", **{keyword => value}) }
      end
    end
  end

  def test_validates_options
    input = fixture("test_file_mono.wav")

    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, samples_per_pixel: 64, pixels_per_second: 100)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, samples_per_pixel: 1)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, pixels_per_second: 100_000)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, pixels_per_second: 0)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, pixels_per_second: 1.5)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, amplitude_scale: Float::INFINITY)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, amplitude_scale: -0.5)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input, amplitude_scale: "loud")
    end
    assert_raises(TypeError) do
      AudioWaveform.generate(nil)
    end
    assert_raises(ArgumentError) do
      AudioWaveform.generate(input).to_dat(bits: 12)
    end
  end

  def test_maps_decode_and_io_failures_to_gem_error
    error = assert_raises(AudioWaveform::Error) do
      AudioWaveform.generate("missing.wav")
    end

    assert_match(/missing\.wav|No such file|cannot find the file/i, error.message)
  end

  private

  def assert_other_ruby_thread_progresses
    start = Queue.new
    ready = Queue.new
    running = true
    progress = 0
    worker = Thread.new do
      ready << true
      start.pop
      progress += 1 while running
    end

    ready.pop
    start << true
    before = progress
    result = yield
    after = progress

    assert_operator after, :>, before
    result
  ensure
    running = false
    worker&.join
  end
end
