# frozen_string_literal: true

require "minitest/autorun"
require "tmpdir"
require "json"
require "pathname"
require "audiowaveform"

module AudioWaveformTestSupport
  FIXTURES = File.expand_path("../../../fixtures", __dir__)

  def fixture(name)
    File.join(FIXTURES, name)
  end

  def write_silence_wav(path, frame_count:)
    data_size = frame_count * 2
    header = [
      "RIFF", 36 + data_size, "WAVE", "fmt ", 16, 1, 1,
      16_000, 32_000, 2, 16, "data", data_size,
    ].pack("a4Va4a4VvvVVvva4V")

    File.binwrite(path, header + ("\0" * data_size))
  end
end
