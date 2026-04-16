# Build And Release

## Local Builds

Build the full workspace in debug mode:

```bash
cargo build --workspace
```

Build only the desktop application in debug mode:

```bash
cargo build -p synchrosonic-app
```

Build the release desktop binary:

```bash
cargo build --release -p synchrosonic-app
```

If you want release builds for every crate, use:

```bash
cargo build --release --workspace
```

## Artifact Paths

Important output paths:

- debug app binary: `target/debug/synchrosonic-app`
- release app binary: `target/release/synchrosonic-app`
- staged packaging root: `target/release-packaging/`

The packaging script creates tarballs in `target/release-packaging/` with names
like:

- `synchrosonic-<version>-linux-<arch>.tar.gz`
- `synchrosonic-<version>-linux-<arch>-AppDir.tar.gz`
- `synchrosonic-<version>-linux-<arch>-deb-layout.tar.gz`

## Linux Packaging Workflow

Build and package in one step:

```bash
bash scripts/package-linux.sh
```

Or build first, then package without rebuilding:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

The packaging script stages these layouts:

- `target/release-packaging/native`
- `target/release-packaging/AppDir`
- `target/release-packaging/deb`

For final release artifact generation, use:

```bash
bash scripts/build-release-artifacts.sh
```

This will also create:

- `synchrosonic-<version>-x86_64.AppImage`
- `synchrosonic_<version>_amd64.deb`
- `synchrosonic-<version>.flatpak`
- `synchrosonic-<version>-linux-x86_64.tar.gz`
- `SHA256SUMS.txt`

If `flatpak` or `flatpak-builder` are not installed locally, the script skips
the Flatpak bundle and still generates the remaining artifacts plus checksums.

## What The Packaging Script Does

`scripts/package-linux.sh`:

- reads the workspace version from the root `Cargo.toml` via `scripts/read-workspace-version.py`
- builds `target/release/synchrosonic-app` unless `--skip-build` is passed
- installs the binary plus desktop assets into staging layouts
- generates a staged Debian-style `DEBIAN/control`
- validates desktop metadata when the validator tools are available
- archives the staged layouts as tarballs

`scripts/build-deb.sh`:

- reads package metadata from `debian/control`
- reads version metadata from `debian/changelog`
- runs `dpkg-shlibdeps` against the staged release binary
- writes substvars for shared-library dependencies
- runs `dpkg-gencontrol` to generate the final `target/release-packaging/deb/DEBIAN/control`
- builds the final `.deb` with `dpkg-deb --build`

## Debug Builds vs Release Builds

Debug builds are best for day-to-day development:

- faster compile/edit/run iteration
- output goes to `target/debug/`

Release builds use the workspace release profile from the root `Cargo.toml`:

- `codegen-units = 1`
- `incremental = false`
- `lto = "thin"`
- `strip = "debuginfo"`

Use release builds when you want packaging-ready binaries or performance closer
to what CI packages.

## Release Status In This Repo

What is implemented today:

- release binary builds for `synchrosonic-app`
- Linux staging layouts for native install trees, AppDir, and Debian-style
  filesystem trees
- AppImage, Debian `.deb`, Flatpak bundle, portable tarball, and checksum
  generation for tagged releases

What is not currently automated:

- signing or repository publication

The Flatpak artifact path is automated in tagged releases, but it still depends
on the host/runtime exposing the PipeWire CLI tools the current backend uses.

## Relationship To GitHub Actions

The `package-linux` job in `.github/workflows/ci.yml` runs after the
`lint-test` job passes. It performs:

```bash
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```

That means the closest local reproduction of the packaging job is:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p synchrosonic-app
bash scripts/package-linux.sh --skip-build
```
