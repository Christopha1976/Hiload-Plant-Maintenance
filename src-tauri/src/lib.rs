use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopStatus {
    is_desktop: bool,
    database_path: String,
    database_exists: bool,
    documents_path: String,
    documents_exists: bool,
    backups_path: String,
    backups_exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    imported: bool,
    duplicate: bool,
    message: String,
    imported_at: String,
    counts: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupResult {
    backup_path: String,
    message: String,
}

struct AppPaths {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    documents_dir: PathBuf,
    backups_dir: PathBuf,
}

fn unix_timestamp_string() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

fn storage_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not read app data path: {e}"))?;

    let db_path = app_data.join("hiload-plant-maintenance.sqlite3");
    let documents_dir = app_data.join("documents");
    let backups_dir = app_data.join("backups");

    Ok(AppPaths {
        app_data_dir: app_data,
        db_path,
        documents_dir,
        backups_dir,
    })
}

fn ensure_storage_and_schema(app: &AppHandle) -> Result<AppPaths, String> {
    let paths = storage_paths(app)?;

    fs::create_dir_all(&paths.app_data_dir)
        .map_err(|e| format!("Could not create app data folder: {e}"))?;
    fs::create_dir_all(&paths.documents_dir)
        .map_err(|e| format!("Could not create documents folder: {e}"))?;
    fs::create_dir_all(&paths.backups_dir)
        .map_err(|e| format!("Could not create backups folder: {e}"))?;

    let conn = open_db(&paths.db_path)?;
    ensure_schema(&conn)?;

    Ok(paths)
}

fn open_db(path: &Path) -> Result<Connection, String> {
    let conn =
        Connection::open(path).map_err(|e| format!("Could not open SQLite database: {e}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Could not configure SQLite database: {e}"))?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS import_history (
          fingerprint TEXT PRIMARY KEY,
          imported_at TEXT NOT NULL,
          source_file_name TEXT,
          counts_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS app_meta (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sites (
          id TEXT PRIMARY KEY,
          name TEXT,
          location TEXT,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS plants (
          id TEXT PRIMARY KEY,
          name TEXT,
          category TEXT,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS hires (
          id TEXT PRIMARY KEY,
          site_id TEXT,
          plant_id TEXT,
          start_date TEXT,
          end_date TEXT,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS maintenance (
          id TEXT PRIMARY KEY,
          plant_id TEXT,
          supplier_id TEXT,
          invoice_id TEXT,
          date TEXT,
          type TEXT,
          cost REAL,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS suppliers (
          id TEXT PRIMARY KEY,
          name TEXT,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS invoices (
          id TEXT PRIMARY KEY,
          supplier_id TEXT,
          plant_id TEXT,
          invoice_number TEXT,
          invoice_date TEXT,
          total_cost REAL,
          status TEXT,
          has_document INTEGER NOT NULL DEFAULT 0,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS invoice_documents (
          invoice_id TEXT PRIMARY KEY,
          file_name TEXT,
          mime_type TEXT,
          size_bytes INTEGER,
          has_data INTEGER NOT NULL DEFAULT 0,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS meters (
          key TEXT PRIMARY KEY,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS service_settings (
          plant_id TEXT PRIMARY KEY,
          raw_json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS site_usage_totals (
          site_id TEXT PRIMARY KEY,
          raw_json TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Could not create database schema: {e}"))
}

fn clear_import_tables(tx: &rusqlite::Transaction<'_>) -> Result<(), String> {
    tx.execute("DELETE FROM sites", [])
        .map_err(|e| format!("Could not clear sites: {e}"))?;
    tx.execute("DELETE FROM plants", [])
        .map_err(|e| format!("Could not clear plants: {e}"))?;
    tx.execute("DELETE FROM hires", [])
        .map_err(|e| format!("Could not clear hires: {e}"))?;
    tx.execute("DELETE FROM maintenance", [])
        .map_err(|e| format!("Could not clear maintenance: {e}"))?;
    tx.execute("DELETE FROM suppliers", [])
        .map_err(|e| format!("Could not clear suppliers: {e}"))?;
    tx.execute("DELETE FROM invoices", [])
        .map_err(|e| format!("Could not clear invoices: {e}"))?;
    tx.execute("DELETE FROM invoice_documents", [])
        .map_err(|e| format!("Could not clear invoice_documents: {e}"))?;
    tx.execute("DELETE FROM meters", [])
        .map_err(|e| format!("Could not clear meters: {e}"))?;
    tx.execute("DELETE FROM service_settings", [])
        .map_err(|e| format!("Could not clear service_settings: {e}"))?;
    tx.execute("DELETE FROM site_usage_totals", [])
        .map_err(|e| format!("Could not clear site_usage_totals: {e}"))?;
    Ok(())
}

fn get_data_block(root: &Value) -> Result<&Map<String, Value>, String> {
    if let Some(data_obj) = root.get("data").and_then(Value::as_object) {
        return Ok(data_obj);
    }

    root.as_object()
        .ok_or_else(|| "Backup file is not a valid JSON object.".to_string())
}

fn value_as_array<'a>(obj: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

fn value_as_object<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    obj.get(key).and_then(Value::as_object)
}

fn string_id_or_fallback(value: &Value, prefix: &str, index: usize) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{}_{}", prefix, index + 1))
}

fn value_str(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn value_f64(value: &Value, field: &str) -> Option<f64> {
    value.get(field).and_then(Value::as_f64)
}

fn insert_backup_data(
    tx: &rusqlite::Transaction<'_>,
    data: &Map<String, Value>,
) -> Result<Value, String> {
    let mut counts = json!({
        "sites": 0,
        "plants": 0,
        "hires": 0,
        "maintenance": 0,
        "suppliers": 0,
        "invoices": 0,
        "invoiceDocuments": 0,
        "meters": 0,
        "serviceSettings": 0,
        "siteUsageTotals": 0
    });

    for (idx, item) in value_as_array(data, "hiph_sites").iter().enumerate() {
        let id = string_id_or_fallback(item, "site", idx);
        tx.execute(
            "INSERT OR REPLACE INTO sites (id, name, location, raw_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                value_str(item, "name"),
                value_str(item, "location"),
                item.to_string()
            ],
        )
        .map_err(|e| format!("Could not import sites: {e}"))?;
        counts["sites"] = json!(counts["sites"].as_i64().unwrap_or(0) + 1);
    }

    for (idx, item) in value_as_array(data, "hiph_plants").iter().enumerate() {
        let id = string_id_or_fallback(item, "plant", idx);
        tx.execute(
            "INSERT OR REPLACE INTO plants (id, name, category, raw_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                value_str(item, "name"),
                value_str(item, "category"),
                item.to_string()
            ],
        )
        .map_err(|e| format!("Could not import plants: {e}"))?;
        counts["plants"] = json!(counts["plants"].as_i64().unwrap_or(0) + 1);
    }

    for (idx, item) in value_as_array(data, "hiph_hires").iter().enumerate() {
        let id = string_id_or_fallback(item, "hire", idx);
        tx.execute(
            "INSERT OR REPLACE INTO hires (id, site_id, plant_id, start_date, end_date, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, value_str(item, "siteId"), value_str(item, "plantId"), value_str(item, "startDate"), value_str(item, "endDate"), item.to_string()],
        )
        .map_err(|e| format!("Could not import hires: {e}"))?;
        counts["hires"] = json!(counts["hires"].as_i64().unwrap_or(0) + 1);
    }

    for (idx, item) in value_as_array(data, "hiph_maintenance").iter().enumerate() {
        let id = string_id_or_fallback(item, "maint", idx);
        tx.execute(
            "INSERT OR REPLACE INTO maintenance (id, plant_id, supplier_id, invoice_id, date, type, cost, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, value_str(item, "plantId"), value_str(item, "supplierId"), value_str(item, "invoiceId"), value_str(item, "date"), value_str(item, "type"), value_f64(item, "cost"), item.to_string()],
        )
        .map_err(|e| format!("Could not import maintenance: {e}"))?;
        counts["maintenance"] = json!(counts["maintenance"].as_i64().unwrap_or(0) + 1);
    }

    for (idx, item) in value_as_array(data, "hiph_suppliers").iter().enumerate() {
        let id = string_id_or_fallback(item, "supplier", idx);
        tx.execute(
            "INSERT OR REPLACE INTO suppliers (id, name, raw_json) VALUES (?1, ?2, ?3)",
            params![id, value_str(item, "name"), item.to_string()],
        )
        .map_err(|e| format!("Could not import suppliers: {e}"))?;
        counts["suppliers"] = json!(counts["suppliers"].as_i64().unwrap_or(0) + 1);
    }

    for (idx, item) in value_as_array(data, "hiph_invoices").iter().enumerate() {
        let id = string_id_or_fallback(item, "invoice", idx);
        let has_document = item
            .get("hasDocument")
            .and_then(Value::as_bool)
            .map(|b| i64::from(b))
            .unwrap_or(0);

        tx.execute(
            "INSERT OR REPLACE INTO invoices (id, supplier_id, plant_id, invoice_number, invoice_date, total_cost, status, has_document, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                value_str(item, "supplierId"),
                value_str(item, "plantId"),
                value_str(item, "invoiceNumber"),
                value_str(item, "invoiceDate"),
                value_f64(item, "totalCost"),
                value_str(item, "status"),
                has_document,
                item.to_string()
            ],
        )
        .map_err(|e| format!("Could not import invoices: {e}"))?;
        counts["invoices"] = json!(counts["invoices"].as_i64().unwrap_or(0) + 1);
    }

    if let Some(docs) = value_as_object(data, "hiph_invoice_docs") {
        for (invoice_id, item) in docs {
            let file_name = item
                .get("fileName")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let mime_type = item
                .get("mimeType")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let size_bytes = item.get("size").and_then(Value::as_i64);
            let has_data = item
                .get("dataUrl")
                .and_then(Value::as_str)
                .map(|s| i64::from(!s.trim().is_empty()))
                .unwrap_or(0);

            tx.execute(
                "INSERT OR REPLACE INTO invoice_documents (invoice_id, file_name, mime_type, size_bytes, has_data, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![invoice_id, file_name, mime_type, size_bytes, has_data, item.to_string()],
            )
            .map_err(|e| format!("Could not import invoice documents: {e}"))?;

            counts["invoiceDocuments"] =
                json!(counts["invoiceDocuments"].as_i64().unwrap_or(0) + 1);
        }
    }

    if let Some(meters) = value_as_object(data, "hiph_meters") {
        for (key, item) in meters {
            tx.execute(
                "INSERT OR REPLACE INTO meters (key, raw_json) VALUES (?1, ?2)",
                params![key, item.to_string()],
            )
            .map_err(|e| format!("Could not import meters: {e}"))?;
            counts["meters"] = json!(counts["meters"].as_i64().unwrap_or(0) + 1);
        }
    }

    if let Some(settings) = value_as_object(data, "hiph_plant_service") {
        for (plant_id, item) in settings {
            tx.execute(
                "INSERT OR REPLACE INTO service_settings (plant_id, raw_json) VALUES (?1, ?2)",
                params![plant_id, item.to_string()],
            )
            .map_err(|e| format!("Could not import service settings: {e}"))?;
            counts["serviceSettings"] = json!(counts["serviceSettings"].as_i64().unwrap_or(0) + 1);
        }
    }

    if let Some(totals) = value_as_object(data, "hiph_site_usage") {
        for (site_id, item) in totals {
            tx.execute(
                "INSERT OR REPLACE INTO site_usage_totals (site_id, raw_json) VALUES (?1, ?2)",
                params![site_id, item.to_string()],
            )
            .map_err(|e| format!("Could not import site usage totals: {e}"))?;
            counts["siteUsageTotals"] = json!(counts["siteUsageTotals"].as_i64().unwrap_or(0) + 1);
        }
    }

    Ok(counts)
}

fn backup_file_name() -> String {
    format!(
        "hiload-plant-maintenance-backup-{}.sqlite3",
        unix_timestamp_string()
    )
}

#[tauri::command]
fn desktop_status(app: AppHandle) -> Result<DesktopStatus, String> {
    let paths = ensure_storage_and_schema(&app)?;

    Ok(DesktopStatus {
        is_desktop: true,
        database_path: paths.db_path.to_string_lossy().to_string(),
        database_exists: paths.db_path.exists(),
        documents_path: paths.documents_dir.to_string_lossy().to_string(),
        documents_exists: paths.documents_dir.exists(),
        backups_path: paths.backups_dir.to_string_lossy().to_string(),
        backups_exists: paths.backups_dir.exists(),
    })
}

#[tauri::command]
fn import_browser_backup_into_sqlite(
    app: AppHandle,
    json_text: String,
    file_name: Option<String>,
) -> Result<ImportResult, String> {
    if json_text.trim().is_empty() {
        return Err("Please choose a JSON backup file first.".to_string());
    }

    let mut hasher = Sha256::new();
    hasher.update(json_text.as_bytes());
    let fingerprint = format!("{:x}", hasher.finalize());

    let parsed: Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("Backup file is not valid JSON: {e}"))?;
    let data = get_data_block(&parsed)?;

    let paths = ensure_storage_and_schema(&app)?;
    let mut conn = open_db(&paths.db_path)?;
    ensure_schema(&conn)?;

    let duplicate: Option<(String, String)> = conn
        .query_row(
            "SELECT imported_at, counts_json FROM import_history WHERE fingerprint = ?1",
            params![fingerprint],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok();

    if let Some((imported_at, counts_json)) = duplicate {
        let counts_value = serde_json::from_str(&counts_json).unwrap_or_else(|_| json!({}));
        return Ok(ImportResult {
            imported: false,
            duplicate: true,
            imported_at,
            counts: counts_value,
            message: "This backup was already imported before. No duplicate import was made."
                .to_string(),
        });
    }

    let tx = conn
        .transaction()
        .map_err(|e| format!("Could not start import transaction: {e}"))?;

    clear_import_tables(&tx)?;
    let counts = insert_backup_data(&tx, data)?;
    let imported_at = unix_timestamp_string();

    tx.execute(
        "INSERT INTO import_history (fingerprint, imported_at, source_file_name, counts_json) VALUES (?1, ?2, ?3, ?4)",
        params![
            fingerprint,
            imported_at,
            file_name.unwrap_or_default(),
            counts.to_string()
        ],
    )
    .map_err(|e| format!("Could not store import history: {e}"))?;

    tx.execute(
        "INSERT OR REPLACE INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params!["last_import", "browser_backup", unix_timestamp_string()],
    )
    .map_err(|e| format!("Could not update import metadata: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Could not commit imported data: {e}"))?;

    Ok(ImportResult {
        imported: true,
        duplicate: false,
        message: "Backup imported into SQLite successfully.".to_string(),
        imported_at,
        counts,
    })
}

#[tauri::command]
fn export_sqlite_backup(app: AppHandle) -> Result<BackupResult, String> {
    let paths = ensure_storage_and_schema(&app)?;

    if !paths.db_path.exists() {
        return Err("SQLite database file was not found yet.".to_string());
    }

    let destination = paths.backups_dir.join(backup_file_name());
    fs::copy(&paths.db_path, &destination)
        .map_err(|e| format!("Could not create backup copy: {e}"))?;

    Ok(BackupResult {
        backup_path: destination.to_string_lossy().to_string(),
        message: "SQLite backup file created successfully.".to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            desktop_status,
            import_browser_backup_into_sqlite,
            export_sqlite_backup
        ])
        .setup(|app| {
            ensure_storage_and_schema(app.handle())
                .map(|_| ())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
