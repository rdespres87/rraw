//! Dedicated domain controller managing batch export options, modals and timelapse sequence configuration.

use crate::state::{ExportBitDepth, ExportFormatChoice, ExportScaleMode, ExportTargetMode};
use std::path::PathBuf;

/// Mutually exclusive modal dialog state for the application shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveModalDialog {
    /// Export & timelapse modal dialog (`Cmd+Shift+E`).
    Export,
    /// Keyboard shortcuts cheat sheet modal (`F1`).
    Shortcuts,
    /// Application "About" version modal.
    About,
}

/// Timelapse-specific sequence export parameters (frame rate, temporal deflicker, Holy Grail ramping).
#[derive(Debug, Clone)]
pub struct TimelapseExportConfig {
    /// Frame rate for timelapse sequence export.
    pub fps: rraw_export::TimelapseFrameRate,
    /// Temporal exposure deflicker configuration for timelapse export.
    pub deflicker: rraw_export::TimelapseDeflickerConfig,
    /// Whether to interpolate exposure & white balance keyframes across the sequence.
    pub keyframe_ramping: bool,
    /// Base filename prefix for numbered timelapse sequence output (`<prefix>_00001.<ext>`).
    pub sequence_prefix: String,
}

impl Default for TimelapseExportConfig {
    fn default() -> Self {
        Self {
            fps: rraw_export::TimelapseFrameRate::Fps24,
            deflicker: rraw_export::TimelapseDeflickerConfig::default(),
            keyframe_ramping: false,
            sequence_prefix: "timelapse".to_string(),
        }
    }
}

/// Persistent user configuration for single, batch, and timelapse image exports.
#[derive(Debug, Clone)]
pub struct ExportBatchConfig {
    /// Selected output format for photo export.
    pub format: ExportFormatChoice,
    /// JPEG quality parameter (1-100).
    pub jpeg_quality: u8,
    /// Bit depth for raster export (8-bit or 16-bit).
    pub bit_depth: ExportBitDepth,
    /// Scaling mode applied during export.
    pub scale_mode: ExportScaleMode,
    /// Whether to compute and emit ISO 21496-1 Gain Map HDR metadata (`.hdrgm.xmp` + `.tmap`).
    pub hdr_gain_map: bool,
    /// Custom export destination folder, if chosen by user.
    pub custom_dir: Option<PathBuf>,
    /// Whether to create a subfolder in the photo directory upon export.
    pub use_subfolder: bool,
    /// Name of the subfolder for exported files.
    pub subfolder: String,
    /// Suffix appended to the exported filename.
    pub suffix: String,
    /// Whether export applies to all selected photos in batch or current photo.
    pub batch_selection: bool,
    /// Target export mode (Single photo, Batch selection, or Timelapse sequence).
    pub target_mode: ExportTargetMode,
    /// Timelapse sequence configuration.
    pub timelapse: TimelapseExportConfig,
}

impl Default for ExportBatchConfig {
    fn default() -> Self {
        Self {
            format: ExportFormatChoice::Jpeg,
            jpeg_quality: 92,
            bit_depth: ExportBitDepth::Bits8,
            scale_mode: ExportScaleMode::Original,
            hdr_gain_map: false,
            custom_dir: None,
            use_subfolder: true,
            subfolder: "Export".to_string(),
            suffix: String::new(),
            batch_selection: false,
            target_mode: ExportTargetMode::CurrentPhoto,
            timelapse: TimelapseExportConfig::default(),
        }
    }
}

/// Live asynchronous runtime telemetry for an active or completed background export job.
#[derive(Debug, Clone, Default)]
pub struct ExportRuntimeStats {
    /// Whether an asynchronous export background task is currently executing.
    pub is_exporting: bool,
    /// Progress fraction of background export task `[0.0, 1.0]`.
    pub progress: f32,
    /// Detailed status message displayed during export.
    pub progress_label: String,
    /// Status message from latest export operation: `(message, is_success)`.
    pub status: Option<(String, bool)>,
}

/// Batch export, modal dialogs and timelapse sequence controller.
#[derive(Debug, Clone)]
pub struct ExportBatchController {
    /// Active top-level modal dialog (mutually exclusive via typed transitions).
    pub active_dialog: Option<ActiveModalDialog>,
    /// Whether the export modal dialog is open.
    pub show_export_dialog: bool,
    /// Whether the keyboard shortcuts cheat sheet dialog is open.
    pub show_shortcuts_dialog: bool,
    /// Whether the "About" version dialog is open.
    pub show_about_dialog: bool,
    /// User-selected export format, destination, scaling, and timelapse settings.
    pub config: ExportBatchConfig,
    /// Asynchronous background export progress and completion status.
    pub stats: ExportRuntimeStats,
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
            active_dialog: None,
            show_export_dialog: false,
            show_shortcuts_dialog: false,
            show_about_dialog: false,
            config: ExportBatchConfig::default(),
            stats: ExportRuntimeStats::default(),
        }
    }

    /// Opens the requested modal dialog exclusively, closing any other active modal dialog.
    pub fn open_dialog(&mut self, dialog: ActiveModalDialog) {
        self.active_dialog = Some(dialog);
        self.show_export_dialog = dialog == ActiveModalDialog::Export;
        self.show_shortcuts_dialog = dialog == ActiveModalDialog::Shortcuts;
        self.show_about_dialog = dialog == ActiveModalDialog::About;
    }

    /// Toggles the requested modal dialog exclusively.
    pub fn toggle_dialog(&mut self, dialog: ActiveModalDialog) {
        if self.active_modal_dialog() == Some(dialog) {
            self.close_all_dialogs();
        } else {
            self.open_dialog(dialog);
        }
    }

    /// Returns the currently active modal dialog, reconciling with legacy boolean flags if mutated directly.
    #[must_use]
    pub fn active_modal_dialog(&self) -> Option<ActiveModalDialog> {
        if self.show_about_dialog {
            Some(ActiveModalDialog::About)
        } else if self.show_shortcuts_dialog {
            Some(ActiveModalDialog::Shortcuts)
        } else if self.show_export_dialog {
            Some(ActiveModalDialog::Export)
        } else {
            None
        }
    }

    /// Sets the export target mode and keeps the legacy `batch_selection` flag strictly synchronized.
    pub fn set_target_mode(&mut self, mode: ExportTargetMode) {
        self.config.target_mode = mode;
        self.config.batch_selection = matches!(
            mode,
            ExportTargetMode::BatchSelection | ExportTargetMode::TimelapseSequence
        );
    }

    /// Returns true if any modal dialog is currently open.
    #[must_use]
    pub fn is_any_dialog_open(&self) -> bool {
        self.active_modal_dialog().is_some()
    }

    /// Opens or closes a specific modal dialog while keeping `active_dialog` and boolean flags synchronized.
    pub fn set_dialog_open(&mut self, dialog: ActiveModalDialog, is_open: bool) {
        if is_open {
            self.open_dialog(dialog);
        } else {
            self.close_dialog(dialog);
        }
    }

    /// Closes a specific modal dialog and reconciles `active_dialog` with any remaining open dialog.
    pub fn close_dialog(&mut self, dialog: ActiveModalDialog) {
        match dialog {
            ActiveModalDialog::Export => self.show_export_dialog = false,
            ActiveModalDialog::Shortcuts => self.show_shortcuts_dialog = false,
            ActiveModalDialog::About => self.show_about_dialog = false,
        }
        self.active_dialog = self.active_modal_dialog();
    }

    /// Closes all open modal dialogs.
    pub fn close_all_dialogs(&mut self) {
        self.active_dialog = None;
        self.show_export_dialog = false;
        self.show_shortcuts_dialog = false;
        self.show_about_dialog = false;
    }
}
