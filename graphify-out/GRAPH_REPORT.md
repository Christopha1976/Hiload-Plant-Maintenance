# Graph Report - hiload-plant-maintenance  (2026-11-17)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 249 nodes · 690 edges · 17 communities (13 shown, 4 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 10 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `eaf9e295`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Backup & desktop status
- Rust crate imports
- Data storage & duplicates
- Invoice file import & paths
- Tauri app config
- npm package config
- OCR corrections & plant matching
- Invoice field parsing
- Invoice inbox batch processing
- Invoice amount analysis
- Invoice document reading
- Tauri permissions
- Backup manifests
- Cargo crate

## God Nodes (most connected - your core abstractions)
1. `process_batch_invoice_file()` - 22 edges
2. `ensure_storage_and_schema()` - 21 edges
3. `build_invoice_analysis()` - 21 edges
4. `open_db()` - 18 edges
5. `AppPaths` - 17 edges
6. `load_live_state()` - 17 edges
7. `create_desktop_backup()` - 15 edges
8. `ensure_schema()` - 15 edges
9. `restore_desktop_backup_at_paths()` - 15 edges
10. `replace_live_state()` - 14 edges

## Surprising Connections (you probably didn't know these)
- `create_desktop_backup()` --calls--> `backup_name_timestamp()`  [INFERRED]
  src-tauri/src/lib.rs → src-tauri/src/lib.rs  _Bridges community 0 → community 3_
- `attachment_reference_count_protects_shared_files()` --calls--> `count_other_document_references()`  [INFERRED]
  src-tauri/src/lib.rs → src-tauri/src/lib.rs  _Bridges community 1 → community 2_
- `batch_processing_saves_document_before_removing_source()` --calls--> `resolve_managed_document_path()`  [INFERRED]
  src-tauri/src/lib.rs → src-tauri/src/lib.rs  _Bridges community 2 → community 3_
- `supplier_history_is_only_a_suggestion_not_an_override()` --calls--> `normalize_for_match()`  [INFERRED]
  src-tauri/src/lib.rs → src-tauri/src/lib.rs  _Bridges community 6 → community 9_
- `DesktopStatus` --references--> `Value`  [EXTRACTED]
  src-tauri/src/lib.rs →   _Bridges community 0 → community 2_

## Import Cycles
- None detected.

## Communities (17 total, 4 thin omitted)

### Community 0 - "Backup & desktop status"
Cohesion: 0.11
Nodes (39): AttachmentActionResult, BackupResult, batch_save_failure_leaves_source_file_untouched(), BatchInvoiceProcessRequest, copy_dir_recursive(), correction_rules_round_trip_through_backup_restore(), create_desktop_backup(), desktop_status() (+31 more)

### Community 1 - "Rust crate imports"
Cohesion: 0.06
Nodes (15): attachment_reference_count_protects_shared_files(), BACKUP_FORMAT_VERSION, clear_removed_local_ai_metadata(), MANAGED_INVOICE_DIR, MAX_BATCH_INVOICE_FILES, MAX_INVOICE_EXTRACT_TEXT_CHARS, MAX_INVOICE_READ_BYTES, normalize_backup_data_adds_missing_defaults() (+7 more)

### Community 2 - "Data storage & duplicates"
Cohesion: 0.17
Nodes (31): analysis_context_from_data(), batch_processing_detects_duplicates_by_checksum(), batch_processing_saves_document_before_removing_source(), clear_import_tables(), count_other_document_references(), default_state_value(), find_duplicate_by_checksum(), find_duplicate_by_supplier_invoice() (+23 more)

### Community 3 - "Invoice file import & paths"
Cohesion: 0.15
Nodes (25): AppPaths, AttachmentImportResult, backup_manifest_rejects_unsafe_paths(), backup_manifest_validates_backup_contents(), backup_name_timestamp(), ensure_relative_safe_path(), extension_from_mime_type(), extract_image_text() (+17 more)

### Community 4 - "Tauri app config"
Cohesion: 0.09
Nodes (21): app, security, windows, withGlobalTauri, build, beforeBuildCommand, beforeDevCommand, devUrl (+13 more)

### Community 5 - "npm package config"
Cohesion: 0.14
Nodes (15): devDependencies, http-server@^14.1.1, @tauri-apps/cli@^2, name, private, scripts, build:frontend, dev (+7 more)

### Community 6 - "OCR corrections & plant matching"
Cohesion: 0.23
Nodes (13): apply_supplier_history_rule(), choose_plant_match(), contains_normalized_text(), find_supplier_id_by_name(), InvoiceAnalysisContext, load_ocr_correction_rules(), normalize_for_match(), OcrCorrectionFeedbackRequest (+5 more)

### Community 7 - "Invoice field parsing"
Cohesion: 0.27
Nodes (13): candidate_segments(), choose_invoice_date(), choose_invoice_number(), choose_notes(), choose_supplier_candidate(), collect_labeled_line_matches(), find_first_date(), is_hiload_identity() (+5 more)

### Community 8 - "Invoice inbox batch processing"
Cohesion: 0.20
Nodes (12): BatchInvoiceProcessEntry, BatchInvoiceProcessResult, detect_invoice_read_kind(), invoice_read_kind_accepts_required_formats(), InvoiceReadKind, Jpeg, Pdf, Png (+4 more)

### Community 9 - "Invoice amount analysis"
Cohesion: 0.22
Nodes (10): AmountMatch, build_invoice_analysis(), choose_best_amount(), collect_amount_matches(), invoice_analysis_extracts_common_fields(), invoice_number_requires_invoice_label_context(), line_has_label(), plant_matching_does_not_guess_when_multiple_aliases_conflict() (+2 more)

### Community 10 - "Invoice document reading"
Cohesion: 0.22
Nodes (10): base_name_or_fallback(), invoice_read_rejects_oversized_files_and_keeps_state_keys_unchanged(), InvoiceAnalysis, InvoiceFieldSuggestion, InvoiceReadRequest, InvoiceReadResult, load_invoice_read_bytes(), normalize_invoice_text() (+2 more)

### Community 11 - "Tauri permissions"
Cohesion: 0.33
Nodes (5): description, identifier, permissions, $schema, windows

### Community 12 - "Backup manifests"
Cohesion: 1.00
Nodes (3): BackupDocumentsManifest, BackupFileManifest, BackupManifest

## Knowledge Gaps
- **4 isolated node(s):** `hiload_plant_maintenance`, `Jpeg`, `Pdf`, `Png`
  These have ≤1 connection - possible missing edges. (Counts symbols only; 68 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `InvoiceReadKind` connect `Invoice inbox batch processing` to `Rust crate imports`?**
  _High betweenness centrality (0.019) - this node is a cross-community bridge._
- **What connects `hiload_plant_maintenance`, `Jpeg`, `Pdf` to the rest of the system?**
  _4 weakly-connected nodes found - possible missing edges._
- **Should `Backup & desktop status` be split into smaller, more focused modules?**
  _Cohesion score 0.1076923076923077 - nodes in this community are weakly interconnected._
- **Why does `build_invoice_analysis()` connect `Invoice amount analysis` to `Rust crate imports`, `Data storage & duplicates`, `OCR corrections & plant matching`, `Invoice field parsing`, `Invoice document reading`?**
  _High betweenness centrality (0.003) - this node is a cross-community bridge._
- **Should `Rust crate imports` be split into smaller, more focused modules?**
  _Cohesion score 0.057057057057057055 - nodes in this community are weakly interconnected._
- **Should `Tauri app config` be split into smaller, more focused modules?**
  _Cohesion score 0.09090909090909091 - nodes in this community are weakly interconnected._
- **Should `npm package config` be split into smaller, more focused modules?**
  _Cohesion score 0.14166666666666666 - nodes in this community are weakly interconnected._