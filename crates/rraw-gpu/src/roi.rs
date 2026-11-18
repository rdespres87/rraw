/// Bounding rectangular sub-window on an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoiWindow {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Expands a Region of Interest (RoI) by a halo margin, safely clamping to image boundaries.
///
/// The 32px halo guarantees that 2D filtering operations (wavelets, demosaicing,
/// Richardson-Lucy iterations) do not exhibit boundary bleeding within the visible RoI.
#[must_use]
pub fn expand_roi_with_halo(
    roi: &RoiWindow,
    halo_px: u32,
    image_width: u32,
    image_height: u32,
) -> RoiWindow {
    let x = roi.x.saturating_sub(halo_px);
    let y = roi.y.saturating_sub(halo_px);

    let right = (roi.x.saturating_add(roi.width).saturating_add(halo_px)).min(image_width);
    let bottom = (roi.y.saturating_add(roi.height).saturating_add(halo_px)).min(image_height);

    let width = right.saturating_sub(x);
    let height = bottom.saturating_sub(y);

    RoiWindow {
        x,
        y,
        width,
        height,
    }
}
