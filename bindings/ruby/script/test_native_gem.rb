# frozen_string_literal: true

require "rubygems/package"
require "tmpdir"
require "rbconfig"

repository = File.expand_path("../../..", __dir__)
packages = ARGV.empty? ? Dir[File.join(repository, "pkg/*.gem")] : ARGV
abort "Expected one native gem, found #{packages.length}" unless packages.length == 1
package = File.expand_path(packages.fetch(0))
spec = Gem::Package.new(package).spec
abort "Expected a precompiled gem" if spec.platform == Gem::Platform::RUBY || !spec.extensions.empty?
abort "Precompiled gem must not need build dependencies" unless spec.dependencies.empty?

Dir.mktmpdir("audiowaveform-install") do |directory|
  # Keep the checkout, Bundler, and globally installed gems out of the load path.
  environment = ENV.keys.grep(/\ABUNDLE_/).to_h { |name| [name, nil] }.merge(
    "GEM_HOME" => directory, "GEM_PATH" => directory, "RUBYOPT" => nil, "RUBYLIB" => nil
  )
  system(environment, RbConfig.ruby, "-S", "gem", "install", "--local", "--no-document",
    "--install-dir", directory, package, exception: true)
  system(environment, RbConfig.ruby, "-e", <<~'RUBY', repository, spec.version.to_s, exception: true)
    require "audiowaveform"
    require "json"
    require "tmpdir"
    repository, version = ARGV
    abort "Wrong installed version" unless AudioWaveform::VERSION == version
    loaded = $LOADED_FEATURES.find { |path| path.match?(%r{/audiowaveform_ruby\.(bundle|so)\z}) }
    gem_root = Gem.loaded_specs.fetch("audiowaveform").full_gem_path
    abort "Loaded an extension from outside the installed gem" unless loaded&.start_with?(gem_root + "/")
    Dir[File.join(repository, "fixtures/formats/*")].sort.each do |path|
      next if File.extname(path) == ".py" || File.basename(path) == "video-only.mp4"
      waveform = AudioWaveform.generate(path)
      abort "Empty waveform: #{path}" if waveform.empty?
    end
    waveform = AudioWaveform.generate(File.join(repository, "fixtures/test_file_stereo.wav"), split_channels: true)
    abort "Wrong metadata" unless waveform.sample_rate == 16_000 && waveform.channels == 2
    abort "Invalid serialization" unless JSON.parse(waveform.to_json).fetch("data") == waveform.data
    exact = AudioWaveform.generate(File.join(repository, "fixtures/formats/stereo.wav"), points: 110)
    abort "Wrong point count or duration" unless exact.length == 110 && exact.duration == 0.25
    peaks = exact.data(bits: 8)
    abort "Invalid direct 8-bit peaks" unless peaks.length == 220 && peaks.all? { |value| value.between?(-128, 127) }
    abort "8-bit peaks differ from serialization" unless peaks == JSON.parse(exact.to_json(bits: 8)).fetch("data")
    fragmented = AudioWaveform.generate(File.join(repository, "fixtures/formats/fragmented.mp4"), points: 110)
    abort "Durationless container failed" unless fragmented.length == 110 && fragmented.duration.positive?
    Dir.mktmpdir do |directory|
      path = File.join(directory, "waveform.dat")
      waveform.save(path)
      abort "Invalid saved waveform" unless File.binread(path) == waveform.to_dat
    end
    puts "Installed gem works on Ruby #{RUBY_VERSION} (#{Gem::Platform.local})"
  RUBY
end
