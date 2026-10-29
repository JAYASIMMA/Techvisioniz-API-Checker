# Techvision API Checker

A Postman-style HTTP client for **Windows, Linux and macOS**, written in Rust with
[egui](https://github.com/emilk/egui)/eframe. One self-contained binary — no Electron, no runtime to
install, no telemetry — plus real installers for every platform.

**Latest release:** <https://github.com/JAYASIMMA/Techvisioniz-API-Checker/releases/latest>
(currently **v1.0.1**)

---

## Contents

- [Features](#features)
- [Install](#install)
  - [Windows](#windows)
  - [macOS](#macos)
  - [Linux](#linux)
  - [Verify a download](#verify-a-download)
- [Build from source](#build-from-source)
- [Repository layout](#repository-layout)
- [How the installers are made](#how-the-installers-are-made)
- [Settings and data](#settings-and-data)
- [Themes](#themes)
- [Tests](#tests)
- [Troubleshooting](#troubleshooting)
- [Licence](#licence)

---

## Features

| Area | What you get |
| --- | --- |
| **Requests** | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE; unlimited request tabs; Ctrl+Enter or F5 to send |
| **Builder** | Query-params grid with "split query out of URL", headers grid with presets, raw JSON body with format/minify, `x-www-form-urlencoded` |
| **Auth** | None / bearer / basic / API key (as header or query param) — all support `{{variables}}` |
| **Per-request settings** | Timeout, redirect policy, TLS verification, cookie jar |
| **Collections** | Folders, collection variables, save/update/delete, Postman **v2.1 JSON import and export** |
| **Environments** | Named variable sets with `{{name}}` substitution; the active environment overrides collection variables |
| **Responses** | Status, time, size, content type; Pretty / Raw / Headers / Cookies / Code / Meta views; copy or save the body |
| **Code generation** | cURL (bash), cURL (Windows), PowerShell, Python `requests`, JavaScript `fetch` |
| **History** | Last 200 requests, filterable, one-click re-run |
| **Themes** | VS Code Dark+, VS Code Light+, or follow the OS setting live |
| **Headless mode** | `--selftest [base-url]` runs real requests and exits non-zero on failure — usable in CI |

Not supported, by design: JavaScript pre-request/test scripts, WebSocket / SSE / gRPC, multipart file
upload, OAuth 2.0 authorization-code flows, mock servers, cloud sync.

---

## Install

Grab files from the [releases page](https://github.com/JAYASIMMA/Techvisioniz-API-Checker/releases/latest).
File names follow `Techvision-Api-Checker-<version>-<platform>.<ext>`.

### Windows

| File | Use it when |
| --- | --- |
| `Techvision-Api-Checker-1.0.1-x64.msi` | **Almost always.** 64-bit Windows 10/11 (also correct for Windows on ARM) |
| `Techvision-Api-Checker-1.0.1-x86.msi` | Only for a genuinely 32-bit Windows install |
| `techvision-api-checker.exe` | No installer wanted: this single file runs as-is |

```powershell
# Install (per-user: no administrator rights, no UAC prompt)
msiexec /i Techvision-Api-Checker-1.0.1-x64.msi

# Silent install, or uninstall
msiexec /i Techvision-Api-Checker-1.0.1-x64.msi /qn
msiexec /x Techvision-Api-Checker-1.0.1-x64.msi
```

What the installer does:

- Installs to `%LOCALAPPDATA%\Techvision API Checker (64-bit)` — per-user, so it never needs admin.
- Creates Start Menu and Desktop shortcuts, and appears in **Settings → Installed apps** with its icon.
- Shows the licence agreement first; installation stops unless you accept it.
- Installing a newer version replaces the older one in place (32- and 64-bit are independent).

**SmartScreen:** builds are unsigned, so the first run may warn. Choose *More info → Run anyway*.
Run the portable `.exe` instead if you prefer to skip installing.

### macOS

| File | Notes |
| --- | --- |
| `Techvision.API.Checker-1.0.1-macos.dmg` | Drag-to-Applications disk image, universal binary (Apple Silicon + Intel) |
| `Techvision.API.Checker-1.0.1-macos.tar.gz` | Same `.app`, no disk image |

> The bundle inside is still `Techvision API Checker.app`. GitHub replaces spaces with dots when it
> uploads release assets, so the downloaded file names differ from the local build output.

```bash
# 1. Move it to Applications
hdiutil attach "Techvision.API.Checker-1.0.1-macos.dmg"
cp -R "/Volumes/Techvision API Checker/Techvision API Checker.app" /Applications/
hdiutil detach "/Volumes/Techvision API Checker"

# 2. First launch: the build is unsigned and un-notarised
open "/Applications/Techvision API Checker.app"
```

If Gatekeeper blocks it, right-click the app → **Open**, or clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine "/Applications/Techvision API Checker.app"
```

Needs macOS 11 or later.

### Linux

| Distro | File | Install | Remove |
| --- | --- | --- | --- |
| Debian, Ubuntu, Linux Mint, Pop!_OS, Zorin | `techvision-api-checker_1.0.1_amd64.deb` | `sudo apt install ./techvision-api-checker_1.0.1_amd64.deb` | `sudo apt remove techvision-api-checker` |
| Fedora, CentOS Stream, Rocky, Alma | `techvision-api-checker-1.0.1-1.x86_64.rpm` | `sudo dnf install ./techvision-api-checker-1.0.1-1.x86_64.rpm` | `sudo dnf remove techvision-api-checker` |
| openSUSE | same `.rpm` | `sudo zypper in ./techvision-api-checker-1.0.1-1.x86_64.rpm` | `sudo zypper rm techvision-api-checker` |
| Arch, Manjaro, NixOS, anything else | `techvision-api-checker-1.0.1-x86_64.AppImage` | see below | delete the file |

```bash
# Debian/Ubuntu — note the ./ so apt resolves the local file
sudo apt install ./techvision-api-checker_1.0.1_amd64.deb

# Fedora
sudo dnf install ./techvision-api-checker-1.0.1-1.x86_64.rpm
```

**AppImage** works on any distro without installing:

```bash
chmod +x techvision-api-checker-1.0.1-x86_64.AppImage
./techvision-api-checker-1.0.1-x86_64.AppImage
```

On Debian/Ubuntu an AppImage needs FUSE: `sudo apt install libfuse2` (22.04+) or
`sudo apt install libfuse2t64` (24.04+). Without root, run it as
`./techvision-api-checker-1.0.1-x86_64.AppImage --appimage-extract-and-run`.

Both packages install the binary as `/usr/bin/techvision-api-checker`, a `.desktop` launcher, and hicolor
icons from 16 to 256 px, so it appears in your applications menu with its own icon.

> **Not yet published:** FlatHub, Snap Store, Homebrew, AUR and the `winget` community repository. Until
> then, use the files above.

### Verify a download

Every release attaches `SHA256SUMS.txt`:

```bash
sha256sum -c --ignore-missing SHA256SUMS.txt          # Linux, macOS
```

```powershell
Get-FileHash .\Techvision-Api-Checker-1.0.1-x64.msi   # compare against SHA256SUMS.txt
```

---

## Build from source

Requires [Rust](https://rustup.rs) stable (MSVC toolchain on Windows).

```bash
git clone https://github.com/JAYASIMMA/Techvisioniz-API-Checker.git
cd Techvisioniz-API-Checker
cargo run --release
```

System libraries needed before the first Linux build:

```bash
sudo apt install -y libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libwayland-dev libdrm-dev \
  libgbm-dev libfontconfig1-dev pkg-config
```

Then produce packages for your OS:

| Goal | Command | Output |
| --- | --- | --- |
| Windows MSI (x64 + x86) | `powershell -ExecutionPolicy Bypass -File build-installer.ps1` | `dist\…-x64.msi`, `dist\…-x86.msi` |
| Windows MSI (one arch) | `… build-installer.ps1 -Arch x64` | one MSI |
| Repackage only | `… build-installer.ps1 -SkipExeBuild` | MSIs, no rebuild |
| Linux deb + rpm + AppImage | `bash packaging/linux/build-linux-packages.sh` | `dist/linux/` |
| macOS app + dmg | `bash packaging/macos/build-macos.sh` (or `universal`) | `dist/macos/` |
| Headless sanity check | `cargo run --release -- --selftest https://httpbin.org` | prints request results |

---

## Repository layout

```
src/
  main.rs           app state, tabs, panels, dialogs, shortcuts, --selftest, tests
  theme.rs          VS Code Dark+ / Light+ schemes and the accent palette
  model.rs          request/response types, URL + query building, {{variable}} substitution
  send.rs           HTTP execution: auth, cookie jar, proxy, redirects, TLS, timing
  collections.rs    collections, folders, Postman v2.1 import/export
  curl.rs           code generation
  storage.rs        persistence of the workspace file
build.rs            embeds the icon and version metadata into the Windows exe
assets/             icon master, .ico, licence RTF, wizard art, make-icon.ps1
tools/              generators: linux icons, licence RTF, wizard bitmaps
wix/main.wxs        Windows installer definition (per-architecture)
wix/inspect-msi.ps1 reads an MSI's tables to verify it without installing
packaging/linux/    .desktop file, build-linux-packages.sh
packaging/macos/    build-macos.sh
.github/workflows/  release.yml — builds Windows, Linux and macOS artifacts
LICENSE.md          the agreement shown by the installers
build-installer.ps1 one-command Windows build
```

---

## How the installers are made

### Windows

Uses **WiX v5** (v7 requires accepting a paid maintenance-fee agreement, so this project pins v5):

```powershell
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2
```

`build-installer.ps1` builds each architecture, **reads the PE header of the produced exe and refuses to
package it** unless the machine word matches the requested architecture, then runs WiX. It regenerates
every derived asset (icon, licence RTF, wizard art) first, so an installer can never ship a stale
agreement or icon.

- The licence page comes from `LICENSE.md` → `tools/make-license-rtf.ps1` → `assets/license.rtf`.
- Wizard art is generated at WiX's exact sizes (banner 493×58, dialog 493×312) and is deliberately free
  of text outside the brand band, because WiX paints its own labels over those regions.
- The installer version is read from `Cargo.toml`, so bumping `version` updates exe, MSI and packages
  together.
- 32- and 64-bit packages use different upgrade codes and folders so installing one never removes the
  other.

To check an MSI without installing it:

```powershell
powershell -ExecutionPolicy Bypass -File wix\inspect-msi.ps1 -Msi dist\Techvision-Api-Checker-1.0.1-x64.msi
```

### Linux

`cargo-deb` and `cargo-generate-rpm` read their metadata straight from `Cargo.toml`
(`[package.metadata.deb]`, `[package.metadata.generate-rpm]`). The AppImage is assembled as a standard
`AppDir` with an `AppRun` launcher; if `appimagetool` is absent the script emits an `AppDir` tarball
instead of failing.

### macOS

`packaging/macos/build-macos.sh` builds the binary, assembles a proper `.app` bundle with an
`Info.plist`, generates `AppIcon.icns` from the same master PNG via `sips`/`iconutil`, ad-hoc signs it,
and produces both a `.dmg` (with an Applications symlink) and a `.tar.gz`. Pass `universal` to lipo
arm64 and x86_64 into one binary.

### Continuous integration

[`.github/workflows/release.yml`](.github/workflows/release.yml) runs `cargo test` and builds:

- **windows (x64)** and **windows (x86)** on `windows-latest`
- **linux** (deb, rpm, AppImage) on `ubuntu-24.04`
- **macOS** (universal app + dmg) on `macos-14`

…then publishes everything, with `SHA256SUMS.txt`, to a GitHub release on any `v*` tag. It can also be
run manually from the Actions tab.

---

## Settings and data

Everything — collections, environments, history, theme, proxy — is stored locally, and the workspace is
auto-saved every 20 seconds when something changed, plus on quit and with Ctrl+Shift+S:

| OS | Path |
| --- | --- |
| Windows | `%APPDATA%\TechvisionApiChecker\workspace.json` |
| Linux / macOS | `~/.config/TechvisionApiChecker/workspace.json` |

There is no account, no sync and no network traffic except the requests you choose to send.

---

## Themes

**View → Theme** or the Settings dialog offers *Light (VS Code)*, *Dark (VS Code)* and *Match Windows*
(which tracks the OS setting live). Colours are the real VS Code tokens — editor `#1E1E1E`/`#FFFFFF`,
panels `#252526`/`#F3F3F3`, focus `#007FD4` — and every accent is checked in CI to meet **WCAG AA
(≥ 4.5:1)** against the backgrounds each theme uses.

---

## Tests

```bash
cargo test
```

30 tests, none needing the network. They cover variable substitution, URL and query composition, base64
for basic auth, Postman v2.1 round-tripping, code generation, config compatibility, and headless egui
render passes over every request view, response view, sidebar and dialog. `theme_tests` asserts the
contrast ratios above and that old config files still load.

---

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| Windows: SmartScreen "app isn't commonly recognized" | Installers are unsigned. *More info → Run anyway*, or use the portable exe |
| Windows: `could not embed Windows resources` while building | `rc.exe` not found — build from a Developer Command Prompt or install the Windows SDK |
| `error WIX7015 … Open Source Maintenance Fee` | WiX v7 installed. `dotnet tool uninstall -g wix` then install `5.0.2` |
| `unhandled extension element 'WixUI'` | `wix extension add -g WixToolset.UI.wixext/5.0.2` |
| MSI refuses to install the same version twice | Expected: `MajorUpgrade` blocks same-version repairs. Uninstall first, or bump the version |
| 32-bit build fails with `LNK1181` | Missing x86 C++ tools, or `rustup target add i686-pc-windows-msvc` |
| Linux: blank window or no window | Missing display server libraries — install the `libxcb*`/`libxkbcommon-dev` list above |
| Linux: `error while loading shared libraries: libxcb.so.1` | The X11 runtime packages aren't installed; the `.deb` pulls them in automatically |
| AppImage won't start | Install `libfuse2` / `libfuse2t64`, or use `--appimage-extract-and-run` |
| macOS: "app cannot be opened" | Unsigned build. Right-click → **Open**, or `xattr -dr com.apple.quarantine` |
| Settings lost after a crash | Autosave interval is 20 s — use Ctrl+Shift+S before force-quitting |
| Requests fail from the API docs page | Backend CORS: `CORS_ORIGINS` must allow the page's origin |

---

## Licence

**Copyright © 2026 Jayasimma D. All rights reserved.** See [LICENSE.md](LICENSE.md) — it is also the
agreement shown during installation. Free to use, modify and share unmodified; no warranty; liability is
limited. The licence text is a plain-language template, not legal advice.

Third-party components (Rust crates such as `eframe`, `egui`, `reqwest`, `tokio`, `serde`, `chrono`, and
the `rustls` crypto stack) remain under their own licences.

---

## Release checklist

1. Bump `version` in `Cargo.toml`.
2. `cargo test`
3. `powershell -ExecutionPolicy Bypass -File build-installer.ps1` (Windows) and build Linux/macOS locally
   or let CI do it.
4. `powershell -ExecutionPolicy Bypass -File wix\inspect-msi.ps1` for each MSI.
5. Commit, tag `v<version>`, push the tag — CI builds all platforms and publishes the release with
   checksums.
6. Install one artifact per platform on a clean machine, run a request, uninstall, confirm leftovers are
   gone.
7. Sign Windows and macOS builds before wider distribution.
