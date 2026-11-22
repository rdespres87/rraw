use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PhotoMetadata,
    PickStatus,
};
use rraw_storage::{DamFilter, SidecarPaths, SqliteDamIndex, SyncAction};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create_temp_dir(prefix: &str) -> Result<(TempDirGuard, PathBuf), std::io::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!("rraw_test_{prefix}_{now}"));
    fs::create_dir_all(&p)?;
    Ok((TempDirGuard(p.clone()), p))
}

fn create_dummy_metadata(
    camera_model: &str,
    rating: u8,
) -> Result<PhotoMetadata, rraw_core::CoreError> {
    let noise_profile =
        NoiseProfile::new([0.0001; 3], [0.0001; 3], NoiseProfileSource::CameraTable)?;

    Ok(PhotoMetadata {
        camera_make: "Sony".to_owned(),
        camera_model: camera_model.to_owned(),
        lens_model: Some("FE 24-70mm F2.8 GM II".to_owned()),
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
        iso: Some(100),
        width: 7000,
        height: 4666,
        black_levels: [512.0; 4],
        white_levels: [16383.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile,
        vignette_handled_by_gainmap: false,
        rating,
        pick: PickStatus::Unflagged,
        label: ColorLabel::None,
        keywords: vec!["landscape".to_owned()],
    })
}

#[test]
fn test_session_database_creation_and_wal_pragmas() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, base_dir) = create_temp_dir("session_wal")?;
    let session_dir = base_dir.join("Photos_2026");
    fs::create_dir_all(&session_dir)?;

    let index = SqliteDamIndex::open_session_database(&session_dir)?;

    let expected_db_path = session_dir.join(".rraw").join("cache.db");
    assert!(
        expected_db_path.exists(),
        "cache.db should exist at .rraw/cache.db"
    );
    assert_eq!(index.db_path(), Some(expected_db_path.as_path()));

    // Verify WAL and performance pragmas
    let journal_mode = index.query_pragma_string("journal_mode")?;
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let synchronous = index.query_pragma_i64("synchronous")?;
    // 1 corresponds to NORMAL in SQLite
    assert_eq!(synchronous, 1);

    let temp_store = index.query_pragma_i64("temp_store")?;
    // 2 corresponds to MEMORY in SQLite
    assert_eq!(temp_store, 2);

    let busy_timeout = index.query_pragma_i64("busy_timeout")?;
    assert_eq!(busy_timeout, 5000);

    Ok(())
}

#[test]
fn test_sidecar_as_single_source_of_truth_on_sync() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("sidecar_truth")?;
    let index = SqliteDamIndex::open_session_database(&session_dir)?;

    let raw_path = session_dir.join("DSC_0001.ARW");
    fs::write(&raw_path, b"dummy raw bytes")?;

    // RAW metadata has rating 1 and label None
    let meta = create_dummy_metadata("ILCE-7M4", 1)?;

    // Create sidecar .rraw.json with rating 4, label "green", pick "pick"
    let sidecars = SidecarPaths::from_raw_path(&raw_path);
    let recipe_content = serde_json::json!({
        "schema_version": 1,
        "rating": 4,
        "color_label": "green",
        "pick_status": "pick"
    });
    fs::write(
        &sidecars.recipe_json,
        serde_json::to_string_pretty(&recipe_content)?,
    )?;

    // Sync photo
    let action = index.sync_photo(&raw_path, &meta)?;
    assert_eq!(action, SyncAction::Updated);

    // Query photo and assert sidecar overrides were applied
    let record = index.get_photo(&raw_path)?.ok_or("record missing")?;
    assert_eq!(
        record.rating, 4,
        "Sidecar rating (4) must override raw rating (1)"
    );
    assert_eq!(
        record.label,
        ColorLabel::Green,
        "Sidecar color label (green) must override None"
    );
    assert_eq!(
        record.pick,
        PickStatus::Pick,
        "Sidecar pick status must override Unflagged"
    );

    Ok(())
}

#[test]
fn test_incremental_sync_skips_unchanged_files() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("incremental_sync")?;
    let index = SqliteDamIndex::open_session_database(&session_dir)?;

    let raw_path = session_dir.join("DSC_0002.ARW");
    fs::write(&raw_path, b"dummy raw bytes")?;
    let meta = create_dummy_metadata("ILCE-7M4", 2)?;

    // First sync should insert/update
    let first_action = index.sync_photo(&raw_path, &meta)?;
    assert_eq!(first_action, SyncAction::Updated);

    // Second sync immediately after without touching file should skip
    let second_action = index.sync_photo(&raw_path, &meta)?;
    assert_eq!(second_action, SyncAction::Skipped);

    // Now update .rraw.json sidecar
    let sidecars = SidecarPaths::from_raw_path(&raw_path);
    // Ensure modified timestamp increases
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let recipe_content = serde_json::json!({
        "schema_version": 1,
        "rating": 5
    });
    fs::write(
        &sidecars.recipe_json,
        serde_json::to_string(&recipe_content)?,
    )?;

    // Third sync should detect newer sidecar and update
    let third_action = index.sync_photo(&raw_path, &meta)?;
    assert_eq!(third_action, SyncAction::Updated);

    let record = index.get_photo(&raw_path)?.ok_or("record missing")?;
    assert_eq!(record.rating, 5);

    Ok(())
}

#[test]
fn test_sync_sidecar_fast_path_without_redecoding() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("sidecar_fast")?;
    let index = SqliteDamIndex::open_session_database(&session_dir)?;

    let raw_path = session_dir.join("DSC_0003.ARW");
    fs::write(&raw_path, b"raw")?;
    let meta = create_dummy_metadata("ILCE-7M4", 0)?;

    index.sync_photo(&raw_path, &meta)?;

    // UI modifies sidecar directly
    let sidecars = SidecarPaths::from_raw_path(&raw_path);
    let recipe_content = serde_json::json!({
        "schema_version": 1,
        "rating": 3,
        "color_label": "yellow",
        "pick_status": "reject"
    });
    fs::write(
        &sidecars.recipe_json,
        serde_json::to_string(&recipe_content)?,
    )?;

    // Call fast sidecar sync
    let updated = index.sync_sidecar(&raw_path)?;
    assert!(
        updated,
        "sync_sidecar should successfully update existing record"
    );

    let record = index.get_photo(&raw_path)?.ok_or("record missing")?;
    assert_eq!(record.rating, 3);
    assert_eq!(record.label, ColorLabel::Yellow);
    assert_eq!(record.pick, PickStatus::Reject);
    assert_eq!(
        record.camera_model, "ILCE-7M4",
        "Camera model must be preserved"
    );

    Ok(())
}

#[test]
fn test_concurrent_access_wal_threads() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("concurrent_wal")?;
    let index = Arc::new(SqliteDamIndex::open_session_database(&session_dir)?);

    let mut handles = Vec::new();

    // Spawn 4 writer threads inserting photos
    for t in 0..4 {
        let index_clone = Arc::clone(&index);
        let session_clone = session_dir.clone();
        let handle = thread::spawn(move || -> Result<(), String> {
            for i in 0..15 {
                let photo_name = format!("IMG_T{t}_{i}.DNG");
                let photo_path = session_clone.join(photo_name);
                let meta = create_dummy_metadata("Sony A7IV", (i % 6) as u8)
                    .map_err(|e| format!("Meta error: {e}"))?;
                index_clone
                    .sync_photo(&photo_path, &meta)
                    .map_err(|e| format!("Sync error: {e}"))?;
            }
            Ok(())
        });
        handles.push(handle);
    }

    // Spawn 4 reader threads querying photos concurrently
    for _ in 0..4 {
        let index_clone = Arc::clone(&index);
        let handle = thread::spawn(move || -> Result<(), String> {
            for _ in 0..20 {
                let filter = DamFilter {
                    min_rating: Some(2),
                    ..Default::default()
                };
                let _ = index_clone
                    .query_photos(&filter)
                    .map_err(|e| format!("Query error: {e}"))?;
                thread::yield_now();
            }
            Ok(())
        });
        handles.push(handle);
    }

    for h in handles {
        h.join()
            .map_err(|_| "thread panicked")?
            .map_err(|e| e.to_string())?;
    }

    let all_records = index.query_photos(&DamFilter::default())?;
    assert_eq!(
        all_records.len(),
        60,
        "All 60 photos must be indexed without loss"
    );

    Ok(())
}
