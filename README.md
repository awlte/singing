<div align="center">

# 🎙 Singing

**A desktop toy that quietly records the last N minutes of your mic — open it when you remember, save the good parts.**

[![Platform](https://img.shields.io/badge/platform-macOS-lightgrey)](#)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](#license)

**English** · [简体中文](README_zh.md)

</div>

---

## Why this exists

You hum a melody you like — by the time you open a recording app, ten seconds have passed and the moment is gone.

Singing is a small menubar app that **silently records the last 10 minutes of your microphone** to memory. When you remember, open the window, scrub the waveform, pick a region, save it as WAV.

No AI, no cloud, no transcription. One icon, one window, one folder.

> Built for myself as a toy, not a product.

---

## Screenshot

<p align="center"><img src="docs/screenshot.png" width="720" alt="Singing main window" /></p>

---

## Features

- 🎚️ **10-minute rolling buffer** (configurable: 5 / 15 / 30 / 60 min)
- 📈 **Live waveform** with drag-to-select and region playback
- 💾 **Save selection or full buffer** as WAV (16 kHz mono)
- ⚙️ **Configurable**: input device, save folder, buffer length, default play length
- 🖥 **Menubar tray** — closing the window hides it without stopping capture
- 🔒 **Local-only**: no network, no upload, no analysis
- 🪶 **Lightweight**: ~20 MB RAM for a 10-min buffer, native Rust binary

---

## Install

Grab the latest `.zip` from [Releases](https://github.com/awlte/singing/releases) (macOS arm64), or [build from source](#build-from-source).

```
unzip Singing-0.1.0-aarch64.zip
mv Singing.app /Applications/
# the app isn't signed — right-click → Open the first time, or:
xattr -d com.apple.quarantine /Applications/Singing.app
```

First launch prompts for microphone access — allow it.

---

## Usage

| Action | How |
|---|---|
| See the waveform | Window appears on launch |
| Select a region | Drag on the waveform |
| Clear selection | Click the waveform, or press `Esc` |
| Play | `▶ Play` or `Space` (plays last N seconds when nothing selected) |
| Save | `⬇ Save` (selection if any, otherwise full buffer) or `⌘S` |
| Open save folder | `📁 Folder` |
| Open settings | `⚙` |
| Hide window, keep recording | Close the window (hides to tray) |
| Show again | Click the tray icon |
| Quit | Tray → Quit |

---

## Settings

Click `⚙` in the toolbar to open the popover:

| Option | Default | Notes |
|---|---|---|
| Input device | System default | Switching clears the buffer |
| Save folder | `~/Music/Captures` | Type a path, or pick via 📁 |
| Buffer length | 10 min | 5 / 10 / 15 / 30 / 60 min (clears buffer on change) |
| Default play length | 30 s | How much to play when no region is selected |

Persisted to:
```
~/Library/Application Support/com.singing.toy/config.json
```

---

## Build from source

Requirements: [Rust](https://rustup.rs) (stable), Node 20+, [bun](https://bun.sh) (or npm).

```bash
git clone https://github.com/awlte/singing.git
cd singing
bun install                              # installs the Tauri CLI
bun tauri build --debug --bundles app    # output: src-tauri/target/debug/bundle/macos/
open src-tauri/target/debug/bundle/macos/Singing.app
```

**Dev mode** (hot reload — mic permission inherits from the parent terminal):

```bash
bun tauri dev
```

**Release build:**

```bash
bun tauri build --bundles app
# → src-tauri/target/release/bundle/macos/Singing.app
```

---

## Project layout

<details>
<summary>Expand</summary>

```
singing/
├── README.md                ← English
├── README_zh.md             ← Chinese
├── package.json             ← Tauri CLI
├── dist/                    ← Frontend (plain HTML/CSS/JS, no build step)
│   ├── index.html
│   ├── styles.css
│   └── main.js
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── Info.plist           ← macOS microphone usage description
    ├── capabilities/        ← Tauri 2 permissions
    ├── icons/
    └── src/
        ├── main.rs          ← Entry point
        ├── lib.rs           ← Tauri commands · tray · window
        ├── audio.rs         ← cpal capture · ring buffer · WAV
        └── config.rs        ← Settings load/save
```

</details>

---

## Tech stack

| Layer | Choice | Notes |
|---|---|---|
| App framework | [Tauri 2](https://tauri.app) | Rust backend + WebView frontend |
| Audio capture | [cpal](https://crates.io/crates/cpal) | Cross-platform microphone |
| WAV encoding | [hound](https://crates.io/crates/hound) | Zero-dep PCM |
| Folder picker | tauri-plugin-dialog | Native dialog |
| Waveform | Canvas 2D | Hand-rolled, ~30 lines |
| Playback | HTML5 `<audio>` + Blob URL | WAV bytes streamed back via Tauri IPC |

Rust backend: ~400 LoC. Frontend: ~250 LoC.

---

## What it doesn't do (on purpose)

- ❌ Silence detection / auto-clipping
- ❌ Transcription / AI analysis
- ❌ Cloud sync / multi-device
- ❌ Mobile
- ❌ File management UI (everything goes to one folder — use Finder)

These are deliberate omissions, not unfinished work. For more featureful tools see [ListenBack](https://apps.apple.com/app/listenback) or [MonkeyC Rewind](https://www.monkeyc.com).

---

## Maybe later

- [ ] Amplitude-threshold markers on the waveform (where the voice is)
- [ ] Global hotkey to save the last N seconds without opening the window
- [ ] Windows / Linux builds

---

## License

[MIT](LICENSE)
