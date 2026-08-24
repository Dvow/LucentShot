# LucentShot

A fast Windows screenshot tool. Capture a region, mark it up, then copy, save, print, or share it.

It lives in the tray. Press **Print Screen** or left-click the tray icon, then drag.

## Features

- Full-screen overlay with region select and resize handles
- Pen, line, arrow, rectangle (outline or filled), marker, and text
- Copy, save, print, upload, and Google image search
- OCR and text-to-speech (default build)
- Customizable hotkeys, including instant fullscreen save/upload and copy focused window
- PNG, JPEG, BMP, and GIF export
- Start with Windows, action notifications, cursor capture, and a remembered selection

## Usage

1. Run LucentShot. It stays in the system tray.
2. Press **Print Screen** or left-click the tray icon.
3. Drag to select a region. Escape cancels.
4. Annotate with the side tools, then use the bottom bar to export.

Right-click the tray icon for Settings or Exit.

### Overlay shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+C` | Copy |
| `Ctrl+S` | Save |
| `Ctrl+D` | Upload |
| `Ctrl+G` | Google image search |
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

Download a [release](https://github.com/Dvow/LucentShot/releases): the setup exe, or the portable zip.

The setup installs to `%LOCALAPPDATA%\Programs\LucentShot`, adds a Start Menu shortcut, and registers uninstall in **Apps**. After that you can find it from Start search. A desktop icon is optional.

The portable zip is the same files with no Start Menu entry. Keep `lucentshot.exe`, the DLLs, and `tessdata` together.

Bumping `version` in `Cargo.toml` on `main`/`master` publishes that release.

### From source

```powershell
git clone https://github.com/Dvow/LucentShot.git
cd LucentShot
cargo build --release
.\installer\Install-LucentShot.ps1
```

`-Build` compiles first. Uninstall with `.\Install-LucentShot.ps1 -Uninstall`.

OCR is on by default. `cargo run --release --no-default-features` turns it off.

## Settings

Right-click the tray icon and choose Settings.

- **General** — start with Windows, capture options, sharing, and the config file path
- **Hotkeys** — overlay and instant-action shortcuts
- **Formats** — upload format and JPEG quality
- **Speech** — voice, rate, and volume (OCR builds)

Config is stored at `%LOCALAPPDATA%\LucentShot\config.json` unless you set another path.

## License

LucentShot is [MIT](LICENSE). Lucide icons, Tesseract, and Leptonica are noted there too.
