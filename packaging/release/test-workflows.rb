require "fileutils"
require "digest"
require "minitest/autorun"
require "open3"
require "tmpdir"
require "yaml"

class ReleaseWorkflowsTest < Minitest::Test
  ROOT = File.expand_path("../..", __dir__)

  def workflow(name)
    YAML.load_file(File.join(ROOT, ".github/workflows/#{name}.yml"))
  end

  def step(job, name)
    job.fetch("steps").find { |entry| entry["name"] == name } || flunk("Missing step: #{name}")
  end

  def run_command(env, *command, directory:)
    output, status = Open3.capture2e(env, *command, chdir: directory)
    assert status.success?, "#{command.join(' ')} failed: #{output}"
    output.strip
  end

  def test_store_failures_do_not_block_other_publication_destinations
    jobs = workflow("release").fetch("jobs")
    builds = %w[preflight linux windows macos android]
    %w[play-closed-testing publish-microsoft-store apple-appstore].each do |name|
      assert_equal builds.sort, jobs.fetch(name).fetch("needs").sort, name
    end
    assert_equal %w[preflight linux windows macos play-closed-testing].sort,
      jobs.fetch("publish-github").fetch("needs").sort
    assert_equal "play-closed-testing", jobs.fetch("play-closed-testing").fetch("environment")
    assert_equal "microsoft-store", jobs.fetch("publish-microsoft-store").fetch("environment")
    assert_equal "apple-appstore", workflow("apple-appstore").fetch("jobs").fetch("appstore").fetch("environment")
  end

  def test_apple_core_and_app_check_out_the_selected_revision
    jobs = workflow("apple-appstore").fetch("jobs")
    assert_equal "${{ inputs.release_tag != '' && format('refs/tags/{0}', inputs.release_tag) || github.sha }}",
      step(jobs.fetch("core"), "Checkout").fetch("with").fetch("ref")
    assert_equal "${{ needs.core.outputs.source_sha }}",
      step(jobs.fetch("appstore"), "Checkout").fetch("with").fetch("ref")
    assert_equal "${{ steps.version.outputs.source_sha }}", jobs.fetch("core").fetch("outputs").fetch("source_sha")
  end

  def test_release_reuses_its_own_core_without_waiting_for_github_publication
    apple = workflow("release").fetch("jobs").fetch("apple-appstore")
    assert_equal "vnidrop-${{ needs.preflight.outputs.version }}-macos-dmg", apple.fetch("with").fetch("core_artifact")
    core = workflow("apple-appstore").fetch("jobs").fetch("core")
    assert_equal "steps.plan.outputs.release_tag == ''", step(core, "Build the core").fetch("if")
    assert_equal "steps.plan.outputs.release_tag != '' && inputs.core_artifact == ''",
      step(core, "Download prebuilt core from the release").fetch("if")
    assert_equal "${{ inputs.core_artifact }}", step(core, "Download core from this release run").fetch("with").fetch("name")
  end

  def test_android_ci_retains_the_checks_moved_out_of_release_packaging
    job = workflow("shared-kmp").fetch("jobs").fetch("jvm-test")
    assert_equal "./gradlew :androidApp:check --no-daemon --stacktrace",
      step(job, "Check Android app and native libraries").fetch("run")
    assert_equal "python3 packaging/android/tests/test_diagnostics_config.py",
      step(job, "Verify bug-report build configuration").fetch("run")
  end

  def test_release_tag_remains_valid_after_master_advances
    Dir.mktmpdir("vnidrop-release-tag") do |directory|
      env = {"GIT_CONFIG_GLOBAL" => File.join(directory, "gitconfig"), "GIT_CONFIG_NOSYSTEM" => "1"}
      run_command(env, "git", "init", "-q", directory: directory)
      run_command(env, "git", "config", "user.name", "Release test", directory: directory)
      run_command(env, "git", "config", "user.email", "release-test@example.invalid", directory: directory)
      run_command(env, "git", "commit", "--allow-empty", "-qm", "release", directory: directory)
      release_sha = run_command(env, "git", "rev-parse", "HEAD", directory: directory)
      run_command(env, "git", "commit", "--allow-empty", "-qm", "next merge", directory: directory)
      run_command(env, "git", "update-ref", "refs/remotes/origin/master", "HEAD", directory: directory)
      FileUtils.mkdir_p(File.join(directory, "packaging/version"))
      FileUtils.cp(File.join(ROOT, "packaging/version/resolve-version.sh"), File.join(directory, "packaging/version"))
      FileUtils.chmod(0755, File.join(directory, "packaging/version/resolve-version.sh"))
      File.write(File.join(directory, "version.properties"), "PRODUCT_VERSION=0.3.3\nRELEASE_CHANNEL=beta\nWINDOWS_VERSION_EPOCH=1\n")
      env.merge!("GITHUB_SHA" => release_sha, "GITHUB_REF_TYPE" => "tag", "GITHUB_REF_NAME" => "v0.3.3",
        "GITHUB_OUTPUT" => File.join(directory, "outputs"))
      script = step(workflow("release").fetch("jobs").fetch("preflight"), "Verify canonical beta tag on master").fetch("run")
      run_command(env, "bash", "-eu", "-o", "pipefail", "-c", script, directory: directory)
      assert_equal "app=0.3.3\nandroid_code=3003\n", File.read(env.fetch("GITHUB_OUTPUT"))

      output, status = Open3.capture2e(env.merge("GITHUB_REF_NAME" => "v0.3.4"), "bash", "-c", script, chdir: directory)
      refute status.success?
      assert_includes output, "Release tag must be v0.3.3"

      run_command(env, "git", "checkout", "-q", release_sha, directory: directory)
      run_command(env, "git", "commit", "--allow-empty", "-qm", "unmerged", directory: directory)
      env["GITHUB_SHA"] = run_command(env, "git", "rev-parse", "HEAD", directory: directory)
      output, status = Open3.capture2e(env, "bash", "-c", script, chdir: directory)
      refute status.success?
      assert_includes output, "Release tags must point to a commit on master"
    end
  end

  def test_missing_release_core_fails_instead_of_rebuilding
    Dir.mktmpdir("vnidrop-missing-core") do |directory|
      File.binwrite(File.join(directory, "gh"), "#!/usr/bin/env bash\nexit 1\n")
      FileUtils.chmod(0755, File.join(directory, "gh"))
      env = {"PATH" => "#{directory}#{File::PATH_SEPARATOR}#{ENV.fetch('PATH')}",
        "RUNNER_TEMP" => directory, "VERSION" => "0.3.3", "TAG" => "v0.3.3", "GITHUB_REPOSITORY" => "example/vnidrop"}
      script = step(workflow("apple-appstore").fetch("jobs").fetch("core"), "Download prebuilt core from the release").fetch("run")
      _, status = Open3.capture2e(env, "bash", "-c", script)
      refute status.success?, "A failed core download must stop the release"
    end
  end

  def test_prebuilt_core_checksum_is_required_and_verified
    Dir.mktmpdir("vnidrop-core-checksum") do |directory|
      FileUtils.mkdir_p(File.join(directory, "core"))
      env = {"RUNNER_TEMP" => directory, "VERSION" => "0.3.3"}
      script = step(workflow("apple-appstore").fetch("jobs").fetch("core"), "Verify prebuilt core").fetch("run")
      _, status = Open3.capture2e(env, "bash", "-c", script)
      refute status.success?, "A missing checksum must fail"
      archive = File.join(directory, "core/VnidropCore-0.3.3.zip")
      File.write(archive, "verified core")
      File.binwrite("#{archive}.sha256", "#{Digest::SHA256.file(archive).hexdigest}  #{File.basename(archive)}\n")
      run_command(env, "bash", "-c", script, directory: directory)
      File.write(archive, "tampered core")
      _, status = Open3.capture2e(env, "bash", "-c", script)
      refute status.success?, "A changed core must fail checksum verification"
    end
  end
end
