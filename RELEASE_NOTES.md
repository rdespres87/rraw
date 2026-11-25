# Rraw v0.3.2

## Highlights

- **SQLite Security Patch** — `rusqlite` 0.33 → 0.43 (bundled `libsqlite3-sys` 0.31.0 → 0.40.0) resolves RUSTSEC-2026-0459 (CVE-2026-119375).
- **XML Parser Security Patch** — `quick-xml` 0.37 → 0.41 removes the vulnerable 0.37.5 from `Cargo.lock` and migrates `rraw-optics` Lensfun XML decoding to `BytesText::decode()`, resolving RUSTSEC-2026-0194 and RUSTSEC-2026-0195.
- **URL Crate Security Patch** — `url` 2.5.8 → 2.5.9 resolves RUSTSEC-2026-0592.
- **Zero-Vulnerability Baseline** — 467 tests across 95 suites passing, 0 Clippy warnings (`-D warnings`), 0 `cargo-audit` vulnerabilities.

## Installation

| File | Description |
|------|-------------|
| `Rraw-v0.3.2.dmg` | macOS installer (Apple Silicon) |
| `Rraw-v0.3.2.dmg.sha256` | SHA-256 checksum for integrity verification |

**Requirements:** macOS 14 (Sonoma) or 15 (Sequoia) on Apple Silicon.

```bash
# Verify download integrity
shasum -a 256 -c Rraw-v0.3.2.dmg.sha256
```

## Upgrade Notes

No migration steps required. All changes are internal dependency version bumps with no effect on saved `.rraw.json` recipes, layout preferences, grading presets, or output formats. The `quick-xml` migration to `BytesText::decode()` is API-transparent to Lensfun XML parsing.
