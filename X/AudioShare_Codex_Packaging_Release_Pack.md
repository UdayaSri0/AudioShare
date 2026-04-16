# AudioShare / SynchroSonic — Codex Packaging + Release Pack

## What this pack is for
This document is a production-ready instruction pack for Codex to:
- turn the current Linux packaging staging flow into real user-installable deliverables
- generate AppImage, Debian package, and Flatpak outputs from the same application
- keep versioning, metadata, CI, and release notes in sync
- produce a clean GitHub release with real validated assets

---

## Important release correction
The repository now targets the canonical public release `0.1.9` with GitHub tag
`v0.1.9`.

For this release, Codex must keep versioning, ownership metadata, documentation,
and packaging aligned on the canonical AudioShare repository.

### Versioning decision rule
Use this release baseline:

1. bump workspace version to `0.1.9`
2. publish GitHub tag `v0.1.9`
3. mark the release as a stable release only after validation passes

Do not leave mismatched version or tag references behind.

---

# Master prompt for Codex

```md
You are working inside the existing `UdayaSri0/AudioShare` repository.

Your job is to convert the current Linux packaging/release staging setup into a real multi-format public release pipeline for the SynchroSonic desktop application.

## Non-negotiable operating rules

1. Read the repository first before changing anything.
2. Preserve the existing Rust + GTK4/libadwaita architecture.
3. Do not rewrite the project into another stack.
4. Keep existing working packaging assets and extend them rather than replacing them blindly.
5. Do not fake packaging support. Only ship formats that are truly produced and validated.
6. Keep docs, version strings, metadata, CI, and release notes in sync.
7. No placeholder release metadata, no placeholder maintainer values, no pretend installers.
8. If a format cannot be completed safely in this pass, document the blocker clearly rather than pretending it is done.
9. All shell scripts must use `set -euo pipefail`.
10. Every change must be production-oriented, incremental, and reviewable.

## Repository reality you must respect

The repository already contains:
- Rust workspace with `synchrosonic-app` and related crates
- packaging metadata under `packaging/linux/`
- a packaging staging script at `scripts/package-linux.sh`
- CI that already runs fmt, clippy, tests, release build, and packaging staging
- documentation in `docs/linux-packaging.md`, `docs/release-checklist.md`, and `CHANGELOG.md`

The current repo already stages:
- native Linux install layout tarball
- AppDir tarball
- Debian filesystem layout tarball

The repo currently does **not** fully automate:
- final AppImage generation
- dependency-complete final Debian package generation
- signing and polished release publication

Your mission is to close those gaps cleanly.

## Primary goal

Implement a packaging and release system that produces these release assets from the current codebase:

### Required release assets
1. `synchrosonic-<version>-x86_64.AppImage`
2. `synchrosonic_<version>_amd64.deb`
3. Flatpak deliverables:
   - manifest under version control
   - CI build of the Flatpak
   - exported `.flatpak` bundle or documented artifact strategy
4. `synchrosonic-<version>-linux-x86_64.tar.gz` portable/native layout
5. `SHA256SUMS.txt` for all published artifacts
6. updated release notes and changelog

## Scope rules

### In scope
- AppImage generation from the existing AppDir output
- proper Debian package generation from the existing staged Debian layout
- Flatpak packaging support for Linux desktop users
- GitHub Actions release workflow for packaging and attaching artifacts
- checksum generation
- version consistency across Cargo manifests, About page, metadata, and package files
- docs updates for build, install, validation, and release publication

### Out of scope unless already easy and justified
- RPM
- Arch PKGBUILD
- custom APT repository publication
- detached signing infrastructure unless the repo already has secrets for it
- Windows and macOS packaging

## Implementation order

### Phase 1 — inspect and map the current packaging flow
Read and analyse at minimum:
- `Cargo.toml`
- `.github/workflows/ci.yml`
- `scripts/package-linux.sh`
- `packaging/linux/org.synchrosonic.SynchroSonic.desktop`
- `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`
- `packaging/linux/org.synchrosonic.SynchroSonic.svg`
- `docs/linux-packaging.md`
- `docs/release-checklist.md`
- `CHANGELOG.md`
- README packaging and release sections

Then produce a short packaging gap summary in a markdown doc such as:
- what already exists
- what is missing for AppImage
- what is missing for Debian
- what is missing for Flatpak
- what CI/release changes are needed

### Phase 2 — version and metadata cleanup
Fix versioning so there is no mismatch between:
- workspace/package version in Cargo manifests
- UI/About page version string
- package metadata
- changelog heading
- GitHub release tag expectation
- README release wording

Use this decision rule:
- align the workspace version, package metadata, About page, docs, and release assets on `0.1.9`
- publish the matching GitHub tag `v0.1.9` only after validation passes

Never leave mixed version or tag references behind.

### Phase 3 — real AppImage output
Build on the existing AppDir staging flow.

Requirements:
- choose one AppImage toolchain and pin it clearly
- preferably use `appimagetool` unless a better repo-local choice is clearly justified
- generate a final `.AppImage` in CI, not only an AppDir tarball
- make the workflow reproducible on GitHub Actions Ubuntu runners
- validate desktop file and AppStream metadata before building AppImage
- preserve existing desktop id and icon wiring
- ensure the produced file name includes version and architecture

Expected changes may include:
- extend `scripts/package-linux.sh`
- add a helper script such as `scripts/build-appimage.sh`
- update CI to fetch the AppImage tool and produce the final artifact
- update docs to explain local AppImage generation

### Phase 4 — real Debian package output
Build on the existing Debian-style filesystem layout.

Requirements:
- generate a real `.deb`, not only a layout tarball
- keep package name `synchrosonic` unless the repo already intentionally uses another final package name
- map architecture correctly (`amd64` for x86_64)
- produce a sane `DEBIAN/control`
- do not use fake maintainer values
- avoid unsafe installation practices like `pip --break-system-packages`
- include desktop file, metainfo, icon, binary, docs, and license
- verify install/remove behaviour locally in CI as far as practical

If needed, add:
- `packaging/deb/control.in`
- `packaging/deb/postinst`
- `packaging/deb/postrm`
- helper script such as `scripts/build-deb.sh`

Preferred implementation:
- generate the Debian filesystem tree
- fill metadata from workspace version
- build final package with `dpkg-deb --build`
- run package inspection checks

If runtime dependencies can be auto-detected safely, do so.
If not, document the chosen dependency policy honestly.

### Phase 5 — Flatpak packaging
Introduce Flatpak packaging in a way that fits the current Linux desktop app.

Requirements:
- add a proper Flatpak manifest under version control
- use the existing app id `org.synchrosonic.SynchroSonic` unless there is a strong documented reason to change it
- include desktop metadata and icon assets correctly
- ensure the build instructions are documented
- CI must build the Flatpak artifact
- export either:
  - a `.flatpak` bundle artifact, or
  - a repository artifact plus clear release docs

Codex must choose one strategy and document why.

Suggested files:
- `packaging/flatpak/org.synchrosonic.SynchroSonic.yml`
- helper script such as `scripts/build-flatpak.sh`
- docs section in `docs/linux-packaging.md`

Important:
- do not oversell Flatpak if host integration limits matter for audio/network discovery
- document any required permissions clearly
- keep sandbox permissions minimal but sufficient

### Phase 6 — checksums and release artifacts
Generate release checksums for every published output.

Requirements:
- produce `SHA256SUMS.txt`
- include AppImage, `.deb`, Flatpak artifact, and tarball in checksum file
- ensure filenames are stable and versioned

### Phase 7 — GitHub Actions release workflow
Extend CI or add a release workflow that:
- runs on version tags
- performs fmt, clippy, tests, release build
- builds final AppImage
- builds final `.deb`
- builds Flatpak artifact
- builds portable tarball
- generates `SHA256SUMS.txt`
- uploads all release assets to the GitHub release

Recommended workflow split:
- keep existing CI for PRs and pushes
- add a separate tag-triggered workflow for release packaging

Make sure the workflow fails loudly if:
- tag/version mismatch exists
- required asset is missing
- packaging validation fails

### Phase 8 — documentation and release discipline
Update at minimum:
- `README.md`
- `CHANGELOG.md`
- `docs/linux-packaging.md`
- `docs/release-checklist.md`
- any developer docs that mention packaging or release commands

Docs must state clearly:
- what is stable in this release
- what formats are produced
- how to build them locally
- what remains experimental if anything

### Phase 9 — validation
Add practical validation steps.

Required checks:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --release -p synchrosonic-app`
- package staging script still works
- final `.AppImage` exists
- final `.deb` exists and is inspectable
- Flatpak build artifact exists
- release notes reflect reality

If possible, add smoke checks like:
- `dpkg-deb --info`
- `dpkg-deb --contents`
- AppImage `--appimage-extract` sanity check
- Flatpak bundle/repo inspection

## File output expectations from you
When you finish, provide:
1. a concise summary of what changed
2. the exact files added/modified
3. the packaging strategy decisions made
4. remaining follow-up items, if any
5. the final release tag suggestion
6. a ready-to-paste GitHub release note draft
7. a ready-to-paste changelog entry

## Hard constraints
- Do not remove existing staging outputs unless replaced by better final outputs.
- Do not break local developer packaging workflows.
- Do not claim final signing if no signing was implemented.
- Do not ship placeholder maintainer or homepage metadata.
- Do not leave docs describing prerelease-only packaging if the repo now produces final installable artifacts.
- Do not create version drift across files.

## Final release naming rule
Use this logic at the end:
- if AppImage + `.deb` + Flatpak + checksums are all produced and validated, suggest `v0.1.9`
- otherwise stop and fix the blockers before tagging

Now execute the work carefully and incrementally.
```

---

## Short prompt version

```md
Read the existing AudioShare repository first, especially Cargo.toml, scripts/package-linux.sh, docs/linux-packaging.md, docs/release-checklist.md, CHANGELOG.md, and the GitHub Actions workflows.

Then extend the current Rust/GTK packaging flow so the repo produces real release artifacts instead of only staging layouts:
- final AppImage
- final amd64 Debian package
- Flatpak artifact
- portable tar.gz
- SHA256SUMS.txt

Keep version, metadata, About page, docs, and release notes fully in sync.
Do not rewrite the stack.
Do not fake packaging coverage.
Use the existing AppDir and Debian layout staging as the base.
Create or update helper scripts and GitHub Actions as needed.
At the end, return the changed files, remaining blockers, a final tag suggestion, and a full GitHub release note draft.
```

---

## Recommended next tag

### Canonical release target
- **Tag:** `v0.1.9`
- **Release title:** `SynchroSonic v0.1.9`
- **Release type:** Stable

---

## Copy-paste GitHub release description for `v0.1.9`

```md
## SynchroSonic v0.1.9

This release aligns SynchroSonic metadata, packaging, and release automation on the canonical AudioShare repository.

### Highlights
- Linux-first Rust + GTK4/libadwaita desktop application for LAN audio casting and receiver playback
- improved packaging pipeline for public distribution
- AppImage generation from the existing AppDir staging flow
- Debian package generation from the repository’s Debian filesystem layout
- Flatpak packaging added for Linux desktop distribution
- portable release archive for users who prefer manual extraction
- release checksums for published artifacts
- version, metadata, and release documentation cleanup to keep the project consistent

### Release assets
- AppImage
- Debian package (`.deb`)
- Flatpak artifact
- portable Linux tarball
- `SHA256SUMS.txt`

### Notes
- This is still a pre-release while packaging, metadata, and runtime validation continue across Linux environments.
- Linux remains the only supported platform in this release line.
- Bluetooth support remains a local sink/output choice on Linux and is not a transport path.

### Upgrade guidance
- Existing testers should replace older staging artifacts with the assets published in this release.
- Please report install, launch, discovery, and playback issues with your distro version and desktop environment.

### Known limitations
- packaging validation may still be narrower than full distro-wide certification
- cross-distro release certification is still in progress
- any remaining gaps are documented in the packaging docs and checklist in the repository
```

---

## Copy-paste GitHub release description for `v0.1.1`

```md
## SynchroSonic v0.1.1

This release delivers installable Linux distribution formats for SynchroSonic and aligns packaging, metadata, and release documentation around a consistent public release.

### Highlights
- Linux-first Rust + GTK4/libadwaita desktop application for LAN audio casting and receiver playback
- final AppImage release artifact
- final Debian package (`.deb`) release artifact
- Flatpak packaging for Linux desktop distribution
- portable Linux tarball for manual deployment
- published `SHA256SUMS.txt` for release asset verification
- packaging and release workflow automation in GitHub Actions
- version and release metadata consistency across code, UI, docs, and package outputs

### Release assets
- AppImage
- Debian package (`.deb`)
- Flatpak artifact
- portable Linux tarball
- `SHA256SUMS.txt`

### Notes
- Linux is the supported platform for this release.
- Bluetooth remains a local sink/output choice on Linux rather than a transport path.

### Upgrade guidance
- Replace older preview/staging artifacts with the installable assets from this release.
- Verify checksums before installation where appropriate.

### Packaging scope
- AppImage for portable desktop use
- Debian package for Debian/Ubuntu-style systems
- Flatpak for users who prefer sandboxed distribution and desktop-store style deployment

### Known limitations
- release certification outside the validated Linux environments may still be narrower than general compatibility
- any environment-specific caveats should be tracked in the repository issue tracker and packaging docs
```

---

## Suggested short release summary

```md
Packaging release for SynchroSonic: real AppImage, `.deb`, Flatpak support, portable tarball, checksums, and release metadata cleanup.
```

---

## Suggested commit message

```text
release: add installable Linux packaging targets and release assets
```

---

## Suggested PR title

```text
Add final Linux packaging outputs for AppImage, Debian, and Flatpak
```
