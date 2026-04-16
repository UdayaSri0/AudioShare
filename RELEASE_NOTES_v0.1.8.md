# SynchroSonic v0.1.8

This release focuses on Debian packaging correctness, metadata consistency, and version alignment.

## What changed

- fixed Debian packaging flow so source-style `debian/control` and binary `DEBIAN/control` are no longer confused
- fixed release artifact generation to avoid `dpkg-shlibdeps` control-file parsing failures
- aligned the entire project to version `0.1.8`
- updated release validation so workspace version and tag must match exactly
- corrected About page, repository links, issue links, and release metadata
- improved release packaging consistency for AppImage, tarball, and Debian outputs
- refreshed docs and release posture to match the real automation state

## Release values

- Version: `0.1.8`
- Tag: `v0.1.8`
- Release title: `SynchroSonic v0.1.8`
- Repository: `https://github.com/UdayaSri0/AudioShare`
