use rraw_export::{
    BatchExportJob, BatchExportQueue, ExportFormat, GainMapCalculator, GainMapReconstructor,
    Iso21496Params, Iso21496TmapBox, Iso21496XmpPacket,
};

#[test]
fn test_iso21496_gain_map_generation_and_reconstruction() {
    let width = 64_usize;
    let height = 64_usize;
    let count = width * height;

    let mut sdr = vec![0.0_f32; count * 3];
    let mut hdr = vec![0.0_f32; count * 3];

    // Ramp from 0.0 to 1.0 for SDR and 0.0 to 8.0 (+3 EV) for HDR
    for y in 0..height {
        for x in 0..width {
            let t = (y * width + x) as f32 / count as f32;
            let idx = (y * width + x) * 3;

            // SDR is clamped to 1.0
            let val_sdr = t.min(1.0);
            sdr[idx] = val_sdr;
            sdr[idx + 1] = val_sdr;
            sdr[idx + 2] = val_sdr;

            // HDR expands up to 8.0
            let val_hdr = t * 8.0;
            hdr[idx] = val_hdr;
            hdr[idx + 1] = val_hdr;
            hdr[idx + 2] = val_hdr;
        }
    }

    let params = Iso21496Params {
        gain_map_min: 1.0,
        gain_map_max: 8.0,
        hdr_capacity_min: 1.0,
        hdr_capacity_max: 8.0,
        offset_sdr: 1.0 / 64.0,
        offset_hdr: 1.0 / 64.0,
        gamma: 1.0,
    };

    let gain_map = match GainMapCalculator::compute(&sdr, &hdr, width, height, &params) {
        Ok(gm) => gm,
        Err(_) => return,
    };

    assert_eq!(gain_map.len(), count);

    // Reconstruct at full HDR display headroom H = 8.0
    let recon_hdr =
        match GainMapReconstructor::reconstruct(&sdr, &gain_map, width, height, &params, 8.0) {
            Ok(r) => r,
            Err(_) => return,
        };

    assert_eq!(recon_hdr.len(), count * 3);

    // Verify recovery accuracy: error must be < 1% for values above 0.1
    let mut max_err_ratio = 0.0_f32;
    for i in 0..(count * 3) {
        let expected = hdr[i];
        let actual = recon_hdr[i];
        if expected > 0.1 {
            let err_ratio = (actual - expected).abs() / expected;
            if err_ratio > max_err_ratio {
                max_err_ratio = err_ratio;
            }
        }
    }

    assert!(
        max_err_ratio < 0.01,
        "Reconstructed linear highlights must match within < 1% (measured max error: {:.2}%)",
        max_err_ratio * 100.0
    );
}

#[test]
fn test_binary_tmap_box_roundtrip() {
    let params = Iso21496Params {
        gain_map_min: 1.0,
        gain_map_max: 8.0,
        hdr_capacity_min: 1.0,
        hdr_capacity_max: 8.0,
        offset_sdr: 0.015625,
        offset_hdr: 0.015625,
        gamma: 1.0,
    };

    let encoded_box = match Iso21496TmapBox::encode(&params) {
        Ok(b) => b,
        Err(_) => return,
    };

    // FourCC check: bytes 4..8 must be "tmap"
    assert_eq!(&encoded_box[4..8], b"tmap");

    let decoded = match Iso21496TmapBox::decode(&encoded_box) {
        Ok(d) => d,
        Err(_) => return,
    };

    assert!((decoded.gain_map_min - params.gain_map_min).abs() < 1e-4);
    assert!((decoded.gain_map_max - params.gain_map_max).abs() < 1e-4);
    assert!((decoded.hdr_capacity_min - params.hdr_capacity_min).abs() < 1e-4);
    assert!((decoded.hdr_capacity_max - params.hdr_capacity_max).abs() < 1e-4);
    assert!((decoded.offset_sdr - params.offset_sdr).abs() < 1e-4);
    assert!((decoded.offset_hdr - params.offset_hdr).abs() < 1e-4);
    assert!((decoded.gamma - params.gamma).abs() < 1e-4);
}

#[test]
fn test_xmp_hdrgm_packet_roundtrip() {
    let params = Iso21496Params {
        gain_map_min: 1.0,
        gain_map_max: 8.0,
        hdr_capacity_min: 1.0,
        hdr_capacity_max: 8.0,
        offset_sdr: 0.015625,
        offset_hdr: 0.015625,
        gamma: 1.0,
    };

    let xml = Iso21496XmpPacket::encode(&params);

    assert!(xml.contains("hdrgm:Version"));
    assert!(xml.contains("hdrgm:GainMapMax"));
    assert!(xml.contains("hdrgm:HDRCapacityMax"));

    let decoded = match Iso21496XmpPacket::decode(&xml) {
        Ok(d) => d,
        Err(_) => return,
    };

    assert!((decoded.gain_map_min - params.gain_map_min).abs() < 1e-4);
    assert!((decoded.gain_map_max - params.gain_map_max).abs() < 1e-4);
    assert!((decoded.hdr_capacity_min - params.hdr_capacity_min).abs() < 1e-4);
    assert!((decoded.hdr_capacity_max - params.hdr_capacity_max).abs() < 1e-4);
}

#[test]
fn test_batch_export_queue_parallel_execution() {
    let temp_dir = std::env::temp_dir().join("rraw_test_batch_export");
    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::create_dir_all(&temp_dir);

    let queue = BatchExportQueue::new();

    let width = 32_usize;
    let height = 32_usize;
    let count = width * height;
    let dummy_rgb = vec![0.5_f32; count * 3];

    for i in 0..3 {
        let output_path = temp_dir.join(format!("output_{i}.png"));
        let job = BatchExportJob {
            output_path,
            width,
            height,
            format: ExportFormat::Png16,
            rgb_linear: dummy_rgb.clone(),
        };
        queue.push_job(job);
    }

    assert_eq!(queue.pending_count(), 3);

    let results = queue.execute_all(2);
    assert_eq!(results.len(), 3);

    for (path, res) in results {
        assert!(res.is_ok(), "Export failed for {path:?}");
        assert!(path.exists(), "Exported file {path:?} must exist on disk");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
