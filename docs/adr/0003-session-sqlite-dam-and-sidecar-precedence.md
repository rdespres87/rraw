# ADR-0003: Session SQLite DAM and Sidecar Precedence

## Status

Accepted

## Context

Rraw is catalog-free: there is no monolithic global database. Folder-level star ratings and grading recipes must appear immediately at startup without blocking the 120 Hz UI thread or racing against active photo decoding.

## Decision

- **Session index:** Store per-folder metadata in `<folder>/.rraw/cache.db` (SQLite with `journal_mode = WAL`, `synchronous = NORMAL`, `temp_store = MEMORY`, `busy_timeout = 5000`).
- **Source of truth:** Treat `<folder>/.rraw/<photo>.rraw.json` atomic sidecars (`write -> flush -> sync_all -> rename`) as the single source of truth; `cache.db` is a derived, rebuildable index.
- **Startup & epoch decoupling:** Initialize session DAM in `RrawApp::new()` as well as `open_directory()`, and give the directory indexer a dedicated `dam_index_epoch` decoupled from `DecodeCoordinator` so selecting a photo never aborts directory indexing.
- **Non-blocking write path:** Offload SQLite rating writes to a bounded background worker (`sync_channel(256)` via `try_send`), with deterministic `flush_dam_writes()` invoked on `Drop`.

## Consequences

- **Positive:** Zero UI-thread mutex or SQLite stalls during interactive rating bursts; deterministic FIFO persistence.
- **Positive:** Instant startup star ratings via synchronous `load_all_ratings()` followed by bounded background streaming (`drain_dam_indexing_events`).
- **Negative:** Deterministic tests must call `flush_dam_writes()` before asserting SQLite state.
- **Negative:** An abrupt `SIGKILL` mid-flush may drop an in-flight SQLite update, though the atomic `.rraw.json` sidecar remains intact and re-syncs automatically.
