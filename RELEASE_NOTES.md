# Rraw v0.1.1 — macOS Desktop Release & Clean Storage Architecture

We are thrilled to announce the **v0.1.1** release of **Rraw**, a high-performance RAW photo darkroom developed in 100% pure, safe Rust for macOS Apple Silicon.

---

### 🚀 Highlights & New Features

- **Automated macOS Application Packaging (`.dmg`)**:
  - Native Drag-and-Drop installer with `/Applications` symlink.
  - Codesigned application bundle (`Rraw.app`) with HiDPI icon and bundled i18n catalogs.
  - SHA-256 cryptographic checksums for verified downloads.
- **Clean Finder Architecture (`.rraw/`)**:
  - Relocated non-destructive recipe sidecars to `<folder>/.rraw/<photo>.rraw.json` alongside `.rraw/cache.db`.
  - Zero clutter in the Finder: RAW folders remain pristine and contain only your original photo files.
  - Purged legacy XMP sidecars in favor of fast, native JSON recipes.
- **Instant Star Ratings**:
  - Cached photo ratings now load synchronously upon folder opening (< 0.1 ms), displaying all ratings at Frame 0 in the filmstrip without requiring manual selection.

---

### 📦 Installation

1. Download **`Rraw-v0.1.1.dmg`** below.
2. Open the disk image and drag **Rraw.app** into your **Applications** folder.
3. *(Optional)* Verify integrity:
   ```bash
   shasum -a 256 -c Rraw-v0.1.1.dmg.sha256
   ```

*Requires macOS 14 (Sonoma) or macOS 15 (Sequoia) on Apple Silicon (M1/M2/M3/M4).*
