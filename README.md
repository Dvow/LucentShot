# LucentShot

A fast Windows screenshot tool. Capture a region, mark it up, then copy, save, print, or share it.

Lives in the tray. Press **Print Screen** (or left-click the tray icon) and drag.

## Features

- Full-screen overlay with region select and resize handles
- Pen, line, arrow, rectangle (outline or filled), marker, and text
- Copy, save, print, upload, and Google Lens search
- OCR and text-to-speech (default build)
- Customizable hotkeys, including instant fullscreen save/upload and copy focused window
- PNG, JPEG, BMP, and GIF export
- Settings for notifications, cursor capture, and a kept selection

## Usage

1. Run `lucentshot.exe`. It stays in the system tray.
2. Press **Print Screen** or left-click the tray icon.
3. Drag to select a region. Escape cancels.
4. Use the side tools to annotate, then the bottom bar to export.

Right-click the tray icon for Settings or Exit.

### Overlay shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+C` | Copy |
| `Ctrl+S` | Save |
| `Ctrl+D` | Upload |
| `Ctrl+G` | Google Lens |
| `Ctrl+P` | Print |
| `Ctrl+A` | Select the whole screen |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| `Esc` | Close |

### Default global hotkeys

| Hotkey | Action |
| --- | --- |
| `Print Screen` | Open the overlay |
| `Shift+Print Screen` | Save a fullscreen shot |
| `Ctrl+Print Screen` | Upload a fullscreen shot |
| `Alt+Print Screen` | Copy the focused window |

Change these in **Settings → Hotkeys**.

## Install

Download a [release](https://github.com/Dvow/LucentShot/releases): the setup exe, or the portable zip. A new `version` in `Cargo.toml` on `main`/`master` publishes that release.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\Install-LucentShot.ps1
```

That puts LucentShot in `%LOCALAPPDATA%\Programs\LucentShot` and adds Start Menu + **Apps & features** uninstall. Or run `lucentshot.exe` from the zip; keep the DLLs and `tessdata` beside it.

```powershell
git clone https://github.com/Dvow/LucentShot.git
cd LucentShot
cargo build --release
.\installer\Install-LucentShot.ps1
```

`-Build` compiles first. Uninstall with `.\Install-LucentShot.ps1 -Uninstall`. OCR is on by default; `cargo run --release --no-default-features` turns it off.

## Settings

Config is stored at `%LOCALAPPDATA%\LucentShot\config.json`. You can point it at another path in Settings.

## License

LucentShot is [MIT](LICENSE). Icon and toolbar artwork includes [Lucide](https://lucide.dev) (ISC). Tesseract and Leptonica binaries are under their own licenses; see [THIRD_PARTY.md](THIRD_PARTY.md).
