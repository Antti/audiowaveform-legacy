# frozen_string_literal: true

require_relative "test_helper"
require "open3"
require "rbconfig"

class NativeSafetyTest < Minitest::Test
  include AudioWaveformTestSupport

  def test_save_raises_when_the_final_buffered_write_fails
    skip "file-size limits are unavailable on Windows" if Gem.win_platform?

    Dir.mktmpdir do |directory|
      assert_ruby_success(<<~RUBY, fixture("test_file_mono.wav"), directory)
        require "audiowaveform"
        waveform = AudioWaveform.generate(ARGV.fetch(0))
        Signal.trap("XFSZ", "IGNORE")
        Process.setrlimit(:FSIZE, 0, 0)

        %w[dat json txt].each do |format|
          path = File.join(ARGV.fetch(1), "waveform.\#{format}")
          begin
            waveform.save(path)
          rescue AudioWaveform::Error
            next
          end
          abort "save silently accepted a failed \#{format} write"
        end
      RUBY
    end
  end

  def test_rbs_accepts_string_and_pathname_paths
    Dir.mktmpdir do |directory|
      signatures = File.expand_path("../../../sig", __dir__)
      assert_ruby_success(<<~RUBY, fixture("test_file_mono.wav"), directory, signatures)
        require "shellwords"
        ENV["RBS_TEST_TARGET"] = "AudioWaveform,AudioWaveform::Waveform"
        ENV["RBS_TEST_OPT"] = ["-I", ARGV.fetch(2)].shelljoin
        ENV["RBS_TEST_LOGLEVEL"] = "error"
        require "rbs/test/setup"
        require "audiowaveform"
        require "pathname"

        [ARGV.fetch(0), Pathname(ARGV.fetch(0))].each do |input|
          waveform = AudioWaveform.generate(input)
          path = File.join(ARGV.fetch(1), "waveform.dat")
          waveform.save(path)
          waveform.save(Pathname(path))
        end
      RUBY
    end
  end

  def test_native_waveform_allocations_are_visible_to_ruby_gc
    with_long_wav do |path|
      assert_ruby_success(<<~RUBY, path)
        require "audiowaveform"
        require "objspace"
        GC.start
        GC.disable
        before = GC.stat(:malloc_increase_bytes)
        waveform = AudioWaveform.generate(ARGV.fetch(0), samples_per_pixel: 2)
        increase = GC.stat(:malloc_increase_bytes) - before
        sample_bytes = waveform.length * waveform.channels * 4
        abort "native allocations were not reported to GC: \#{increase}" if increase < sample_bytes
        abort "ObjectSpace excludes the native buffer" if ObjectSpace.memsize_of(waveform) < sample_bytes

        waveform = nil
        GC.enable
        GC.start
        before = GC.count
        12.times do
          AudioWaveform.generate(ARGV.fetch(0), samples_per_pixel: 2)
          # Let MRI evaluate malloc pressure at a Ruby-managed allocation.
          # Reusing existing Ruby heap slots need not trigger that check.
          Array.new(128)
        end
        abort "discarded native buffers did not trigger GC" if GC.count == before
      RUBY
    end
  end

  def test_interrupted_serialization_reclaims_native_results
    with_long_wav do |path|
      assert_ruby_success(<<~RUBY, path)
        require "audiowaveform"
        require "timeout"
        waveform = AudioWaveform.generate(ARGV.fetch(0), samples_per_pixel: 2)
        # Start Timeout's helper thread before measuring allocations.
        begin
          Timeout.timeout(0.001) { sleep 0.1 }
        rescue Timeout::Error
        end
        GC.start
        GC.disable
        before = GC.stat(:malloc_increase_bytes)
        8.times do
          begin
            Timeout.timeout(0.001) { waveform.to_json }
            abort "serialization completed before the interrupt"
          rescue Timeout::Error
          end
        end
        increase = GC.stat(:malloc_increase_bytes) - before
        abort "interrupted serialization leaked \#{increase} bytes" if increase > 2_000_000
      RUBY
    end
  end

  private

  def with_long_wav
    Dir.mktmpdir do |directory|
      path = File.join(directory, "long.wav")
      write_silence_wav(path, frame_count: 5_000_000)
      yield path
    end
  end

  def assert_ruby_success(script, *arguments)
    output, status = Open3.capture2e(
      RbConfig.ruby, "-I", File.expand_path("../lib", __dir__), "-e", script, *arguments
    )
    assert status.success?, output
  end
end
