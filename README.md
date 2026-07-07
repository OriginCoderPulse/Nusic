<div align="center">

# 🎵 Nusic

**A fast, beautiful terminal music player for your local library**

[![Version](https://img.shields.io/badge/version-0.1.3-blue?style=flat-square)](https://github.com/OriginCoderPulse/Nusic/releases/tag/v0.1.3)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey?style=flat-square)](#-installation)

[English](#-nusic) · [简体中文](./README.zh-CN.md)

</div>

---

## ✨ Features

| | |
|---|---|
| 🖥️ **Terminal UI** | Built with [ratatui](https://github.com/ratatui/ratatui) — responsive layout, rounded panels, live spectrum |
| 🎧 **Local playback** | MP3, FLAC, OGG, Opus, M4A, AAC, WAV, AIFF and more via Symphonia |
| 📂 **Auto library scan** | Watches `~/.music` — drop files in and they appear instantly |
| 🔀 **Smart queue** | Shuffle from current track, repeat off / all / one |
| 🔊 **System volume** | Adjust macOS / Linux system volume from the keyboard |
| 📝 **Lyrics** | Side-by-side `.lrc` sync when a matching file exists |
| 🏷️ **Metadata** | Reads embedded tags; falls back to `Artist - Title` filename parsing |

---

## 📸 Interface

<p align="center">
  <img src="docs/images/screenshot.png" alt="Nusic terminal UI — library, track info, lyrics, and player with spectrum visualizer" width="720">
</p>

Press **`K`** anytime for the in-app shortcut cheat sheet.

---

## 📦 Installation

### Homebrew (macOS / Linux)

```bash
brew tap OriginCoderPulse/HomeBrew-Tap
brew install nusic
```

### Build from source

Requires **Rust 1.75+**.

```bash
git clone https://github.com/OriginCoderPulse/Nusic.git
cd Nusic
cargo install --path .
```

---

## 🚀 Quick start

1. **Add music** — copy audio files into `~/.music` (created automatically on first launch).
2. **Launch** — run `nusic` in any terminal.
3. **Play** — use `j`/`k` to select a track, press `Enter` or `Space`.
4. **Open folder** — press `o` to reveal `~/.music` in Finder / file manager.

> 💡 **Tip:** Files without embedded tags (common with some download services) are parsed from filenames like `Artist - Title.ext`.

---

## ⌨️ Keyboard shortcuts

### Playback

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `Enter` | Play selected track |
| `n` / `]` | Next track |
| `p` / `[` | Previous track |

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Ctrl+u` / `Ctrl+d` | Scroll half a page |
| `PgUp` / `PgDn` | Jump 10 items |
| `Home` / `End` | First / last track |

### Modes & volume

| Key | Action |
|-----|--------|
| `s` | Toggle shuffle (random from current track) |
| `r` | Cycle repeat: Off → All → One |
| `h` `l` `←` `→` `,` `.` | System volume down / up |
| `+` / `-` | System volume up / down |

### Other

| Key | Action |
|-----|--------|
| `/` | Search library |
| `o` | Open music folder |
| `K` | Show / hide help |
| `q` / `Esc` / `Ctrl+s` | Quit |

---

## 🔀 Playback modes

| Mode | Behavior |
|------|----------|
| **Shuffle off** | Tracks play in library order; next/prev follow the list (wraps at ends when repeat-all is on) |
| **Shuffle on** | Queue reshuffles from the **current** track — you stay on what you're listening to |
| **Repeat off** | Stops at the last track |
| **Repeat all** | Loops the entire queue |
| **Repeat one** | Repeats the current track (disables shuffle) |

---

## 🏷️ Metadata & lyrics

### Tags

Nusic reads ID3, Vorbis, MP4/iTunes, and other tags via Symphonia. Missing fields display as **`--`** instead of placeholder text.

### Filename fallback

When tags are empty, the filename is parsed:

```
张碧晨 - 光的方向 (Live).m4a  →  Artist: 张碧晨  ·  Title: 光的方向 (Live)
```

### Lyrics (`.lrc`)

Place a sidecar file next to the audio track with the same base name:

```
~/.music/
├── 张碧晨 - 光的方向 (Live).m4a
└── 张碧晨 - 光的方向 (Live).lrc
```

---

## 🎵 Supported formats

`mp3` · `flac` · `ogg` · `opus` · `m4a` · `m4p` · `aac` · `wav` · `aiff` · `aif`

---

## 🛠️ Development

```bash
cargo build          # debug build
cargo run            # run from source
cargo build --release
```

Project layout:

```
src/
├── app.rs            # application state & event loop
├── audio/            # rodio + symphonia decoder
├── library/          # scan, metadata, lyrics, file watcher
├── player/           # queue, shuffle, repeat
├── system_volume.rs  # macOS / Linux volume control
└── ui/               # ratatui layout & widgets
```

---

## 📄 License

[MIT](LICENSE) © OriginCoderPulse

---

<div align="center">

**Enjoy your music in the terminal** 🎶

[Report an issue](https://github.com/OriginCoderPulse/Nusic/issues) · [简体中文文档](./README.zh-CN.md)

</div>
