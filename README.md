# Slipstream

Per-app DPI bypass for Windows. Tauri 2 (Rust) + React, engine = zapret `winws` on WinDivert.

## Setup
1. Install Node 18+, Rust (MSVC toolchain) and the Tauri 2 prerequisites (WebView2).
2. Download a zapret release and copy `winws.exe`, `WinDivert.dll`, `WinDivert64.sys` into `src-tauri/engine/`.
3. `npm install`
4. `npm run tauri dev` (run the terminal as Administrator) or `npm run tauri build` for an installer.

## How it stays out of the way
Only ports 80/443 (and Discord's UDP voice range when Discord is on) enter the driver filter. winws only
alters the first packets of flows whose SNI/Host is on your list or whose process you added; the rest is
reinjected untouched.

## Extending
- ISP presets: `src-tauri/src/isp.rs`. App profiles: `src-tauri/src/profiles.rs`.
