# SynchroSonic v0.1.10

SynchroSonic v0.1.10 is a release-readiness and Linux packaging polish update
focused on making tagged releases publish real installer assets more
consistently and documenting the supported package paths more honestly.

## Highlights

- fixed the tagged release workflow so version/tag validation comes from the
  workspace version in `Cargo.toml`
- kept GitHub release publication focused on real Linux installable artifacts:
  AppImage, Debian package, Flatpak bundle, portable tarball, checksums, and a
  release manifest
- added version-specific release note resolution so tagged releases can publish
  with the correct release body automatically
- aligned the project on version `0.1.10` across workspace metadata,
  packaging metadata, changelog entries, issue templates, and release-facing
  docs
- expanded the documented Linux packaging story for RPM, Arch Linux, Snap, and
  signed APT repository publication without overstating preview-only runtime
  paths

## Fixed

- fixed the release workflow to validate `v0.1.10` against the root workspace
  version instead of relying on fragile `cargo pkgid` parsing
- fixed AppImage packaging validation around staged AppDir inputs and cached
  `appimagetool` usage
- fixed Debian packaging so source-style `debian/control` and generated
  `DEBIAN/control` are handled correctly for the final `.deb`
- fixed Flatpak bundling and verification so the tagged release path expects a
  real `.flatpak` output before publication
- fixed the portable tarball and checksum flow so release outputs are easier to
  inspect, verify, and describe consistently

## Packaging Scope

- Published by the main tagged release workflow:
  - `synchrosonic-<version>-x86_64.AppImage`
  - `synchrosonic_<version>_amd64.deb`
  - `synchrosonic-<version>.flatpak`
  - `synchrosonic-<version>-linux-x86_64.tar.gz`
  - `SHA256SUMS.txt`
  - `RELEASE_MANIFEST.json`
- Added as documented local or follow-up packaging targets:
  - RPM packaging for Fedora/openSUSE/RHEL-family systems
  - Arch Linux `PKGBUILD` support for local `makepkg` or future AUR work
  - Snap packaging as an optional build-ready path
  - signed APT repository publication through a separate GitHub Pages workflow

## Notes

- Flatpak and Snap builds remain runtime-sensitive because the current audio
  backend still depends on PipeWire CLI tooling.
- Signed APT repository publication is available through a separate manual
  workflow and is not part of the main tagged release publish job.
- AppImage signing is still future work.

## Release

- Version: `0.1.10`
- Tag: `v0.1.10`
- Release title: `SynchroSonic v0.1.10`
- Repository: `https://github.com/UdayaSri0/AudioShare`
- Issues: `https://github.com/UdayaSri0/AudioShare/issues`
