# ADR-0002: Fused WGPU Interactive Preview Pipeline

## Status

Accepted

## Context

Rraw's interactive darkroom renders scene-referred linear ACEScg frames through seven pipeline stages:

- Stages 1-4: RAW decode, demosaic, and optical corrections
- Stage 5: Color transform and 3D CLUT application
- Stage 6: Tonal curve and local/semantic mask compositing
- Stage 7: AgX EDR display transform and scope overlays

Previously Stages 5, 6, and 7 were dispatched as three separate WGPU command submissions. Each boundary forced an intermediate GPU-to-CPU readback and a per-frame clone of the 3D CLUT data, adding avoidable round-trip latency to the 120 Hz preview loop.

## Decision

- Encode Stages 5, 6, and 7 into a single WGPU command submission (`execute_stages5_to_7_fused` in `crates/rraw-gpu/src/fused.rs`) with GPU-resident intermediate storage buffers.
- Share 3D LUTs across pipeline invocations via `Arc<ColorLut3d>`, eliminating per-frame clones.
- Retain fixed `Rgba16Float` EDR headroom clamping (guarded with `is_finite()`) at the Stage 7 output.
- Preserve a transparent CPU/Rayon fallback path for hosts without a usable GPU adapter (headless CI or unsupported drivers).

## Consequences

- **Positive:** Eliminates intermediate GPU-to-CPU readbacks between Stages 5-7 and per-frame 3D CLUT clones.
- **Positive:** Fused execution matches sequential GPU execution within `< 1e-6` (guarded by `fused_stages5_to_7_matches_sequential_execution_and_shares_arc_clut`).
- **Negative:** GPU shader stages and CPU reference kernels must stay mathematically synchronized via parity tests.
