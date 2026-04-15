# Release Checklist

Use this checklist before publishing a Linux release of SynchroSonic.

## Repository

- Confirm the root `LICENSE` file is present.
- Decide whether to expand the current short-form GPL notice to the full license text.
- Update [CHANGELOG.md](../CHANGELOG.md) for the release being tagged.
- Review [README.md](../README.md) for version-specific wording.
- Review [CONTRIBUTING.md](../CONTRIBUTING.md) and [SECURITY.md](../SECURITY.md).
- Confirm issue templates reflect the current support expectations.

## Metadata And Assets

- Verify the application id is still `org.synchrosonic.SynchroSonic`.
- Verify `packaging/linux/org.synchrosonic.SynchroSonic.desktop`.
- Verify `packaging/linux/org.synchrosonic.SynchroSonic.metainfo.xml`.
- Verify `packaging/linux/org.synchrosonic.SynchroSonic.svg`.
- Verify the About page metadata matches the release version and support links.

## Validation

- Run `cargo fmt --all --check`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `cargo test --workspace`.
- Run `cargo build --release -p synchrosonic-app`.
- Run `bash scripts/build-release-artifacts.sh --skip-build`.

## Packaging Review

- Inspect the native Linux staging layout in `target/release-packaging/native`.
- Inspect the AppDir staging layout in `target/release-packaging/AppDir`.
- Inspect the Debian staging layout in `target/release-packaging/deb`.
- Decide whether this release will ship staging artifacts only or final package artifacts too.
- If shipping a final AppImage, validate the selected toolchain and signing flow.
- If shipping a final `.deb`, validate dependency metadata and install/remove behavior.

## Documentation

- Update [docs/linux-packaging.md](linux-packaging.md) if packaging scope changed.
- Update screenshots in the README if real captures are available.
- Confirm known limitations are documented honestly for Bluetooth, synchronization, and packaging.

## Release Publication

- If stable blockers remain, publish the tag as a GitHub pre-release instead of a stable release.
- Tag the release only after CI passes on the intended commit.
- Upload only the artifacts that were actually produced and validated.
- Include a release note summary of what Linux packaging means in that version.
- Call out remaining blockers instead of implying full installer coverage if only staging artifacts were built.
