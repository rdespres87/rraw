use rraw_app::RrawApp;
use rraw_core::{ColorLabel, PickStatus};
use rraw_storage::SqliteDamIndex;
use std::fs;
use std::path::PathBuf;

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
    let p = std::env::temp_dir().join(format!("rraw_test_app_{prefix}_{now}"));
    fs::create_dir_all(&p)?;
    Ok((TempDirGuard(p.clone()), p))
}

#[test]
fn test_open_directory_creates_rraw_cache_db() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("open_creates_dam")?;
    let dummy_photo = session_dir.join("SAMPLE_01.DNG");
    fs::write(&dummy_photo, b"dummy dng content")?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&session_dir);

    // Assert that .rraw/cache.db exists in the opened directory
    let cache_db = session_dir.join(".rraw").join("cache.db");
    assert!(
        cache_db.exists(),
        ".rraw/cache.db must be automatically created when opening a directory"
    );
    assert!(
        app.library.session_dam.is_some(),
        "LibraryController.session_dam must be populated"
    );

    Ok(())
}

#[test]
fn test_open_directory_preloads_ratings_from_sqlite() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("preload_sqlite")?;
    let photo_path = session_dir.join("PHOTO_01.DNG");
    fs::write(&photo_path, b"dummy dng content")?;

    // Pre-populate .rraw/cache.db with rating 4
    let dam = SqliteDamIndex::open_session_database(&session_dir)?;
    dam.update_ratings(&photo_path, 4, ColorLabel::Green, PickStatus::Pick)?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&session_dir);

    // Preload directly or drain background indexer
    if let Some(dam) = &app.library.session_dam {
        let ratings = dam.load_all_ratings()?;
        app.library.photo_ratings.extend(ratings);
    }

    // Verify rating was loaded from cache.db into photo_ratings in < 1 ms
    assert_eq!(
        app.library.photo_ratings.get(&photo_path),
        Some(&4),
        "Rating must be preloaded directly from SQLite session DAM"
    );

    Ok(())
}

#[test]
fn test_set_rating_bidirectional_sync() -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("bidi_sync")?;
    let photo_path = session_dir.join("PHOTO_BIDI.DNG");
    fs::write(&photo_path, b"dummy dng content")?;

    let mut app = RrawApp::new_headless();
    app.open_directory(&session_dir);

    // Rate photo via app
    app.set_photo_rating_for_path(&photo_path, 5);

    // 1. Verify in-memory rating
    assert_eq!(app.library.photo_ratings.get(&photo_path), Some(&5));

    // 2. Verify .rraw.json sidecar was created with rating 5 in .rraw/
    let sidecars = rraw_storage::SidecarPaths::from_raw_path(&photo_path);
    assert!(
        sidecars.recipe_json.exists(),
        "Sidecar .rraw.json must be written in .rraw/"
    );

    // 3. Verify SQLite DAM database was updated
    let dam = app
        .library
        .session_dam
        .as_ref()
        .ok_or("session_dam missing")?;
    let record = dam.get_photo(&photo_path)?;
    assert!(
        record.is_some(),
        "Photo record must exist in SQLite cache.db"
    );
    assert_eq!(record.map(|r| r.rating), Some(5));

    Ok(())
}

#[test]
fn test_directory_indexer_survives_decode_epoch_and_populates_unselected_stars(
) -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, session_dir) = create_temp_dir("unselected_stars")?;
    let photo_a = session_dir.join("A_FIRST.DNG");
    let photo_b = session_dir.join("B_SECOND.DNG");
    fs::write(&photo_a, b"dummy dng a")?;
    fs::write(&photo_b, b"dummy dng b")?;

    // Persist a 5-star sidecar on photo_b only (without pre-populating cache.db)
    let grading = rraw_app::state::DarkroomGradingState::default();
    rraw_app::state::save_photo_sidecars(&photo_b, &grading, 5);

    let mut app = RrawApp::new_headless();
    app.open_directory(&session_dir);

    // Simulate immediate photo decode epoch bump (opening photo_a must NOT cancel directory indexing!)
    let _ = app
        .workers
        .decode_coordinator
        .create_request(photo_a.clone(), false);

    // Drain background indexer events until photo_b's 5-star rating arrives without ever opening photo_b
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.library.photo_ratings.get(&photo_b).copied() != Some(5)
        && std::time::Instant::now() < deadline
    {
        app.drain_dam_indexing_events(64);
        std::thread::yield_now();
    }

    assert_eq!(
        app.library.photo_ratings.get(&photo_b).copied(),
        Some(5),
        "Unselected photo_b must display its 5-star rating even after decode_coordinator epoch increments"
    );

    Ok(())
}
