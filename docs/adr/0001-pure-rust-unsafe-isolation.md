# ADR-0001: Pure-Rust Parsing with Bounded Unsafe Isolation

## Context

Rraw decodes camera RAW files (TIFF/ISOBMFF, MakerNotes, DNG opcodes), Lensfun XML databases, and modern image export formats (JPEG XL, AVIF, PNG, TIFF, JPEG). Linking C/C++ parsing libraries such as `libraw` or `exiv2` introduces memory-safety risks, complex native build dependencies, and fragile cross-platform packaging. At the same time, macOS hardware integrations (AppKit `NSScreen` EDR headroom and Apple CoreML / ANE runtime bridges) require narrow platform FFI boundaries.

## Decision

- All RAW, EXIF, MakerNote, DNG opcode, Lensfun XML, and image codec parsing/encoding uses pure safe Rust crates (`rawler`, `quick-xml`, `zune-jpegxl`, `image`/`ravif`) and pure-Rust parsers — C/C++ parser bindings (`libraw`, `exiv2`) are forbidden.
- 11 of 12 workspace crates enforce `#![forbid(unsafe_code)]` (including `rraw-macos-edr`, which queries `NSScreen` through safe `objc2` bindings).
- Only `rraw-coreml` uses `#![deny(unsafe_code)]` with targeted `#[allow(unsafe_code)]` blocks gated by `#[cfg(all(target_os = "macos", target_arch = "aarch64"))]` and documented `// SAFETY:` invariants.
- Platform-specific crates expose safe Rust traits (`DisplayHeadroomProvider`, `RawDenoiser`, `SemanticSegmenter`) with cross-platform fallbacks so non-macOS CI targets compile and test cleanly.

## Consequences

- **Positive:** Memory-safety audit surface is strictly bounded to 2 isolated hardware-bridge crates; parser defects cannot cause undefined behavior or segfaults.
- **Positive:** Cross-platform Linux CI can compile and verify all engine, storage, and export crates without native macOS SDKs.
- **Negative:** Camera format support depends on pure-Rust ecosystem maturity (`rawler`) rather than `libraw`; mitigated by dedicated MakerNote/CFA/opcode integration tests.
- **Negative:** Any new macOS FFI call requires explicit `// SAFETY:` review in `rraw-macos-edr` or `rraw-coreml`.
