use base64::Engine;
#[cfg(test)]
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};
use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
};

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
    has_live_data: bool,
    live_data_source: String,
    live_counts: Value,
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
    manifest_path: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentImportResult {
    relative_path: String,
    full_path: String,
    file_name: String,
    mime_type: String,
    size_bytes: u64,
    sha256: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentActionResult {
    file_deleted: bool,
    full_path: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveStateLoadResult {
    data: Value,
    source: String,
    is_empty: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    message: String,
    saved_at: String,
    counts: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreResult {
    restored_from: String,
    safety_backup_path: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OcrCorrectionFeedbackRequest {
    ocr_text: String,
    supplier_candidate: Option<String>,
    supplier_id: Option<String>,
    plant_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchInvoiceProcessRequest {
    folder_path: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InvoiceFieldSuggestion {
    field: String,
    label: String,
    value: String,
    confidence: String,
    evidence: String,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchInvoiceProcessEntry {
    file_name: String,
    outcome: String,
    reason: String,
    invoice_id: String,
    duplicate_invoice_id: String,
    supplier_name: String,
    invoice_number: String,
    needs_review: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchInvoiceProcessResult {
    source_folder: String,
    processed_count: u64,
    needs_review_count: u64,
    duplicate_count: u64,
    skipped_count: u64,
    error_count: u64,
    entries: Vec<BatchInvoiceProcessEntry>,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceReadResult {
    source_file_name: String,
    mime_type: String,
    provider: String,
    message: String,
    extracted_text: String,
    suggestions: Vec<InvoiceFieldSuggestion>,
    warnings: Vec<String>,
    limitations: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceReadRequest {
    relative_path: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    data_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    format_version: u32,
    created_at: String,
    app_name: String,
    database: BackupFileManifest,
    documents: BackupDocumentsManifest,
    live_counts: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupFileManifest {
    relative_path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupDocumentsManifest {
    relative_dir: String,
    managed_files_count: usize,
    files: Vec<BackupFileManifest>,
}

struct AppPaths {
    app_data_dir: PathBuf,
    db_path: PathBuf,
    documents_dir: PathBuf,
    backups_dir: PathBuf,
}

const MANAGED_INVOICE_DIR: &str = "invoices";
const BACKUP_FORMAT_VERSION: u32 = 1;
const MAX_INVOICE_READ_BYTES: usize = 10 * 1024 * 1024;
const MAX_INVOICE_EXTRACT_TEXT_CHARS: usize = 12_000;
const MAX_BATCH_INVOICE_FILES: usize = 250;

const STATE_ARRAY_KEYS: &[&str] = &[
    "hiph_sites",
    "hiph_plants",
    "hiph_hires",
    "hiph_maintenance",
    "hiph_suppliers",
    "hiph_invoices",
];

const STATE_OBJECT_KEYS: &[&str] = &[
    "hiph_site_usage",
    "hiph_meters",
    "hiph_plant_service",
    "hiph_rate_models",
    "hiph_alerts",
    "hiph_invoice_docs",
];

const STATE_KEYS: &[&str] = &[
    "hiph_sites",
    "hiph_plants",
    "hiph_hires",
    "hiph_maintenance",
    "hiph_site_usage",
    "hiph_meters",
    "hiph_plant_service",
    "hiph_rate_models",
    "hiph_alerts",
    "hiph_suppliers",
    "hiph_invoices",
    "hiph_invoice_docs",
];

fn unix_timestamp_string() -> String {
    OffsetDateTime::now_utc().unix_timestamp().to_string()
}

fn iso_timestamp_string() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| unix_timestamp_string())
}

fn backup_name_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_else(|_| unix_timestamp_string())
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

fn managed_invoice_documents_dir(paths: &AppPaths) -> PathBuf {
    paths.documents_dir.join(MANAGED_INVOICE_DIR)
}

fn ensure_storage_and_schema(app: &AppHandle) -> Result<AppPaths, String> {
    let paths = storage_paths(app)?;

    fs::create_dir_all(&paths.app_data_dir)
        .map_err(|e| format!("Could not create app data folder: {e}"))?;
    fs::create_dir_all(&paths.documents_dir)
        .map_err(|e| format!("Could not create documents folder: {e}"))?;
    fs::create_dir_all(managed_invoice_documents_dir(&paths))
        .map_err(|e| format!("Could not create invoice documents folder: {e}"))?;
    fs::create_dir_all(&paths.backups_dir)
        .map_err(|e| format!("Could not create backups folder: {e}"))?;

    let conn = open_db(&paths.db_path)?;
    ensure_schema(&conn)?;
    clear_removed_local_ai_metadata(&conn)?;

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

        CREATE TABLE IF NOT EXISTS app_state (
          state_key TEXT PRIMARY KEY,
          raw_json TEXT NOT NULL,
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

        CREATE TABLE IF NOT EXISTS invoice_ocr_correction_rules (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          rule_type TEXT NOT NULL,
          supplier_key TEXT NOT NULL DEFAULT '',
          hint_key TEXT NOT NULL DEFAULT '',
          target_value TEXT NOT NULL,
          usage_count INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(rule_type, supplier_key, hint_key, target_value)
        );
        ",
    )
    .map_err(|e| format!("Could not create database schema: {e}"))
}

fn sanitize_path_component(input: &str, fallback: &str) -> String {
    let mut output = String::new();
    let mut last_was_separator = false;

    for ch in input.chars() {
        let safe = if ch.is_ascii_alphanumeric() {
            Some(ch)
        } else if ch == '.' || ch == '_' || ch == '-' {
            Some(ch)
        } else if ch.is_whitespace() {
            Some('-')
        } else {
            None
        };

        if let Some(next) = safe {
            if next == '-' && last_was_separator {
                continue;
            }
            output.push(next);
            last_was_separator = next == '-';
        } else if !last_was_separator {
            output.push('-');
            last_was_separator = true;
        }
    }

    let mut trimmed = output
        .trim_matches(|c| c == '.' || c == '-' || c == '_' || c == ' ')
        .to_string();
    while trimmed.contains("..") {
        trimmed = trimmed.replace("..", ".");
    }

    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

fn extension_from_mime_type(mime_type: &str) -> &'static str {
    match mime_type {
        "application/pdf" => "pdf",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

fn safe_display_file_name(file_name: &str, mime_type: &str) -> String {
    let source = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);

    let mut safe_name = sanitize_path_component(source, "invoice-document");
    while safe_name.contains("..") {
        safe_name = safe_name.replace("..", ".");
    }
    if !safe_name.contains('.') {
        safe_name.push('.');
        safe_name.push_str(extension_from_mime_type(mime_type));
    }
    safe_name
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read file checksum: {e}"))?;
    Ok(sha256_hex(&bytes))
}

fn relative_path_string(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn ensure_relative_safe_path(relative_path: &str) -> Result<&Path, String> {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err("The saved document path is not valid.".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err("The saved document path is not valid.".to_string());
    }
    Ok(path)
}

fn resolve_managed_document_path(paths: &AppPaths, relative_path: &str) -> Result<PathBuf, String> {
    let relative = ensure_relative_safe_path(relative_path)?;
    Ok(paths.documents_dir.join(relative))
}

fn count_other_document_references(
    data: &Map<String, Value>,
    relative_path: &str,
    exclude_invoice_id: &str,
) -> usize {
    value_as_object(data, "hiph_invoice_docs")
        .map(|docs| {
            docs.iter()
                .filter(|(invoice_id, item)| {
                    invoice_id.as_str() != exclude_invoice_id
                        && item
                            .get("managedRelativePath")
                            .and_then(Value::as_str)
                            .map(|value| value == relative_path)
                            .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Could not prepare the invoice documents folder.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Could not create the invoice documents folder: {e}"))?;

    let temp_path = (0..16)
        .map(|attempt| {
            path.with_extension(format!(
                "{}{}.{}.tmp",
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!("{ext}."))
                    .unwrap_or_default(),
                OffsetDateTime::now_utc().unix_timestamp_nanos(),
                attempt
            ))
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| {
            "Could not prepare a safe temporary file for this attachment.".to_string()
        })?;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("Could not create document file: {e}"))?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .map_err(|e| format!("Could not write document file: {e}"))?;
    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Could not save document file: {e}")
    })
}

fn managed_document_relative_path(invoice_id: &str, file_name: &str, checksum: &str) -> String {
    let safe_invoice_id = sanitize_path_component(invoice_id, "invoice");
    let safe_file_name = safe_display_file_name(file_name, "");
    let short_checksum = checksum.chars().take(12).collect::<String>();
    format!(
        "{MANAGED_INVOICE_DIR}/{}--{}--{}",
        safe_invoice_id, short_checksum, safe_file_name
    )
}

fn save_managed_invoice_document(
    paths: &AppPaths,
    invoice_id: &str,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<AttachmentImportResult, String> {
    if invoice_id.trim().is_empty() {
        return Err("The invoice ID is missing, so the attachment could not be saved.".to_string());
    }
    if bytes.is_empty() {
        return Err("The selected attachment file was empty.".to_string());
    }

    let safe_file_name = safe_display_file_name(file_name, mime_type);
    let checksum = sha256_hex(bytes);
    let relative_path = managed_document_relative_path(invoice_id, &safe_file_name, &checksum);
    let full_path = resolve_managed_document_path(paths, &relative_path)?;

    write_file_atomic(&full_path, bytes)?;

    Ok(AttachmentImportResult {
        relative_path,
        full_path: full_path.to_string_lossy().to_string(),
        file_name: safe_file_name,
        mime_type: mime_type.to_string(),
        size_bytes: bytes.len() as u64,
        sha256: checksum,
        message: "Attachment saved in the desktop documents folder.".to_string(),
    })
}

fn clear_removed_local_ai_metadata(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "DELETE FROM app_meta WHERE key = 'local_ai_settings_json'",
        [],
    )
    .map_err(|e| format!("Could not remove retired Local AI settings: {e}"))?;
    Ok(())
}

#[cfg(test)]
fn read_app_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM app_meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(|e| format!("Could not read app setting \"{key}\": {e}"))
}

#[derive(Debug, Clone)]
struct OcrCorrectionRule {
    rule_type: String,
    supplier_key: String,
    hint_key: String,
    target_value: String,
    usage_count: i64,
}

#[derive(Default, Clone)]
struct InvoiceAnalysisContext {
    suppliers: Vec<Value>,
    plants: Vec<Value>,
    rules: Vec<OcrCorrectionRule>,
}

struct InvoiceAnalysis {
    suggestions: Vec<InvoiceFieldSuggestion>,
    warnings: Vec<String>,
    supplier_candidate: Option<String>,
    matched_supplier_id: Option<String>,
    invoice_number: Option<String>,
    invoice_date: Option<String>,
    notes: Option<String>,
    net_amount: Option<f64>,
    vat_amount: Option<f64>,
    total_cost: Option<f64>,
    matched_plant_id: Option<String>,
    needs_review: bool,
}

struct InvoiceTextRead {
    provider: String,
    raw_text: String,
    limitations: Vec<String>,
}

#[derive(Clone)]
struct LineValueMatch {
    value: String,
    evidence: String,
    label: String,
    index: usize,
}

#[derive(Clone)]
struct AmountMatch {
    amount: f64,
    evidence: String,
    index: usize,
    priority: i32,
}

#[derive(Clone)]
struct PlantMatch {
    plant_id: String,
    plant_name: String,
    alias: String,
    score: i32,
}

fn load_ocr_correction_rules(conn: &Connection) -> Result<Vec<OcrCorrectionRule>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT rule_type, supplier_key, hint_key, target_value, usage_count FROM invoice_ocr_correction_rules ORDER BY usage_count DESC, updated_at DESC",
        )
        .map_err(|e| format!("Could not prepare OCR correction rule query: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(OcrCorrectionRule {
                rule_type: row.get(0)?,
                supplier_key: row.get(1)?,
                hint_key: row.get(2)?,
                target_value: row.get(3)?,
                usage_count: row.get(4)?,
            })
        })
        .map_err(|e| format!("Could not read OCR correction rules: {e}"))?;

    let mut rules = Vec::new();
    for row in rows {
        rules.push(row.map_err(|e| format!("Could not parse OCR correction rule row: {e}"))?);
    }
    Ok(rules)
}

fn upsert_ocr_correction_rule(
    conn: &Connection,
    rule_type: &str,
    supplier_key: &str,
    hint_key: &str,
    target_value: &str,
) -> Result<(), String> {
    let updated_at = unix_timestamp_string();
    conn.execute(
        "
        INSERT INTO invoice_ocr_correction_rules (
          rule_type, supplier_key, hint_key, target_value, usage_count, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)
        ON CONFLICT(rule_type, supplier_key, hint_key, target_value)
        DO UPDATE SET usage_count = usage_count + 1, updated_at = excluded.updated_at
        ",
        params![rule_type, supplier_key, hint_key, target_value, updated_at],
    )
    .map_err(|e| format!("Could not save OCR correction rule: {e}"))?;
    Ok(())
}

fn analysis_context_from_data(
    data: &Map<String, Value>,
    rules: Vec<OcrCorrectionRule>,
) -> InvoiceAnalysisContext {
    InvoiceAnalysisContext {
        suppliers: value_as_array(data, "hiph_suppliers").to_vec(),
        plants: value_as_array(data, "hiph_plants").to_vec(),
        rules,
    }
}

fn load_analysis_context(conn: &Connection) -> Result<InvoiceAnalysisContext, String> {
    let (data, _) = load_live_state(conn)?;
    let rules = load_ocr_correction_rules(conn)?;
    Ok(analysis_context_from_data(&data, rules))
}

fn normalize_whitespace_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_invoice_text(text: &str) -> String {
    text.replace('\r', "")
        .lines()
        .map(normalize_whitespace_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_chars(text: &str, limit: usize) -> String {
    let mut result = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= limit {
            result.push('…');
            break;
        }
        result.push(ch);
    }
    result
}

fn normalize_for_match(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_normalized_text(haystack: &str, needle: &str) -> bool {
    let left = normalize_for_match(haystack);
    let right = normalize_for_match(needle);
    !right.is_empty() && left.contains(&right)
}

fn base_name_or_fallback(file_name: &str, fallback: &str) -> String {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback)
        .to_string()
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum InvoiceReadKind {
    Pdf,
    Png,
    Jpeg,
}

fn detect_invoice_read_kind(file_name: &str, mime_type: &str) -> Result<InvoiceReadKind, String> {
    let mime = mime_type.trim().to_ascii_lowercase();
    if mime == "application/pdf" {
        return Ok(InvoiceReadKind::Pdf);
    }
    if mime == "image/png" {
        return Ok(InvoiceReadKind::Png);
    }
    if mime == "image/jpeg" || mime == "image/jpg" {
        return Ok(InvoiceReadKind::Jpeg);
    }

    let ext = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match ext.as_str() {
        "pdf" => Ok(InvoiceReadKind::Pdf),
        "png" => Ok(InvoiceReadKind::Png),
        "jpg" | "jpeg" => Ok(InvoiceReadKind::Jpeg),
        _ => {
            Err("Invoice reading currently supports PDF, PNG and JPG/JPEG files only.".to_string())
        }
    }
}

fn mime_type_from_kind(kind: InvoiceReadKind) -> &'static str {
    match kind {
        InvoiceReadKind::Pdf => "application/pdf",
        InvoiceReadKind::Png => "image/png",
        InvoiceReadKind::Jpeg => "image/jpeg",
    }
}

fn load_invoice_read_bytes(
    paths: &AppPaths,
    request: &InvoiceReadRequest,
) -> Result<(Vec<u8>, String, String), String> {
    let fallback_name = request
        .file_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("invoice-document");
    let mime_type = request
        .mime_type
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();

    if let Some(relative_path) = request
        .relative_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let full_path = resolve_managed_document_path(paths, relative_path)?;
        if !full_path.is_file() {
            return Err("The saved invoice attachment file could not be found. Please attach it again and try another read.".to_string());
        }
        let size = full_path
            .metadata()
            .map_err(|e| format!("Could not check the attachment size safely: {e}"))?
            .len() as usize;
        if size > MAX_INVOICE_READ_BYTES {
            return Err(format!(
                "This file is too large to read safely. Please use a file under {} MB.",
                MAX_INVOICE_READ_BYTES / (1024 * 1024)
            ));
        }
        let bytes = fs::read(&full_path)
            .map_err(|e| format!("The saved invoice attachment could not be read safely: {e}"))?;
        let file_name = base_name_or_fallback(&full_path.to_string_lossy(), fallback_name);
        return Ok((bytes, file_name, mime_type));
    }

    let data_base64 = request
        .data_base64
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Please choose or attach a PDF or photo first, then try Read invoice with OCR again."
                .to_string()
        })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|e| format!("The selected invoice file could not be read safely: {e}"))?;
    if bytes.is_empty() {
        return Err("The selected invoice file was empty.".to_string());
    }
    if bytes.len() > MAX_INVOICE_READ_BYTES {
        return Err(format!(
            "This file is too large to read safely. Please use a file under {} MB.",
            MAX_INVOICE_READ_BYTES / (1024 * 1024)
        ));
    }
    Ok((bytes, fallback_name.to_string(), mime_type))
}

#[cfg(target_os = "windows")]
fn make_temp_invoice_read_path(paths: &AppPaths, file_name: &str) -> Result<PathBuf, String> {
    let temp_dir = paths.app_data_dir.join("temp");
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Could not prepare the temporary OCR folder: {e}"))?;

    let safe_name = safe_display_file_name(file_name, "");
    for attempt in 0..32 {
        let candidate = temp_dir.join(format!(
            "ocr-{}-{}-{}",
            backup_name_timestamp(),
            attempt,
            safe_name
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("Could not prepare a safe temporary OCR file.".to_string())
}

#[cfg(target_os = "windows")]
fn run_powershell_image_ocr(path: &Path) -> Result<String, String> {
    const POWERSHELL_OCR_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$filePath = $env:HILOAD_OCR_FILE
if ([string]::IsNullOrWhiteSpace($filePath)) { throw 'Missing OCR file path.' }
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Storage.FileAccessMode, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapPixelFormat, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapAlphaMode, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$file = [Windows.Storage.StorageFile]::GetFileFromPathAsync($filePath).AsTask().GetAwaiter().GetResult()
$stream = $file.OpenAsync([Windows.Storage.FileAccessMode]::Read).AsTask().GetAwaiter().GetResult()
$decoder = [Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream).AsTask().GetAwaiter().GetResult()
$bitmap = $decoder.GetSoftwareBitmapAsync().AsTask().GetAwaiter().GetResult()
$ocrBitmap = [Windows.Graphics.Imaging.SoftwareBitmap]::Convert(
  $bitmap,
  [Windows.Graphics.Imaging.BitmapPixelFormat]::Bgra8,
  [Windows.Graphics.Imaging.BitmapAlphaMode]::Premultiplied
)
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
if (-not $engine) { throw 'Windows local OCR is not available for the current desktop language settings.' }
$result = $engine.RecognizeAsync($ocrBitmap).AsTask().GetAwaiter().GetResult()
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Write-Output $result.Text
"#;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            POWERSHELL_OCR_SCRIPT,
        ])
        .env("HILOAD_OCR_FILE", path.as_os_str())
        .output()
        .map_err(|e| format!("Could not start Windows local OCR: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Windows local OCR could not read this image file.".to_string()
        } else {
            format!("Windows local OCR could not read this image file: {stderr}")
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn extract_image_text(paths: &AppPaths, file_name: &str, bytes: &[u8]) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let temp_path = make_temp_invoice_read_path(paths, file_name)?;
        write_file_atomic(&temp_path, bytes)?;
        let result = run_powershell_image_ocr(&temp_path);
        let _ = fs::remove_file(&temp_path);
        result
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (paths, file_name, bytes);
        Err("Image OCR is available only in the Windows desktop app build. PDF files with selectable text can still be read locally.".to_string())
    }
}

fn read_invoice_text_from_bytes(
    paths: &AppPaths,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<InvoiceTextRead, String> {
    let kind = detect_invoice_read_kind(file_name, mime_type)?;
    let mut limitations = vec![
        "Nothing is saved or changed by OCR alone. You must review every suggested value before you apply or save it.".to_string(),
        "OCR suggestions never post invoices, never mark them paid, and never overwrite existing fields unless you explicitly apply them.".to_string(),
    ];

    let (provider, raw_text) = match kind {
        InvoiceReadKind::Pdf => (
            "local_pdf_text".to_string(),
            pdf_extract::extract_text_from_mem(bytes).map_err(|e| {
                format!("This PDF could not be read locally. If it is a scanned PDF image, try a clear PNG or JPG photo instead. Technical detail: {e}")
            })?,
        ),
        InvoiceReadKind::Png | InvoiceReadKind::Jpeg => {
            limitations.push("Image OCR uses the local Windows OCR capability in the desktop app when available.".to_string());
            (
                "windows_local_ocr".to_string(),
                extract_image_text(paths, file_name, bytes)?,
            )
        }
    };

    if kind == InvoiceReadKind::Pdf {
        limitations.push("PDF reading in this release works best when the PDF already contains selectable text. Scanned image-only PDFs may return little or no text.".to_string());
    }

    Ok(InvoiceTextRead {
        provider,
        raw_text,
        limitations,
    })
}

fn candidate_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '.' | ',' | ':' | '#') {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn parse_amount_candidate(raw: &str) -> Option<f64> {
    let mut cleaned = raw
        .chars()
        .filter(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '.' | '-'))
        .collect::<String>();
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.starts_with('-') {
        return None;
    }

    let dot_count = cleaned.matches('.').count();
    let comma_count = cleaned.matches(',').count();

    if dot_count > 0 && comma_count > 0 {
        let last_dot = cleaned.rfind('.').unwrap_or(0);
        let last_comma = cleaned.rfind(',').unwrap_or(0);
        if last_dot > last_comma {
            cleaned = cleaned.replace(',', "");
        } else {
            cleaned = cleaned.replace('.', "").replace(',', ".");
        }
    } else if comma_count > 0 {
        let last_comma = cleaned.rfind(',').unwrap_or(0);
        let decimals = cleaned.len().saturating_sub(last_comma + 1);
        if decimals == 2 {
            cleaned = cleaned.replace('.', "").replace(',', ".");
        } else {
            cleaned = cleaned.replace(',', "");
        }
    } else if dot_count > 1 {
        let last_dot = cleaned.rfind('.').unwrap_or(0);
        let decimals = cleaned.len().saturating_sub(last_dot + 1);
        if decimals == 2 {
            let integer = cleaned[..last_dot].replace('.', "");
            cleaned = format!("{integer}.{}", &cleaned[last_dot + 1..]);
        } else {
            cleaned = cleaned.replace('.', "");
        }
    }

    cleaned.parse::<f64>().ok().filter(|value| *value >= 0.0)
}

fn parse_date_candidate(candidate: &str) -> Option<String> {
    let cleaned = candidate
        .trim_matches(|ch: char| matches!(ch, ':' | ',' | ';' | '.'))
        .trim();
    if cleaned.is_empty() {
        return None;
    }

    let numeric_formats = [
        format_description!("[year]-[month]-[day]"),
        format_description!("[day]-[month]-[year]"),
        format_description!("[day]/[month]/[year]"),
        format_description!("[day].[month].[year]"),
    ];
    for format in numeric_formats {
        if let Ok(date) = Date::parse(cleaned, format) {
            return Some(
                date.format(&format_description!("[year]-[month]-[day]"))
                    .ok()?,
            );
        }
    }

    let alpha_candidate = cleaned.replace(',', "");
    let alpha_formats = [
        format_description!("[day] [month repr:short] [year]"),
        format_description!("[day] [month repr:long] [year]"),
        format_description!("[month repr:short] [day] [year]"),
        format_description!("[month repr:long] [day] [year]"),
    ];
    for format in alpha_formats {
        if let Ok(date) = Date::parse(&alpha_candidate, format) {
            return Some(
                date.format(&format_description!("[year]-[month]-[day]"))
                    .ok()?,
            );
        }
    }

    None
}

fn find_first_date(text: &str) -> Option<String> {
    for segment in candidate_segments(text) {
        if let Some(date) = parse_date_candidate(&segment) {
            return Some(date);
        }
    }
    None
}

fn is_hiload_identity(text: &str) -> bool {
    let normalized = normalize_for_match(text);
    normalized.contains("hiload")
        || normalized.contains("hiload inyanga construction")
        || normalized.contains("hiload inyanga plant hire")
}

fn line_has_label(line: &str, label: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    let label = label.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(found) = lower[search_from..].find(&label) {
        let position = search_from + found;
        let before = lower[..position].chars().last();
        let after = lower[position + label.len()..].chars().next();
        let valid_before = before.map(|ch| !ch.is_ascii_alphanumeric()).unwrap_or(true);
        let valid_after = after.map(|ch| !ch.is_ascii_alphanumeric()).unwrap_or(true);
        if valid_before && valid_after {
            return Some(position);
        }
        search_from = position + label.len();
    }
    None
}

fn collect_labeled_line_matches(lines: &[&str], labels: &[&str]) -> Vec<LineValueMatch> {
    let mut matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        for label in labels {
            if let Some(position) = line_has_label(line, label) {
                let after = line[position + label.len()..]
                    .trim_matches(|ch: char| matches!(ch, ':' | '#' | '-' | ' ' | '\t'))
                    .trim();
                let value = if !after.is_empty() {
                    after.to_string()
                } else if let Some(next_line) = lines
                    .get(index + 1)
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    next_line.to_string()
                } else {
                    continue;
                };
                matches.push(LineValueMatch {
                    value,
                    evidence: line.trim().to_string(),
                    label: (*label).to_string(),
                    index,
                });
            }
        }
    }
    matches
}

fn looks_like_invoice_number(value: &str) -> bool {
    let trimmed = value
        .trim()
        .trim_matches(|ch: char| matches!(ch, ':' | '#' | '.'));
    if trimmed.len() < 3 || trimmed.len() > 40 {
        return false;
    }
    if parse_date_candidate(trimmed).is_some() || parse_amount_candidate(trimmed).is_some() {
        return false;
    }
    let has_digit = trimmed.chars().any(|ch| ch.is_ascii_digit());
    let has_alpha = trimmed.chars().any(|ch| ch.is_ascii_alphabetic());
    let cleaned = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '/' | '_'))
        .collect::<String>();
    has_digit && (has_alpha || cleaned.chars().filter(|ch| ch.is_ascii_digit()).count() >= 4)
}

fn choose_invoice_number(lines: &[&str], warnings: &mut Vec<String>) -> Option<LineValueMatch> {
    let labels = [
        "tax invoice no",
        "tax invoice number",
        "invoice number",
        "invoice no",
        "invoice #",
        "inv no",
        "inv #",
    ];
    let matches = collect_labeled_line_matches(lines, &labels)
        .into_iter()
        .filter(|item| looks_like_invoice_number(&item.value))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let distinct = matches
        .iter()
        .map(|item| normalize_for_match(&item.value))
        .collect::<Vec<_>>();
    if distinct
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1
    {
        warnings.push(
            "More than one invoice-number candidate was found, so invoice number was left blank."
                .to_string(),
        );
        return None;
    }
    matches.into_iter().next()
}

fn choose_invoice_date(lines: &[&str], warnings: &mut Vec<String>) -> Option<LineValueMatch> {
    let labels = [
        "invoice date",
        "tax invoice date",
        "date issued",
        "issue date",
        "invoice dt",
        "date",
    ];
    let matches = collect_labeled_line_matches(lines, &labels)
        .into_iter()
        .filter_map(|item| {
            let lower = item.evidence.to_ascii_lowercase();
            if lower.contains("due date") || lower.contains("payment due") {
                return None;
            }
            let parsed =
                parse_date_candidate(&item.value).or_else(|| find_first_date(&item.value))?;
            Some(LineValueMatch {
                value: parsed,
                evidence: item.evidence,
                label: item.label,
                index: item.index,
            })
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let distinct = matches
        .iter()
        .map(|item| item.value.clone())
        .collect::<Vec<_>>();
    if distinct
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1
    {
        warnings.push(
            "More than one invoice-date candidate was found, so invoice date was left blank."
                .to_string(),
        );
        return None;
    }
    matches.into_iter().next()
}

fn collect_amount_matches(
    lines: &[&str],
    labels: &[(&str, i32)],
    exclude_words: &[&str],
) -> Vec<AmountMatch> {
    let mut matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        if exclude_words.iter().any(|word| lower.contains(word)) {
            continue;
        }
        for (label, priority) in labels {
            if let Some(position) = line_has_label(line, label) {
                let after = line[position + label.len()..]
                    .trim_matches(|ch: char| matches!(ch, ':' | '-' | ' ' | '\t'))
                    .trim();
                let amount = parse_amount_candidate(after).or_else(|| {
                    lines
                        .get(index + 1)
                        .map(|value| value.trim())
                        .and_then(parse_amount_candidate)
                });
                if let Some(amount) = amount {
                    matches.push(AmountMatch {
                        amount,
                        evidence: line.trim().to_string(),
                        index,
                        priority: *priority,
                    });
                }
            }
        }
    }
    matches
}

fn choose_best_amount(
    mut matches: Vec<AmountMatch>,
    warnings: &mut Vec<String>,
    ambiguity_message: &str,
) -> Option<AmountMatch> {
    if matches.is_empty() {
        return None;
    }
    matches.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.index.cmp(&b.index))
    });
    let top_priority = matches[0].priority;
    let top = matches
        .iter()
        .filter(|item| item.priority == top_priority)
        .cloned()
        .collect::<Vec<_>>();
    if top
        .iter()
        .map(|item| format!("{:.2}", item.amount))
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1
    {
        warnings.push(ambiguity_message.to_string());
        return None;
    }
    top.into_iter().next()
}

fn choose_notes(lines: &[&str]) -> Option<LineValueMatch> {
    let labeled = collect_labeled_line_matches(
        lines,
        &[
            "description",
            "details",
            "goods",
            "services",
            "work done",
            "remarks",
            "notes",
        ],
    );
    if let Some(item) = labeled
        .into_iter()
        .find(|item| item.value.chars().any(|ch| ch.is_ascii_alphabetic()))
    {
        return Some(LineValueMatch {
            value: truncate_chars(&item.value, 240),
            evidence: item.evidence,
            label: item.label,
            index: item.index,
        });
    }

    let filtered = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !line.is_empty()
                && !lower.contains("invoice")
                && !lower.contains("subtotal")
                && !lower.contains("vat")
                && !lower.contains("total")
                && !lower.contains("date")
                && !lower.contains("bill to")
                && !lower.contains("customer")
                && line.chars().any(|ch| ch.is_ascii_alphabetic())
        })
        .take(2)
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        None
    } else {
        Some(LineValueMatch {
            value: truncate_chars(&filtered.join(" | "), 240),
            evidence: filtered.join(" | "),
            label: "description".to_string(),
            index: 0,
        })
    }
}

fn is_probable_company_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 || trimmed.len() > 120 {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let blocked = [
        "invoice",
        "tax invoice",
        "subtotal",
        "total",
        "amount due",
        "invoice date",
        "due date",
        "customer",
        "bill to",
        "ship to",
        "payment",
        "bank",
        "vat",
        "telephone",
        "phone",
        "email",
        "www",
        "page",
    ];
    if blocked.iter().any(|word| lower.contains(word)) {
        return false;
    }
    if is_hiload_identity(trimmed) {
        return false;
    }
    if parse_date_candidate(trimmed).is_some() || parse_amount_candidate(trimmed).is_some() {
        return false;
    }
    let digit_count = trimmed.chars().filter(|ch| ch.is_ascii_digit()).count();
    let alpha_count = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    alpha_count >= 2 && digit_count <= trimmed.len() / 3
}

fn choose_supplier_candidate(lines: &[&str], warnings: &mut Vec<String>) -> Option<LineValueMatch> {
    let labeled = collect_labeled_line_matches(
        lines,
        &[
            "supplier",
            "vendor",
            "from",
            "billed by",
            "seller",
            "issued by",
        ],
    )
    .into_iter()
    .filter(|item| !is_hiload_identity(&item.value))
    .collect::<Vec<_>>();
    if let Some(item) = labeled.into_iter().next() {
        return Some(item);
    }

    let contact_words = ["vat", "tel", "phone", "email", "bank", "cell", "fax", "reg"];
    let header_words = ["invoice", "tax invoice"];
    let mut best: Option<(i32, LineValueMatch)> = None;
    for (index, line) in lines.iter().take(12).enumerate() {
        if !is_probable_company_line(line) {
            continue;
        }
        let mut score = if index < 4 { 4 } else { 2 };
        let start = index.saturating_sub(1);
        let end = usize::min(lines.len(), index + 3);
        let nearby = lines[start..end].join(" ").to_ascii_lowercase();
        if contact_words.iter().any(|word| nearby.contains(word)) {
            score += 3;
        }
        if header_words.iter().any(|word| nearby.contains(word)) {
            score += 2;
        }
        let candidate = LineValueMatch {
            value: line.trim().to_string(),
            evidence: line.trim().to_string(),
            label: "header".to_string(),
            index,
        };
        if best
            .as_ref()
            .map(|(best_score, _)| score > *best_score)
            .unwrap_or(true)
        {
            best = Some((score, candidate));
        }
    }
    if best.is_none()
        && lines
            .iter()
            .any(|line| contains_normalized_text(line, "hiload"))
    {
        warnings.push("Hiload details were detected in the OCR text and were excluded from supplier matching.".to_string());
    }
    best.map(|(_, item)| item)
}

fn find_supplier_id_by_name<'a>(suppliers: &'a [Value], name: &str) -> Option<&'a str> {
    let target = normalize_for_match(name);
    suppliers.iter().find_map(|supplier| {
        let supplier_name = supplier.get("name").and_then(Value::as_str)?;
        (normalize_for_match(supplier_name) == target).then_some(
            supplier
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    })
}

fn apply_supplier_history_rule(
    candidate: &str,
    context: &InvoiceAnalysisContext,
) -> Option<String> {
    let key = normalize_for_match(candidate);
    let mut matches = context
        .rules
        .iter()
        .filter(|rule| rule.rule_type == "supplier_match" && rule.supplier_key == key)
        .map(|rule| rule.target_value.clone())
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    if matches.len() == 1 {
        Some(matches.remove(0))
    } else {
        None
    }
}

fn plant_aliases(plant: &Value) -> Vec<String> {
    let mut aliases = Vec::new();
    for key in ["name", "make", "model", "serial"] {
        if let Some(value) = plant.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                aliases.push(trimmed.to_string());
            }
        }
    }
    let make = plant
        .get("make")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let model = plant
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if !make.is_empty() && !model.is_empty() {
        aliases.push(format!("{make} {model}"));
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn plant_rule_boost(
    rules: &[OcrCorrectionRule],
    supplier_id: Option<&str>,
    alias: &str,
    plant_id: &str,
) -> i32 {
    let hint_key = normalize_for_match(alias);
    let supplier_id = supplier_id.unwrap_or("");
    rules
        .iter()
        .filter(|rule| {
            rule.rule_type == "plant_match"
                && rule.target_value == plant_id
                && rule.hint_key == hint_key
                && (rule.supplier_key.is_empty() || rule.supplier_key == supplier_id)
        })
        .map(|rule| 2 + i32::try_from(rule.usage_count).unwrap_or(0).min(4))
        .max()
        .unwrap_or(0)
}

fn choose_plant_match(
    text: &str,
    context: &InvoiceAnalysisContext,
    supplier_id: Option<&str>,
) -> Option<PlantMatch> {
    let normalized_text = normalize_for_match(text);
    let mut matches = Vec::new();
    for plant in &context.plants {
        let plant_id = plant.get("id").and_then(Value::as_str).unwrap_or("").trim();
        if plant_id.is_empty() {
            continue;
        }
        let plant_name = plant
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Plant")
            .trim();
        for alias in plant_aliases(plant) {
            let normalized_alias = normalize_for_match(&alias);
            if normalized_alias.len() < 3 || !normalized_text.contains(&normalized_alias) {
                continue;
            }
            let mut score =
                i32::try_from(normalized_alias.split_whitespace().count()).unwrap_or(0) * 2;
            if alias.chars().any(|ch| ch.is_ascii_digit()) {
                score += 3;
            }
            if alias.eq_ignore_ascii_case(plant.get("serial").and_then(Value::as_str).unwrap_or(""))
            {
                score += 4;
            }
            score += plant_rule_boost(&context.rules, supplier_id, &alias, plant_id);
            matches.push(PlantMatch {
                plant_id: plant_id.to_string(),
                plant_name: plant_name.to_string(),
                alias,
                score,
            });
        }
    }
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.alias.len().cmp(&a.alias.len()))
    });
    let best = matches.first()?.clone();
    let second = matches.get(1);
    if best.score < 5 {
        return None;
    }
    if let Some(other) = second {
        if other.plant_id != best.plant_id && other.score >= best.score - 1 {
            return None;
        }
    }
    Some(best)
}

fn build_invoice_analysis(text: &str, context: &InvoiceAnalysisContext) -> InvoiceAnalysis {
    let lines = text.lines().collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();

    let supplier = choose_supplier_candidate(&lines, &mut warnings);
    let supplier_candidate = supplier.as_ref().map(|item| item.value.clone());
    let matched_supplier_id = supplier_candidate
        .as_deref()
        .and_then(|candidate| {
            find_supplier_id_by_name(&context.suppliers, candidate).map(ToOwned::to_owned)
        })
        .or_else(|| {
            supplier_candidate
                .as_deref()
                .and_then(|candidate| apply_supplier_history_rule(candidate, context))
        });
    if let Some(item) = supplier {
        let confidence = if matched_supplier_id.is_some() || item.label != "header" {
            "high"
        } else {
            "medium"
        };
        suggestions.push(InvoiceFieldSuggestion {
            field: "supplierName".to_string(),
            label: "Supplier".to_string(),
            value: item.value,
            confidence: confidence.to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: if matched_supplier_id.is_some() {
                "Matched from invoice issuer evidence and existing supplier history.".to_string()
            } else {
                "Issuer-style company text was found, but no reliable existing supplier match was confirmed.".to_string()
            },
        });
    }

    let invoice_number = choose_invoice_number(&lines, &mut warnings);
    if let Some(item) = &invoice_number {
        suggestions.push(InvoiceFieldSuggestion {
            field: "invoiceNumber".to_string(),
            label: "Invoice number".to_string(),
            value: truncate_chars(&item.value, 80),
            confidence: "high".to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Found next to an invoice-number label.".to_string(),
        });
    }

    let invoice_date = choose_invoice_date(&lines, &mut warnings);
    if let Some(item) = &invoice_date {
        suggestions.push(InvoiceFieldSuggestion {
            field: "invoiceDate".to_string(),
            label: "Invoice date".to_string(),
            value: item.value.clone(),
            confidence: if item.label == "date" {
                "medium"
            } else {
                "high"
            }
            .to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Taken from an invoice-date style label. Due dates are excluded.".to_string(),
        });
    }

    let net_match = choose_best_amount(
        collect_amount_matches(
            &lines,
            &[("subtotal", 3), ("sub total", 3), ("net amount", 3)],
            &[],
        ),
        &mut warnings,
        "More than one subtotal/net amount candidate was found, so net amount was left blank.",
    );
    if let Some(item) = &net_match {
        suggestions.push(InvoiceFieldSuggestion {
            field: "netAmount".to_string(),
            label: "Net amount".to_string(),
            value: format!("{:.2}", item.amount),
            confidence: "medium".to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Found next to a subtotal or net-amount label.".to_string(),
        });
    }

    let vat_match = choose_best_amount(
        collect_amount_matches(
            &lines,
            &[("vat", 3), ("tax", 2)],
            &["subtotal", "amount due", "total due", "invoice total"],
        ),
        &mut warnings,
        "More than one VAT/tax amount candidate was found, so VAT amount was left blank.",
    );
    if let Some(item) = &vat_match {
        suggestions.push(InvoiceFieldSuggestion {
            field: "vatAmount".to_string(),
            label: "VAT amount".to_string(),
            value: format!("{:.2}", item.amount),
            confidence: "medium".to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Found next to a VAT/tax label.".to_string(),
        });
    }

    let total_match = choose_best_amount(
        collect_amount_matches(
            &lines,
            &[
                ("invoice total", 5),
                ("grand total", 5),
                ("amount due", 5),
                ("total due", 5),
                ("total", 2),
            ],
            &[
                "subtotal",
                "sub total",
                "amount paid",
                "paid",
                "balance brought forward",
            ],
        ),
        &mut warnings,
        "More than one total candidate was found, so total amount was left blank.",
    );
    if let Some(item) = &total_match {
        suggestions.push(InvoiceFieldSuggestion {
            field: "totalCost".to_string(),
            label: "Total amount".to_string(),
            value: format!("{:.2}", item.amount),
            confidence: if item.priority >= 5 { "high" } else { "medium" }.to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Taken from final invoice-total style wording, not subtotal or VAT."
                .to_string(),
        });
    }

    let notes = choose_notes(&lines);
    if let Some(item) = &notes {
        suggestions.push(InvoiceFieldSuggestion {
            field: "notes".to_string(),
            label: "Description / notes".to_string(),
            value: item.value.clone(),
            confidence: if item.label == "description" {
                "medium"
            } else {
                "low"
            }
            .to_string(),
            evidence: truncate_chars(&item.evidence, 220),
            reason: "Taken from invoice description/goods/services text when available."
                .to_string(),
        });
    }

    let matched_plant = choose_plant_match(text, context, matched_supplier_id.as_deref());
    if let Some(item) = &matched_plant {
        suggestions.push(InvoiceFieldSuggestion {
            field: "plantId".to_string(),
            label: "Plant / machine".to_string(),
            value: item.plant_id.clone(),
            confidence: if item.score >= 8 { "high" } else { "medium" }.to_string(),
            evidence: item.alias.clone(),
            reason: format!(
                "Matched invoice text to existing plant \"{}\" using the alias shown in evidence.",
                item.plant_name
            ),
        });
    } else if !context.plants.is_empty() {
        warnings.push("No reliable plant/equipment match was confirmed from the OCR text, so the invoice should stay in Needs Review.".to_string());
    }

    if let (Some(net_amount), Some(vat_amount), Some(total_amount)) = (
        net_match.as_ref().map(|item| item.amount),
        vat_match.as_ref().map(|item| item.amount),
        total_match.as_ref().map(|item| item.amount),
    ) {
        if (net_amount + vat_amount - total_amount).abs() > 1.0 {
            warnings.push("Net amount + VAT does not match the total amount, so the invoice should be reviewed carefully.".to_string());
        }
    }

    let needs_review = matched_supplier_id.is_none()
        || invoice_number.is_none()
        || invoice_date.is_none()
        || total_match.is_none()
        || matched_plant.is_none()
        || !warnings.is_empty();

    InvoiceAnalysis {
        suggestions,
        warnings,
        supplier_candidate,
        matched_supplier_id,
        invoice_number: invoice_number.map(|item| item.value),
        invoice_date: invoice_date.map(|item| item.value),
        notes: notes.map(|item| item.value),
        net_amount: net_match.map(|item| item.amount),
        vat_amount: vat_match.map(|item| item.amount),
        total_cost: total_match.map(|item| item.amount),
        matched_plant_id: matched_plant.map(|item| item.plant_id),
        needs_review,
    }
}

fn read_invoice_document_inner(
    paths: &AppPaths,
    request: &InvoiceReadRequest,
    context: &InvoiceAnalysisContext,
) -> Result<InvoiceReadResult, String> {
    let (bytes, file_name, mime_type) = load_invoice_read_bytes(paths, request)?;
    let read = read_invoice_text_from_bytes(paths, &file_name, &mime_type, &bytes)?;
    let mut warnings = Vec::new();
    let normalized_text = normalize_invoice_text(&read.raw_text);
    let analysis = build_invoice_analysis(&normalized_text, context);
    warnings.extend(analysis.warnings.clone());
    if normalized_text.is_empty() {
        warnings.push(
            "No readable text was found. If this is a scanned PDF, try a clear PNG or JPG photo in the Windows desktop app."
                .to_string(),
        );
    }
    let extracted_text = if normalized_text.chars().count() > MAX_INVOICE_EXTRACT_TEXT_CHARS {
        warnings.push("Only the first part of the extracted text is shown here so the review stays manageable.".to_string());
        truncate_chars(&normalized_text, MAX_INVOICE_EXTRACT_TEXT_CHARS)
    } else {
        normalized_text
    };
    let message = if extracted_text.is_empty() {
        "The file was checked, but no readable invoice text was found.".to_string()
    } else if analysis.suggestions.is_empty() {
        "The file text was read, but no invoice fields were confidently recognised. Please review the raw text below.".to_string()
    } else {
        format!(
            "Read {} suggestion(s). Please check the evidence carefully before applying any value.",
            analysis.suggestions.len()
        )
    };

    Ok(InvoiceReadResult {
        source_file_name: file_name,
        mime_type,
        provider: read.provider,
        message,
        extracted_text,
        suggestions: analysis.suggestions,
        warnings,
        limitations: read.limitations,
    })
}

fn next_batch_invoice_id(file_name: &str) -> String {
    let safe = sanitize_path_component(file_name, "invoice");
    format!(
        "inv_batch_{}_{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos(),
        safe
    )
}

fn value_as_array_mut<'a>(
    obj: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Vec<Value>, String> {
    let entry = obj
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    entry
        .as_array_mut()
        .ok_or_else(|| format!("State field '{key}' must be a list."))
}

fn value_as_object_mut<'a>(
    obj: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let entry = obj
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    entry
        .as_object_mut()
        .ok_or_else(|| format!("State field '{key}' must be an object."))
}

fn find_duplicate_by_checksum(
    data: &Map<String, Value>,
    checksum: &str,
) -> Option<(String, String)> {
    value_as_object(data, "hiph_invoice_docs")?
        .iter()
        .find_map(|(invoice_id, item)| {
            (item.get("sha256").and_then(Value::as_str) == Some(checksum)).then(|| {
                (
                    invoice_id.clone(),
                    item.get("fileName")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                )
            })
        })
}

fn find_duplicate_by_supplier_invoice(
    data: &Map<String, Value>,
    supplier_id: &str,
    invoice_number: &str,
) -> Option<String> {
    let wanted_number = normalize_for_match(invoice_number);
    value_as_array(data, "hiph_invoices")
        .iter()
        .find_map(|invoice| {
            let same_supplier = invoice
                .get("supplierId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                == supplier_id;
            let same_number = normalize_for_match(
                invoice
                    .get("invoiceNumber")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ) == wanted_number;
            (same_supplier && same_number).then(|| {
                invoice
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
        })
}

fn save_managed_invoice_document_in_area(
    paths: &AppPaths,
    invoice_id: &str,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
    area: Option<&str>,
) -> Result<AttachmentImportResult, String> {
    if invoice_id.trim().is_empty() {
        return Err("The invoice ID is missing, so the attachment could not be saved.".to_string());
    }
    if bytes.is_empty() {
        return Err("The selected attachment file was empty.".to_string());
    }

    let safe_file_name = safe_display_file_name(file_name, mime_type);
    let checksum = sha256_hex(bytes);
    let safe_area = area
        .map(|value| sanitize_path_component(value, ""))
        .filter(|value| !value.is_empty());
    let safe_invoice_id = sanitize_path_component(invoice_id, "invoice");
    let short_checksum = checksum.chars().take(12).collect::<String>();
    let relative_path = match safe_area {
        Some(area) => format!(
            "{MANAGED_INVOICE_DIR}/{area}/{}--{}--{}",
            safe_invoice_id, short_checksum, safe_file_name
        ),
        None => managed_document_relative_path(invoice_id, &safe_file_name, &checksum),
    };
    let full_path = resolve_managed_document_path(paths, &relative_path)?;
    write_file_atomic(&full_path, bytes)?;

    Ok(AttachmentImportResult {
        relative_path,
        full_path: full_path.to_string_lossy().to_string(),
        file_name: safe_file_name,
        mime_type: mime_type.to_string(),
        size_bytes: bytes.len() as u64,
        sha256: checksum,
        message: "Attachment saved in the desktop documents folder.".to_string(),
    })
}

fn persist_live_state(
    conn: &mut Connection,
    data: &Map<String, Value>,
    source: &str,
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("Could not start save transaction: {e}"))?;
    replace_live_state(&tx, data, source)?;
    tx.commit()
        .map_err(|e| format!("Could not commit saved data: {e}"))?;
    Ok(())
}

fn process_batch_invoice_file(
    conn: &mut Connection,
    paths: &AppPaths,
    current_data: &mut Map<String, Value>,
    rules: &[OcrCorrectionRule],
    source_path: &Path,
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
    extracted_text: &str,
    provider: &str,
) -> Result<BatchInvoiceProcessEntry, String> {
    let checksum = sha256_hex(bytes);
    if let Some((invoice_id, existing_file_name)) =
        find_duplicate_by_checksum(current_data, &checksum)
    {
        return Ok(BatchInvoiceProcessEntry {
            file_name: file_name.to_string(),
            outcome: "possible duplicate".to_string(),
            reason: format!("Managed document checksum already exists on invoice {invoice_id} ({existing_file_name}). Source file was left in place."),
            invoice_id: String::new(),
            duplicate_invoice_id: invoice_id,
            supplier_name: String::new(),
            invoice_number: String::new(),
            needs_review: true,
        });
    }

    let normalized_text = normalize_invoice_text(extracted_text);
    let context = analysis_context_from_data(current_data, rules.to_vec());
    let analysis = build_invoice_analysis(&normalized_text, &context);
    if let (Some(supplier_id), Some(invoice_number)) = (
        analysis.matched_supplier_id.as_deref(),
        analysis.invoice_number.as_deref(),
    ) {
        if let Some(duplicate_invoice_id) =
            find_duplicate_by_supplier_invoice(current_data, supplier_id, invoice_number)
        {
            return Ok(BatchInvoiceProcessEntry {
                file_name: file_name.to_string(),
                outcome: "possible duplicate".to_string(),
                reason: format!("Supplier + invoice number already exist on invoice {duplicate_invoice_id}. Source file was left in place."),
                invoice_id: String::new(),
                duplicate_invoice_id,
                supplier_name: analysis.supplier_candidate.unwrap_or_default(),
                invoice_number: invoice_number.to_string(),
                needs_review: true,
            });
        }
    }

    let invoice_id = next_batch_invoice_id(file_name);
    let area = if analysis.needs_review {
        Some("needs-review")
    } else {
        Some("processed")
    };
    let document = save_managed_invoice_document_in_area(
        paths,
        &invoice_id,
        file_name,
        mime_type,
        bytes,
        area,
    )?;

    let supplier_name = analysis.supplier_candidate.clone().unwrap_or_default();
    let invoice_number = analysis.invoice_number.clone().unwrap_or_default();
    let needs_review = analysis.needs_review;
    let review_warnings = analysis.warnings.clone();
    let review_suggestions = analysis.suggestions.clone();
    let document_file_name = document.file_name.clone();
    let document_mime_type = document.mime_type.clone();
    let document_relative_path = document.relative_path.clone();
    let document_sha256 = document.sha256.clone();
    let invoice_record = json!({
        "id": invoice_id.clone(),
        "supplierId": analysis.matched_supplier_id.clone().unwrap_or_default(),
        "supplierNameCandidate": supplier_name.clone(),
        "invoiceNumber": invoice_number.clone(),
        "invoiceDate": analysis.invoice_date.clone().unwrap_or_default(),
        "receivedAt": unix_timestamp_string(),
        "createdAt": unix_timestamp_string(),
        "plantId": analysis.matched_plant_id.clone().unwrap_or_default(),
        "maintenanceType": "",
        "notes": analysis.notes.clone().unwrap_or_default(),
        "netAmount": analysis.net_amount,
        "vatAmount": analysis.vat_amount,
        "totalCost": analysis.total_cost.unwrap_or(0.0),
        "filename": document_file_name.clone(),
        "fileMimeType": document_mime_type.clone(),
        "hasDocument": true,
        "docStorageStatus": "desktop_managed",
        "status": "pending_review",
        "duplicateOfIds": [],
        "isDuplicateNumber": false,
        "batchImported": true,
        "needsReview": needs_review,
        "ocrReview": {
            "provider": provider,
            "suggestions": review_suggestions,
            "warnings": review_warnings
        }
    });
    let document_record = json!({
        "fileName": document_file_name,
        "mimeType": document_mime_type,
        "size": document.size_bytes,
        "storedAt": unix_timestamp_string(),
        "managedRelativePath": document_relative_path,
        "sha256": document_sha256,
        "storageKind": "desktop_managed_file"
    });

    let previous_data = current_data.clone();
    let save_result = (|| -> Result<(), String> {
        value_as_array_mut(current_data, "hiph_invoices")?.push(invoice_record);
        value_as_object_mut(current_data, "hiph_invoice_docs")?
            .insert(invoice_id.clone(), document_record);
        persist_live_state(conn, current_data, "batch_invoice_import")?;
        Ok(())
    })();

    if let Err(err) = save_result {
        *current_data = previous_data;
        let _ = fs::remove_file(Path::new(&document.full_path));
        return Err(err);
    }

    let managed_path = PathBuf::from(&document.full_path);
    if !managed_path.is_file() {
        return Err("The managed invoice document could not be verified after save, so the source file was left untouched.".to_string());
    }

    let remove_result = fs::remove_file(source_path);
    let mut outcome = if needs_review {
        "needs review".to_string()
    } else {
        "processed".to_string()
    };
    let mut reason = if needs_review {
        "Draft invoice saved in the managed Needs Review area. Review the OCR evidence before posting.".to_string()
    } else {
        "Draft invoice saved and source file moved into the managed Processed area.".to_string()
    };
    if let Err(err) = remove_result {
        outcome = "error".to_string();
        reason = format!("Invoice draft and managed document were saved, but the source file could not be removed from the selected folder: {err}");
    }

    Ok(BatchInvoiceProcessEntry {
        file_name: file_name.to_string(),
        outcome,
        reason,
        invoice_id,
        duplicate_invoice_id: String::new(),
        supplier_name,
        invoice_number,
        needs_review,
    })
}

fn process_invoice_inbox_folder_inner(
    paths: &AppPaths,
    conn: &mut Connection,
    folder_path: &Path,
) -> Result<BatchInvoiceProcessResult, String> {
    if !folder_path.is_dir() {
        return Err(
            "Choose an existing folder that contains your monthly invoice files.".to_string(),
        );
    }
    let mut current_data = load_live_state(conn)?.0;
    let rules = load_ocr_correction_rules(conn)?;

    let mut files = fs::read_dir(folder_path)
        .map_err(|e| format!("Could not read the selected invoice folder: {e}"))?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();
    if files.len() > MAX_BATCH_INVOICE_FILES {
        return Err(format!(
            "The selected folder contains {} files. Please keep each run to about {} files or fewer.",
            files.len(), MAX_BATCH_INVOICE_FILES
        ));
    }

    let mut entries = Vec::new();
    let mut processed_count = 0u64;
    let mut needs_review_count = 0u64;
    let mut duplicate_count = 0u64;
    let mut skipped_count = 0u64;
    let mut error_count = 0u64;

    for path in files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("invoice-document")
            .to_string();
        let kind = match detect_invoice_read_kind(&file_name, "") {
            Ok(kind) => kind,
            Err(_) => {
                skipped_count += 1;
                entries.push(BatchInvoiceProcessEntry {
                    file_name,
                    outcome: "skipped".to_string(),
                    reason:
                        "Unsupported file type. Supported files are PDF, PNG, JPG and JPEG only."
                            .to_string(),
                    invoice_id: String::new(),
                    duplicate_invoice_id: String::new(),
                    supplier_name: String::new(),
                    invoice_number: String::new(),
                    needs_review: true,
                });
                continue;
            }
        };
        let mime_type = mime_type_from_kind(kind).to_string();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                error_count += 1;
                entries.push(BatchInvoiceProcessEntry {
                    file_name,
                    outcome: "error".to_string(),
                    reason: format!("Could not read the source file: {err}"),
                    invoice_id: String::new(),
                    duplicate_invoice_id: String::new(),
                    supplier_name: String::new(),
                    invoice_number: String::new(),
                    needs_review: true,
                });
                continue;
            }
        };
        if bytes.len() > MAX_INVOICE_READ_BYTES {
            error_count += 1;
            entries.push(BatchInvoiceProcessEntry {
                file_name,
                outcome: "error".to_string(),
                reason: format!(
                    "File is larger than the safe OCR limit of {} MB, so it was left in the source folder.",
                    MAX_INVOICE_READ_BYTES / (1024 * 1024)
                ),
                invoice_id: String::new(),
                duplicate_invoice_id: String::new(),
                supplier_name: String::new(),
                invoice_number: String::new(),
                needs_review: true,
            });
            continue;
        }
        let read = match read_invoice_text_from_bytes(paths, &file_name, &mime_type, &bytes) {
            Ok(read) => read,
            Err(err) => {
                error_count += 1;
                entries.push(BatchInvoiceProcessEntry {
                    file_name,
                    outcome: "error".to_string(),
                    reason: format!(
                        "OCR could not read this file, so it was left in the source folder. {err}"
                    ),
                    invoice_id: String::new(),
                    duplicate_invoice_id: String::new(),
                    supplier_name: String::new(),
                    invoice_number: String::new(),
                    needs_review: true,
                });
                continue;
            }
        };
        match process_batch_invoice_file(
            conn,
            paths,
            &mut current_data,
            &rules,
            &path,
            &file_name,
            &mime_type,
            &bytes,
            &read.raw_text,
            &read.provider,
        ) {
            Ok(entry) => {
                match entry.outcome.as_str() {
                    "processed" => processed_count += 1,
                    "needs review" => {
                        processed_count += 1;
                        needs_review_count += 1;
                    }
                    "possible duplicate" => duplicate_count += 1,
                    "skipped" => skipped_count += 1,
                    _ => error_count += 1,
                }
                entries.push(entry);
            }
            Err(err) => {
                error_count += 1;
                entries.push(BatchInvoiceProcessEntry {
                    file_name,
                    outcome: "error".to_string(),
                    reason: format!("Invoice could not be saved safely, so the source file was left untouched. {err}"),
                    invoice_id: String::new(),
                    duplicate_invoice_id: String::new(),
                    supplier_name: String::new(),
                    invoice_number: String::new(),
                    needs_review: true,
                });
            }
        }
    }

    let message = format!(
        "Processed: {processed_count}. Needs Review: {needs_review_count}. Possible duplicates: {duplicate_count}. Skipped: {skipped_count}. Errors: {error_count}. 'Processed' means the draft invoice and managed document were saved safely; nothing was posted automatically."
    );

    Ok(BatchInvoiceProcessResult {
        source_folder: folder_path.to_string_lossy().to_string(),
        processed_count,
        needs_review_count,
        duplicate_count,
        skipped_count,
        error_count,
        entries,
        message,
    })
}

fn record_invoice_ocr_feedback_inner(
    conn: &Connection,
    context: &InvoiceAnalysisContext,
    request: &OcrCorrectionFeedbackRequest,
) -> Result<(), String> {
    let text = normalize_invoice_text(&request.ocr_text);
    if text.trim().is_empty() {
        return Ok(());
    }

    if let (Some(candidate), Some(supplier_id)) = (
        request.supplier_candidate.as_deref().map(str::trim),
        request.supplier_id.as_deref().map(str::trim),
    ) {
        if !candidate.is_empty()
            && !supplier_id.is_empty()
            && !is_hiload_identity(candidate)
            && context
                .suppliers
                .iter()
                .any(|supplier| supplier.get("id").and_then(Value::as_str) == Some(supplier_id))
        {
            upsert_ocr_correction_rule(
                conn,
                "supplier_match",
                &normalize_for_match(candidate),
                "",
                supplier_id,
            )?;
        }
    }

    if let Some(plant_id) = request
        .plant_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(plant) = context
            .plants
            .iter()
            .find(|plant| plant.get("id").and_then(Value::as_str) == Some(plant_id))
        {
            if let Some(alias) = plant_aliases(plant)
                .into_iter()
                .filter(|alias| contains_normalized_text(&text, alias))
                .max_by_key(|alias| alias.len())
            {
                upsert_ocr_correction_rule(
                    conn,
                    "plant_match",
                    request.supplier_id.as_deref().unwrap_or_default().trim(),
                    &normalize_for_match(&alias),
                    plant_id,
                )?;
            }
        }
    }

    Ok(())
}

fn read_backup_manifest(backup_path: &Path) -> Result<BackupManifest, String> {
    let manifest_path = backup_path.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Could not read backup manifest.json: {e}"))?;
    let manifest: BackupManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("Backup manifest.json is not valid JSON: {e}"))?;

    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(format!(
            "This backup format version ({}) is not supported by this app.",
            manifest.format_version
        ));
    }

    ensure_relative_safe_path(&manifest.database.relative_path)
        .map_err(|_| "The backup manifest contains an unsafe database path.".to_string())?;
    ensure_relative_safe_path(&manifest.documents.relative_dir)
        .map_err(|_| "The backup manifest contains an unsafe documents path.".to_string())?;

    let db_path = backup_path.join(&manifest.database.relative_path);
    if !db_path.is_file() {
        return Err("The selected backup is missing its SQLite database copy.".to_string());
    }
    if sha256_file(&db_path)? != manifest.database.sha256 {
        return Err("The selected backup database copy does not match its manifest.".to_string());
    }

    let docs_root = backup_path.join(&manifest.documents.relative_dir);
    if !docs_root.is_dir() {
        return Err("The selected backup is missing its documents folder.".to_string());
    }

    for file in &manifest.documents.files {
        ensure_relative_safe_path(&file.relative_path)
            .map_err(|_| "The backup manifest contains an unsafe document path.".to_string())?;
        let doc_path = backup_path.join(&file.relative_path);
        if !doc_path.is_file() {
            return Err(format!(
                "The selected backup is missing document file '{}'.",
                file.relative_path
            ));
        }
        if sha256_file(&doc_path)? != file.sha256 {
            return Err(format!(
                "Document file '{}' does not match the backup manifest.",
                file.relative_path
            ));
        }
    }

    Ok(manifest)
}

fn list_files_recursive(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(root)
        .map_err(|e| format!("Could not list folder '{}': {e}", root.display()))?
    {
        let entry = entry.map_err(|e| format!("Could not read folder entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(list_files_recursive(&path)?);
        } else if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        fs::create_dir_all(dest)
            .map_err(|e| format!("Could not create folder '{}': {e}", dest.display()))?;
        return Ok(());
    }

    fs::create_dir_all(dest)
        .map_err(|e| format!("Could not create folder '{}': {e}", dest.display()))?;

    for entry in
        fs::read_dir(src).map_err(|e| format!("Could not read folder '{}': {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("Could not read folder entry: {e}"))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if from.is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create folder '{}': {e}", parent.display()))?;
            }
            fs::copy(&from, &to).map_err(|e| {
                format!(
                    "Could not copy '{}' into the backup folder: {e}",
                    from.display()
                )
            })?;
        }
    }

    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|e| format!("Could not remove folder '{}': {e}", path.display()))?;
    } else if path.is_file() {
        fs::remove_file(path)
            .map_err(|e| format!("Could not remove file '{}': {e}", path.display()))?;
    }
    Ok(())
}

fn create_desktop_backup(
    paths: &AppPaths,
    live_counts: Value,
    label: &str,
) -> Result<BackupResult, String> {
    if !paths.db_path.exists() {
        return Err("SQLite database file was not found yet.".to_string());
    }

    let backup_root = paths.backups_dir.join(format!(
        "hiload-plant-maintenance-{}-{}",
        label,
        backup_name_timestamp()
    ));
    let database_dir = backup_root.join("database");
    let documents_dir = backup_root.join("documents");
    fs::create_dir_all(&database_dir)
        .map_err(|e| format!("Could not create backup folder: {e}"))?;
    fs::create_dir_all(&documents_dir)
        .map_err(|e| format!("Could not create backup documents folder: {e}"))?;

    let backup_db_path = database_dir.join("hiload-plant-maintenance.sqlite3");
    fs::copy(&paths.db_path, &backup_db_path)
        .map_err(|e| format!("Could not copy the SQLite database into the backup folder: {e}"))?;
    copy_dir_recursive(&paths.documents_dir, &documents_dir)?;

    let document_files = list_files_recursive(&documents_dir)?;
    let mut manifest_files = Vec::new();
    for path in document_files {
        let relative_from_backup = path
            .strip_prefix(&backup_root)
            .map_err(|_| "Could not prepare the backup manifest.".to_string())?;
        let metadata = fs::metadata(&path)
            .map_err(|e| format!("Could not read backup document metadata: {e}"))?;
        manifest_files.push(BackupFileManifest {
            relative_path: relative_path_string(relative_from_backup),
            size_bytes: metadata.len(),
            sha256: sha256_file(&path)?,
        });
    }

    let database_metadata = fs::metadata(&backup_db_path)
        .map_err(|e| format!("Could not read backup database metadata: {e}"))?;
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        created_at: iso_timestamp_string(),
        app_name: "Hiload Plant Maintenance".to_string(),
        database: BackupFileManifest {
            relative_path: "database/hiload-plant-maintenance.sqlite3".to_string(),
            size_bytes: database_metadata.len(),
            sha256: sha256_file(&backup_db_path)?,
        },
        documents: BackupDocumentsManifest {
            relative_dir: "documents".to_string(),
            managed_files_count: manifest_files.len(),
            files: manifest_files,
        },
        live_counts,
    };

    let manifest_path = backup_root.join("manifest.json");
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Could not write backup manifest: {e}"))?;
    fs::write(&manifest_path, manifest_text)
        .map_err(|e| format!("Could not write backup manifest.json: {e}"))?;

    Ok(BackupResult {
        backup_path: backup_root.to_string_lossy().to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        message: "Desktop backup created with the SQLite database, managed invoice documents, and a manifest."
            .to_string(),
    })
}

fn open_path_with_system_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Could not open the saved attachment: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Could not open the saved attachment: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Could not open the saved attachment: {e}"))?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = path;
        return Err(
            "Opening saved attachments is not supported on this operating system build."
                .to_string(),
        );
    }
    Ok(())
}

fn restore_desktop_backup_at_paths(
    paths: &AppPaths,
    selected_backup_path: &Path,
) -> Result<RestoreResult, String> {
    if !selected_backup_path.is_dir() {
        return Err(
            "Please choose a desktop backup folder that contains manifest.json.".to_string(),
        );
    }

    let manifest = read_backup_manifest(selected_backup_path)?;

    let staged_root = paths
        .backups_dir
        .join(format!("restore-staging-{}", backup_name_timestamp()));
    let staged_db = staged_root.join("hiload-plant-maintenance.sqlite3");
    let staged_documents = staged_root.join("documents");
    fs::create_dir_all(&staged_root)
        .map_err(|e| format!("Could not prepare the restore staging folder: {e}"))?;
    fs::copy(
        selected_backup_path.join(&manifest.database.relative_path),
        &staged_db,
    )
    .map_err(|e| format!("Could not stage the backup database copy: {e}"))?;
    copy_dir_recursive(
        &selected_backup_path.join(&manifest.documents.relative_dir),
        &staged_documents,
    )?;

    let conn = open_db(&paths.db_path)?;
    let (data, _) = load_live_state(&conn)?;
    let safety_backup = create_desktop_backup(paths, live_counts(&data), "pre-restore-safety")?;

    let rollback_root = paths
        .backups_dir
        .join(format!("restore-rollback-{}", backup_name_timestamp()));
    fs::create_dir_all(&rollback_root)
        .map_err(|e| format!("Could not prepare the rollback folder: {e}"))?;
    let rollback_db = rollback_root.join("hiload-plant-maintenance.sqlite3");
    let rollback_documents = rollback_root.join("documents");

    if paths.db_path.exists() {
        fs::rename(&paths.db_path, &rollback_db)
            .map_err(|e| format!("Could not secure the current database before restore: {e}"))?;
    }
    if paths.documents_dir.exists() {
        if let Err(err) = fs::rename(&paths.documents_dir, &rollback_documents) {
            if rollback_db.exists() {
                let _ = fs::rename(&rollback_db, &paths.db_path);
            }
            let _ = remove_path_if_exists(&rollback_root);
            let _ = remove_path_if_exists(&staged_root);
            return Err(format!(
                "Could not secure the current documents before restore: {err}"
            ));
        }
    }

    let restore_attempt = (|| -> Result<(), String> {
        fs::copy(&staged_db, &paths.db_path)
            .map_err(|e| format!("Could not restore the SQLite database copy: {e}"))?;
        copy_dir_recursive(&staged_documents, &paths.documents_dir)?;
        let conn = open_db(&paths.db_path)?;
        ensure_schema(&conn)?;
        Ok(())
    })();

    if let Err(err) = restore_attempt {
        let _ = remove_path_if_exists(&paths.db_path);
        let _ = remove_path_if_exists(&paths.documents_dir);
        if rollback_db.exists() {
            let _ = fs::rename(&rollback_db, &paths.db_path);
        }
        if rollback_documents.exists() {
            let _ = fs::rename(&rollback_documents, &paths.documents_dir);
        }
        let _ = remove_path_if_exists(&staged_root);
        let _ = remove_path_if_exists(&rollback_root);
        return Err(format!(
            "Restore stopped before replacing your data safely: {err}"
        ));
    }

    let _ = remove_path_if_exists(&rollback_root);
    let _ = remove_path_if_exists(&staged_root);

    Ok(RestoreResult {
        restored_from: selected_backup_path.to_string_lossy().to_string(),
        safety_backup_path: safety_backup.backup_path,
        message: "Desktop restore completed. The app will now reload the restored SQLite data and invoice documents.".to_string(),
    })
}

fn default_state_value(key: &str) -> Value {
    if STATE_ARRAY_KEYS.contains(&key) {
        json!([])
    } else {
        json!({})
    }
}

fn normalize_backup_data(data: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut normalized = Map::new();

    for key in STATE_KEYS {
        let next = match data.get(*key) {
            Some(Value::Null) | None => default_state_value(key),
            Some(value) if STATE_ARRAY_KEYS.contains(key) && value.is_array() => value.clone(),
            Some(value) if STATE_OBJECT_KEYS.contains(key) && value.is_object() => value.clone(),
            Some(_) if STATE_ARRAY_KEYS.contains(key) => {
                return Err(format!("Backup field '{key}' must be a list."));
            }
            Some(_) if STATE_OBJECT_KEYS.contains(key) => {
                return Err(format!("Backup field '{key}' must be an object."));
            }
            Some(value) => value.clone(),
        };
        normalized.insert((*key).to_string(), next);
    }

    Ok(normalized)
}

fn has_live_data(data: &Map<String, Value>) -> bool {
    data.values().any(|value| match value {
        Value::Array(items) => !items.is_empty(),
        Value::Object(items) => !items.is_empty(),
        Value::Null => false,
        _ => true,
    })
}

fn live_counts(data: &Map<String, Value>) -> Value {
    let mut counts = Map::new();
    for key in STATE_KEYS {
        let count = match data.get(*key) {
            Some(Value::Array(items)) => items.len() as i64,
            Some(Value::Object(items)) => items.len() as i64,
            Some(Value::Null) | None => 0,
            Some(_) => 1,
        };
        counts.insert((*key).to_string(), json!(count));
    }
    Value::Object(counts)
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

fn write_app_state(
    tx: &rusqlite::Transaction<'_>,
    data: &Map<String, Value>,
) -> Result<(), String> {
    tx.execute("DELETE FROM app_state", [])
        .map_err(|e| format!("Could not replace app state: {e}"))?;

    let updated_at = unix_timestamp_string();
    for key in STATE_KEYS {
        let value = data
            .get(*key)
            .cloned()
            .unwrap_or_else(|| default_state_value(key));
        tx.execute(
            "INSERT INTO app_state (state_key, raw_json, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value.to_string(), updated_at],
        )
        .map_err(|e| format!("Could not save app state '{key}': {e}"))?;
    }

    Ok(())
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
                .get("managedRelativePath")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(|_| 1)
                .or_else(|| {
                    item.get("dataUrl")
                        .and_then(Value::as_str)
                        .map(|s| i64::from(!s.trim().is_empty()))
                })
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

fn read_json_rows_as_array(conn: &Connection, table: &str) -> Result<Vec<Value>, String> {
    let sql = format!("SELECT raw_json FROM {table} ORDER BY rowid");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Could not read {table}: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("Could not query {table}: {e}"))?;

    let mut items = Vec::new();
    for row in rows {
        let raw = row.map_err(|e| format!("Could not read {table} row: {e}"))?;
        let parsed =
            serde_json::from_str(&raw).map_err(|e| format!("Could not parse {table} JSON: {e}"))?;
        items.push(parsed);
    }
    Ok(items)
}

fn read_json_rows_as_object(
    conn: &Connection,
    table: &str,
    key_column: &str,
) -> Result<Map<String, Value>, String> {
    let sql = format!("SELECT {key_column}, raw_json FROM {table}");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Could not read {table}: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Could not query {table}: {e}"))?;

    let mut items = Map::new();
    for row in rows {
        let (key, raw) = row.map_err(|e| format!("Could not read {table} row: {e}"))?;
        let parsed =
            serde_json::from_str(&raw).map_err(|e| format!("Could not parse {table} JSON: {e}"))?;
        items.insert(key, parsed);
    }
    Ok(items)
}

fn read_app_state(conn: &Connection) -> Result<Option<Map<String, Value>>, String> {
    let mut stmt = conn
        .prepare("SELECT state_key, raw_json FROM app_state")
        .map_err(|e| format!("Could not read app state: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Could not query app state: {e}"))?;

    let mut data = Map::new();
    for row in rows {
        let (key, raw) = row.map_err(|e| format!("Could not read app state row: {e}"))?;
        let parsed = serde_json::from_str(&raw)
            .map_err(|e| format!("Could not parse app state JSON for '{key}': {e}"))?;
        data.insert(key, parsed);
    }

    if data.is_empty() {
        return Ok(None);
    }

    normalize_backup_data(&data).map(Some)
}

fn read_legacy_state(conn: &Connection) -> Result<Map<String, Value>, String> {
    let mut data = Map::new();
    data.insert(
        "hiph_sites".to_string(),
        Value::Array(read_json_rows_as_array(conn, "sites")?),
    );
    data.insert(
        "hiph_plants".to_string(),
        Value::Array(read_json_rows_as_array(conn, "plants")?),
    );
    data.insert(
        "hiph_hires".to_string(),
        Value::Array(read_json_rows_as_array(conn, "hires")?),
    );
    data.insert(
        "hiph_maintenance".to_string(),
        Value::Array(read_json_rows_as_array(conn, "maintenance")?),
    );
    data.insert(
        "hiph_suppliers".to_string(),
        Value::Array(read_json_rows_as_array(conn, "suppliers")?),
    );
    data.insert(
        "hiph_invoices".to_string(),
        Value::Array(read_json_rows_as_array(conn, "invoices")?),
    );
    data.insert(
        "hiph_invoice_docs".to_string(),
        Value::Object(read_json_rows_as_object(
            conn,
            "invoice_documents",
            "invoice_id",
        )?),
    );
    data.insert(
        "hiph_meters".to_string(),
        Value::Object(read_json_rows_as_object(conn, "meters", "key")?),
    );
    data.insert(
        "hiph_plant_service".to_string(),
        Value::Object(read_json_rows_as_object(
            conn,
            "service_settings",
            "plant_id",
        )?),
    );
    data.insert(
        "hiph_site_usage".to_string(),
        Value::Object(read_json_rows_as_object(
            conn,
            "site_usage_totals",
            "site_id",
        )?),
    );
    data.insert("hiph_rate_models".to_string(), json!({}));
    data.insert("hiph_alerts".to_string(), json!({}));

    normalize_backup_data(&data)
}

fn load_live_state(conn: &Connection) -> Result<(Map<String, Value>, String), String> {
    if let Some(data) = read_app_state(conn)? {
        return Ok((data, "app_state".to_string()));
    }

    let legacy = read_legacy_state(conn)?;
    if has_live_data(&legacy) {
        return Ok((legacy, "legacy_tables".to_string()));
    }

    Ok((normalize_backup_data(&Map::new())?, "empty".to_string()))
}

fn replace_live_state(
    tx: &rusqlite::Transaction<'_>,
    data: &Map<String, Value>,
    source: &str,
) -> Result<Value, String> {
    let normalized = normalize_backup_data(data)?;
    clear_import_tables(tx)?;
    let counts = insert_backup_data(tx, &normalized)?;
    write_app_state(tx, &normalized)?;

    tx.execute(
        "INSERT OR REPLACE INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params!["last_live_state_source", source, unix_timestamp_string()],
    )
    .map_err(|e| format!("Could not update live state metadata: {e}"))?;

    Ok(counts)
}

#[tauri::command]
fn desktop_status(app: AppHandle) -> Result<DesktopStatus, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let conn = open_db(&paths.db_path)?;
    let (data, source) = load_live_state(&conn)?;

    Ok(DesktopStatus {
        is_desktop: true,
        database_path: paths.db_path.to_string_lossy().to_string(),
        database_exists: paths.db_path.exists(),
        documents_path: paths.documents_dir.to_string_lossy().to_string(),
        documents_exists: paths.documents_dir.exists(),
        backups_path: paths.backups_dir.to_string_lossy().to_string(),
        backups_exists: paths.backups_dir.exists(),
        has_live_data: has_live_data(&data),
        live_data_source: source,
        live_counts: live_counts(&data),
    })
}

#[tauri::command]
fn record_invoice_ocr_feedback(
    app: AppHandle,
    request: OcrCorrectionFeedbackRequest,
) -> Result<bool, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let conn = open_db(&paths.db_path)?;
    let context = load_analysis_context(&conn)?;
    record_invoice_ocr_feedback_inner(&conn, &context, &request)?;
    Ok(true)
}

#[tauri::command]
fn process_invoice_inbox_folder(
    app: AppHandle,
    request: BatchInvoiceProcessRequest,
) -> Result<BatchInvoiceProcessResult, String> {
    let folder_path = PathBuf::from(request.folder_path.trim());
    let paths = ensure_storage_and_schema(&app)?;
    let mut conn = open_db(&paths.db_path)?;
    process_invoice_inbox_folder_inner(&paths, &mut conn, &folder_path)
}

#[tauri::command]
fn load_live_state_from_sqlite(app: AppHandle) -> Result<LiveStateLoadResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let conn = open_db(&paths.db_path)?;
    let (data, source) = load_live_state(&conn)?;

    Ok(LiveStateLoadResult {
        is_empty: !has_live_data(&data),
        source,
        data: Value::Object(data),
    })
}

#[tauri::command]
fn save_live_state_to_sqlite(
    app: AppHandle,
    data: Value,
    source: Option<String>,
) -> Result<SaveResult, String> {
    let data = get_data_block(&data)?;
    let paths = ensure_storage_and_schema(&app)?;
    let mut conn = open_db(&paths.db_path)?;
    ensure_schema(&conn)?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Could not start save transaction: {e}"))?;
    let counts = replace_live_state(&tx, data, source.as_deref().unwrap_or("desktop_app"))?;
    let saved_at = unix_timestamp_string();

    tx.execute(
        "INSERT OR REPLACE INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params!["last_live_state_saved_at", saved_at, saved_at],
    )
    .map_err(|e| format!("Could not store save metadata: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Could not commit saved data: {e}"))?;

    Ok(SaveResult {
        message: "Saved live desktop data to SQLite.".to_string(),
        saved_at,
        counts,
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

    let counts = replace_live_state(&tx, data, "browser_backup_import")?;
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
fn import_invoice_document_bytes(
    app: AppHandle,
    invoice_id: String,
    file_name: String,
    mime_type: String,
    data_base64: String,
) -> Result<AttachmentImportResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.trim())
        .map_err(|e| format!("The selected attachment could not be read safely: {e}"))?;
    save_managed_invoice_document(&paths, &invoice_id, &file_name, &mime_type, &bytes)
}

#[tauri::command]
fn read_invoice_document(
    app: AppHandle,
    request: InvoiceReadRequest,
) -> Result<InvoiceReadResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let conn = open_db(&paths.db_path)?;
    let context = load_analysis_context(&conn)?;
    read_invoice_document_inner(&paths, &request, &context)
}

#[tauri::command]
fn open_invoice_document(
    app: AppHandle,
    relative_path: String,
) -> Result<AttachmentActionResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let full_path = resolve_managed_document_path(&paths, &relative_path)?;
    if !full_path.is_file() {
        return Err("The saved invoice attachment file could not be found. You can attach it again with Replace document.".to_string());
    }
    open_path_with_system_default(&full_path)?;
    Ok(AttachmentActionResult {
        file_deleted: false,
        full_path: full_path.to_string_lossy().to_string(),
        message: "Attachment opened in Windows.".to_string(),
    })
}

#[tauri::command]
fn remove_invoice_document(
    app: AppHandle,
    invoice_id: String,
    relative_path: String,
    data: Value,
) -> Result<AttachmentActionResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let data = get_data_block(&data)?;
    let full_path = resolve_managed_document_path(&paths, &relative_path)?;
    let shared_count = count_other_document_references(data, &relative_path, &invoice_id);

    if shared_count > 0 {
        return Ok(AttachmentActionResult {
            file_deleted: false,
            full_path: full_path.to_string_lossy().to_string(),
            message: "Attachment reference removed. The file was kept because another record still uses it.".to_string(),
        });
    }

    if full_path.exists() {
        fs::remove_file(&full_path)
            .map_err(|e| format!("The attachment file could not be removed safely: {e}"))?;
    }

    Ok(AttachmentActionResult {
        file_deleted: true,
        full_path: full_path.to_string_lossy().to_string(),
        message: "Attachment removed from the desktop documents folder.".to_string(),
    })
}

#[tauri::command]
fn export_sqlite_backup(app: AppHandle) -> Result<BackupResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let conn = open_db(&paths.db_path)?;
    let (data, _) = load_live_state(&conn)?;
    create_desktop_backup(&paths, live_counts(&data), "backup")
}

#[tauri::command]
fn restore_desktop_backup(app: AppHandle, backup_path: String) -> Result<RestoreResult, String> {
    let paths = ensure_storage_and_schema(&app)?;
    let selected_backup_path = PathBuf::from(backup_path.trim());
    restore_desktop_backup_at_paths(&paths, &selected_backup_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            desktop_status,
            record_invoice_ocr_feedback,
            process_invoice_inbox_folder,
            load_live_state_from_sqlite,
            save_live_state_to_sqlite,
            import_browser_backup_into_sqlite,
            import_invoice_document_bytes,
            read_invoice_document,
            open_invoice_document,
            remove_invoice_document,
            export_sqlite_backup,
            restore_desktop_backup
        ])
        .setup(|app| {
            ensure_storage_and_schema(app.handle())
                .map(|_| ())
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample_state() -> Map<String, Value> {
        let mut data = Map::new();
        data.insert(
            "hiph_sites".to_string(),
            json!([{ "id": "s_1", "name": "Main Site", "location": "Yard" }]),
        );
        data.insert(
            "hiph_plants".to_string(),
            json!([{ "id": "p_1", "name": "Excavator", "category": "yellow" }]),
        );
        data.insert(
            "hiph_hires".to_string(),
            json!([{ "id": "h_1", "siteId": "s_1", "plantId": "p_1", "startDate": "2026-09-01" }]),
        );
        data.insert(
            "hiph_maintenance".to_string(),
            json!([{ "id": "m_1", "plantId": "p_1", "supplierId": "sup_1", "invoiceId": "inv_1", "date": "2026-09-02", "type": "service", "cost": 2500.0, "meterAtService": 0 }]),
        );
        data.insert(
            "hiph_site_usage".to_string(),
            json!({ "s_1": { "plants": { "p_1": 4 }, "history": [] } }),
        );
        data.insert(
            "hiph_meters".to_string(),
            json!({ "p_1": { "2026-09": 1450 } }),
        );
        data.insert(
            "hiph_plant_service".to_string(),
            json!({ "p_1": { "reminders": [], "licenseRenewal": "" } }),
        );
        data.insert(
            "hiph_rate_models".to_string(),
            json!({ "p_1": { "unit": "day", "rate": "500.00" } }),
        );
        data.insert("hiph_alerts".to_string(), json!({ "overdue": [] }));
        data.insert(
            "hiph_suppliers".to_string(),
            json!([{ "id": "sup_1", "name": "ServiceCo" }]),
        );
        data.insert(
            "hiph_invoices".to_string(),
            json!([{ "id": "inv_1", "supplierId": "sup_1", "plantId": "p_1", "invoiceNumber": "1001", "invoiceDate": "2026-09-02", "totalCost": 2500.0, "status": "posted", "hasDocument": true }]),
        );
        data.insert(
            "hiph_invoice_docs".to_string(),
            json!({ "inv_1": { "fileName": "invoice.pdf", "mimeType": "application/pdf", "size": 12, "dataUrl": "data:application/pdf;base64,AAAA" } }),
        );
        data
    }

    fn sample_state_with_managed_document(relative_path: &str) -> Map<String, Value> {
        let mut data = sample_state();
        data.insert(
            "hiph_invoice_docs".to_string(),
            json!({
                "inv_1": {
                    "fileName": "invoice.pdf",
                    "mimeType": "application/pdf",
                    "size": 12,
                    "managedRelativePath": relative_path,
                    "storageKind": "desktop_managed_file"
                }
            }),
        );
        data
    }

    fn temp_app_paths(label: &str) -> AppPaths {
        let root = std::env::temp_dir().join(format!(
            "hiload-plant-maintenance-tests-{}-{}",
            label,
            unix_timestamp_string()
        ));
        let app_data_dir = root.join("app-data");
        let db_path = app_data_dir.join("hiload-plant-maintenance.sqlite3");
        let documents_dir = app_data_dir.join("documents");
        let backups_dir = app_data_dir.join("backups");
        fs::create_dir_all(managed_invoice_documents_dir(&AppPaths {
            app_data_dir: app_data_dir.clone(),
            db_path: db_path.clone(),
            documents_dir: documents_dir.clone(),
            backups_dir: backups_dir.clone(),
        }))
        .expect("create managed docs dir");
        fs::create_dir_all(&backups_dir).expect("create backups dir");
        AppPaths {
            app_data_dir,
            db_path,
            documents_dir,
            backups_dir,
        }
    }

    #[test]
    fn normalize_backup_data_adds_missing_defaults() {
        let data = Map::from_iter([(
            "hiph_sites".to_string(),
            json!([{ "id": "s_1", "name": "Main Site" }]),
        )]);

        let normalized = normalize_backup_data(&data).expect("state should normalize");

        assert_eq!(normalized["hiph_sites"][0]["id"], "s_1");
        assert_eq!(normalized["hiph_plants"], json!([]));
        assert_eq!(normalized["hiph_alerts"], json!({}));
    }

    #[test]
    fn replace_live_state_round_trips_full_state() {
        let mut conn = Connection::open_in_memory().expect("memory db");
        ensure_schema(&conn).expect("schema");
        let data = sample_state();

        let tx = conn.transaction().expect("transaction");
        let counts = replace_live_state(&tx, &data, "test_save").expect("save state");
        tx.commit().expect("commit");

        assert_eq!(counts["plants"], json!(1));
        assert_eq!(counts["invoiceDocuments"], json!(1));

        let (loaded, source) = load_live_state(&conn).expect("load state");
        assert_eq!(source, "app_state");
        assert_eq!(loaded.get("hiph_rate_models"), data.get("hiph_rate_models"));
        assert_eq!(loaded["hiph_maintenance"][0]["invoiceId"], json!("inv_1"));
        assert_eq!(
            loaded["hiph_invoice_docs"]["inv_1"]["fileName"],
            json!("invoice.pdf")
        );
    }

    #[test]
    fn load_live_state_falls_back_to_legacy_tables() {
        let mut conn = Connection::open_in_memory().expect("memory db");
        ensure_schema(&conn).expect("schema");
        let data = sample_state();

        let tx = conn.transaction().expect("transaction");
        clear_import_tables(&tx).expect("clear tables");
        insert_backup_data(&tx, &data).expect("insert legacy data");
        tx.commit().expect("commit");

        let (loaded, source) = load_live_state(&conn).expect("load state");
        assert_eq!(source, "legacy_tables");
        assert_eq!(loaded["hiph_sites"][0]["name"], json!("Main Site"));
        assert_eq!(loaded["hiph_alerts"], json!({}));
        assert_eq!(
            loaded["hiph_invoice_docs"]["inv_1"]["mimeType"],
            json!("application/pdf")
        );
    }

    #[test]
    fn managed_document_paths_are_sanitized_and_confined() {
        let paths = temp_app_paths("sanitize");
        let saved = save_managed_invoice_document(
            &paths,
            "inv/../1",
            "..\\bad invoice name?.pdf",
            "application/pdf",
            b"pdf-bytes",
        )
        .expect("save managed document");

        assert!(saved.relative_path.starts_with("invoices/"));
        assert!(saved.relative_path.contains("inv"));
        assert!(saved.relative_path.contains("invoice-name"));
        assert!(!saved.relative_path.contains(".."));
        assert!(resolve_managed_document_path(&paths, "../escape").is_err());
        assert!(Path::new(&saved.full_path).starts_with(&paths.documents_dir));
    }

    #[test]
    fn attachment_reference_count_protects_shared_files() {
        let shared = "invoices/inv_1--abc--invoice.pdf";
        let data = Map::from_iter([(
            "hiph_invoice_docs".to_string(),
            json!({
                "inv_1": { "managedRelativePath": shared },
                "inv_2": { "managedRelativePath": shared },
                "inv_3": { "managedRelativePath": "invoices/other.pdf" }
            }),
        )]);

        assert_eq!(count_other_document_references(&data, shared, "inv_1"), 1);
        assert_eq!(count_other_document_references(&data, shared, "inv_2"), 1);
        assert_eq!(count_other_document_references(&data, shared, "inv_9"), 2);
    }

    #[test]
    fn backup_manifest_validates_backup_contents() {
        let paths = temp_app_paths("backup");
        let saved = save_managed_invoice_document(
            &paths,
            "inv_1",
            "invoice.pdf",
            "application/pdf",
            b"invoice-bytes",
        )
        .expect("save managed document");

        let data = sample_state_with_managed_document(&saved.relative_path);
        let mut conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        let tx = conn.transaction().expect("tx");
        replace_live_state(&tx, &data, "test").expect("replace live state");
        tx.commit().expect("commit");

        let backup =
            create_desktop_backup(&paths, live_counts(&data), "backup-test").expect("backup");
        let manifest = read_backup_manifest(Path::new(&backup.backup_path)).expect("manifest");

        assert_eq!(manifest.format_version, BACKUP_FORMAT_VERSION);
        assert_eq!(manifest.documents.managed_files_count, 1);
        assert_eq!(manifest.live_counts["hiph_invoices"], json!(1));
    }

    #[test]
    fn backup_manifest_rejects_unsafe_paths() {
        let paths = temp_app_paths("unsafe-manifest");
        let backup_root = paths.backups_dir.join("unsafe-backup");
        fs::create_dir_all(&backup_root).expect("backup root");
        fs::write(
            backup_root.join("manifest.json"),
            r#"{
          "formatVersion": 1,
          "createdAt": "2026-09-10T00:00:00Z",
          "appName": "Hiload Plant Maintenance",
          "database": { "relativePath": "../db.sqlite3", "sizeBytes": 1, "sha256": "abc" },
          "documents": { "relativeDir": "documents", "managedFilesCount": 0, "files": [] },
          "liveCounts": {}
        }"#,
        )
        .expect("write manifest");

        let err = read_backup_manifest(&backup_root).expect_err("unsafe path should fail");
        assert!(err.contains("unsafe database path"));
    }

    #[test]
    fn restore_round_trip_recovers_prior_database_and_documents() {
        let paths = temp_app_paths("restore");
        let original_doc = save_managed_invoice_document(
            &paths,
            "inv_1",
            "invoice.pdf",
            "application/pdf",
            b"original-bytes",
        )
        .expect("save original");
        let original_state = sample_state_with_managed_document(&original_doc.relative_path);
        let mut conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        let tx = conn.transaction().expect("tx");
        replace_live_state(&tx, &original_state, "original").expect("save original state");
        tx.commit().expect("commit original");

        let backup = create_desktop_backup(&paths, live_counts(&original_state), "restore-test")
            .expect("backup");

        let replacement_doc = save_managed_invoice_document(
            &paths,
            "inv_1",
            "replacement.pdf",
            "application/pdf",
            b"replacement-bytes",
        )
        .expect("save replacement");
        let replacement_state = sample_state_with_managed_document(&replacement_doc.relative_path);
        let mut conn = open_db(&paths.db_path).expect("db reopened");
        ensure_schema(&conn).expect("schema reopened");
        let tx = conn.transaction().expect("tx reopened");
        replace_live_state(&tx, &replacement_state, "replacement").expect("save replacement state");
        tx.commit().expect("commit replacement");

        let result = restore_desktop_backup_at_paths(&paths, Path::new(&backup.backup_path))
            .expect("restore backup");
        let conn = open_db(&paths.db_path).expect("open restored db");
        let (loaded, source) = load_live_state(&conn).expect("load restored state");

        assert_eq!(source, "app_state");
        assert_eq!(
            loaded["hiph_invoice_docs"]["inv_1"]["managedRelativePath"],
            json!(original_doc.relative_path)
        );
        assert!(
            resolve_managed_document_path(&paths, &original_doc.relative_path)
                .expect("restored path")
                .exists()
        );
        assert!(Path::new(&result.safety_backup_path).exists());
    }

    #[test]
    fn invoice_read_kind_accepts_required_formats() {
        assert!(matches!(
            detect_invoice_read_kind("invoice.pdf", "application/pdf"),
            Ok(InvoiceReadKind::Pdf)
        ));
        assert!(matches!(
            detect_invoice_read_kind("invoice.png", ""),
            Ok(InvoiceReadKind::Png)
        ));
        assert!(matches!(
            detect_invoice_read_kind("invoice.jpeg", ""),
            Ok(InvoiceReadKind::Jpeg)
        ));
        assert!(detect_invoice_read_kind("invoice.tif", "image/tiff").is_err());
    }

    #[test]
    fn invoice_analysis_extracts_common_fields() {
        let context = InvoiceAnalysisContext::default();
        let text = "\
ACME Mining Supplies
Invoice Number: INV-2026-0099
Invoice Date: 10/09/2026
Subtotal: 1,250.00
VAT: 187.50
Total: 1,437.50
Wheel bearing service for CAT loader";

        let analysis = build_invoice_analysis(text, &context);
        let suggestions = analysis.suggestions;

        assert_eq!(suggestions[0].field, "supplierName");
        assert_eq!(suggestions[1].field, "invoiceNumber");
        assert_eq!(suggestions[1].value, "INV-2026-0099");
        assert_eq!(suggestions[2].field, "invoiceDate");
        assert_eq!(suggestions[2].value, "2026-09-10");
        assert_eq!(suggestions[0].evidence, "ACME Mining Supplies");
        assert!(suggestions
            .iter()
            .any(|item| item.field == "netAmount" && item.value == "1250.00"));
        assert!(suggestions
            .iter()
            .any(|item| item.field == "vatAmount" && item.value == "187.50"));
        assert!(suggestions
            .iter()
            .any(|item| item.field == "totalCost" && item.value == "1437.50"));
    }

    #[test]
    fn supplier_detection_excludes_hiload_and_due_date() {
        let mut data = sample_state();
        data.insert(
            "hiph_suppliers".to_string(),
            json!([{ "id": "sup_1", "name": "ACME Mining Supplies" }]),
        );
        let context = analysis_context_from_data(&data, Vec::new());
        let text = "\
Tax Invoice
Hiload Inyanga Construction
Invoice Number: INV-2026-0099
Due Date: 20/09/2026
Supplier: ACME Mining Supplies
Invoice Date: 10/09/2026
Amount Due: 1,437.50";

        let analysis = build_invoice_analysis(text, &context);
        assert_eq!(
            analysis.supplier_candidate.as_deref(),
            Some("ACME Mining Supplies")
        );
        assert_eq!(analysis.matched_supplier_id.as_deref(), Some("sup_1"));
        assert_eq!(analysis.invoice_date.as_deref(), Some("2026-09-10"));
        assert!(!analysis
            .suggestions
            .iter()
            .any(|item| item.value.contains("2026-09-20")));
    }

    #[test]
    fn total_detection_prefers_final_total_and_flags_mismatch() {
        let context = InvoiceAnalysisContext::default();
        let text = "\
Subtotal: 100.00
VAT: 15.00
Invoice Total: 160.00
Notes: repair work";
        let analysis = build_invoice_analysis(text, &context);
        let total = analysis
            .suggestions
            .iter()
            .find(|item| item.field == "totalCost")
            .expect("total suggestion");
        assert_eq!(total.value, "160.00");
        let warnings = analysis.warnings;
        assert!(warnings.iter().any(|w| w.contains("does not match")));
    }

    #[test]
    fn invoice_number_requires_invoice_label_context() {
        let context = InvoiceAnalysisContext::default();
        let text = "\
ACME Mining Supplies
Account Number: 556677
Reference: 998877
Invoice Date: 10/09/2026";
        let analysis = build_invoice_analysis(text, &context);
        assert!(analysis.invoice_number.is_none());
    }

    #[test]
    fn plant_matching_does_not_guess_when_multiple_aliases_conflict() {
        let context = InvoiceAnalysisContext {
            suppliers: Vec::new(),
            plants: vec![
                json!({ "id": "p_1", "name": "JCB Handler", "make": "JCB", "model": "540-140", "serial": "ABC123" }),
                json!({ "id": "p_2", "name": "JCB Backhoe", "make": "JCB", "model": "3CX", "serial": "XYZ999" }),
            ],
            rules: Vec::new(),
        };
        let text = "Service parts supplied for JCB machine";
        let analysis = build_invoice_analysis(text, &context);
        assert!(analysis.matched_plant_id.is_none());
    }

    #[test]
    fn supplier_history_is_only_a_suggestion_not_an_override() {
        let data = Map::from_iter([
            (
                "hiph_suppliers".to_string(),
                json!([
                    { "id": "sup_old", "name": "Old Supplier" },
                    { "id": "sup_new", "name": "Current Supplier" }
                ]),
            ),
            ("hiph_plants".to_string(), json!([])),
            ("hiph_sites".to_string(), json!([])),
            ("hiph_hires".to_string(), json!([])),
            ("hiph_maintenance".to_string(), json!([])),
            ("hiph_site_usage".to_string(), json!({})),
            ("hiph_meters".to_string(), json!({})),
            ("hiph_plant_service".to_string(), json!({})),
            ("hiph_rate_models".to_string(), json!({})),
            ("hiph_alerts".to_string(), json!({})),
            ("hiph_invoices".to_string(), json!([])),
            ("hiph_invoice_docs".to_string(), json!({})),
        ]);
        let context = analysis_context_from_data(
            &data,
            vec![OcrCorrectionRule {
                rule_type: "supplier_match".to_string(),
                supplier_key: normalize_for_match("Current Supplier"),
                hint_key: String::new(),
                target_value: "sup_old".to_string(),
                usage_count: 4,
            }],
        );
        let analysis = build_invoice_analysis("Supplier: Current Supplier", &context);
        assert_eq!(analysis.matched_supplier_id.as_deref(), Some("sup_new"));
    }

    #[test]
    fn invoice_read_rejects_oversized_files_and_keeps_state_keys_unchanged() {
        let paths = temp_app_paths("ocr-size-limit");
        let context = InvoiceAnalysisContext::default();
        let request = InvoiceReadRequest {
            relative_path: None,
            file_name: Some("invoice.pdf".to_string()),
            mime_type: Some("application/pdf".to_string()),
            data_base64: Some(base64::engine::general_purpose::STANDARD.encode(vec![
                b'a';
                MAX_INVOICE_READ_BYTES
                    + 1
            ])),
        };

        let err = read_invoice_document_inner(&paths, &request, &context)
            .expect_err("should reject big file");
        assert!(err.contains("too large"));
        assert!(!STATE_KEYS
            .iter()
            .any(|key| key.contains("api") || key.contains("local_ai")));
    }

    #[test]
    fn batch_processing_detects_duplicates_by_checksum() {
        let paths = temp_app_paths("batch-dup");
        let mut conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        let saved = save_managed_invoice_document(
            &paths,
            "inv_1",
            "invoice.pdf",
            "application/pdf",
            b"same-file",
        )
        .expect("save managed document");
        let mut data = sample_state_with_managed_document(&saved.relative_path);
        data["hiph_invoice_docs"]["inv_1"]["sha256"] = json!(sha256_hex(b"same-file"));
        persist_live_state(&mut conn, &data, "test").expect("persist");

        let source_dir = paths.app_data_dir.join("source");
        fs::create_dir_all(&source_dir).expect("source dir");
        let source_path = source_dir.join("invoice-copy.pdf");
        fs::write(&source_path, b"same-file").expect("source file");

        let entry = process_batch_invoice_file(
            &mut conn,
            &paths,
            &mut data,
            &[],
            &source_path,
            "invoice-copy.pdf",
            "application/pdf",
            b"same-file",
            "Invoice Number: INV-1",
            "local_pdf_text",
        )
        .expect("entry");
        assert_eq!(entry.outcome, "possible duplicate");
        assert!(source_path.exists());
    }

    #[test]
    fn batch_processing_saves_document_before_removing_source() {
        let paths = temp_app_paths("batch-save");
        let mut conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        let mut data = sample_state();
        data.insert(
            "hiph_suppliers".to_string(),
            json!([{ "id": "sup_2", "name": "ACME Mining Supplies" }]),
        );
        data.insert(
            "hiph_plants".to_string(),
            json!([{ "id": "p_2", "name": "JCB 540-140", "make": "JCB", "model": "540-140", "serial": "REG123", "category": "yellow" }]),
        );
        persist_live_state(&mut conn, &data, "test").expect("persist");

        let source_dir = paths.app_data_dir.join("source");
        fs::create_dir_all(&source_dir).expect("source dir");
        let source_path = source_dir.join("invoice-a.pdf");
        fs::write(&source_path, b"new-file").expect("source file");

        let entry = process_batch_invoice_file(
            &mut conn,
            &paths,
            &mut data,
            &[],
            &source_path,
            "invoice-a.pdf",
            "application/pdf",
            b"new-file",
            "Supplier: ACME Mining Supplies\nInvoice Number: INV-22\nInvoice Date: 10/09/2026\nAmount Due: 100.00\nJCB 540-140",
            "local_pdf_text",
        )
        .expect("entry");

        assert!(entry.outcome == "processed" || entry.outcome == "needs review");
        assert!(!source_path.exists());
        let docs = value_as_object(&data, "hiph_invoice_docs").expect("docs");
        let managed = docs
            .get(&entry.invoice_id)
            .and_then(|item| item.get("managedRelativePath"))
            .and_then(Value::as_str)
            .expect("managed path");
        assert!(resolve_managed_document_path(&paths, managed)
            .expect("resolved path")
            .exists());
    }

    #[test]
    fn batch_save_failure_leaves_source_file_untouched() {
        let paths = temp_app_paths("batch-fail");
        let mut conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        let mut bad_data = sample_state();
        bad_data.insert("hiph_invoices".to_string(), json!({ "broken": true }));

        let source_dir = paths.app_data_dir.join("source");
        fs::create_dir_all(&source_dir).expect("source dir");
        let source_path = source_dir.join("invoice-b.pdf");
        fs::write(&source_path, b"new-file").expect("source file");

        let err = process_batch_invoice_file(
            &mut conn,
            &paths,
            &mut bad_data,
            &[],
            &source_path,
            "invoice-b.pdf",
            "application/pdf",
            b"new-file",
            "Invoice Number: INV-22\nInvoice Date: 10/09/2026\nAmount Due: 100.00",
            "local_pdf_text",
        )
        .expect_err("save should fail");
        assert!(err.contains("must be a list"));
        assert!(source_path.exists());
    }

    #[test]
    fn correction_rules_round_trip_through_backup_restore() {
        let paths = temp_app_paths("rules-backup");
        let conn = open_db(&paths.db_path).expect("db");
        ensure_schema(&conn).expect("schema");
        upsert_ocr_correction_rule(
            &conn,
            "supplier_match",
            &normalize_for_match("ACME Mining Supplies"),
            "",
            "sup_1",
        )
        .expect("rule");
        let data = sample_state();
        let backup =
            create_desktop_backup(&paths, live_counts(&data), "rules-backup").expect("backup");

        conn.execute("DELETE FROM invoice_ocr_correction_rules", [])
            .expect("clear rules");
        restore_desktop_backup_at_paths(&paths, Path::new(&backup.backup_path)).expect("restore");
        let restored = open_db(&paths.db_path).expect("restored db");
        let rules = load_ocr_correction_rules(&restored).expect("rules");
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn removed_local_ai_settings_are_cleared() {
        let conn = Connection::open_in_memory().expect("memory db");
        ensure_schema(&conn).expect("schema");
        conn.execute(
            "INSERT OR REPLACE INTO app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![
                "local_ai_settings_json",
                "{\"enabled\":true}",
                unix_timestamp_string()
            ],
        )
        .expect("seed local ai setting");
        clear_removed_local_ai_metadata(&conn).expect("cleanup");
        let value = read_app_meta_value(&conn, "local_ai_settings_json").expect("read");
        assert!(value.is_none());
    }
}
