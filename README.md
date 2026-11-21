# CropStudio Pro — Rust Edition 🎬📐

[![Tauri v2](https://img.shields.io/badge/Tauri-v2.0-blue.svg?logo=tauri)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-2021%20Edition-DEA584.svg?logo=rust)](https://www.rust-lang.org/)
[![Vite](https://img.shields.io/badge/Vite-6.0-646CFF.svg?logo=vite)](https://vitejs.dev/)
[![FFmpeg](https://img.shields.io/badge/FFmpeg-Engine-007808.svg?logo=ffmpeg)](https://ffmpeg.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-brightgreen.svg)]()
[![Privacy](https://img.shields.io/badge/Privacy-100%25%20Local%20First-success.svg)]()

> **Make the frame yours.** A lightning-fast, private, local-first media cropper and studio built with **Tauri 2**, **Rust**, and **FFmpeg**. Features a tactile Neo-Brutalist interface, hardware-accelerated video encoding, intelligent auto-reframing, 3D LUT color grading, and streaming batch export.

---

## 🌟 Key Features

### 🗂️ Media Queue & Workflows
* **Drag-and-Drop & File Picker:** Import multi-file stacks of images (`JPG`, `PNG`, `WebP`, `TIFF`, `BMP`), videos (`MP4`, `MOV`, `MKV`, `WebM`), and audio tracks.
* **On-Device Previews:** Instant local thumbnails generated via Rust's `image` crate and early-timestamp extraction via FFmpeg.
* **Live Search & Filter:** Instantly filter queue by media type (Images, Videos, Audio) and search by file name.
* **Independent Per-File State:** Crop geometry, aspect presets, color adjustments, trim points, and export settings are saved independently per file and persisted locally across app launches.
* **Cross-Item Copy & Apply:** Copy a crop frame with `Ctrl+C` and apply it to every image and video in the queue with `Ctrl+Shift+V`.

### 📐 Interactive Canvas & Framing
* **8-Handle Crop Box:** Smooth pointer-based moving, handle resizing, or freehand drawing of custom crop boxes.
* **Aspect Presets:** One-click snapping for `Free`, `1:1`, `4:5`, `3:4`, `9:16`, `16:9`, `3:2`, and `21:9`.
* **Social Media Framing:** Instant aspect matching for **Instagram Portrait Post** (4:5), **Story / Reel / TikTok** (9:16), **YouTube Thumbnail** (16:9), **Facebook Cover** (2.63:1), and **X / Twitter Post** (16:9).
* **Numeric Controls:** Direct pixel-level control over X, Y, Width, and Height coordinates, with automatic canvas scaling and Rule-of-Thirds grid guidelines.

### ⏱️ Video Trimming & Visual Filmstrip
* **8-Frame Filmstrip Preview:** Extracts representative frames across video duration. Click any frame to seek the player directly to that moment.
* **Dual IN / OUT Trimmers:** Interactive dual-thumb range slider and numeric seconds inputs.
* **Hold to Compare:** Hold down the **HOLD TO COMPARE ◉** button after export to instantly preview the cropped result against the source file.

### 🎨 Color Grading & Looks
* **Real-time Looks:** Warm Film, Cool Tone, Vintage Fade, Cinematic, and Monochrome presets.
* **Custom 3D LUTs:** Load your own `.cube` 3D LUT files per media item applied directly via FFmpeg's `lut3d` engine.
* **Precision Adjustments:** Sliders for Brightness, Contrast, Saturation, and Sharpness (`unsharp` filter).
* **Watermark Overlay:** Text/caption overlays rendered via FFmpeg `drawtext`.

### ⚡ Smart Auto-Reframe
* **Black-Bar Detection:** Automatically identifies and crops out letterboxes/pillarboxes across sample windows.
* **Motion-Aware Tracking:** Detects active motion regions using motion vectors (`mvedges`) with smoothed transitions between sample windows to follow changing focal points.

### 🚀 Pro Export & Hardware Acceleration
* **Hardware Video Encoding:** Auto-probes and verifies hardware acceleration at runtime:
  * **NVIDIA NVENC** (`h264_nvenc`)
  * **Intel Quick Sync** (`h264_qsv`)
  * **AMD AMF** (`h264_amf`)
  * Automatic graceful fallback to CPU **x264** (`libx264`) if hardware acceleration is unavailable.
* **Resolution Scaling:** Source size, 720p, 1080p, 1440p (2K), and 2160p (4K).
* **Bitrate & Framerate Controls:** 4, 8, 16, or 25 Mbps target video bitrates + 24, 25, 30, 50, or 60 FPS conversion.
* **Batch & Streaming ZIP:** Run sequential exports with live progress feedback, or bundle all finished items into a single `.zip` file streamed without RAM buffering.

---

## 🏗️ Architecture

```text
┌────────────────────────────────────────────────────────┐
│               Tauri WebView Frontend                   │
│   • Neo-Brutalist UI (CSS variables, paper & ink)      │
│   • 8-handle Crop Overlay & Interactive Gestures       │
│   • LocalStorage state persistence                     │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (invoke / listen)
┌───────────────────────────▼────────────────────────────┐
│                    Rust Desktop Core                   │
│   • Sandbox storage (AppData uploads & outputs)        │
│   • Still image processing (Rust `image` crate)        │
│   • Streaming archive bundler (Rust `zip` crate)       │
│   • Auto-crop heuristic & motion smoothing             │
└───────────────────────────┬────────────────────────────┘
                            │ Subprocess spawn & pipes
┌───────────────────────────▼────────────────────────────┐
│                  FFmpeg & FFprobe CLI                  │
│   • Stream probe & JSON metadata                       │
│   • Hardware encoder test probe & encoding             │
│   • Filter chains: crop, unsharp, eq, lut3d, drawtext  │
│   • Filmstrip & video thumbnail extraction             │
└────────────────────────────────────────────────────────┘
```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|:---|:---|
| `Ctrl / ⌘ + O` | Open file picker to import images & videos |
| `Ctrl / ⌘ + C` | Copy active crop rectangle & aspect ratio |
| `Ctrl / ⌘ + Shift + V` | Apply copied crop frame to all other queue items |
| `Shift + ←` | Navigate to previous queue item |
| `Shift + →` | Navigate to next queue item |
| `Escape` | Switch from Draw Crop mode back to Move/Resize mode |

---

## 📋 Prerequisites

1. **Rust Toolchain:** Stable Rust with MSVC on Windows (`x86_64-pc-windows-msvc`).
   * Install via [rustup.rs](https://rustup.rs/).
2. **Node.js:** Node.js 20+ or 22 LTS with `npm`.
   * Install from [nodejs.org](https://nodejs.org/).
3. **FFmpeg & FFprobe:** Must be available on system `PATH`.
   * **Windows:** Run the included PowerShell script:
     ```powershell
     powershell -ExecutionPolicy Bypass -File install-ffmpeg.ps1
     ```
     *Or via winget:*
     ```powershell
     winget install Gyan.FFmpeg
     ```
   * **macOS:** `brew install ffmpeg`
   * **Linux:** `sudo apt install ffmpeg`

Verify installations:
```bash
ffmpeg -version
ffprobe -version
cargo --version
node --version
```

---

## 🛠️ Getting Started

### 1. Clone the repository
```bash
git clone https://github.com/YOUR_USERNAME/CropStudio-Pro.git
cd CropStudio-Pro
```

### 2. Install dependencies
```bash
npm install
```

### 3. Run in development mode
* **Windows (recommended):**
  ```cmd
  run-windows-dev.bat
  ```
  *(Or execute directly via `npm run tauri dev`)*

---

## 📦 Building Standalone Installers

To produce a production `.msi` or `.exe` (NSIS) installer:

* **Windows (recommended):**
  ```cmd
  build-windows.bat
  ```
  *(Or via `npm run tauri build`)*

The compiled installers are emitted under:
```text
%LOCALAPPDATA%\CropStudio\cargo-target\release\bundle\
```

> **Note on Windows Watcher & EBUSY:** The Windows launcher scripts automatically direct Cargo builds to `%LOCALAPPDATA%\CropStudio\cargo-target` while `vite.config.js` ignores `src-tauri/**`. This prevents file-watcher lock collisions on Windows executables during live development.

---

## 📂 Project Structure

```text
CropStudio-Pro/
├── build-windows.bat          # One-click Windows build script
├── run-windows-dev.bat        # One-click Windows development script
├── install-ffmpeg.ps1         # FFmpeg installation helper script
├── index.html                 # Main application UI layout
├── vite.config.js             # Vite configuration with Tauri exclusions
├── package.json               # Frontend dependencies & scripts
├── PORTING-NOTES.md           # Architecture notes on Python-to-Rust migration
├── UPSTREAM-NOTICE.md         # Upstream provenance & attribution
├── src/
│   ├── main.js                # State engine, UI binding & Tauri IPC
│   └── style.css              # Neo-Brutalist design system
└── src-tauri/
    ├── Cargo.toml             # Rust crate definitions & dependencies
    ├── build.rs               # Tauri build hooks
    ├── tauri.conf.json        # Tauri window, security & packaging configuration
    ├── capabilities/          # Tauri v2 permission definitions
    ├── icons/                 # Application icons (ICO, PNG, ICNS)
    └── src/
        └── main.rs            # Backend engine (FFmpeg orchestrator, storage, filters)
```

---

## 🔒 Privacy & Local Processing

CropStudio Pro runs **100% on your local machine**:
* No cloud servers, accounts, or telemetry.
* No media is ever uploaded to external servers.
* Files are staged locally in your system's temporary AppData directory and can be cleared at any time directly from the queue.

---

## 📄 License & Attribution

This project is adapted from the Cropper reference architecture by `cocktailpeanut/cropper`. Please refer to [UPSTREAM-NOTICE.md](UPSTREAM-NOTICE.md) and verify upstream licensing conditions prior to commercial redistribution.
