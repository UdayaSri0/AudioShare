# APT Repository

SynchroSonic now supports two APT repository states:

- an unsigned local scaffold for inspection and follow-up work
- a signed GitHub Pages publication path driven by a separate workflow

The existing tagged release flow is unchanged. The `.deb` release asset remains
the primary Debian/Ubuntu install path until the signed Pages repo is enabled
for the repository.

## Local Unsigned Scaffold

After building the Debian package, run:

```bash
bash scripts/build-apt-repo.sh
```

This writes an inspectable repository tree to:

```text
target/release-packaging/apt-repo/
```

The scaffold includes:

- `pool/main/s/synchrosonic/` with the built `.deb`
- `dists/stable/main/binary-<arch>/Packages`
- `dists/stable/main/binary-<arch>/Packages.gz`
- `dists/stable/Release`
- `index.html`
- `README.txt`
- `.nojekyll`

This default output is intentionally unsigned. It is useful for:

- checking repository structure locally
- validating `Packages` and `Release` metadata
- preparing a Pages deploy artifact before signing is configured

It is not appropriate to present as a production APT repository for unattended
client installs.

## Signed GitHub Pages Publication

The repo now includes a separate workflow:

- `.github/workflows/publish-apt-repository.yml`

This workflow:

1. checks out a selected ref
2. builds the release binary
3. builds the `.deb`
4. imports a GPG signing key from GitHub Secrets
5. runs `scripts/build-apt-repo.sh --sign`
6. publishes the resulting repository tree to GitHub Pages

When signing is enabled, the published repository also includes:

- `dists/stable/InRelease`
- `dists/stable/Release.gpg`
- `keyrings/synchrosonic-archive-keyring.gpg`
- `keyrings/synchrosonic-archive-keyring.asc`
- `install/synchrosonic.sources`

## Required One-Time Setup

Before the Pages publication workflow can succeed, configure:

1. GitHub Pages for the repository
2. repository secret `APT_GPG_PRIVATE_KEY_BASE64`
3. repository secret `APT_GPG_PASSPHRASE`

Recommended Pages configuration:

- Source: GitHub Actions

Recommended secret format:

- `APT_GPG_PRIVATE_KEY_BASE64`: base64-encoded ASCII-armored private key export
- `APT_GPG_PASSPHRASE`: passphrase for that private key

Example key export flow on a trusted local machine:

```bash
gpg --armor --export-secret-keys <KEY_ID> | base64 -w0
```

Store that base64 output in `APT_GPG_PRIVATE_KEY_BASE64`, and store the
passphrase separately in `APT_GPG_PASSPHRASE`.

## Manual Publication Trigger

Once Pages and secrets are configured, run the workflow manually from GitHub:

- Workflow: `Publish APT Repository`
- Input: the ref to publish from, such as `main` or `v0.1.10`

The workflow currently computes the expected GitHub Pages URL as:

```text
https://udayasri0.github.io/AudioShare
```

If the repo later moves to a custom Pages domain, update the workflow or pass a
different repository URL into `scripts/build-apt-repo.sh`.

## Client Install Flow For The Signed Repository

After the Pages publication workflow succeeds, Debian/Ubuntu users can install
the key and source descriptor like this:

```bash
curl -fsSL https://udayasri0.github.io/AudioShare/keyrings/synchrosonic-archive-keyring.gpg | sudo tee /usr/share/keyrings/synchrosonic-archive-keyring.gpg >/dev/null
curl -fsSL https://udayasri0.github.io/AudioShare/install/synchrosonic.sources | sudo tee /etc/apt/sources.list.d/synchrosonic.sources >/dev/null
sudo apt update
sudo apt install synchrosonic
```

## Current Limits

The APT publication path is now signed and automatable, but it still has a few
explicit boundaries:

- the main GitHub release workflow does not publish the APT repo automatically
- the signed publication is manual-by-design through the separate Pages workflow
- the repository relies on a correctly managed private key secret
- the `.deb` release asset remains the fallback path if Pages or secrets are
  not configured yet

## Exact Next Manual Step

If the signed publication path has not been enabled yet, the next manual step
is:

1. enable GitHub Pages with `GitHub Actions` as the source
2. add `APT_GPG_PRIVATE_KEY_BASE64` and `APT_GPG_PASSPHRASE`
3. run the `Publish APT Repository` workflow against `main` or `v0.1.10`
