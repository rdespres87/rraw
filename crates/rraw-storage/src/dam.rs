#![forbid(unsafe_code)]

//! SQLite DAM metadata indexing, session caching, and multi-criteria querying.

use crate::error::StorageError;
use crate::xmp::XmpEditor;
use crate::SidecarPaths;
use rraw_core::{ColorLabel, PhotoMetadata, PickStatus};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

/// Result of an incremental synchronization operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    /// The record was newly inserted or updated.
    Updated,
    /// The record was already up-to-date with disk files; write skipped.
    Skipped,
}

/// Multi-criteria search filter for querying indexed photos.
#[derive(Debug, Clone, Default)]
pub struct DamFilter {
    pub min_rating: Option<u8>,
    pub color_label: Option<ColorLabel>,
    pub pick_status: Option<PickStatus>,
    pub camera_model: Option<String>,
    pub keyword: Option<String>,
}

/// Lightweight record representing an indexed photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamPhotoRecord {
    pub raw_path: String,
    pub camera_make: String,
    pub camera_model: String,
    pub rating: u8,
    pub label: ColorLabel,
    pub pick: PickStatus,
}

/// Thread-safe SQLite index for catalog-free RAW photos and directory sessions.
#[derive(Clone)]
pub struct SqliteDamIndex {
    conn: Arc<Mutex<Connection>>,
    db_path: Option<PathBuf>,
}

impl SqliteDamIndex {
    /// Returns the canonical path to `.rraw/cache.db` within `folder`.
    #[must_use]
    pub fn session_db_path<P: AsRef<Path>>(folder: P) -> PathBuf {
        folder.as_ref().join(".rraw").join("cache.db")
    }

    /// Opens or creates the session database inside `folder/.rraw/cache.db` with WAL pragmas enabled.
    pub fn open_session_database<P: AsRef<Path>>(folder: P) -> Result<Self, StorageError> {
        let folder = folder.as_ref();
        let rraw_dir = folder.join(".rraw");
        std::fs::create_dir_all(&rraw_dir).map_err(|e| {
            StorageError::Database(format!("Failed to create .rraw directory: {e}"))
        })?;
        let db_path = rraw_dir.join("cache.db");
        Self::open_internal(Some(db_path))
    }

    /// Opens an in-memory SQLite DAM database for testing.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::open_internal(None)
    }

    /// Opens or creates an SQLite DAM database at `db_path` with WAL pragmas enabled.
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self, StorageError> {
        let p = db_path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    StorageError::Database(format!("Failed to create DB directory: {e}"))
                })?;
            }
        }
        Self::open_internal(Some(p))
    }

    fn open_internal(db_path: Option<PathBuf>) -> Result<Self, StorageError> {
        let conn = match &db_path {
            Some(path) => Connection::open(path)
                .map_err(|e| StorageError::Database(format!("Failed to open DB: {e}")))?,
            None => Connection::open_in_memory()
                .map_err(|e| StorageError::Database(format!("Failed to open in-memory DB: {e}")))?,
        };

        let index = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        };

        index.apply_pragmas()?;
        index.init_schema()?;
        Ok(index)
    }

    /// Returns the database file path on disk, or `None` if opened in-memory.
    #[must_use]
    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    fn apply_pragmas(&self) -> Result<(), StorageError> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        if self.db_path.is_some() {
            guard
                .execute_batch(
                    "PRAGMA journal_mode = WAL;
                     PRAGMA synchronous = NORMAL;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA busy_timeout = 5000;",
                )
                .map_err(|e| {
                    StorageError::Database(format!("Failed to configure WAL pragmas: {e}"))
                })?;
        } else {
            guard
                .execute_batch(
                    "PRAGMA synchronous = NORMAL;
                     PRAGMA temp_store = MEMORY;
                     PRAGMA busy_timeout = 5000;",
                )
                .map_err(|e| {
                    StorageError::Database(format!("Failed to configure in-memory pragmas: {e}"))
                })?;
        }

        Ok(())
    }

    /// Queries a SQLite PRAGMA value as a String (safe alphanumeric pragma names only).
    pub fn query_pragma_string(&self, pragma: &str) -> Result<String, StorageError> {
        if !pragma
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(StorageError::Database(
                "Invalid pragma identifier".to_owned(),
            ));
        }
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;
        let sql = format!("PRAGMA {pragma};");
        let mut stmt = guard.prepare(&sql).map_err(|e| {
            StorageError::Database(format!("Failed to prepare pragma {pragma}: {e}"))
        })?;
        let val: String = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| StorageError::Database(format!("Failed to query pragma {pragma}: {e}")))?;
        Ok(val)
    }

    /// Queries a SQLite PRAGMA value as an i64 integer (safe alphanumeric pragma names only).
    pub fn query_pragma_i64(&self, pragma: &str) -> Result<i64, StorageError> {
        if !pragma
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(StorageError::Database(
                "Invalid pragma identifier".to_owned(),
            ));
        }
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;
        let sql = format!("PRAGMA {pragma};");
        let mut stmt = guard.prepare(&sql).map_err(|e| {
            StorageError::Database(format!("Failed to prepare pragma {pragma}: {e}"))
        })?;
        let val: i64 = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| StorageError::Database(format!("Failed to query pragma {pragma}: {e}")))?;
        Ok(val)
    }

    fn init_schema(&self) -> Result<(), StorageError> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        guard
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS photos (
                    raw_path TEXT PRIMARY KEY,
                    camera_make TEXT NOT NULL,
                    camera_model TEXT NOT NULL,
                    lens_model TEXT,
                    focal_length_mm REAL,
                    aperture_f_number REAL,
                    iso INTEGER,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    rating INTEGER NOT NULL,
                    pick_status TEXT NOT NULL,
                    color_label TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS keywords (
                    raw_path TEXT NOT NULL,
                    keyword TEXT NOT NULL,
                    PRIMARY KEY (raw_path, keyword)
                );
                CREATE INDEX IF NOT EXISTS idx_photos_rating ON photos(rating);
                CREATE INDEX IF NOT EXISTS idx_photos_label ON photos(color_label);
                CREATE INDEX IF NOT EXISTS idx_photos_pick ON photos(pick_status);
                CREATE INDEX IF NOT EXISTS idx_photos_camera ON photos(camera_model);
                CREATE INDEX IF NOT EXISTS idx_photos_updated_at ON photos(updated_at);
                CREATE INDEX IF NOT EXISTS idx_keywords_kw ON keywords(keyword);",
            )
            .map_err(|e| StorageError::Database(format!("Failed to init schema: {e}")))?;

        Ok(())
    }

    /// Indexes or updates a photo's metadata in the DAM database.
    pub fn index_photo(&self, path: &Path, meta: &PhotoMetadata) -> Result<(), StorageError> {
        let current_time = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.index_photo_with_mtime(path, meta, current_time)
    }

    fn index_photo_with_mtime(
        &self,
        path: &Path,
        meta: &PhotoMetadata,
        updated_at: i64,
    ) -> Result<(), StorageError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let tx = guard
            .transaction()
            .map_err(|e| StorageError::Database(format!("Transaction begin failed: {e}")))?;

        let raw_path = path.to_string_lossy().to_string();
        let pick_str = pick_status_to_str(meta.pick);
        let label_str = color_label_to_str(meta.label);

        tx.execute(
            "INSERT INTO photos (
                raw_path, camera_make, camera_model, lens_model, focal_length_mm,
                aperture_f_number, iso, width, height, rating, pick_status, color_label, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(raw_path) DO UPDATE SET
                camera_make=excluded.camera_make,
                camera_model=excluded.camera_model,
                lens_model=excluded.lens_model,
                focal_length_mm=excluded.focal_length_mm,
                aperture_f_number=excluded.aperture_f_number,
                iso=excluded.iso,
                width=excluded.width,
                height=excluded.height,
                rating=excluded.rating,
                pick_status=excluded.pick_status,
                color_label=excluded.color_label,
                updated_at=excluded.updated_at",
            params![
                raw_path,
                meta.camera_make,
                meta.camera_model,
                meta.lens_model,
                meta.focal_length_mm,
                meta.aperture_f_number,
                meta.iso.map(|v| v as i64),
                meta.width as i64,
                meta.height as i64,
                meta.rating as i64,
                pick_str,
                label_str,
                updated_at,
            ],
        )
        .map_err(|e| StorageError::Database(format!("Insert photo failed: {e}")))?;

        tx.execute(
            "DELETE FROM keywords WHERE raw_path = ?1",
            params![raw_path],
        )
        .map_err(|e| StorageError::Database(format!("Delete keywords failed: {e}")))?;

        for kw in &meta.keywords {
            tx.execute(
                "INSERT OR IGNORE INTO keywords (raw_path, keyword) VALUES (?1, ?2)",
                params![raw_path, kw],
            )
            .map_err(|e| StorageError::Database(format!("Insert keyword failed: {e}")))?;
        }

        tx.commit()
            .map_err(|e| StorageError::Database(format!("Commit failed: {e}")))?;

        Ok(())
    }

    /// Incrementally synchronizes a photo with the DAM database.
    ///
    /// Respects `.rraw.json` (and `.xmp`) as the single source of truth for ratings and tags.
    /// If the database record already exists and has `updated_at >= max(raw_mtime, sidecars_mtime)`,
    /// indexing is skipped to maximize performance.
    pub fn sync_photo(
        &self,
        raw_path: &Path,
        meta: &PhotoMetadata,
    ) -> Result<SyncAction, StorageError> {
        let latest_mtime = get_latest_mtime(raw_path);

        if let Some(updated_at) = self.get_photo_updated_at(raw_path)? {
            if latest_mtime > 0 && updated_at >= latest_mtime {
                return Ok(SyncAction::Skipped);
            }
        }

        let (sidecar_rating, sidecar_label, sidecar_pick) = read_sidecar_overrides(raw_path);
        let rating = sidecar_rating.unwrap_or(meta.rating);
        let label = sidecar_label.unwrap_or(meta.label);
        let pick = sidecar_pick.unwrap_or(meta.pick);

        let mut merged_meta = meta.clone();
        merged_meta.rating = rating;
        merged_meta.label = label;
        merged_meta.pick = pick;

        let record_time = if latest_mtime > 0 {
            latest_mtime
        } else {
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        };

        self.index_photo_with_mtime(raw_path, &merged_meta, record_time)?;
        Ok(SyncAction::Updated)
    }

    /// Fast sidecar sync path: updates star rating, color label, and pick status
    /// directly from `.rraw.json` / `.xmp` without requiring photo re-decoding.
    pub fn sync_sidecar<P: AsRef<Path>>(&self, raw_path: P) -> Result<bool, StorageError> {
        let p = raw_path.as_ref();
        let (sidecar_rating, sidecar_label, sidecar_pick) = read_sidecar_overrides(p);
        if sidecar_rating.is_none() && sidecar_label.is_none() && sidecar_pick.is_none() {
            return Ok(false);
        }

        let existing = self.get_photo(p)?;
        let record = match existing {
            Some(r) => r,
            None => return Ok(false),
        };

        let rating = sidecar_rating.unwrap_or(record.rating);
        let label = sidecar_label.unwrap_or(record.label);
        let pick = sidecar_pick.unwrap_or(record.pick);
        let latest_mtime = get_latest_mtime(p);

        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let path_str = p.to_string_lossy().to_string();
        let label_str = color_label_to_str(label);
        let pick_str = pick_status_to_str(pick);
        let updated_at = if latest_mtime > 0 {
            latest_mtime
        } else {
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        };

        let rows_affected = guard
            .execute(
                "UPDATE photos SET rating = ?1, color_label = ?2, pick_status = ?3, updated_at = ?4 WHERE raw_path = ?5",
                params![rating as i64, label_str, pick_str, updated_at, path_str],
            )
            .map_err(|e| StorageError::Database(format!("Update sidecar ratings failed: {e}")))?;

        Ok(rows_affected > 0)
    }

    /// Updates rating, color label, and pick status directly in the database.
    pub fn update_ratings<P: AsRef<Path>>(
        &self,
        raw_path: P,
        rating: u8,
        label: ColorLabel,
        pick: PickStatus,
    ) -> Result<bool, StorageError> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let path_str = raw_path.as_ref().to_string_lossy().to_string();
        let label_str = color_label_to_str(label);
        let pick_str = pick_status_to_str(pick);

        let rows_affected = guard
            .execute(
                "UPDATE photos SET rating = ?1, color_label = ?2, pick_status = ?3, updated_at = unixepoch() WHERE raw_path = ?4",
                params![rating as i64, label_str, pick_str, path_str],
            )
            .map_err(|e| StorageError::Database(format!("Update ratings failed: {e}")))?;

        Ok(rows_affected > 0)
    }

    /// Retrieves an indexed photo record by raw file path.
    pub fn get_photo<P: AsRef<Path>>(
        &self,
        raw_path: P,
    ) -> Result<Option<DamPhotoRecord>, StorageError> {
        let path_str = raw_path.as_ref().to_string_lossy().to_string();
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let mut stmt = guard
            .prepare(
                "SELECT raw_path, camera_make, camera_model, rating, color_label, pick_status
                 FROM photos WHERE raw_path = ?1 LIMIT 1",
            )
            .map_err(|e| StorageError::Database(format!("Prepare get_photo failed: {e}")))?;

        let mut rows = stmt
            .query_map(params![path_str], |row| {
                let raw_path: String = row.get(0)?;
                let camera_make: String = row.get(1)?;
                let camera_model: String = row.get(2)?;
                let rating: u8 = row.get::<_, i64>(3)? as u8;
                let label_str: String = row.get(4)?;
                let pick_str: String = row.get(5)?;

                let label = parse_color_label(&label_str);
                let pick = parse_pick_status(&pick_str);

                Ok(DamPhotoRecord {
                    raw_path,
                    camera_make,
                    camera_model,
                    rating,
                    label,
                    pick,
                })
            })
            .map_err(|e| StorageError::Database(format!("Query get_photo failed: {e}")))?;

        if let Some(r) = rows.next() {
            let record =
                r.map_err(|e| StorageError::Database(format!("Row error in get_photo: {e}")))?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    /// Retrieves the `updated_at` timestamp for a photo record.
    pub fn get_photo_updated_at<P: AsRef<Path>>(
        &self,
        raw_path: P,
    ) -> Result<Option<i64>, StorageError> {
        let path_str = raw_path.as_ref().to_string_lossy().to_string();
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let mut stmt = guard
            .prepare("SELECT updated_at FROM photos WHERE raw_path = ?1 LIMIT 1")
            .map_err(|e| {
                StorageError::Database(format!("Prepare get_photo_updated_at failed: {e}"))
            })?;

        let mut rows = stmt
            .query_map(params![path_str], |row| row.get::<_, i64>(0))
            .map_err(|e| {
                StorageError::Database(format!("Query get_photo_updated_at failed: {e}"))
            })?;

        if let Some(r) = rows.next() {
            let val = r.map_err(|e| StorageError::Database(format!("Row error: {e}")))?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    /// Queries indexed photos matching the provided criteria.
    pub fn query_photos(&self, filter: &DamFilter) -> Result<Vec<DamPhotoRecord>, StorageError> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let mut query = "SELECT DISTINCT p.raw_path, p.camera_make, p.camera_model, p.rating, p.color_label, p.pick_status
                         FROM photos p
                         LEFT JOIN keywords k ON p.raw_path = k.raw_path
                         WHERE 1=1".to_owned();

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(r) = filter.min_rating {
            query.push_str(" AND p.rating >= ?");
            params_vec.push(Box::new(r as i64));
        }

        if let Some(l) = filter.color_label {
            query.push_str(" AND p.color_label = ?");
            params_vec.push(Box::new(color_label_to_str(l).to_owned()));
        }

        if let Some(p) = filter.pick_status {
            query.push_str(" AND p.pick_status = ?");
            params_vec.push(Box::new(pick_status_to_str(p).to_owned()));
        }

        if let Some(ref m) = filter.camera_model {
            query.push_str(" AND p.camera_model = ?");
            params_vec.push(Box::new(m.clone()));
        }

        if let Some(ref kw) = filter.keyword {
            query.push_str(" AND k.keyword = ?");
            params_vec.push(Box::new(kw.clone()));
        }

        query.push_str(" ORDER BY p.raw_path ASC");

        let mut stmt = guard
            .prepare(&query)
            .map_err(|e| StorageError::Database(format!("Prepare query failed: {e}")))?;

        let slice_params: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(&slice_params[..], |row| {
                let raw_path: String = row.get(0)?;
                let camera_make: String = row.get(1)?;
                let camera_model: String = row.get(2)?;
                let rating: u8 = row.get::<_, i64>(3)? as u8;
                let label_str: String = row.get(4)?;
                let pick_str: String = row.get(5)?;

                let label = parse_color_label(&label_str);
                let pick = parse_pick_status(&pick_str);

                Ok(DamPhotoRecord {
                    raw_path,
                    camera_make,
                    camera_model,
                    rating,
                    label,
                    pick,
                })
            })
            .map_err(|e| StorageError::Database(format!("Query failed: {e}")))?;

        let mut records = Vec::new();
        for r in rows {
            records.push(r.map_err(|e| StorageError::Database(format!("Row error: {e}")))?);
        }

        Ok(records)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_color_label(s: &str) -> ColorLabel {
    match s.trim().to_ascii_lowercase().as_str() {
        "red" => ColorLabel::Red,
        "yellow" => ColorLabel::Yellow,
        "green" => ColorLabel::Green,
        "blue" => ColorLabel::Blue,
        "purple" => ColorLabel::Purple,
        _ => ColorLabel::None,
    }
}

fn parse_pick_status(s: &str) -> PickStatus {
    match s.trim().to_ascii_lowercase().as_str() {
        "pick" => PickStatus::Pick,
        "reject" => PickStatus::Reject,
        _ => PickStatus::Unflagged,
    }
}

fn color_label_to_str(label: ColorLabel) -> &'static str {
    match label {
        ColorLabel::None => "none",
        ColorLabel::Red => "red",
        ColorLabel::Yellow => "yellow",
        ColorLabel::Green => "green",
        ColorLabel::Blue => "blue",
        ColorLabel::Purple => "purple",
    }
}

fn pick_status_to_str(pick: PickStatus) -> &'static str {
    match pick {
        PickStatus::Unflagged => "unflagged",
        PickStatus::Pick => "pick",
        PickStatus::Reject => "reject",
    }
}

fn get_latest_mtime(raw_path: &Path) -> i64 {
    let sidecars = SidecarPaths::from_raw_path(raw_path);
    let mut max_mtime = 0i64;

    for path in [
        &raw_path.to_path_buf(),
        &sidecars.recipe_json,
        &sidecars.xmp_sidecar,
    ] {
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    let secs = duration.as_secs() as i64;
                    if secs > max_mtime {
                        max_mtime = secs;
                    }
                }
            }
        }
    }
    max_mtime
}

fn read_sidecar_overrides(raw_path: &Path) -> (Option<u8>, Option<ColorLabel>, Option<PickStatus>) {
    let sidecars = SidecarPaths::from_raw_path(raw_path);
    let mut rating = None;
    let mut label = None;
    let mut pick = None;

    // 1. Read .rraw.json (Single source of truth)
    if sidecars.recipe_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&sidecars.recipe_json) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(r) = val.get("rating").and_then(|v| v.as_u64()) {
                    rating = Some(r.min(5) as u8);
                }
                if let Some(l) = val
                    .get("color_label")
                    .or_else(|| val.get("label"))
                    .and_then(|v| v.as_str())
                {
                    label = Some(parse_color_label(l));
                }
                if let Some(p) = val
                    .get("pick_status")
                    .or_else(|| val.get("pick"))
                    .and_then(|v| v.as_str())
                {
                    pick = Some(parse_pick_status(p));
                }
            }
        }
    }

    // 2. Read .xmp sidecar (complements / fallback)
    if sidecars.xmp_sidecar.exists() {
        if let Ok(content) = std::fs::read_to_string(&sidecars.xmp_sidecar) {
            if let Ok(editor) = XmpEditor::parse_str(&content) {
                if rating.is_none() {
                    rating = editor.rating();
                }
                if label.is_none() {
                    label = editor.label();
                }
            }
        }
    }

    (rating, label, pick)
}
