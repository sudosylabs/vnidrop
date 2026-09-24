# Homebrew cask for the direct-download (notarized .dmg) macOS builds.
#
# This file is the source template. The Apple release workflow substitutes the
# version and both architecture checksums, then pushes the result to the tap repo
# (sudosylabs/homebrew-vnidrop, path Casks/vnidrop.rb). Users then install with:
#   brew install --cask sudosylabs/vnidrop/vnidrop
#
# `auto_updates true` tells Homebrew that the app updates itself via Sparkle, so
# `brew upgrade` won't fight the in-app updater.
cask "vnidrop" do
  version "0.0.0"

  on_arm do
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    url "https://github.com/sudosylabs/vnidrop/releases/download/v#{version}/VniDrop-#{version}.dmg",
        verified: "github.com/sudosylabs/vnidrop/"
  end
  on_intel do
    sha256 "1111111111111111111111111111111111111111111111111111111111111111"
    url "https://github.com/sudosylabs/vnidrop/releases/download/v#{version}/VniDrop-#{version}-x86_64.dmg",
        verified: "github.com/sudosylabs/vnidrop/"
  end

  name "VniDrop"
  desc "Direct device-to-device file and folder transfer over the network"
  homepage "https://github.com/sudosylabs/vnidrop"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on macos: :sequoia

  app "VniDrop.app"

  zap trash: [
    "~/Library/Application Support/com.vnidrop.app",
    "~/Library/Caches/com.vnidrop.app",
    "~/Library/Preferences/com.vnidrop.app.plist",
  ]
end
