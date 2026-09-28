<div align="center">

<img src="lib/logo.png" alt="Skinart+" width="140">

# Skinart+

Convert pixel art into Minecraft skinart and upload it to NameMC.

[![GitHub release](https://img.shields.io/github/v/release/iraqies/skinartplus?color=14b8a6&label=release)](https://github.com/iraqies/skinartplus/releases)
[![Stars](https://img.shields.io/github/stars/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/stargazers)
[![Forks](https://img.shields.io/github/forks/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/network)
[![Issues](https://img.shields.io/github/issues/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/issues)
[![Pull requests](https://img.shields.io/github/issues-pr/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/pulls)
[![Contributors](https://img.shields.io/github/contributors/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/graphs/contributors)
[![License](https://img.shields.io/github/license/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus)
[![Views](https://api.visitorbadge.io/api/visitors?path=iraqies.skinartplus&label=views&labelColor=%23222&countColor=%2314b8a6)](https://github.com/iraqies/skinartplus)

</div>

Skinart+ is a desktop app for converting images into 64×64 Minecraft skinart. You can generate every layer combination, start with a bundled template, and upload skins through your NameMC account. It runs on Tauri, using Rust and a WebView.

## Features

- Convert images into Minecraft skinart.
- Sign in to NameMC, preview your avatar, and upload skins.
- Claim the NameMC profile associated with your signed-in account.
- Start with one of hundreds of bundled base skinarts.
- Copy another user's skinart with Skinart Stealer.

## Install

Get the latest build from [Releases](https://github.com/iraqies/skinartplus/releases).

### Windows

[Download the Windows installer](https://github.com/iraqies/skinartplus/releases/latest).

Run the installer. It installs for your user account, needs no administrator rights, and requires no extra dependencies.

### Linux

Choose a package for your distribution.

| Distro family | Format | Install |
|---------------|--------|---------|
| Any distro | Flatpak | `flatpak install SkinartPlus.flatpak` then `flatpak run com.skinartplus.app` |
| Any distro | AppImage | `chmod +x skinartplus_1.0.0_amd64.AppImage && ./skinartplus_1.0.0_amd64.AppImage` |
| Debian / Ubuntu / Linux Mint / Pop!_OS | `.deb` | `sudo dpkg -i skinartplus_1.0.0_amd64.deb` |
| Fedora / RHEL / CentOS / openSUSE | `.rpm` | `sudo rpm -i skinartplus-1.0.0-1.x86_64.rpm` |
| Arch / Manjaro / EndeavourOS | Flatpak or AppImage | `flatpak install SkinartPlus.flatpak` or run the AppImage directly |
| Other rolling or source-based distros | Flatpak or AppImage | No system package manager install needed |

The Flatpak and AppImage builds support modern x86_64 Linux distributions. Use either if there is no native package for your distro.

If `dpkg -i` reports missing dependencies, run `sudo apt-get install -f`.

If the AppImage cannot find FUSE, install `libfuse2` with `sudo apt install libfuse2`, or run it with `./skinartplus_1.0.0_amd64.AppImage --appimage-extract-and-run`.

## Build from source

### Requirements

- [Rust](https://rustup.rs), stable
- [Node.js](https://nodejs.org) LTS and npm
- The system packages for your platform, listed below

### Linux system dependencies

**Debian / Ubuntu / Mint**

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf build-essential
```

**Fedora / RHEL**

```bash
sudo dnf install -y webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel patchelf gcc
```

**Arch / Manjaro**

```bash
sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf base-devel
```

**openSUSE**

```bash
sudo zypper install -y webkit2gtk3-soup2-devel libappindicator3-devel librsvg2-devel patchelf gcc
```

### Build

```bash
npm install
npm run tauri dev
npm run tauri build
```

`npm run tauri dev` opens a development window. `npm run tauri build` creates packages for your current OS in `src-tauri/target/release/bundle/`.

Some authentication features require client credentials supplied at build time. The source includes a public fallback, so you can build and run the app without private credentials. Official releases include the private client IDs needed for the full sign-in experience.

## Packaging

The [GitHub Actions workflow](.github/workflows/build.yml) builds on pushes and tags. It produces a Windows NSIS installer, Linux `.deb`, `.rpm`, and `.AppImage` packages, and a Flatpak bundle in `packaging/flatpak/`.

Tag a commit, for example `v1.0.1`, to have the workflow attach installers to a GitHub Release.

## Credits

- Iraqies, founder and developer
- GoldenGR, authentication provider
- Hyloduck, logo design

## License

Licensed under [Creative Commons Attribution 4.0 International (CC BY 4.0)](LICENSE).

You may fork, modify, and redistribute the project as long as you credit iraqies / Skinart+ and link to the original project. The Skinart+ name and logo are not covered by this license.
