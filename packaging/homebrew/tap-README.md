# homebrew-vnidrop

Homebrew tap for [VniDrop](https://github.com/sudosylabs/vnidrop) — direct,
private device-to-device file and folder transfer for macOS.

> This repo only holds the Homebrew **cask**. The app itself lives at
> [sudosylabs/vnidrop](https://github.com/sudosylabs/vnidrop). The cask here is
> updated automatically by VniDrop's release pipeline on each tagged release.

## Install

```sh
brew tap sudosylabs/vnidrop
brew install --cask vnidrop
```

Or in one line:

```sh
brew install --cask sudosylabs/vnidrop/vnidrop
```

## Update

VniDrop updates itself in-app via [Sparkle](https://sparkle-project.org), so you
normally don't need to do anything. To update through Homebrew instead:

```sh
brew upgrade --cask vnidrop
```

## Uninstall

```sh
brew uninstall --cask vnidrop
```

Add `--zap` to also remove VniDrop's application support, cache, and preference
files:

```sh
brew uninstall --zap --cask vnidrop
```

## Requirements

- macOS 15 (Sequoia) or later, Apple Silicon.

## What you get

The cask installs the Developer ID–signed, notarized `VniDrop.app` from the
matching [GitHub Release](https://github.com/sudosylabs/vnidrop/releases). App
Store users should install from the Mac App Store instead — that build does not
include the Sparkle self-updater.
