# Hiload Plant Maintenance (Browser + First Windows Desktop Foundation)

This repository now has **two ways** to run:

1. **Browser app (existing):** open `Hiload Plant Latest.html` directly.
2. **Windows desktop foundation (new):** Tauri 2 + SQLite wrapper around the same app.

## 1) What this PR adds (and what it does not yet do)

### Added now
- Tauri 2 desktop project scaffold (`package.json` + `src-tauri/`)
- Local SQLite database in your Windows app-data folder
- Automatic creation of:
  - SQLite database file
  - `documents/` folder (for future invoice files)
  - `backups/` folder
- New **Help → Desktop / Database** section in the app
- Desktop command to import old browser `.json` backup into SQLite
- Desktop command to create a SQLite backup copy

### Not yet automated in this first desktop foundation
- Daily screens (Sites, Plant, Hire, Maintenance, Invoices) still save to **browser local storage** for now
- No email inbox integration yet
- No OCR yet
- No cloud APIs/local AI extraction yet

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

Run:

```bash
npm install
```

## 5) Run desktop app in development mode

Run:

```bash
npm run tauri dev
```

This opens the Windows desktop app using your existing HTML workflow.

## 6) Build the Windows installer

Run:

```bash
npm run tauri build
```

After build, installer files are in:

- `src-tauri\target\release\bundle\` (for example `.msi` or setup `.exe`)

## 7) Move old browser data into SQLite and back up

1. In browser mode, export your old data from **Help → Export data (.json)**.
2. Start desktop app (`npm run tauri dev` or installed app).
3. Open **Help → Desktop / Database**.
4. Click **Import Browser Backup into SQLite** and select your `.json` backup.
5. Use **Create SQLite backup copy** to create a backup of the desktop database.
6. The screen shows where your database, documents folder, and backups folder are located.

## 8) Simple troubleshooting

1. If `npm install` fails, close terminal, reopen it, and run `npm install` again.
2. If Rust/C++ toolchain errors appear, re-check that Rust and **Desktop development with C++** are installed.
3. If desktop window does not open, ensure WebView2 Runtime is installed.
4. If import fails, verify you selected a valid app backup `.json` file.
5. If build is slow, wait (first build can take a while).

---

## Browser mode still works

You can still run the original app by opening:

- `Hiload Plant Latest.html`

No desktop installation is required for normal browser use.
