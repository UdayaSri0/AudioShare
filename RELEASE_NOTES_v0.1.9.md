# SynchroSonic v0.1.9

SynchroSonic v0.1.9 is a release-engineering and packaging completion update focused on shipping real Linux installable artifacts reliably.

## Highlights

- completed the Linux release pipeline for publishable installer artifacts
- aligned the project to version `0.1.9` across code, packaging, docs, and release metadata
- fixed tag/version validation so release tags and workspace version stay synchronized
- corrected release asset publication so GitHub releases include actual Linux installables
- improved packaging consistency for AppImage, Debian, Flatpak, and portable tarball outputs

## Fixed

- fixed packaging pipeline issues that previously prevented Linux installer assets from being attached reliably
- fixed Debian packaging flow so a real `.deb` is generated instead of only staging layouts
- fixed Flatpak release generation so a final `.flatpak` bundle is produced and published
- fixed release workflow validation and artifact checks to reduce broken tagged releases
- fixed stale version and release metadata across the repository

## Release

- Version: `0.1.9`
- Tag: `v0.1.9`
- Release title: `SynchroSonic v0.1.9`
- Repository: `https://github.com/UdayaSri0/AudioShare`
- Issues: `https://github.com/UdayaSri0/AudioShare/issues`
