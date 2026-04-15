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

- `synchrosonic-<version>-<arch>-native-layout.tar.gz`
- `synchrosonic-<version>-<arch>-AppDir.tar.gz`
- `synchrosonic-<version>-<arch>-deb-layout.tar.gz`

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

## What The Packaging Script Does

`scripts/package-linux.sh`:

- reads the package version from `cargo pkgid -p synchrosonic-app`
- builds `target/release/synchrosonic-app` unless `--skip-build` is passed
- installs the binary plus desktop assets into staging layouts
- generates a Debian-style `DEBIAN/control`
- validates desktop metadata when the validator tools are available
- archives the staged layouts as tarballs

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
- CI artifact upload of those staged layouts

What is not currently automated:

- final signed AppImage generation
- final dependency-complete `.deb` production
- signing or repository publication

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

