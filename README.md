# Hiload Plant Maintenance (Browser + Windows Desktop SQLite)

This repository now has **two ways** to run:

1. **Browser app:** open `Hiload Plant Latest.html` directly.
2. **Windows desktop app:** Tauri 2 + SQLite around the same app.

## 1) What the desktop app does now

- In the **Windows desktop app**, SQLite is now the **live day-to-day data store**.
- On desktop startup, the app loads Sites, Plant, Hires, Maintenance, Suppliers, Invoices, invoice documents, meters, service settings, rate models, alerts, and site totals from SQLite.
- In the **Windows desktop app**, invoice attachments are copied into the app-managed documents folder so they survive restarts and can be backed up safely.
- In the **Windows desktop app**, invoice attachments can now be read locally to draft OCR suggestions before you save or edit an invoice. The app never saves, posts, or overwrites invoice fields automatically.
- Normal add/edit/delete/import/reset actions in desktop mode now save back to SQLite automatically.
- In **browser mode**, the app still uses normal browser local storage and does not depend on Tauri or SQLite.
- **Help → Desktop / Database** now lets you:
  - reload live data from SQLite
  - import a browser `.json` backup into SQLite and refresh the visible screens
  - create a dated desktop backup folder containing the SQLite database, managed invoice documents, and a manifest
  - restore a previous desktop backup folder after creating an automatic safety backup first

## 2) Invoice OCR and optional AI

### What works in this release

- **Read invoice with OCR** is available only in the **Windows desktop app**.
- It reads:
  - **PDF** files that already contain selectable text
  - **PNG** and **JPG/JPEG** image files through local Windows OCR when that Windows feature is available
- OCR only creates **draft suggestions** for supported invoice fields such as supplier name, invoice number, invoice date, notes, subtotal/VAT/total.
- You must **tick the suggestions you want** and click **Apply reviewed suggestions** yourself.
- You must still **save the invoice manually**. OCR never posts to maintenance and never changes suppliers, payments, or other records automatically.

### Current limitations

- **Scanned image-only PDFs** may return little or no text in this release. If that happens, try a clear PNG/JPG photo or type the values manually.
- Browser mode keeps working as before, but invoice OCR stays disabled there because browser mode does not use Tauri desktop APIs.
- In the Linux cloud agent, the Windows image OCR path cannot be exercised directly, so it needs manual Windows testing.

### Optional AI status

- Optional cloud AI review is **disabled by default**.
- No AI API key is required for normal app use.
- No AI API key is stored in source code, SQLite backups, or restore data.
- Cloud providers are **not enabled in this release**. The app shows the future setup/privacy notes only.

### Future AI plan

When optional AI is added later, it should remain a separate opt-in step with these rules:

- the user must enable it deliberately in the Windows desktop app;
- the app must explain that provider charges may apply;
- the app must explain that invoice text may be sent to that provider;
- no invoice data may be sent unless the user starts that AI action;
- all suggested values must still be reviewed and applied manually by the user;
- secrets/API keys must be stored only in a secure desktop mechanism and never in backups.

## 3) One-time Windows prerequisites

Install these once on the Windows PC that will build the desktop app:

1. **Node.js LTS**
2. **Rust** (install with rustup, default MSVC option)
3. **Microsoft C++ Build Tools**
   - In installer, select workload: **Desktop development with C++**
4. **WebView2 Runtime** (usually already installed on Windows 10/11)

## 4) Download source and open terminal

1. Download or clone this repository to a folder on your Windows PC.
2. Open that folder in File Explorer.
3. In the folder, open a terminal (PowerShell or Command Prompt).

## 5) Install project dependencies

Run one of these:

```bash
npm install
```

If PowerShell blocks `npm`, use:

```bash
npm.cmd install
```

## 6) Run desktop app in development mode

Run one of these:

```bash
npm run tauri dev
```

If you are using PowerShell, use:

```bash
npm.cmd run tauri dev
```

This opens the Windows desktop app using the same screens and workflow.

## 7) Build the Windows installer

Run:

```bash
npm run tauri build
```

After build, installer files are in:

- `src-tauri\target\release\bundle\` (for example `.msi` or setup `.exe`)

## 8) Move old browser data into SQLite and back up

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

## 9) Simple troubleshooting

1. If `npm install` fails, close terminal, reopen it, and run `npm install` again.
2. If Rust/C++ toolchain errors appear, re-check that Rust and **Desktop development with C++** are installed.
3. If desktop window does not open, ensure WebView2 Runtime is installed.
4. If import fails, verify you selected a valid app backup `.json` file.
5. If a desktop attachment does not open, reopen the invoice and use **Replace document** to attach the file again.
6. If the desktop app opens but you do not see data, open **Help → Desktop / Database** and confirm it says **Loaded live data from SQLite** or click **Reload live data from SQLite**.
7. If build is slow, wait (first build can take a while).
8. If OCR says no readable text was found, check whether the PDF contains selectable text. If not, try a clear PNG or JPG photo in the desktop app.
9. If OCR suggests a wrong value, leave that box unticked and type the correct value manually before saving.

---

## Browser mode still works

You can still run the original app by opening:

- `Hiload Plant Latest.html`

No desktop installation is required for normal browser use. Browser mode keeps using browser local storage. The Windows desktop app uses SQLite for daily work and keeps real invoice files in its managed documents folder, so keep your original JSON backup until you have tested the desktop version a few times successfully and keep at least one desktop backup outside the PC.
