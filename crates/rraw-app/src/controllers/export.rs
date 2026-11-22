//! Dedicated domain controller managing batch export options, modals and timelapse sequence configuration.

use crate::state::{ExportBitDepth, ExportFormatChoice, ExportScaleMode, ExportTargetMode};
use std::path::PathBuf;

/// Batch export, modal dialogs and timelapse sequence controller.
#[derive(Debug, Clone)]
pub struct ExportBatchController {
    /// Whether the export modal dialog is open.
    pub show_export_dialog: bool,
    /// Whether the keyboard shortcuts cheat sheet dialog is open.
    pub show_shortcuts_dialog: bool,
    /// Whether the "About" version dialog is open.
    pub show_about_dialog: bool,
    /// Selected output format for photo export.
    pub export_format: ExportFormatChoice,
    /// JPEG quality parameter (1-100).
    pub export_jpeg_quality: u8,
    /// Bit depth for raster export (8-bit or 16-bit).
    pub export_bit_depth: ExportBitDepth,
    /// Scaling mode applied during export.
    pub export_scale_mode: ExportScaleMode,
    /// Custom export destination folder, if chosen by user.
    pub export_custom_dir: Option<PathBuf>,
    /// Status message from latest export operation: (message, is_success).
    pub export_status: Option<(String, bool)>,
    /// Whether to create a subfolder in the photo directory upon export.
    pub export_use_subfolder: bool,
    /// Name of the subfolder for exported files.
    pub export_subfolder: String,
    /// Suffix appended to the exported filename.
    pub export_suffix: String,
    /// Whether export applies to all selected photos in batch or current photo.
    pub export_batch_selection: bool,
    /// Target export mode (Single photo, Batch selection, or Timelapse sequence).
    pub export_target_mode: ExportTargetMode,
    /// Frame rate for timelapse sequence export.
    pub timelapse_fps: rraw_export::TimelapseFrameRate,
    /// Temporal exposure deflicker configuration for timelapse export.
    pub timelapse_deflicker: rraw_export::TimelapseDeflickerConfig,
    /// Whether to interpolate exposure & white balance keyframes across the sequence.
    pub timelapse_keyframe_ramping: bool,
    /// Base filename prefix for numbered timelapse sequence output (`<prefix>_00001.<ext>`).
    pub timelapse_sequence_prefix: String,
    /// Whether an asynchronous export background task is currently executing.
    pub is_exporting: bool,
    /// Progress fraction of background export task [0.0, 1.0].
    pub export_progress: f32,
    /// Detailed status message displayed during export.
    pub export_progress_label: String,
}

impl Default for ExportBatchController {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportBatchController {
    /// Creates a default export batch controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            show_export_dialog: false,
            show_shortcuts_dialog: false,
            show_about_dialog: false,
            export_format: ExportFormatChoice::Jpeg,
            export_jpeg_quality: 92,
            export_bit_depth: ExportBitDepth::Bits8,
            export_scale_mode: ExportScaleMode::Original,
            export_custom_dir: None,
            export_status: None,
            export_use_subfolder: true,
            export_subfolder: "Export".to_string(),
            export_suffix: String::new(),
            export_batch_selection: false,
            export_target_mode: ExportTargetMode::CurrentPhoto,
            timelapse_fps: rraw_export::TimelapseFrameRate::Fps24,
            timelapse_deflicker: rraw_export::TimelapseDeflickerConfig::default(),
            timelapse_keyframe_ramping: false,
            timelapse_sequence_prefix: "timelapse".to_string(),
            is_exporting: false,
            export_progress: 0.0,
            export_progress_label: String::new(),
        }
    }

    /// Returns true if any modal dialog is currently open.
    #[must_use]
    pub fn is_any_dialog_open(&self) -> bool {
        self.show_export_dialog || self.show_shortcuts_dialog || self.show_about_dialog
    }

    /// Closes all open modal dialogs.
    pub fn close_all_dialogs(&mut self) {
        self.show_export_dialog = false;
        self.show_shortcuts_dialog = false;
        self.show_about_dialog = false;
    }
}
