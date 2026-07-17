# Releasing jcode

jcode has two release paths: a fast local path for hotfixes, and CI for full releases.

## Quick Release (local, ~2.5 minutes)

For hotfixes and urgent updates. Builds Linux + macOS locally and stages them on a draft release while CI completes the remaining platforms.

```bash
scripts/quick-release.sh v0.5.5                # Build + tag + release
scripts/quick-release.sh v0.5.5 "Fix bug"      # With custom title
scripts/quick-release.sh --dry-run v0.5.5       # Build only, don't publish
```

### How it works

1. Builds Linux x86_64 natively and macOS aarch64 via osxcross **in parallel**
2. Verifies both binaries (ELF and Mach-O checks)
3. Creates a git tag and pushes it (this also triggers CI for the Windows build and signing job)
4. Uploads both binaries to a draft GitHub Release
5. CI publishes after the required Linux/macOS builds pass; signed Windows and FreeBSD assets are included when available

### Prerequisites

Already set up on the dev laptop (xps13):

- **osxcross** at `~/.osxcross` with macOS 14.5 SDK (darwin triple: `aarch64-apple-darwin23.5`)
- **rustup** with `aarch64-apple-darwin` target installed
- **`~/.cargo/config.toml`** has the osxcross linker configured
- **`gh` CLI** authenticated with GitHub

### Timeline

```
0s     Start parallel builds (Linux native + macOS cross-compile)
~90s   Linux build finishes
~150s  macOS build finishes
~153s  Linux + macOS binaries attached to the draft release
~16m   CI finishes platform jobs and checksums
         ✅ Linux + macOS become public; optional successful platforms join them
```

## CI Release (automated, ~11 min Linux+macOS, ~16 min Windows)

Triggered automatically when a `v*` tag is pushed to GitHub.

### Workflow: `.github/workflows/release.yml`

```
Tag push (v*)
    │
    ├─► create-release
    │     └─► Create or update a hidden draft release
    │
    ├─► build-linux-macos (parallel)
    │     ├─► Linux x86_64   (ubuntu-latest)     ~8 min
    │     └─► macOS aarch64  (macos-latest)       ~11 min
    │
    ├─► build-windows (parallel)
    │     ├─► Windows x86_64 (windows-latest)     ~16 min
    │     └─► Windows ARM64 (windows-11-arm)      ~16 min
    │
    ├─► publish-windows (after both Windows builds)
    │     ├─► Sign x86_64 + ARM64 with Azure Artifact Signing
    │     ├─► Verify Authenticode signatures
    │     └─► Package and upload final Windows assets
    │
    └─► release (after platform jobs finish)
          ├─► Require Linux x86_64/aarch64 + macOS x86_64/aarch64
          ├─► Include Windows + FreeBSD when their jobs succeed
          ├─► Generate and upload SHA256SUMS
          ├─► Publish the available release assets
          ├─► Update Homebrew formula (1jehuang/homebrew-jcode)
          └─► Update AUR package (jcode-bin)
```

Key design decisions:
- **Linux and macOS are the required release core.** Their four architecture assets must all pass before publication.
- **Windows and FreeBSD are optional release platforms.** Failures remain visible in CI and are called out in the release notes, but do not hold back Linux/macOS users.
- **Windows executables must be signed before public upload.** Signing is required for Windows assets. `WINDOWS_SIGNING_REQUIRED=false` remains an explicit emergency override and is not suitable for an official Windows build.
- **Checksums describe exactly the assets published in that release.** Late or failed optional platforms are omitted instead of blocking unrelated platforms.
- **Shallow clones** (`fetch-depth: 1`) to minimize checkout time.
- **`CARGO_INCREMENTAL=0`** for CI (incremental adds overhead on clean CI builds).
- **sccache + rust-cache** for dependency caching across runs.
- **mold linker** on Linux for faster linking.

### Package manager updates

CI handles Homebrew and AUR updates automatically:

- **Homebrew**: Updates `Formula/jcode.rb` in `1jehuang/homebrew-jcode` with new SHA256 hashes
- **AUR**: Updates `PKGBUILD` and `.SRCINFO` in the `jcode-bin` AUR repo

Both are triggered by the final `release` job after the required Linux/macOS builds complete and optional platform jobs reach a terminal state.

### Windows signing prerequisites

The full one-time setup is documented in [docs/WINDOWS.md](docs/WINDOWS.md#enable-authenticode-signing). The release repository needs:

- Secrets: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`
- Variables: `WINDOWS_SIGNING_ENDPOINT`, `WINDOWS_SIGNING_ACCOUNT`, `WINDOWS_SIGNING_CERTIFICATE_PROFILE`
- Optional emergency override only: `WINDOWS_SIGNING_REQUIRED=false`

Before announcing Defender or SmartScreen remediation, download both Windows executables from the public release and confirm `Get-AuthenticodeSignature` reports `Valid`.

## Which to use

| Scenario | Method | Time to Linux+macOS | Time to Windows |
|----------|--------|-------------------|-----------------|
| Hotfix / urgent bug | `scripts/quick-release.sh` | ~16 min | ~16 min when Windows succeeds |
| Regular release | Push `v*` tag | ~11 min | ~16 min |
| Need Homebrew/AUR | Push `v*` tag | ~11 min | ~16 min |

The quick-release script reduces local build latency, but it deliberately leaves the release as a draft. The tag-triggered workflow publishes it after the required Linux/macOS and checksum gates pass. Windows and FreeBSD are attached only when their platform-specific gates succeed, then Homebrew and AUR are updated.

## Cross-Compilation Setup

macOS binaries are cross-compiled from Linux using [osxcross](https://github.com/tpoechtrager/osxcross).

### Current configuration

| Component | Value |
|-----------|-------|
| SDK | macOS 14.5 |
| SDK source | [joseluisq/macosx-sdks](https://github.com/joseluisq/macosx-sdks) |
| Install location | `~/.osxcross/` |
| Darwin triple | `aarch64-apple-darwin23.5` |
| Linker | `aarch64-apple-darwin23.5-clang` |

### Cargo config (`~/.cargo/config.toml`)

```toml
[target.aarch64-apple-darwin]
linker = "aarch64-apple-darwin23.5-clang"

[env]
CC_aarch64_apple_darwin = "aarch64-apple-darwin23.5-clang"
CXX_aarch64_apple_darwin = "aarch64-apple-darwin23.5-clang++"
```

### Rebuilding osxcross from scratch

```bash
git clone https://github.com/tpoechtrager/osxcross /tmp/osxcross
curl -L -o /tmp/osxcross/tarballs/MacOSX14.5.sdk.tar.xz \
  https://github.com/joseluisq/macosx-sdks/releases/download/14.5/MacOSX14.5.sdk.tar.xz
cd /tmp/osxcross && UNATTENDED=1 TARGET_DIR=~/.osxcross ./build.sh
rustup target add aarch64-apple-darwin
```

Build takes ~5 minutes. Requires `clang`, `cmake`, `libxml2` (all available via pacman on Arch).

### Why osxcross (not zigbuild)

`cargo-zigbuild` can cross-compile pure Rust code to macOS, but jcode depends on crates that link against macOS system frameworks:
- `arboard` (clipboard) - links `AppKit`, `Foundation`
- `native-tls` / `security-framework` - links `Security`, `SystemConfiguration`
- `objc2` - links Objective-C runtime

These require actual macOS SDK headers and framework stubs, which osxcross provides.

## Build Performance

### Current timing (laptop, 8-core Intel Ultra 7 256V)

| Build | Clean | Cached deps |
|-------|-------|-------------|
| Linux x86_64 (native) | ~90s | ~90s |
| macOS aarch64 (cross) | ~3 min | ~2.5 min |
| Both in parallel | ~3 min | ~2.5 min |

The bottleneck is compiling jcode itself (120k lines of Rust). Dependencies are cached and don't need recompilation. The `build.rs` timestamp causes a full recompile of the main crate on every build.

### Why not faster

- `opt-level = 1`, `codegen-units = 256`, `incremental = true` are already set in `[profile.release]`
- 8 cores is the hardware limit
- Splitting into workspace crates would allow partial recompilation (~1 min for small changes)
- A 20+ core machine on LAN (not Tailscale) would cut build time to ~40-50s
