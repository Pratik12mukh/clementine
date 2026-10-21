# Clementine

Drag a file, tap a key, drop. An offline file converter for Windows.

Hold the files over the window and tap **Shift**, **Ctrl** or **Alt**: a wheel opens
around your cursor. Move onto an option and drop. Everything runs on your PC and
nothing is uploaded.

Clementine is an original app built with Tauri (Rust + a small web UI). It does the
conversions by driving free, well-known open-source tools.

## Using it

1. Drag one or more files (or folders) onto the window.
2. While still dragging, tap a key:

   | Key (default) | Wheel   | What it offers                                                          |
   |---------------|---------|-------------------------------------------------------------------------|
   | **Shift**     | Convert | Every format that file can be converted to                              |
   | **Ctrl**      | Tools   | Compress, trim, resize, crop, rotate, speed, merge, split, ... (by type) |
   | **Alt**       | Actions | Open, show in Explorer, copy path, copy to, move to, terminal, float    |

3. Move the cursor onto an option and **drop** to run it. Tapping another key switches wheel.
   Dropping between options keeps the wheel open so you can click or use arrows + Enter.
   Esc, or dragging out of the window, cancels.

Output goes next to the original and never overwrites anything (`photo.webp`, then `photo (1).webp`).
Folders and multi-file selections work too (see Merge below). Keys can be remapped in **Settings**.

## What it can do

**519 conversion options** (each input format to each output format) and **20 file tools**.

| Type         | Input                                                    | Output                                                       | Engine |
|--------------|----------------------------------------------------------|--------------------------------------------------------------|--------|
| Images       | JPG PNG WebP TIFF AVIF BMP GIF ICO SVG HEIC              | JPG PNG WebP TIFF AVIF BMP GIF ICO PDF, GIF to MP4/WebM      | FFmpeg, LibreOffice (SVG/PDF), ImageMagick for HEIC if present |
| Video        | MP4 MOV MKV WebM AVI WMV M4V FLV MPG 3GP TS              | MP4 WebM MOV MKV AVI WMV GIF, or audio-only MP3 M4A WAV FLAC OGG Opus | FFmpeg |
| Audio        | MP3 M4A WAV FLAC OGG Opus AIFF WMA AAC AMR               | MP3 M4A WAV FLAC OGG Opus AIFF WMA                           | FFmpeg |
| PDF          | PDF                                                      | DOCX PPTX XLSX ODT EPUB TXT MD, one JPG/PNG per page         | Ghostscript, LibreOffice, Pandoc |
| Documents    | DOCX DOC MD TXT EPUB RTF HTML ODT                        | DOCX PDF MD TXT HTML EPUB RTF ODT                            | Pandoc, LibreOffice |
| Slides       | PPT PPTX ODP                                             | PDF PPTX ODP, one JPG/PNG per slide                          | LibreOffice, Ghostscript |
| Spreadsheets | XLSX XLS ODS CSV TSV                                     | XLSX ODS CSV TSV PDF HTML                                    | LibreOffice |
| Subtitles    | SRT VTT ASS SSA                                          | SRT VTT TXT                                                  | FFmpeg |
| Archives / folders | ZIP TAR GZ TGZ 7Z RAR BZ2 XZ, or any folder        | ZIP TAR TAR.GZ TAR.XZ 7Z                                     | 7-Zip  |

**File tools:** compress, trim, resize, crop, rotate, flip, grayscale, speed, volume, normalize,
fade, reverse, mute, extract audio, thumbnail, split (audio/video by length, PDF by page),
extract PDF pages, extract archive, strip metadata, and **merge** (select several PDFs, audio
clips or videos; several images become one PDF).

## Setup

Clementine needs five free programs. The coloured badges at the top of the window show which
are found (click a red one to install it with winget), or run once in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/install-tools.ps1
```

FFmpeg, Pandoc, LibreOffice, Ghostscript and 7-Zip. A missing tool only disables the
conversions that need it. Clementine also finds them in their default install folders, so no
restart or PATH editing is needed. You can bundle them next to `clementine.exe` in a `tools` folder.

## Build

Needs [Rust](https://rustup.rs), [Node.js](https://nodejs.org), the "Desktop development with
C++" workload from Visual Studio Build Tools, and WebView2 (built in to Windows 11).

```powershell
npm install
npm run dev      # run with live reload
npm run build    # installers in src-tauri/target/release/bundle/ (.exe and .msi)
```

No Windows machine at hand? Push the project to GitHub: `.github/workflows/windows.yml` builds
the installer on a Windows runner (download it from the run's Artifacts) and runs the engine
tests against the real tools there.

## Tests

```powershell
npm run test:ui       # 54 checks of the wheel, drag flow, settings and tool installer (jsdom)
npm run test:engine   # 13 tests running the real converters (needs the five tools)
```

## What is and isn't verified

Verified on Linux with the real tools: every conversion path and file tool listed above (the
engine tests), the whole UI behaviour (a simulated Tauri bridge), and the full Tauri app compiling.

**Not yet run on Windows** (the build environment was Linux): the Windows-only pieces. These are the
live Shift/Ctrl/Alt reader (`GetAsyncKeyState` in `src-tauri/src/main.rs`), the winget installer,
Windows tool-folder discovery, "Show in Explorer" / "Terminal here", and the always-on-top toggle.
They are small and the CI workflow compiles and tests them on Windows, but please try a real drag
before relying on them. If the wheel does not open mid-drag, click a tab after dropping and
open an issue with your Windows version.

## Known limits

- **PDF to DOCX/PPTX** goes through LibreOffice's PDF import: text and simple layouts survive,
  complex layouts will not match exactly. **PDF to XLSX** rebuilds tables from the text layout.
  **Scanned PDFs** have no text layer, so TXT/MD/XLSX come out empty (no OCR included).
- **HEIC** needs an FFmpeg build with HEIF support, or ImageMagick installed.
- **Slides to JPG/PNG** and **PDF to JPG/PNG** write one image per page into a new folder.
- **Float** keeps the window on top of others; there is no file shelf and no AirDrop equivalent.
- Output always goes next to the source file (no custom output folder yet).
- Tools such as trim/resize apply to images, audio, video and PDFs; documents and spreadsheets only convert.

## Layout

```
src/                          web UI: index.html, style.css, app.js (no bundler)
src-tauri/src/converter.rs    conversion engine + tests (std only)
src-tauri/src/main.rs         Tauri commands, live key reader, folder picker, always-on-top
src-tauri/tauri.conf.json     window + installer config
scripts/install-tools.ps1     winget installer for the helper tools
tests/ui.test.js              UI tests
.github/workflows/windows.yml installer build + engine tests on Windows
```
