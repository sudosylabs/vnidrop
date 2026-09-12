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
    assert_equal %w[preflight android].sort, jobs.fetch("play-closed-testing").fetch("needs").sort
    assert_equal %w[preflight windows].sort, jobs.fetch("publish-microsoft-store").fetch("needs").sort
    assert_equal %w[preflight macos].sort, jobs.fetch("apple-appstore").fetch("needs").sort
    assert_equal %w[preflight linux windows macos android play-closed-testing].sort,
      jobs.fetch("publish-github").fetch("needs").sort
    assert_equal "play-closed-testing", jobs.fetch("play-closed-testing").fetch("environment")
    assert_equal "microsoft-store", jobs.fetch("publish-microsoft-store").fetch("environment")
    assert_equal "apple-appstore", workflow("apple-appstore").fetch("jobs").fetch("appstore").fetch("environment")
  end

  def test_apple_core_and_app_check_out_the_selected_revision
    jobs = workflow("apple-appstore").fetch("jobs")
    assert_equal "${{ inputs.source_ref || (inputs.release_tag != '' && format('refs/tags/{0}', inputs.release_tag)) || github.sha }}",
      step(jobs.fetch("core"), "Checkout").fetch("with").fetch("ref")
    assert_equal "${{ needs.core.outputs.source_sha }}",
      step(jobs.fetch("appstore"), "Checkout").fetch("with").fetch("ref")
    assert_equal "${{ steps.version.outputs.source_sha }}", jobs.fetch("core").fetch("outputs").fetch("source_sha")
  end

  def test_release_reuses_its_own_core_without_waiting_for_github_publication
    apple = workflow("release").fetch("jobs").fetch("apple-appstore")
    assert_equal "${{ needs.macos.result == 'success' && format('vnidrop-{0}-macos-dmg', needs.preflight.outputs.version) || '' }}",
      apple.fetch("with").fetch("core_artifact")
    assert_equal "${{ needs.macos.result == 'skipped' }}", apple.fetch("with").fetch("build_core")
    assert_equal "${{ needs.preflight.outputs.apple_platforms }}", apple.fetch("with").fetch("platforms")
    core = workflow("apple-appstore").fetch("jobs").fetch("core")
    assert_equal "steps.plan.outputs.release_tag == '' || inputs.build_core", step(core, "Build the core").fetch("if")
    assert_equal "steps.plan.outputs.release_tag != '' && inputs.core_artifact == '' && !inputs.build_core",
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

  def test_preview_is_manual_and_only_publishes_direct_packages
    preview = workflow("preview-release")
    assert_equal ["workflow_dispatch"], preview.fetch(true).keys
    assert_equal %w[windows linux android macos].sort,
      preview.fetch(true).fetch("workflow_dispatch").fetch("inputs").keys.sort
    jobs = preview.fetch("jobs")
    assert_equal %w[preflight linux windows macos android publish].sort, jobs.keys.sort
    assert_equal %w[preflight linux windows macos android].sort, jobs.fetch("publish").fetch("needs").sort
    assert_equal({"direct_only" => true}, jobs.fetch("windows").fetch("with"))
    assert_equal({"preview" => true}, jobs.fetch("macos").fetch("with"))
    assert_equal({"preview_number" => "${{ needs.preflight.outputs.number }}"}, jobs.fetch("android").fetch("with"))
    publish = step(jobs.fetch("publish"), "Publish preview").fetch("run")
    assert_includes publish, "--draft --prerelease --latest=false"
    assert_includes publish, '--target "$GITHUB_SHA"'
    assert_includes publish, "--draft=false --prerelease --latest=false"
    refute_equal workflow("release").fetch("concurrency").fetch("group"), preview.fetch("concurrency").fetch("group")
    %w[android-release apple-release windows-store linux-packages].each do |name|
      assert_includes workflow(name).fetch("concurrency").fetch("group"), "${{ github.workflow }}"
    end
  end

  def test_normal_releases_are_manual_and_selected_jobs_can_be_skipped
    release = workflow("release")
    assert_equal ["workflow_dispatch"], release.fetch(true).keys
    inputs = release.fetch(true).fetch("workflow_dispatch").fetch("inputs")
    assert_equal %w[windows linux android macos ios distribution release_tag].sort, inputs.keys.sort
    jobs = release.fetch("jobs")
    %w[windows linux android macos].each do |platform|
      assert_equal "needs.preflight.outputs.#{platform} == 'true'", jobs.fetch(platform).fetch("if")
      assert_equal "${{ needs.preflight.outputs.source_sha }}", jobs.fetch(platform).fetch("with").fetch("source_ref")
    end
    condition = jobs.fetch("publish-github").fetch("if")
    assert_includes condition, "!cancelled()"
    assert_includes condition, "needs.preflight.result == 'success'"
    assert_includes condition, "!contains(needs.*.result, 'failure')"
    assert_includes condition, "!contains(needs.*.result, 'cancelled')"
    assert_includes jobs.fetch("apple-appstore").fetch("if"), "needs.macos.result == 'skipped'"
    publish = step(jobs.fetch("publish-github"), "Create GitHub Release").fetch("run")
    assert_includes publish, "--verify-tag --draft"
    assert_includes publish, "--draft=false --latest"
  end

  def test_preview_options_leave_normal_release_defaults_intact
    apple = workflow("apple-release")
    assert_equal false, apple.fetch(true).fetch("workflow_call").fetch("inputs").fetch("preview").fetch("default")
    ["Download Sparkle tools", "Write Sparkle signing key", "Package prebuilt core", "Generate appcast"].each do |name|
      assert_equal "${{ !inputs.preview }}", step(apple.fetch("jobs").fetch("build"), name).fetch("if")
    end
    windows = workflow("windows-store")
    assert_equal false, windows.fetch(true).fetch("workflow_call").fetch("inputs").fetch("direct_only").fetch("default")
    assert_equal "${{ !inputs.direct_only }}",
      step(windows.fetch("jobs").fetch("build-msix"), "Create and validate Store artifacts").fetch("if")
    android = workflow("android-release")
    assert_equal "", android.fetch(true).fetch("workflow_call").fetch("inputs").fetch("preview_number").fetch("default")
    signing = step(android.fetch("jobs").fetch("build"), "Build and verify signed release").fetch("env")
    assert_equal "${{ secrets[inputs.preview_number != '' && 'ANDROID_PREVIEW_KEY_PASSWORD' || 'ANDROID_UPLOAD_KEY_PASSWORD'] }}",
      signing.fetch("VNIDROP_ANDROID_KEY_PASSWORD")
  end

  def test_preview_identity_never_matches_a_normal_release_tag
    script = step(workflow("preview-release").fetch("jobs").fetch("preflight"), "Resolve preview identity").fetch("run")
    Dir.mktmpdir("vnidrop-preview-id") do |directory|
      env = {"GITHUB_REF" => "refs/heads/master", "GITHUB_REF_TYPE" => "branch", "GITHUB_RUN_NUMBER" => "17",
        "GITHUB_OUTPUT" => File.join(directory, "outputs")}
      run_command(env, "bash", "-c", script, directory: ROOT)
      outputs = File.read(env.fetch("GITHUB_OUTPUT")).lines.to_h { |line| line.strip.split("=", 2) }
      assert_equal "17", outputs.fetch("number")
      assert_equal "preview-#{outputs.fetch('version')}-17", outputs.fetch("tag")
      refute File.fnmatch?("v*.*.*", outputs.fetch("tag"))
      [{"GITHUB_REF" => "refs/heads/feature"}, {"GITHUB_REF" => "refs/tags/v0.3.3"},
       {"GITHUB_RUN_NUMBER" => "0"}, {"GITHUB_RUN_NUMBER" => "2100000001"}].each do |invalid|
        _, status = Open3.capture2e(env.merge(invalid), "bash", "-c", script, chdir: ROOT)
        refute status.success?, "Invalid preview dispatch must fail: #{invalid}"
      end
    end
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
      env.merge!("GITHUB_SHA" => release_sha, "GITHUB_REF_TYPE" => "branch", "GITHUB_REF" => "refs/heads/master", "RELEASE_TAG" => "v0.3.3",
        "GITHUB_OUTPUT" => File.join(directory, "outputs"))
      run_command(env, "git", "checkout", "-q", release_sha, directory: directory)
      script = step(workflow("release").fetch("jobs").fetch("preflight"), "Verify canonical beta tag on master").fetch("run")
      run_command(env, "bash", "-eu", "-o", "pipefail", "-c", script, directory: directory)
      assert_equal "source_sha=#{release_sha}\napp=0.3.3\nandroid_code=3003\n", File.read(env.fetch("GITHUB_OUTPUT"))

      output, status = Open3.capture2e(env.merge("RELEASE_TAG" => "v0.3.4"), "bash", "-c", script, chdir: directory)
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
