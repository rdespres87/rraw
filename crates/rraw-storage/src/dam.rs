#![forbid(unsafe_code)]

//! SQLite DAM metadata indexing and multi-criteria querying.

use crate::error::StorageError;
use rraw_core::{ColorLabel, PhotoMetadata, PickStatus};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

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

/// Thread-safe SQLite index for catalog-free RAW photos.
#[derive(Clone)]
pub struct SqliteDamIndex {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteDamIndex {
    /// Opens an in-memory SQLite DAM database for testing.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| StorageError::Database(format!("Failed to open in-memory DB: {e}")))?;
        let index = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        index.init_schema()?;
        Ok(index)
    }

    /// Opens or creates an SQLite DAM database at `db_path`.
    pub fn open<P: AsRef<Path>>(db_path: P) -> Result<Self, StorageError> {
        let conn = Connection::open(db_path)
            .map_err(|e| StorageError::Database(format!("Failed to open DB: {e}")))?;
        let index = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        index.init_schema()?;
        Ok(index)
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
                CREATE INDEX IF NOT EXISTS idx_keywords_kw ON keywords(keyword);",
            )
            .map_err(|e| StorageError::Database(format!("Failed to init schema: {e}")))?;

        Ok(())
    }

    /// Indexes or updates a photo's metadata in the DAM database.
    pub fn index_photo(&self, path: &Path, meta: &PhotoMetadata) -> Result<(), StorageError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| StorageError::Database("Mutex poisoned".to_owned()))?;

        let tx = guard
            .transaction()
            .map_err(|e| StorageError::Database(format!("Transaction begin failed: {e}")))?;

        let raw_path = path.to_string_lossy().to_string();

        let pick_str = match meta.pick {
            PickStatus::Unflagged => "unflagged",
            PickStatus::Pick => "pick",
            PickStatus::Reject => "reject",
        };

        let label_str = match meta.label {
            ColorLabel::None => "none",
            ColorLabel::Red => "red",
            ColorLabel::Yellow => "yellow",
            ColorLabel::Green => "green",
            ColorLabel::Blue => "blue",
            ColorLabel::Purple => "purple",
        };

        tx.execute(
            "INSERT INTO photos (
                raw_path, camera_make, camera_model, lens_model, focal_length_mm,
                aperture_f_number, iso, width, height, rating, pick_status, color_label, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, unixepoch())
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
                updated_at=unixepoch()",
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
            let label_str = match l {
                ColorLabel::None => "none",
                ColorLabel::Red => "red",
                ColorLabel::Yellow => "yellow",
                ColorLabel::Green => "green",
                ColorLabel::Blue => "blue",
                ColorLabel::Purple => "purple",
            };
            query.push_str(" AND p.color_label = ?");
            params_vec.push(Box::new(label_str.to_owned()));
        }

        if let Some(p) = filter.pick_status {
            let pick_str = match p {
                PickStatus::Unflagged => "unflagged",
                PickStatus::Pick => "pick",
                PickStatus::Reject => "reject",
            };
            query.push_str(" AND p.pick_status = ?");
            params_vec.push(Box::new(pick_str.to_owned()));
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

                let label = match label_str.as_str() {
                    "red" => ColorLabel::Red,
                    "yellow" => ColorLabel::Yellow,
                    "green" => ColorLabel::Green,
                    "blue" => ColorLabel::Blue,
                    "purple" => ColorLabel::Purple,
                    _ => ColorLabel::None,
                };

                let pick = match pick_str.as_str() {
                    "pick" => PickStatus::Pick,
                    "reject" => PickStatus::Reject,
                    _ => PickStatus::Unflagged,
                };

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
