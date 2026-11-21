# Porting notes

This project uses the supplied CropStudio batch-cropper build as the behavioral reference. The Python/FastAPI service has been replaced by a Tauri/Rust desktop process; no Python runtime or local HTTP server is used by the Rust app.

| Existing workflow | Rust edition implementation |
|---|---|
| Image/video queue | Native multi-file picker + Tauri drag/drop event |
| Per-file crop state | Queue item state persisted in localStorage; source media copied into app data |
| Crop handles and aspect presets | Frontend pointer-based crop editor with eight handles and independent per-file geometry |
| Metadata | Rust invokes FFprobe and parses JSON |
| Still-image crop | Rust `image` crate decode/crop/encode path |
| Video crop and trim | Rust orchestrates FFmpeg with crop filter and H.264 / VP9 output profiles |
| Auto crop | Rust time-window sampler + FFmpeg cropdetect; static and adaptive plans |
| Processing progress | FFmpeg `-progress` output forwarded as Tauri events |
| Batch processing | Sequential Rust jobs coordinated by the UI, with overall progress |
| ZIP output | Rust `zip` crate with streaming file copies (no full-file buffering) |
| Download/export | Native save dialog and Rust file copy |

## Intentionally not included in this first port

- Original thumbnail-strip generator and audio waveform UI.
- Original multi-segment cut editor. The current timeline exposes a single in/out range for videos.
- Adaptive semantic subject tracking. The retained motion-edge mode is a cropdetect heuristic, not AI tracking.

The supplied source archive did not include a clear upstream license file. Verify licensing before redistribution.
