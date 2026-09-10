# Hiload Plant Maintenance (Browser + Windows Desktop SQLite)

This repository now has **two ways** to run:

1. **Browser app:** open `Hiload Plant Latest.html` directly.
2. **Windows desktop app:** Tauri 2 + SQLite around the same app.

## 1) What the desktop app does now

- In the **Windows desktop app**, SQLite is now the **live day-to-day data store**.
- On desktop startup, the app loads Sites, Plant, Hires, Maintenance, Suppliers, Invoices, invoice documents, meters, service settings, rate models, alerts, and site totals from SQLite.
- In the **Windows desktop app**, invoice attachments are copied into the app-managed documents folder so they survive restarts and can be backed up safely.
- Normal add/edit/delete/import/reset actions in desktop mode now save back to SQLite automatically.
- In **browser mode**, the app still uses normal browser local storage and does not depend on Tauri or SQLite.
- **Help → Desktop / Database** now lets you:
  - reload live data from SQLite
  - import a browser `.json` backup into SQLite and refresh the visible screens
  - create a dated desktop backup folder containing the SQLite database, managed invoice documents, and a manifest
  - restore a previous desktop backup folder after creating an automatic safety backup first

## 2) One-time Windows prerequisites

Install these once on the Windows PC that will build the desktop app:

1. **Node.js LTS**
2. **Rust** (install with rustup, default MSVC option)
3. **Microsoft C++ Build Tools**
   - In installer, select workload: **Desktop development with C++**
4. **WebView2 Runtime** (usually already installed on Windows 10/11)

## 3) Download source and open terminal

1. Download or clone this repository to a folder on your Windows PC.
2. Open that folder in File Explorer.
3. In the folder, open a terminal (PowerShell or Command Prompt).

## 4) Install project dependencies

Run one of these:

```bash
npm install
```

If PowerShell blocks `npm`, use:

```bash
npm.cmd install
```

## 5) Run desktop app in development mode

Run one of these:

```bash
npm run tauri dev
```

If you are using PowerShell, use:

```bash
npm.cmd run tauri dev
```

This opens the Windows desktop app using the same screens and workflow.

## 6) Build the Windows installer

Run:

```bash
npm run tauri build
```

After build, installer files are in:

- `src-tauri\target\release\bundle\` (for example `.msi` or setup `.exe`)

## 7) Move old browser data into SQLite and back up

1. In browser mode, export your old data from **Help → Export data (.json)**.
2. Start desktop app (`npm run tauri dev`, `npm.cmd run tauri dev`, or installed app).
3. Open **Help → Desktop / Database**.
4. If SQLite is empty but this device already has browser-style data, the desktop app will offer to copy that data into SQLite.
5. Or click **Import Browser Backup into SQLite** and select your `.json` backup.
6. The desktop app will reload the visible screens from SQLite so your plants and invoices appear straight away.
7. Use **Reload live data from SQLite** any time you want to refresh the screen from the database.
8. Use **Create desktop backup** to create a dated backup folder of the desktop database, managed invoice files, and manifest.
9. Use **Restore desktop backup** to select one of those backup folders and replace the current desktop data after an automatic safety backup is created.
10. The screen shows where your database, documents folder, and backups folder are located.
11. Keep at least one backup copy outside the PC as well, for example on a USB drive, OneDrive, or another safe place.

## 8) Simple troubleshooting

1. If `npm install` fails, close terminal, reopen it, and run `npm install` again.
2. If Rust/C++ toolchain errors appear, re-check that Rust and **Desktop development with C++** are installed.
3. If desktop window does not open, ensure WebView2 Runtime is installed.
4. If import fails, verify you selected a valid app backup `.json` file.
5. If a desktop attachment does not open, reopen the invoice and use **Replace document** to attach the file again.
6. If the desktop app opens but you do not see data, open **Help → Desktop / Database** and confirm it says **Loaded live data from SQLite** or click **Reload live data from SQLite**.
7. If build is slow, wait (first build can take a while).

---

## Browser mode still works

You can still run the original app by opening:

- `Hiload Plant Latest.html`

No desktop installation is required for normal browser use. Browser mode keeps using browser local storage. The Windows desktop app uses SQLite for daily work and keeps real invoice files in its managed documents folder, so keep your original JSON backup until you have tested the desktop version a few times successfully and keep at least one desktop backup outside the PC.
