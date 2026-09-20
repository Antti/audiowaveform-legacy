# frozen_string_literal: true

require_relative "audiowaveform/version"
require "rbconfig"

# Platform gems contain one extension per Ruby minor version. Source builds put
# the extension directly under audiowaveform/.
native_extension = File.join(
  __dir__, "audiowaveform", RUBY_VERSION[/\A\d+\.\d+/],
  "audiowaveform_ruby.#{RbConfig::CONFIG.fetch('DLEXT')}"
)
if File.file?(native_extension)
  require native_extension
else
  require "audiowaveform/audiowaveform_ruby"
end

module AudioWaveform
  class << self
    # Generates waveform data from an audio file.
    def generate(
      input,
      samples_per_pixel: nil,
      pixels_per_second: nil,
      points: nil,
      split_channels: false,
      amplitude_scale: nil
    )
      scale_kind, scale_value = resolve_scale(samples_per_pixel, pixels_per_second, points)
      amplitude_kind, amplitude_value = resolve_amplitude_scale(amplitude_scale)

      Native.generate(
        File.path(input),
        scale_kind,
        scale_value,
        !!split_channels,
        amplitude_kind,
        amplitude_value
      )
    end

    private

    def resolve_scale(samples_per_pixel, pixels_per_second, points)
      if [samples_per_pixel, pixels_per_second, points].count { |value| !value.nil? } > 1
        raise ArgumentError, "samples_per_pixel, pixels_per_second, and points are mutually exclusive"
      end

      unless points.nil?
        value = positive_integer(points, :points)
        raise ArgumentError, "points must be at most 4294967295" if value > 0xffff_ffff

        return ["points", value]
      end

      if pixels_per_second
        ["pixels_per_second", positive_integer(pixels_per_second, :pixels_per_second)]
      else
        value = samples_per_pixel || 256
        ["samples_per_pixel", positive_integer(value, :samples_per_pixel, minimum: 2)]
      end
    end

    def resolve_amplitude_scale(value)
      return ["none", 0.0] if value.nil?
      return ["auto", 0.0] if value == :auto || value == "auto"

      numeric = Float(value)
      unless numeric.finite? && numeric >= 0.0
        raise ArgumentError, "amplitude_scale must be a finite non-negative number or :auto"
      end

      ["fixed", numeric]
    rescue TypeError, ArgumentError
      raise ArgumentError, "amplitude_scale must be a finite non-negative number or :auto"
    end

    def positive_integer(value, name, minimum: 1)
      unless value.is_a?(Integer) && value >= minimum
        raise ArgumentError, "#{name} must be an integer greater than or equal to #{minimum}"
      end

      value
    end
  end

  class Waveform
    private_class_method :new

    alias size length
    alias bits storage_bits
    alias duration_seconds duration

    # Returns interleaved [minimum, maximum, ...] values at 8 or 16 bits.
    def data(bits: 16)
      __data(validate_bits(bits))
    end

    # Returns the [minimum, maximum] pair at +index+ for +channel+.
    def point(index, channel: 0)
      unless index.is_a?(Integer) && index.between?(0, length - 1) &&
          channel.is_a?(Integer) && channel.between?(0, channels - 1)
        raise IndexError, "waveform point is outside the available channel or index range"
      end

      value = __point(channel, index)
      return value if value

      raise IndexError, "waveform point is outside the available channel or index range"
    end

    # Writes waveform data to +path+ and returns self.
    def save(path, format: nil, bits: storage_bits)
      resolved_format = format || File.extname(File.path(path)).delete_prefix(".")
      __save(File.path(path), resolved_format.to_s, validate_bits(bits))
      self
    end

    # Returns binary DAT waveform data.
    def to_dat(bits: storage_bits)
      __serialize("dat", validate_bits(bits))
    end

    # Returns JSON waveform data.
    def to_json(*, bits: storage_bits)
      __serialize("json", validate_bits(bits)).force_encoding(Encoding::UTF_8)
    end

    # Returns plain-text waveform data.
    def to_txt(bits: storage_bits)
      __serialize("txt", validate_bits(bits)).force_encoding(Encoding::UTF_8)
    end

    private

    def validate_bits(bits)
      return bits if bits.is_a?(Integer) && (bits == 8 || bits == 16)

      raise ArgumentError, "bits must be either 8 or 16"
    end
  end

  private_constant :Native
end
