<div align="center">

<img src="lib/logo.png" alt="Skinart+" width="140">

# Skinart+

**Turn pixel art into Minecraft skinarts and upload them straight to NameMC**

[![GitHub release](https://img.shields.io/github/v/release/iraqies/skinartplus?color=14b8a6&label=release)](https://github.com/iraqies/skinartplus/releases)
[![Stars](https://img.shields.io/github/stars/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/stargazers)
[![Forks](https://img.shields.io/github/forks/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/network)
[![Issues](https://img.shields.io/github/issues/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/issues)
[![Pull requests](https://img.shields.io/github/issues-pr/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/pulls)
[![Contributors](https://img.shields.io/github/contributors/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus/graphs/contributors)
[![License](https://img.shields.io/github/license/iraqies/skinartplus?color=14b8a6)](https://github.com/iraqies/skinartplus)
[![Views](https://api.visitorbadge.io/api/visitors?path=iraqies.skinartplus&label=views&labelColor=%23222&countColor=%2314b8a6)](https://github.com/iraqies/skinartplus)

</div>

Skinart+ is a desktop app that converts images into 64×64 Minecraft skinart. It supports NameMC account integration, batch generation across every layer combination, template base skinarts, and a custom watermark — all wrapped in a fast Tauri (Rust + WebView) shell.

---

## ✨ Features

- **Image → skinart conversion** — pick any image and get a ready-to-use Minecraft skinart
- **NameMC integration** — log in, preview your avatar, and upload skinarts with one click
- **Batch generation** — generate every possible art layer combination automatically
- **Template library** — hundreds of bundled base skinarts to build from
- **Watermark** — your art stays signed with the Skinart+ watermark
- **Fast & light** — built with Tauri, so it uses a fraction of the memory of Electron apps

---

## 🚀 Install

Prebuilt packages are produced automatically by GitHub Actions for every release. Grab the latest from the [Releases page](https://github.com/iraqies/skinartplus/releases).

### Windows

```
SkinartPlus_1.0.0_x64-setup.exe
```

Run the installer — no extra dependencies needed. It installs per-user, so no admin rights are required.

### Linux

There is no single "Linux" — pick the package that matches your distro:

| Distro family | Format | Install |
|---------------|--------|---------|
| **Any distro** | Flatpak | `flatpak install SkinartPlus.flatpak` then `flatpak run com.skinartplus.app` |
| **Any distro** | AppImage | `chmod +x skinartplus_1.0.0_amd64.AppImage && ./skinartplus_1.0.0_amd64.AppImage` |
| **Debian / Ubuntu / Linux Mint / Pop!_OS** | `.deb` | `sudo dpkg -i skinartplus_1.0.0_amd64.deb` |
| **Fedora / RHEL / CentOS / openSUSE** | `.rpm` | `sudo rpm -i skinartplus-1.0.0-1.x86_64.rpm` |
| **Arch / Manjaro / EndeavourOS** | Flatpak or AppImage | `flatpak install SkinartPlus.flatpak` or run the AppImage directly |
| **Any rolling / source-based distro** | Flatpak or AppImage | works without touching the system package manager |

> The **Flatpak** and **AppImage** builds run on any modern Linux distribution (x86_64) without system package installs — use these if your distro isn't listed above.

**Notes**

- If `dpkg -i` fails on dependency errors (older Ubuntu/Debian), run `sudo apt-get install -f` afterwards to resolve them.
- If the AppImage needs a newer FUSE, either install `libfuse2` (`sudo apt install libfuse2`) or run it with `./skinartplus_1.0.0_amd64.AppImage --appimage-extract-and-run`.

---

## 🛠 Build from source

### Prerequisites

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) LTS and npm
- Platform system dependencies (below)

### Linux system dependencies

**Debian / Ubuntu / Mint:**

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf build-essential
```

**Fedora / RHEL:**

```bash
sudo dnf install -y webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel patchelf gcc
```

**Arch / Manjaro:**

```bash
sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf base-devel
```

**openSUSE:**

```bash
sudo zypper install -y webkit2gtk3-soup2-devel libappindicator3-devel librsvg2-devel patchelf gcc
```

### Build

```bash
# install dependencies
npm install

# development (runs a live dev window)
npm run tauri dev

# production build (bundles for your current OS)
npm run tauri build
```

`npm run tauri build` outputs the installers for your current OS (e.g. `.exe` on Windows, `.deb`/`.rpm`/`.AppImage` on Linux) into `src-tauri/target/release/bundle/`.

> Note: some auth features require client credentials that are injected at build time. A permissive public fallback is baked into the source, so the app builds and runs out of the box — but for the full sign-in experience the official releases are built with the private client IDs.

---

## 📦 Packaging

This repository includes a [GitHub Actions workflow](.github/workflows/build.yml) that builds on every push and tag:

- Windows NSIS installer (`.exe`)
- Linux `.deb`, `.rpm`, and `.AppImage`
- A universal **Flatpak** bundle (`packaging/flatpak/`)

Tag a commit (`v1.0.1`, `v1.1.0`, …) and the workflow will attach all installers to a GitHub Release automatically.

---

## 💚 Credits

- **Iraqies** — founder & developer
- **GoldenGR** — auth provider
- **Hyloduck** — logo design

---

## 📄 License

Licensed under the [Creative Commons Attribution 4.0 International (CC BY 4.0)](LICENSE).

You are free to **fork, modify, and redistribute** this project — but you **must give credit** to the original author (iraqies / Skinart+) and link back to the original project. The "Skinart+" name and logo are not covered by this license.
