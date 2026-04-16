# SynchroSonic v0.1.10 Prompt Pack

This pack gives you separate Codex prompts for each Linux packaging target, one general polishing prompt, and one final release/version/tag prompt.

Use them in order.

---

## Prompt 1 — Fix the broken release workflow first

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Fix the release workflow so tagged releases actually run and upload Linux artifacts instead of leaving the GitHub release with only the default source archives.

Critical context:
- The repository already contains release-oriented scripts and packaging assets for AppImage, Debian, Flatpak, tarball, and an unsigned APT repository scaffold.
- The current release workflow file .github/workflows/release.yml is malformed and must be rewritten as valid multi-line YAML.
- Do not do a partial fix. Replace the workflow with a clean, valid, readable GitHub Actions workflow.
- Preserve the release intent: on push of tags matching v*, validate version/tag consistency, run format/lint/tests, build release artifacts, verify them, and publish them to the GitHub release.

Required actions:
1. Open and inspect:
   - .github/workflows/release.yml
   - scripts/build-release-artifacts.sh
   - scripts/verify-release-artifacts.sh
   - scripts/read-workspace-version.py
   - Cargo.toml
2. Replace .github/workflows/release.yml with a valid YAML workflow.
3. Ensure the workflow:
   - triggers on push tags: v*
   - sets permissions: contents: write
   - checks out the repo
   - installs required Linux dependencies
   - installs stable Rust
   - validates that tag vX.Y.Z matches [workspace.package].version in Cargo.toml
   - runs cargo fmt --all --check
   - runs cargo clippy --workspace --all-targets -- -D warnings
   - runs cargo test --workspace
   - builds cargo build --release -p synchrosonic-app
   - runs bash scripts/build-release-artifacts.sh --skip-build
   - runs bash scripts/verify-release-artifacts.sh
   - publishes:
     - target/release-packaging/*.AppImage
     - target/release-packaging/*.deb
     - target/release-packaging/*.flatpak
     - target/release-packaging/*.tar.gz
     - target/release-packaging/SHA256SUMS.txt
4. Make the workflow readable and properly indented. No single-line YAML.
5. Use current best-practice action versions where appropriate.
6. Keep the job strict: fail fast if any artifact is missing.
7. Add comments inside the workflow for the main stages.

Acceptance criteria:
- GitHub Actions validates the YAML successfully.
- A new tag push can run the release job.
- The workflow uploads real assets to the release page.

Output required:
- Show the full final contents of .github/workflows/release.yml
- Summarize exactly what was fixed
- Mention any assumptions
```

---

## Prompt 2 — AppImage packaging hardening

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Make AppImage packaging production-ready and reliable for SynchroSonic v0.1.10.

Current context:
- The repo already contains:
  - scripts/package-linux.sh
  - scripts/build-appimage.sh
  - packaging/linux/org.synchrosonic.SynchroSonic.desktop
  - packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml
  - packaging/linux/org.synchrosonic.SynchroSonic.svg
  - packaging/linux/AppRun
- The project is a Rust GTK4/libadwaita application with the main binary synchrosonic-app.
- The output must be a real AppImage in target/release-packaging/.

Tasks:
1. Inspect the existing AppImage path end to end.
2. Validate the AppDir layout.
3. Ensure the desktop file, icon, AppStream metadata, AppRun, and binary are all staged correctly.
4. Improve scripts/build-appimage.sh so it is robust and actionable on failure:
   - set -euo pipefail
   - clear error messages
   - architecture validation
   - AppImage tool download caching
   - explicit checks for required files before calling appimagetool
5. Ensure package-linux.sh stages:
   - binary
   - desktop file
   - metainfo
   - icon
   - README
   - LICENSE
6. Ensure the final output name is stable:
   - synchrosonic-${version}-x86_64.AppImage
7. Verify the AppImage is executable after build.
8. Add or improve local validation steps:
   - desktop-file-validate
   - appstreamcli validate --no-net
9. Update docs/linux-packaging.md if needed to accurately describe the AppImage flow.
10. Do not introduce signing yet unless the repo already has a safe signing flow. If not, leave signing documented as future work.

Acceptance criteria:
- bash scripts/build-appimage.sh --skip-build produces a real AppImage
- the file appears in target/release-packaging/
- the release workflow can publish it
- docs match reality

Output required:
- show diffs or full file contents for every changed file
- explain why previous AppImage generation was fragile
- list any remaining limitations
```

---

## Prompt 3 — Debian package hardening

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Make the Debian/Ubuntu package path production-ready for SynchroSonic v0.1.10 and ensure a real .deb is always built and verifiable.

Current repo context:
- The repo already has:
  - debian/control
  - debian/changelog
  - scripts/build-deb.sh
  - scripts/package-linux.sh
- The app binary is synchrosonic-app.
- The package should end up in target/release-packaging/ as:
  - synchrosonic_${version}_amd64.deb

Tasks:
1. Inspect the current Debian build pipeline.
2. Verify the distinction between:
   - source-style debian/control
   - debian/changelog
   - generated binary DEBIAN/control inside the staged package root
3. Harden scripts/build-deb.sh:
   - strict shell mode
   - clear error messages
   - architecture mapping
   - validation that binary exists
   - validation that control/changelog exist
   - dpkg-shlibdeps handling
   - dpkg-gencontrol handling
   - dpkg-deb --build verification
4. Ensure runtime dependencies are correct for the current app:
   - include only real runtime dependencies
   - do not invent package names
5. Validate the resulting package with:
   - dpkg-deb --info
   - dpkg-deb --contents
6. Ensure the staged install layout is correct:
   - /usr/bin/synchrosonic-app
   - desktop file
   - icon
   - metainfo
   - docs
7. Update docs/linux-packaging.md if needed.
8. Keep the implementation simple and maintainable.

Acceptance criteria:
- bash scripts/build-deb.sh --skip-build creates a real .deb
- the output lands in target/release-packaging/
- the release workflow can publish it
- metadata is aligned with version 0.1.10

Output required:
- changed files
- explanation of what was fixed
- any caveats for Debian/Ubuntu support
```

---

## Prompt 4 — Flatpak hardening

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Stabilize and harden the Flatpak packaging path for SynchroSonic v0.1.10.

Current repo context:
- The repo already has:
  - packaging/flatpak/org.synchrosonic.SynchroSonic.yml
  - scripts/build-flatpak.sh
  - scripts/build-flatpak-runner.sh
  - optional Docker-based fallback builder logic
- The release flow expects:
  - target/release-packaging/synchrosonic-${version}.flatpak

Tasks:
1. Inspect the current Flatpak manifest and scripts.
2. Validate the finish-args and runtime assumptions for this app.
3. Make scripts/build-flatpak.sh robust:
   - strict shell mode
   - clear checks for flatpak and flatpak-builder
   - clean Docker fallback behavior
   - useful errors if neither native tooling nor Docker is available
4. Validate that the bundle export path is correct.
5. Ensure the output file is named consistently and ends up in target/release-packaging/.
6. Keep Flatpak clearly documented as a preview/runtime-sensitive path if host PipeWire CLI access is still required.
7. Update docs/linux-packaging.md to reflect the exact state honestly.
8. Do not fake sandbox independence. Be transparent.

Acceptance criteria:
- a real .flatpak bundle is produced locally or in CI
- the release workflow can publish it
- docs correctly state runtime limitations

Output required:
- changed files
- explanation of improvements
- remaining Flatpak runtime caveats
```

---

## Prompt 5 — RPM packaging for Fedora/openSUSE/RHEL-family

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Add first-class RPM packaging support for SynchroSonic v0.1.10 for Fedora/openSUSE/RHEL-like distributions.

Important:
- This repo currently focuses on AppImage, .deb, Flatpak, tarball, and an APT scaffold.
- RPM support must be added cleanly without breaking existing packaging.
- Use fpm only if absolutely necessary. Prefer a normal rpmbuild-based path if practical.

Tasks:
1. Create a new packaging path for RPM, for example:
   - packaging/rpm/
   - scripts/build-rpm.sh
2. Stage an install tree from the existing package-linux.sh flow or a clean reusable helper.
3. Create a .spec file template or generation path that includes:
   - package name
   - version
   - summary
   - license
   - desktop file
   - icon
   - metainfo
   - binary
   - docs
4. Ensure the output lands in:
   - target/release-packaging/
5. Name outputs consistently, for example:
   - synchrosonic-${version}-1.x86_64.rpm
6. Add validation:
   - rpm -qip
   - rpm -qlp
7. Update docs/linux-packaging.md to add RPM support.
8. Extend scripts/verify-release-artifacts.sh carefully:
   - do not break current release flow
   - only require RPM in the release workflow if you also update the workflow to build and publish it
9. If adding RPM to the main release workflow is too much for one change, wire the build script first and document the follow-up.

Acceptance criteria:
- build-rpm.sh can create a real RPM on an RPM-capable build environment
- existing packaging outputs remain intact
- docs explain how RPM support works

Output required:
- new files and changed files
- exact build instructions
- whether RPM is included in the release workflow now or staged for the next step
```

---

## Prompt 6 — Arch Linux package support

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Add an Arch Linux packaging path for SynchroSonic v0.1.10.

Important:
- Do not pretend you are publishing to AUR automatically.
- Add a clean PKGBUILD and supporting docs for local makepkg builds or future AUR submission.

Tasks:
1. Add a packaging/arch/PKGBUILD for SynchroSonic.
2. Use the current workspace version from Cargo.toml.
3. Package:
   - synchrosonic-app binary
   - desktop file
   - icon
   - metainfo
   - docs
4. Add a helper script such as scripts/build-arch-package.sh if useful.
5. Decide whether the package builds from:
   - local release binary
   - local source tree
   Explain the choice clearly.
6. Update docs/linux-packaging.md with Arch support notes.
7. Keep this as a packaging target and documentation path, not a fake official AUR publication.

Acceptance criteria:
- PKGBUILD is valid and reasonable
- documentation explains how to build/install on Arch
- existing packaging is not broken

Output required:
- PKGBUILD
- any helper script
- documentation update
- explanation of decisions
```

---

## Prompt 7 — Snap package support

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Add Snap packaging support for SynchroSonic v0.1.10 as an optional Linux distribution format.

Important:
- Do not publish automatically to the Snap Store unless the repo already has secure credentials and a conscious publishing decision.
- This step is about creating a valid snapcraft packaging path and local/CI build readiness.

Tasks:
1. Add snap/snapcraft.yaml.
2. Choose the correct base and build strategy for a Rust GTK4/libadwaita desktop app.
3. Include:
   - app binary
   - desktop launcher integration
   - icon
   - metadata
4. Consider the app’s runtime needs:
   - network
   - audio
   - desktop integration
   - PipeWire/PulseAudio access where appropriate
5. Document any confinement caveats honestly.
6. Add documentation to docs/linux-packaging.md.
7. If Snap is not mature enough for the main release workflow, keep it optional and document why.

Acceptance criteria:
- snapcraft.yaml is valid or close to valid for a real build environment
- docs explain how to build the snap
- limitations are honest

Output required:
- snap/snapcraft.yaml
- documentation changes
- summary of viability and caveats
```

---

## Prompt 8 — Portable tarball and release manifest polishing

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Polish the portable tarball and checksum/release-manifest output for SynchroSonic v0.1.10.

Current repo context:
- The release flow already expects:
  - a portable .tar.gz
  - SHA256SUMS.txt
- verify-release-artifacts.sh already checks for these outputs

Tasks:
1. Inspect how the tarball is produced in scripts/package-linux.sh and scripts/build-release-artifacts.sh.
2. Ensure the tarball is deterministic and clearly named:
   - synchrosonic-${version}-linux-${arch}.tar.gz
3. Ensure its contents are sensible for end users.
4. Ensure SHA256SUMS.txt includes every published asset.
5. Consider adding a machine-readable release manifest file if it helps, for example:
   - RELEASE_MANIFEST.txt or release-manifest.json
   but only if it improves maintainability.
6. Keep the release output clean and predictable.
7. Update docs if necessary.

Acceptance criteria:
- tarball output is correct
- checksum manifest is complete
- release artifacts are easy to inspect and validate

Output required:
- changed files
- explanation of improvements
```

---

## Prompt 9 — APT repository publication automation

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Take the existing unsigned APT repository scaffold and turn it into a publishable GitHub Pages-based APT repository path for SynchroSonic v0.1.10, but only if it can be done cleanly and safely.

Current repo context:
- The repo already has scripts/build-apt-repo.sh
- docs mention an unsigned APT repository scaffold
- signing and publication are not yet automated

Tasks:
1. Inspect the current APT scaffold builder.
2. Improve it so the generated repo structure is consistent and inspectable.
3. If practical, add a separate GitHub Actions workflow for APT publication.
4. Only add signing if the implementation can use GitHub Secrets safely and clearly.
5. If signing cannot be completed safely in one pass, keep publication scaffolded and document exactly what remains.
6. Update docs/apt-repository.md and docs/linux-packaging.md.

Acceptance criteria:
- the repo either:
  a) produces a solid unsigned APT scaffold with honest docs, or
  b) fully automates signed publication safely
- no pretend implementation
- existing release flow remains stable

Output required:
- changed files
- honest status summary
- exact next manual step if full publication is still deferred
```

---

## Prompt 10 — General polishing for v0.1.10

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Do a focused production-polish pass for SynchroSonic v0.1.10 after packaging is stable.

Important:
- This is not a random cleanup pass.
- Do not rewrite the product.
- Do not remove working functionality.
- Focus on release-readiness, consistency, and polish.

Tasks:
1. Inspect the whole repository and identify polish gaps in:
   - README
   - CONTRIBUTING
   - docs/quick-start.md
   - docs/developer/README.md
   - docs/linux-packaging.md
   - docs/release-checklist.md
   - changelog
   - desktop metadata
   - version strings
   - maintainer metadata
2. Ensure version 0.1.10 is consistent everywhere that should show it.
3. Ensure About/branding/package naming are consistent:
   - SynchroSonic
   - repository name AudioShare
   - binary synchrosonic-app
   - package name synchrosonic
4. Review release-facing metadata:
   - desktop file
   - metainfo
   - README screenshots section
   - packaging docs
5. Remove misleading claims if any output is still preview-only.
6. Ensure all shell scripts use strict mode and actionable errors.
7. Improve release checklist accuracy.
8. Keep all edits small, grounded, and honest.

Acceptance criteria:
- docs and metadata match reality
- versioning is aligned
- no fake claims remain
- packaging status is described accurately

Output required:
- grouped list of polish changes
- changed files
- brief rationale for each group
```

---

## Prompt 11 — Final v0.1.10 release prep, commit, tag, push, and GitHub release update

```text
You are working in the GitHub repository UdayaSri0/AudioShare.

Goal:
Prepare and finalize the SynchroSonic v0.1.10 release after packaging fixes are complete.

Important:
- Do not proceed until the release workflow is valid and packaging outputs are confirmed locally or by CI.
- This prompt includes version bump, changelog, tag creation, push, and release-note preparation.

Tasks:
1. Inspect current version references, including:
   - Cargo.toml
   - debian/changelog
   - README
   - CHANGELOG.md
   - release note files
   - any packaging metadata files
2. Bump version from 0.1.9 to 0.1.10 everywhere appropriate.
3. Add or update:
   - CHANGELOG.md
   - RELEASE_NOTES_v0.1.10.md
4. Prepare a clean release summary focused on:
   - fixed release workflow
   - AppImage publication
   - Debian package publication
   - Flatpak publication
   - tarball/checksum publication
   - any newly added formats such as RPM, Arch, Snap, or APT support
5. Run or instruct the exact validation sequence:
   - cargo fmt --all --check
   - cargo clippy --workspace --all-targets -- -D warnings
   - cargo test --workspace
   - cargo build --release -p synchrosonic-app
   - bash scripts/build-release-artifacts.sh --skip-build
   - bash scripts/verify-release-artifacts.sh
6. Create clear git commands:
   - git add ...
   - git commit -m "release: prepare v0.1.10"
   - git tag -a v0.1.10 -m "SynchroSonic v0.1.10"
   - git push origin main
   - git push origin v0.1.10
7. If using GitHub CLI, provide:
   - gh release create v0.1.10 ... only if needed
   but prefer letting the workflow publish the release assets on tag push if that flow is now fixed.
8. Explain the recommended order of operations so the release page does not end up empty again.
9. Be explicit about whether the release should be created manually first or left to the workflow.

Acceptance criteria:
- repo versioning is aligned to 0.1.10
- release notes are ready
- tag instructions are correct
- release workflow can publish the assets

Output required:
- list of version-bumped files
- final release notes text
- exact git commands in order
- recommendation on manual vs automatic GitHub release creation
```

---

# Recommended execution order

1. Prompt 1 — fix the release workflow first
2. Prompt 2 — AppImage
3. Prompt 3 — Debian
4. Prompt 4 — Flatpak
5. Prompt 8 — tarball/checksum polish
6. Prompt 9 — APT repository path
7. Prompt 5 — RPM
8. Prompt 6 — Arch
9. Prompt 7 — Snap
10. Prompt 10 — general polishing
11. Prompt 11 — final v0.1.10 release prep

---

# Suggested tag and release title

- Version: `0.1.10`
- Git tag: `v0.1.10`
- Release title: `SynchroSonic v0.1.10`

---

# Draft release notes for v0.1.10

```md
# SynchroSonic v0.1.10

SynchroSonic v0.1.10 is a release-engineering and packaging stabilization update focused on making GitHub releases publish real Linux installable artifacts reliably.

## Highlights

- fixed the GitHub release workflow so tagged releases can build and publish real assets
- hardened AppImage generation and validation
- hardened Debian package generation and verification
- improved Flatpak build reliability and documentation
- polished portable tarball and checksum output
- aligned versioning, release metadata, packaging metadata, and docs for `0.1.10`

## Fixed

- fixed the broken release workflow YAML that prevented tagged release jobs from running
- fixed release publication so GitHub releases can include actual Linux installables instead of only default source archives
- improved packaging validation for AppImage, Debian `.deb`, Flatpak bundle, and portable tarball outputs
- improved checksum generation and artifact verification before release publishing
- corrected or clarified packaging documentation where behavior was preview-only or environment-dependent

## Packaging

This release is intended to publish the following Linux artifacts when the tag workflow succeeds:

- AppImage
- Debian `.deb`
- Flatpak `.flatpak`
- portable Linux tarball
- `SHA256SUMS.txt`

Additional distribution packaging such as RPM, Arch, Snap, or APT repository publication may be included if completed and validated as part of the `0.1.10` release cycle.

## Release

- **Version:** `0.1.10`
- **Tag:** `v0.1.10`
- **Release title:** `SynchroSonic v0.1.10`
- **Repository:** `https://github.com/UdayaSri0/AudioShare`
- **Issues:** `https://github.com/UdayaSri0/AudioShare/issues`
```

---

# Exact git command sequence for the final release

```bash
git checkout main
git pull --ff-only origin main

# after applying and reviewing all packaging/release fixes
git add .
git commit -m "release: prepare v0.1.10"

# optional but recommended local validation
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p synchrosonic-app
bash scripts/build-release-artifacts.sh --skip-build
bash scripts/verify-release-artifacts.sh

git tag -a v0.1.10 -m "SynchroSonic v0.1.10"
git push origin main
git push origin v0.1.10
```

Recommended release behavior:
- do not manually create an empty GitHub release first
- let the fixed tag-triggered workflow build and upload the assets
- only use `gh release create` manually if your workflow is intentionally not responsible for publishing assets
