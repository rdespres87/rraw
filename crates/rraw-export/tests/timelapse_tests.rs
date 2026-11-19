#![forbid(unsafe_code)]

use rraw_export::timelapse::{
    compute_deflicker_ev_offsets, compute_frame_log_luminance, format_sequence_filename,
    interpolate_keyframes, write_timelapse_manifest, TimelapseDeflickerConfig, TimelapseFrameRate,
    TimelapseKeyframe,
};

#[test]
fn test_frame_log_luminance_and_deflicker_suppresses_aperture_jitter() {
    // Simulate a 7-frame sequence with a smooth exposure ramp (-1.0 EV to +0.5 EV)
    // corrupted by alternating +/- 0.25 EV mechanical aperture flicker on frames 2 and 4.
    let true_ramp_ev = [-1.0_f32, -0.75, -0.50, -0.25, 0.0, 0.25, 0.50];
    let jitter_ev = [0.0_f32, 0.0, 0.25, 0.0, -0.25, 0.0, 0.0];

    let mut measured_log_luma = Vec::with_capacity(7);
    for (&base_ev, &jit_ev) in true_ramp_ev.iter().zip(jitter_ev.iter()) {
        let linear_val = 0.18_f32 * (base_ev + jit_ev).exp2();
        // 4 pixels RGB buffer
        let frame_rgb = vec![linear_val; 4 * 3];
        let log_y = compute_frame_log_luminance(&frame_rgb);
        measured_log_luma.push(log_y);
    }

    let cfg = TimelapseDeflickerConfig {
        enabled: true,
        window_radius: 2,
        strength: 1.0,
        max_correction_ev: 1.5,
    };

    let offsets = compute_deflicker_ev_offsets(&measured_log_luma, &cfg);
    assert_eq!(offsets.len(), 7);

    // Frame 2 had +0.25 EV spike -> offset must be negative to pull it down
    assert!(
        offsets[2] < -0.10,
        "Positive flicker spike must receive negative EV compensation, got {}",
        offsets[2]
    );
    // Frame 4 had -0.25 EV drop -> offset must be positive to pull it up
    assert!(
        offsets[4] > 0.10,
        "Negative flicker drop must receive positive EV compensation, got {}",
        offsets[4]
    );

    // Check variance of first differences before vs after deflicker
    let corrected: Vec<f32> = measured_log_luma
        .iter()
        .zip(offsets.iter())
        .map(|(l, o)| l + o)
        .collect();

    let raw_roughness: f32 = measured_log_luma
        .windows(3)
        .map(|w| ((w[2] - w[1]) - (w[1] - w[0])).abs())
        .sum();
    let smoothed_roughness: f32 = corrected
        .windows(3)
        .map(|w| ((w[2] - w[1]) - (w[1] - w[0])).abs())
        .sum();

    assert!(
        smoothed_roughness < raw_roughness * 0.45,
        "Deflickered sequence must be significantly smoother than raw jittered sequence ({smoothed_roughness} vs {raw_roughness})"
    );
}

#[test]
fn test_keyframe_ramping_interpolates_holy_grail_day_to_night() {
    let kf_day = TimelapseKeyframe {
        frame_index: 0,
        exposure_ev: 0.0,
        kelvin: 5600.0,
        tint: 0.0,
        contrast: 10.0,
        saturation: 0.0,
    };
    let kf_night = TimelapseKeyframe {
        frame_index: 4,
        exposure_ev: 2.0,
        kelvin: 3600.0,
        tint: -12.0,
        contrast: 20.0,
        saturation: 10.0,
    };

    let ramped = interpolate_keyframes(5, &[kf_night, kf_day]);
    assert_eq!(ramped.len(), 5);

    assert!((ramped[0].exposure_ev - 0.0).abs() < 1e-4);
    assert!((ramped[0].kelvin - 5600.0).abs() < 1e-2);

    // Midpoint (frame 2 of 0..=4) must be exact 50% blend
    assert!((ramped[2].exposure_ev - 1.0).abs() < 1e-3);
    assert!((ramped[2].kelvin - 4600.0).abs() < 1.0);
    assert!((ramped[2].tint - (-6.0)).abs() < 1e-3);

    assert!((ramped[4].exposure_ev - 2.0).abs() < 1e-4);
    assert!((ramped[4].kelvin - 3600.0).abs() < 1e-2);
}

#[test]
fn test_sequence_filename_and_manifest_generation() -> Result<(), Box<dyn std::error::Error>> {
    let name = format_sequence_filename("holy_grail", 42, "tiff");
    assert_eq!(name, "holy_grail_00042.tiff");

    let temp_dir = std::env::temp_dir().join("rraw_timelapse_manifest_test");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let offsets = vec![0.0, -0.08, 0.12, -0.03];
    let manifest_path = write_timelapse_manifest(
        &temp_dir,
        "holy_grail",
        "tiff",
        TimelapseFrameRate::Fps24,
        4,
        &offsets,
    )?;

    assert!(manifest_path.exists());
    let content = std::fs::read_to_string(&manifest_path)?;
    assert!(content.contains("\"fps\": 24"));
    assert!(content.contains("\"frame_count\": 4"));
    assert!(content.contains("holy_grail_%05d.tiff"));

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}
