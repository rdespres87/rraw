//! Dedicated domain controller managing the viewport, zoom, pan, split compare and interactive guides.

use crate::state::{CropDragHandle, CurveChannel, HistogramData, ParallelToolPhase};
use crate::view_tools::ClippingOverlay;

/// Mutually exclusive interactive canvas tool active in the central viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveCanvasTool {
    /// Standard pan/zoom inspection mode (no interactive drawing overlay active).
    None,
    /// Interactive crop & aspect ratio framing handles.
    Crop,
    /// Interactive horizon leveling line tool.
    Horizon,
    /// Interactive 2-parallel-lines perspective keystone guide tool.
    ParallelLines(ParallelToolPhase),
    /// Interactive freehand brush painting on the specified local mask index.
    FreehandBrush(usize),
}

/// Viewport and interactive overlay controller.
#[derive(Debug, Clone)]
pub struct ViewportController {
    /// Viewport zoom scale: 0.0 = Fit to viewport, 1.0 = 100% 1:1 pixel mapping, etc.
    pub zoom_level: f32,
    /// Viewport pan offset in logical points.
    pub pan_offset: egui::Vec2,
    /// Last computed fit-to-viewport scale.
    pub last_fit_scale: f32,
    /// Split-screen Before/After comparison mode (toggled with `Y`).
    pub split_compare: bool,
    /// Normalized horizontal position of the Before/After divider in `[0, 1]`.
    pub split_position: f32,
    /// Shadow / highlight clipping warnings painted over the preview (`J`).
    pub clipping_overlay: ClippingOverlay,
    /// Pre-computed live RGB histogram data.
    pub histogram: HistogramData,
    /// Whether the leveling line tool is active.
    pub horizon_tool_active: bool,
    /// Drag start point for leveling line tool.
    pub horizon_start_point: Option<egui::Pos2>,
    /// Phase of the interactive 2-parallel-lines perspective correction tool.
    pub parallel_tool_phase: ParallelToolPhase,
    /// First user-drawn reference line for parallel alignment.
    pub parallel_line1: Option<(egui::Pos2, egui::Pos2)>,
    /// Second user-drawn reference line for parallel alignment.
    pub parallel_line2: Option<(egui::Pos2, egui::Pos2)>,
    /// Pointer start coordinate when dragging a parallel reference line.
    pub parallel_drag_start: Option<egui::Pos2>,
    /// Whether the crop frame is actively being manipulated in the central viewport.
    pub is_crop_editing: bool,
    /// Whether the rule-of-thirds grid overlay is displayed while editing the crop frame.
    pub crop_show_grid: bool,
    /// Active interactive handle being dragged for cropping.
    pub crop_drag_handle: CropDragHandle,
    /// Active color channel in the tone curves inspector.
    pub active_curve_channel: CurveChannel,
    /// Index of curve point currently being dragged, if any.
    pub dragging_curve_point: Option<usize>,
}

impl Default for ViewportController {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportController {
    /// Creates a default viewport controller in "Fit to Window" state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            zoom_level: 0.0,
            pan_offset: egui::Vec2::ZERO,
            last_fit_scale: 1.0,
            split_compare: false,
            split_position: 0.5,
            clipping_overlay: ClippingOverlay::default(),
            histogram: HistogramData::default(),
            horizon_tool_active: false,
            horizon_start_point: None,
            parallel_tool_phase: ParallelToolPhase::Inactive,
            parallel_line1: None,
            parallel_line2: None,
            parallel_drag_start: None,
            is_crop_editing: false,
            crop_show_grid: true,
            crop_drag_handle: CropDragHandle::None,
            active_curve_channel: CurveChannel::CombinedRgb,
            dragging_curve_point: None,
        }
    }

    /// Resets the camera pan and zoom back to "Fit to Window" (zoom_level = 0.0, pan = 0).
    pub fn reset_view(&mut self) {
        self.zoom_level = 0.0;
        self.pan_offset = egui::Vec2::ZERO;
    }

    /// Returns true if the camera is currently in automatic "Fit to Window" scale.
    #[must_use]
    pub fn is_zoom_fit(&self) -> bool {
        self.zoom_level <= 0.0
    }

    /// Toggles the Before/After split screen comparison view.
    pub fn toggle_split_compare(&mut self) {
        self.split_compare = !self.split_compare;
    }
}
