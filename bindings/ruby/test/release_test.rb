# frozen_string_literal: true

require "minitest/autorun"
require "tmpdir"
require "fileutils"
require "open3"
require_relative "../script/publish_gems"
require_relative "../lib/audiowaveform/version"

class ReleaseTest < Minitest::Test
  def test_retry_skips_only_the_same_platform_and_checksum
    with_artifact do |spec, path|
      body = {"sha" => Digest::SHA256.file(path).hexdigest, "platform" => "ruby", "yanked" => false}
      assert GemPublishing.already_published?(spec, path, http: response(200, body))
      refute GemPublishing.already_published?(spec, path, http: response(404))

      [{"sha" => "different"}, {"platform" => "x86_64-linux-gnu"}, {"yanked" => true}].each do |change|
        assert_raises(RuntimeError) do
          GemPublishing.already_published?(spec, path, http: response(200, body.merge(change)))
        end
      end
    end
  end

  def test_lookup_failures_do_not_trigger_a_new_upload
    with_artifact do |spec, path|
      [401, 429, 500].each do |code|
        assert_raises(RuntimeError) do
          GemPublishing.already_published?(spec, path, http: response(code))
        end
      end
    end
  end

  def test_native_release_requires_every_supported_ruby_abi
    Dir.mktmpdir do |directory|
      build_package(directory, rubies: %w[3.2 3.3 3.4 4.0])
      output, status = check(directory, "x86_64-linux")
      assert status.success?, output
      output, status = check(directory)
      refute status.success?
      assert_includes output, "Expected 8 gems"
    end

    Dir.mktmpdir do |directory|
      build_package(directory, rubies: %w[3.4])
      output, status = check(directory, "x86_64-linux")
      refute status.success?
      assert_includes output, "Incorrect Ruby ABI binaries"
    end
  end

  def test_native_release_rejects_install_time_compilation
    Dir.mktmpdir do |directory|
      build_package(directory, rubies: %w[3.2 3.3 3.4 4.0], extensions: ["extconf.rb"])
      output, status = check(directory, "x86_64-linux")
      refute status.success?
      assert_includes output, "would compile during installation"
    end
  end

  private

  def with_artifact
    Dir.mktmpdir do |directory|
      path = File.join(directory, "artifact.gem")
      File.write(path, "release artifact")
      spec = Gem::Specification.new("audiowaveform", AudioWaveform::VERSION)
      yield spec, path
    end
  end

  def response(code, body = {})
    result = Net::HTTPResponse::CODE_TO_OBJ.fetch(code.to_s).new("1.1", code.to_s, "test")
    result.define_singleton_method(:body) { JSON.generate(body) }
    connection = Object.new
    connection.define_singleton_method(:get) { |_path| result }
    http = Object.new
    http.define_singleton_method(:start) { |*args, **options, &block| block.call(connection) }
    http
  end

  def build_package(directory, rubies:, extensions: [])
    Dir.chdir(directory) do
      files = ["COPYING", "sig/audiowaveform.rbs", *extensions] + rubies.map do |version|
        "bindings/ruby/lib/audiowaveform/#{version}/audiowaveform_ruby.so"
      end
      files.each do |path|
        FileUtils.mkdir_p(File.dirname(path))
        File.write(path, "fixture")
      end
      spec = Gem::Specification.new("audiowaveform", AudioWaveform::VERSION) do |gem|
        gem.summary = "Release validation fixture"
        gem.authors = ["Test"]
        gem.license = "GPL-3.0-or-later"
        gem.homepage = "https://github.com/Antti/audiowaveform"
        gem.platform = "x86_64-linux-gnu"
        gem.required_ruby_version = [">= 3.2", "< 4.1.dev"]
        gem.files = files
        gem.extensions = extensions
      end
      capture_io { Gem::Package.build(spec) }
    end
  end

  def check(directory, *platform)
    Open3.capture2e(RbConfig.ruby, File.expand_path("../script/check_release.rb", __dir__), directory, *platform)
  end
end
