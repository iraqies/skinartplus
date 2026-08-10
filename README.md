<div align="center">

# Skinart+

**Turn pixel art into Minecraft skins and upload them straight to NameMC**

</div>

Skinart+ is a desktop app that converts images into 64×64 Minecraft skin art. It supports NameMC account integration, batch generation across every layer combination, template base skins, and a custom watermark — all wrapped in a fast Tauri (Rust + WebView) shell.

---

## ✨ Features

- **Image → skin conversion** — pick any image and get a ready-to-use Minecraft skin
- **NameMC integration** — log in, preview your avatar, and upload skins with one click
- **Batch generation** — generate every possible art layer combination automatically
- **Template library** — hundreds of bundled base skins to build from
- **Watermark** — your art stays signed with the Skinart+ watermark
- **Fast & light** — built with Tauri, so it uses a fraction of the memory of Electron apps

---

## 🚀 Install

Prebuilt packages are produced automatically by GitHub Actions for every release.

### Windows

```
SkinartPlus_1.0.0_x64-setup.exe
```

Run the installer — no extra dependencies needed.

### Linux

Choose whichever fits your distro:

| Format | Command |
|--------|---------|
| **Flatpak** (works on **every** distro) | `flatpak install SkinartPlus.flatpak` then `flatpak run com.skinartplus.app` |
| **.deb** (Debian / Ubuntu / Mint) | `sudo dpkg -i skinartplus_1.0.0_amd64.deb` |
| **.rpm** (Fedora / RHEL / openSUSE) | `sudo rpm -i skinartplus-1.0.0-1.x86_64.rpm` |
| **AppImage** (any distro) | `chmod +x skinartplus_1.0.0_amd64.AppImage && ./skinartplus_1.0.0_amd64.AppImage` |

> The **Flatpak** and **AppImage** builds run on any modern Linux distribution without system package installs.

---

## 🛠 Build from source

Requires [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) LTS.

```bash
# install dependencies
npm install

# development
npm run tauri dev

# production build (bundles for your current OS)
npm run tauri build
```

### Linux system dependencies

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

---

## 📦 Packaging

This repository includes a [GitHub Actions workflow](.github/workflows/build.yml) that builds on every push and tag:

- Windows NSIS installer (`.exe`)
- Linux `.deb`, `.rpm`, and `.AppImage`
- A universal **Flatpak** bundle (`packaging/flatpak/`)

Tag a commit (`v1.0.1`, `v1.1.0`, …) and the workflow will attach all installers to a GitHub Release automatically.

---

## 📄 License

This project is private and not yet published under an open-source license.
