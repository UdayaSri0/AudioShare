# APT Repository Scaffold

SynchroSonic now includes an unsigned APT repository scaffold generator for the
release `.deb` artifact.

## What It Generates

After building the release artifacts, run:

```bash
bash scripts/build-apt-repo.sh
```

The script writes an unsigned repository tree to:

```text
target/release-packaging/apt-repo/
```

That tree includes:

- `pool/main/s/synchrosonic/` with the built `.deb`
- `dists/stable/main/binary-<arch>/Packages`
- `dists/stable/main/binary-<arch>/Packages.gz`
- `dists/stable/Release`

## Current Limits

This scaffold is intentionally incomplete for public APT hosting:

- `Release` is not signed
- `InRelease` is not generated
- no repository key is created or published
- no GitHub Pages or branch publication flow is wired yet

## Recommended Use

For `v0.1.9`, the primary Debian/Ubuntu install path remains the release
asset:

- `synchrosonic_<version>_amd64.deb`

Treat the APT repository scaffold as follow-up-ready infrastructure for a later
signed publication flow.
