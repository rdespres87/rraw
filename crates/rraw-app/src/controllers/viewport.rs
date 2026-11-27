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

/// Camera pan/zoom state for the central viewport.
#[derive(Debug, Clone)]
pub struct ViewportCamera {
    /// Zoom scale: 0.0 = Fit, 1.0 = 100% 1:1, etc.
    pub zoom_level: f32,
    /// Pan offset in logical points.
    pub pan_offset: egui::Vec2,
    /// Last computed fit-to-viewport scale.
    pub last_fit_scale: f32,
}

impl Default for ViewportCamera {
    fn default() -> Self {
        Self {
            zoom_level: 0.0,
            pan_offset: egui::Vec2::ZERO,
            last_fit_scale: 1.0,
        }
    }
}

impl ViewportCamera {
    /// Resets to "Fit to Window" (zoom = 0, pan = 0).
    pub fn reset(&mut self) {
        self.zoom_level = 0.0;
        self.pan_offset = egui::Vec2::ZERO;
    }

    /// Returns `true` if in automatic "Fit to Window" scale.
    #[must_use]
    pub fn is_zoom_fit(&self) -> bool {
        self.zoom_level <= 0.0
    }
}

/// Split-screen Before/After comparison state.
#[derive(Debug, Clone)]
pub struct SplitCompareState {
    /// Whether split compare is active.
    pub active: bool,
    /// Normalized divider position in `[0, 1]`.
    pub position: f32,
}

impl Default for SplitCompareState {
    fn default() -> Self {
        Self {
            active: false,
            position: 0.5,
        }
    }
}

impl SplitCompareState {
    /// Toggles the Before/After split view.
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }
}

/// Interactive crop frame manipulation state.
#[derive(Debug, Clone)]
pub struct CropToolState {
    /// Whether the crop frame is being manipulated.
    pub is_editing: bool,
    /// Whether the rule-of-thirds grid is shown.
    pub show_grid: bool,
    /// Active handle being dragged.
    pub drag_handle: CropDragHandle,
}

impl Default for CropToolState {
    fn default() -> Self {
        Self {
            is_editing: false,
            show_grid: true,
            drag_handle: CropDragHandle::None,
        }
    }
}

impl CropToolState {
    /// Cancels the crop editing session.
    pub fn cancel(&mut self) {
        self.is_editing = false;
        self.drag_handle = CropDragHandle::None;
    }
}

/// Horizon leveling and parallel-lines perspective tool state.
#[derive(Debug, Clone)]
pub struct GeometryToolsState {
    /// Whether the horizon tool is active.
    pub horizon_active: bool,
    /// Drag start for horizon line.
    pub horizon_start_point: Option<egui::Pos2>,
    /// Phase of the parallel-lines tool.
    pub parallel_phase: ParallelToolPhase,
    /// First reference line.
    pub parallel_line1: Option<(egui::Pos2, egui::Pos2)>,
    /// Second reference line.
    pub parallel_line2: Option<(egui::Pos2, egui::Pos2)>,
    /// Pointer start when dragging a parallel line.
    pub parallel_drag_start: Option<egui::Pos2>,
}

impl Default for GeometryToolsState {
    fn default() -> Self {
        Self {
            horizon_active: false,
            horizon_start_point: None,
            parallel_phase: ParallelToolPhase::Inactive,
            parallel_line1: None,
            parallel_line2: None,
            parallel_drag_start: None,
        }
    }
}

impl GeometryToolsState {
    /// Cancels the horizon leveling tool.
    pub fn cancel_horizon(&mut self) {
        self.horizon_active = false;
        self.horizon_start_point = None;
    }

    /// Cancels the parallel-lines perspective tool.
    pub fn cancel_parallel(&mut self) {
        self.parallel_phase = ParallelToolPhase::Inactive;
        self.parallel_line1 = None;
        self.parallel_line2 = None;
        self.parallel_drag_start = None;
    }

    /// Resets all geometry tools to inactive.
    pub fn reset_all(&mut self) {
        self.cancel_horizon();
        self.cancel_parallel();
    }
}

/// Tone curve editor interaction state.
#[derive(Debug, Clone)]
pub struct CurveEditorState {
    /// Active color channel.
    pub active_channel: CurveChannel,
    /// Index of point being dragged, if any.
    pub dragging_point: Option<usize>,
}

impl Default for CurveEditorState {
    fn default() -> Self {
        Self {
            active_channel: CurveChannel::CombinedRgb,
            dragging_point: None,
        }
    }
}

/// Viewport and interactive overlay controller.
#[derive(Debug, Clone)]
pub struct ViewportController {
    /// Camera pan/zoom.
    pub camera: ViewportCamera,
    /// Split-screen compare.
    pub split: SplitCompareState,
    /// Clipping warnings overlay.
    pub clipping_overlay: ClippingOverlay,
    /// Live RGB histogram.
    pub histogram: HistogramData,
    /// Horizon & parallel-lines tools.
    pub geometry_tools: GeometryToolsState,
    /// Crop frame tool.
    pub crop: CropToolState,
    /// Tone curve editor.
    pub curve_editor: CurveEditorState,
}

impl Default for ViewportController {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportController {
    /// Creates a default controller in "Fit to Window" state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            camera: ViewportCamera::default(),
            split: SplitCompareState::default(),
            clipping_overlay: ClippingOverlay::default(),
            histogram: HistogramData::default(),
            geometry_tools: GeometryToolsState::default(),
            crop: CropToolState::default(),
            curve_editor: CurveEditorState::default(),
        }
    }

    /// Resets camera to "Fit to Window".
    pub fn reset_view(&mut self) {
        self.camera.reset();
    }

    /// Returns `true` if in "Fit to Window" mode.
    #[must_use]
    pub fn is_zoom_fit(&self) -> bool {
        self.camera.is_zoom_fit()
    }

    /// Toggles Before/After split compare.
    pub fn toggle_split_compare(&mut self) {
        self.split.toggle();
    }

    /// Cancels crop editing and resets all geometry tools.
    pub fn cancel_all_tools(&mut self) {
        self.crop.cancel();
        self.geometry_tools.reset_all();
    }
}
