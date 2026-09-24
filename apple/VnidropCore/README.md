# VnidropCore (Swift package)

Swift bindings for the VniDrop Rust transfer core (`crates/vnidrop`), generated
with UniFFI in library mode. The Rust crate is never modified for this — the
Swift surface is produced from the compiled staticlib.

## Regenerate

From the repository root:

```bash
apple/scripts/build-core.sh release   # or: debug
```

This builds `libvnidrop.a` for `aarch64-apple-ios` and `aarch64-apple-darwin`,
generates `Sources/VnidropCore/Vnidrop.swift`, and assembles `vnidrop.xcframework`.
Debug builds also include the `aarch64-apple-ios-sim` and `x86_64-apple-ios`
simulator slices. Set `VNIDROP_APPLE_SIMULATOR=1` to include them in a release build.
Set `VNIDROP_APPLE_INTEL=1` to add a separate `x86_64-apple-darwin` slice. The
direct-download script sets that so the Intel disk image can link it. The two
macOS libraries stay separate slices; they are not combined into one universal archive.

## Generated / ignored artifacts

- `vnidrop.xcframework/`
- `Sources/VnidropCore/Vnidrop.swift`

Both are gitignored. A clean checkout must run the build script before opening
the Xcode project.
