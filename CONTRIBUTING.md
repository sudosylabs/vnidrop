# Contributing to VniDrop

Thank you for helping improve VniDrop. Contributions may include bug reports,
feature proposals, documentation, tests, design feedback, and code.

By participating, you agree to follow the project
[Code of Conduct](CODE_OF_CONDUCT.md).

## Before You Start

- Search existing issues and pull requests before opening a duplicate.
- For a substantial feature or architecture change, open an issue first so the
  approach and platform impact can be discussed.
- Keep each change focused. Avoid unrelated refactors, dependency upgrades, or
  repository-wide formatting.
- Report suspected vulnerabilities through the private process in
  [`SECURITY.md`](SECURITY.md), never in a public issue with technical details.
- Never include secrets, private tickets, file contents, key material, or
  passphrases in an issue, log, test fixture, commit, or pull request.

## Development Setup

Clone the repository and create a branch from an up-to-date `master`:

```bash
git clone https://github.com/vnidrop/vnidrop.git
cd vnidrop
git switch master
git pull --ff-only
git switch -c feat/short-description
```

Use a branch name that describes the outcome, such as
`feat/folder-share`, `fix/cancel-export-hang`, or `docs/contributing`.

Install the tools needed for the area you plan to change:

- GNU Make and Bash for the root command interface
- JDK 17 or newer for Gradle and application builds
- Rust stable with `rustfmt` and Clippy for the transfer core
- Bun for localization generation
- Android SDK and NDK for Android builds
- Xcode, XcodeGen, and SwiftLint on macOS for native Apple builds and simulator tests
- .NET 10 SDK, Visual Studio C++ Build Tools, and the Windows SDK for WinUI
- GTK 4.10+, libadwaita 1.5+, and a session Secret Service for native Linux
- Node.js 22.12 or newer for the optional diagnostics service

The first Rust and Gradle builds may take several minutes while dependencies are
downloaded and native components are compiled.

Use the [Windows](windows/README.md), [Apple](apple/README.md), or
[native Linux](linux/README.md) guide for the complete host setup and run commands.

## Command Interface

Run Rust, Compose, Apple, native Linux, and JavaScript commands through the root
`Makefile`:

```bash
make help       # list commands
make doctor     # report missing host tools
make setup      # install repository-local JavaScript dependencies
make check      # portable Rust, shared, localization, docs, and service checks
```

Configuration can be passed on the command line, for example
`make package-deb VERSION=1.2.0`, or placed in an ignored
`config.override.mk`. Windows use requires GNU Make in a Bash environment; the
underlying Gradle and PowerShell entry points remain available when Make is not
installed.

## Repository Structure

| Path | Purpose |
|------|---------|
| `crates/vnidrop/` | Rust transfer core, persistence, approval, and streaming |
| `shared/` | Compose UI, ViewModels, and bridges for Android and Compose desktop hosts |
| `androidApp/` | Android application shell |
| `desktopApp/` | Retained legacy Windows/Linux Compose host |
| `apple/` | Native SwiftUI application and Rust/UniFFI integration for Apple platforms |
| `windows/` | Released Windows WinUI application and Rust/UniFFI C# integration |
| `linux/` | Linux release application using GTK/libadwaita |
| `localization/` | Source strings and generators for all four frontend targets |
| `services/diagnostics-api/` | Optional Cloudflare diagnostics service |

Read the nearest contributor guidance before editing:

- [`AGENTS.md`](AGENTS.md) contains repository-wide engineering rules.
- [`crates/vnidrop/AGENTS.md`](crates/vnidrop/AGENTS.md) covers the Rust core.
- [`shared/AGENTS.md`](shared/AGENTS.md) covers Compose and Kotlin
  Multiplatform work.

## Engineering Expectations

VniDrop follows a few important design constraints:

- File transfer payloads are streamed by Rust and should not be routed through
  the Kotlin heap as the primary design.
- Android directory sharing expands SAF trees into individual file descriptors;
  directory file descriptors are not passed to Rust.
- Approval and access checks must not be weakened for convenience.
- Receive publishing must preserve the existing no-overwrite behavior.
- Locks and synchronous guards must not be held across Rust `.await` points.
- Bug fixes require a regression test at the lowest layer that demonstrates the
  failure.

Match the style of nearby code. Comments should explain non-obvious invariants,
platform constraints, concurrency behavior, or design decisions instead of
restating the code.

## Testing

Run checks from the repository root. Choose the suite for the files you changed.

### Rust Core

```bash
make format
make test-rust
```

For cancel, export, or output-sink changes, also run:

```bash
make test-rust-output-sink
```

For broader core changes, run the complete workspace suite:

```bash
make check-rust
```

### Shared Kotlin and Compose

```bash
make test-shared
```

Platform-specific checks may also be appropriate:

```bash
make test-android-host
make check-android
```

### Native Apple App

```bash
make check-apple
```

Override the selected simulator when needed with
`make check-apple APPLE_DESTINATION='platform=iOS Simulator,name=iPhone 16'`.

### Native Windows App

From PowerShell:

```powershell
pwsh windows/scripts/build.ps1 -Test
```

Add `-Run` to launch the app and inspect affected screens. Use `-SkipCore` for
subsequent UI-only builds when the Rust core and generated bindings are current.

### Native Linux App

```bash
make check-linux
make test-linux-ui  # GTK changes; requires a graphical session or Xvfb
```

### Documentation

For Markdown edits, verify paths, links, and commands against the source.
For website changes under `docs/`, run `make check-docs`.

### Diagnostics Service

```bash
make check-diagnostics
```

If a required check cannot run on your machine, explain why in the pull request
and list the checks you did run.

## Commits

Use concise commit messages that describe the outcome. The repository commonly
uses Conventional Commit-style subjects:

```text
feat(core): add folder transfer metadata
fix(ui): preserve receive progress after rotation
docs: clarify desktop setup
```

Create signed commits when your repository configuration requires signing. Do
not bypass a signing requirement with an unsigned commit.

## Pull Requests

Open pull requests against `master`. A good pull request should:

1. Explain what changed and why.
2. Stay limited to one coherent outcome.
3. Link the relevant issue, when one exists.
4. Describe platform or compatibility implications.
5. Include regression coverage for bug fixes and behavior changes.
6. Provide an executable test plan with the exact commands or concrete manual
   scenarios used for verification.
7. Avoid generated files, unrelated formatting, and dependency changes unless
   they are required by the contribution.

Review feedback is part of the collaboration process. Keep follow-up commits
focused, and resolve review threads only after the concern has been addressed.

## Licensing

VniDrop is distributed under the [Apache License 2.0](LICENSE). Unless explicitly
stated otherwise, contributions accepted into this repository are distributed
under the same license.
