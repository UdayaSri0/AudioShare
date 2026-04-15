# SynchroSonic v0.1.6 Release Notes

## Overview

This release introduces automated Linux packaging for SynchroSonic, enabling easy distribution and installation across different Linux environments. We've added support for AppImage, Debian packages, Flatpak bundles, and portable tarballs, all built automatically through GitHub Actions CI.

## What's New

### Packaging & Distribution
- **AppImage Support**: Single-file executable that runs on most Linux distributions without installation
- **Debian Packages**: Native `.deb` packages with automatic dependency resolution
- **Flatpak Integration**: Sandboxed Flatpak bundle for secure, isolated execution
- **Portable Tarball**: Simple compressed archive for manual installation

### Build Automation
- New build scripts for each packaging format
- GitHub Actions workflow for automated releases
- Version consistency validation between tags and code
- Multi-architecture support (x86_64)

## Installation

### AppImage (Recommended for most users)
```bash
# Download the .AppImage file
chmod +x SynchroSonic-0.1.6-x86_64.AppImage
./SynchroSonic-0.1.6-x86_64.AppImage
```

### Debian Package
```bash
# Download the .deb file
sudo dpkg -i synchrosonic_0.1.6_amd64.deb
sudo apt-get install -f  # Install dependencies if needed
```

### Flatpak
```bash
# Download the .flatpak file
flatpak install --user SynchroSonic-0.1.6-x86_64.flatpak
flatpak run org.synchrosonic.SynchroSonic
```

### Portable Tarball
```bash
# Download and extract the tarball
tar -xzf synchrosonic-0.1.6-x86_64.tar.gz
cd synchrosonic-0.1.6-x86_64
./synchrosonic-app
```

## System Requirements

- Linux distribution (Ubuntu 20.04+, Fedora 34+, or compatible)
- PipeWire audio system
- GTK4 libraries
- For Flatpak: Flatpak runtime support

## Known Issues

- Flatpak builds are available but PipeWire integration may require additional host permissions
- Package signing is not yet implemented
- Bluetooth output support is planned for future releases

## Download Assets

This release includes the following artifacts:
- `SynchroSonic-0.1.6-x86_64.AppImage` - AppImage executable
- `synchrosonic_0.1.6_amd64.deb` - Debian package
- `SynchroSonic-0.1.6-x86_64.flatpak` - Flatpak bundle
- `synchrosonic-0.1.6-x86_64.tar.gz` - Portable tarball
- `SHA256SUMS` - Checksums for all artifacts

## Contributing

We welcome feedback on the packaging and any issues you encounter. Please report bugs through GitHub Issues.

## Previous Releases

See [CHANGELOG.md](CHANGELOG.md) for detailed change history.