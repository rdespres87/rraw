use super::*;

#[test]
fn test_huff_tree_insert_and_decode() -> Result<(), Box<dyn std::error::Error>> {
    let mut tree = HuffTree::new();
    // '0' -> 0, '10' -> 1, '11' -> -1
    tree.insert(1, 0b0, 0)?;
    tree.insert(2, 0b10, 1)?;
    tree.insert(2, 0b11, -1)?;

    // Bitstream: 0_10_11_000 = 0b0101_1000 = 0x58
    let buf = [0x58u8];
    let mut reader = BitReader::new(&buf);
    assert_eq!(tree.decode(&mut reader)?, 0);
    assert_eq!(tree.decode(&mut reader)?, 1);
    assert_eq!(tree.decode(&mut reader)?, -1);
    Ok(())
}

#[test]
fn test_decode_true2_plane_constant_seed() -> Result<(), Box<dyn std::error::Error>> {
    // Tree where single bit '0' decodes to 0-bit diff (diff = 0).
    let mut tree = HuffTree::new();
    tree.insert(1, 0, 0)?;
    let zeros = [0u8; 4];
    let plane = decode_true2_plane(&zeros, 4, 4, 512, &tree)?;
    assert_eq!(plane, vec![512u16; 16]);
    Ok(())
}

#[test]
fn test_calibrate_foveon_neutral_ramp() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = Vec::with_capacity(100);
    for i in 0..100u16 {
        let v = 100 + i * 10;
        raw.push([v, v, v]);
    }
    let rgb = calibrate_foveon_to_linear_srgb(&raw, 10, 10, None, false)?;
    assert_eq!(rgb.width(), 10);
    assert_eq!(rgb.height(), 10);
    let mid = rgb.samples()[55];
    assert!(mid[0].is_finite() && mid[1].is_finite() && mid[2].is_finite());
    assert!((mid[0] - mid[1]).abs() < 0.05 && (mid[1] - mid[2]).abs() < 0.05);
    Ok(())
}

#[test]
fn test_try_decode_x3f_format5_and_errors() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let tf_bad =
        TempFile(std::env::temp_dir().join(format!("rraw_bad_{}.x3f", std::process::id())));
    std::fs::write(&tf_bad.0, b"NOPE")?;
    assert!(super::try_decode_x3f(&tf_bad.0).is_err());

    let mut file = Vec::new();
    file.extend_from_slice(b"FOVb");
    file.extend_from_slice(&[0u8; 36]);

    let ima_off: u32 = 40;
    let mut ima = Vec::new();
    ima.extend_from_slice(b"SECi");
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&1u32.to_le_bytes());
    ima.extend_from_slice(&5u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&0u32.to_le_bytes());
    for i in 0..1024 {
        let v: i16 = if i == 1 { 500 } else { 0 };
        ima.extend_from_slice(&v.to_le_bytes());
    }
    for row in 0..16 {
        for col in 0..16 {
            let px: u32 = if row == 0 && col == 0 { 0x0010_0401 } else { 0 };
            ima.extend_from_slice(&px.to_le_bytes());
        }
    }
    let ima_len = ima.len() as u32;
    file.extend_from_slice(&ima);

    let dir_off = file.len() as u32;
    file.extend_from_slice(b"SECd");
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&1u32.to_le_bytes());
    file.extend_from_slice(&ima_off.to_le_bytes());
    file.extend_from_slice(&ima_len.to_le_bytes());
    file.extend_from_slice(b"IMA2");
    file.extend_from_slice(&dir_off.to_le_bytes());

    let tf = TempFile(std::env::temp_dir().join(format!("rraw_ok_{}.x3f", std::process::id())));
    std::fs::write(&tf.0, &file)?;
    let raw = super::try_decode_x3f(&tf.0)?;
    assert_eq!(raw.metadata.width, 16);
    assert_eq!(raw.metadata.height, 16);
    assert!(raw.linear_rgb.is_some());
    Ok(())
}

#[test]
fn test_try_decode_x3f_missing_secd() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let tf = TempFile(std::env::temp_dir().join(format!("rraw_nosecd_{}.x3f", std::process::id())));
    let mut buf = Vec::new();
    buf.extend_from_slice(b"FOVb");
    buf.extend_from_slice(&[0u8; 36]);
    buf.extend_from_slice(&0u32.to_le_bytes());
    std::fs::write(&tf.0, &buf)?;
    assert!(super::try_decode_x3f(&tf.0).is_err());
    Ok(())
}

#[test]
fn test_try_decode_x3f_format6_huffman() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let mut file = Vec::new();
    file.extend_from_slice(b"FOVb");
    file.extend_from_slice(&[0u8; 36]);

    let ima_off: u32 = 40;
    let mut ima = Vec::new();
    ima.extend_from_slice(b"SECi");
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&1u32.to_le_bytes());
    ima.extend_from_slice(&6u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&0u32.to_le_bytes());
    for i in 0..1024 {
        let v: i16 = if i == 0 { 100 } else { 0 };
        ima.extend_from_slice(&v.to_le_bytes());
    }
    for i in 0..1024 {
        let elem: u32 = if i == 0 { 1u32 << 27 } else { 0 };
        ima.extend_from_slice(&elem.to_le_bytes());
    }
    ima.extend_from_slice(&[0u8; 16 * 8]);
    for _ in 0..16 {
        ima.extend_from_slice(&0u32.to_le_bytes());
    }
    let ima_len = ima.len() as u32;
    file.extend_from_slice(&ima);

    let dir_off = file.len() as u32;
    file.extend_from_slice(b"SECd");
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&1u32.to_le_bytes());
    file.extend_from_slice(&ima_off.to_le_bytes());
    file.extend_from_slice(&ima_len.to_le_bytes());
    file.extend_from_slice(b"IMA2");
    file.extend_from_slice(&dir_off.to_le_bytes());

    let tf = TempFile(std::env::temp_dir().join(format!("rraw_f6_{}.x3f", std::process::id())));
    std::fs::write(&tf.0, &file)?;
    let raw = super::try_decode_x3f(&tf.0)?;
    assert_eq!(raw.metadata.width, 16);
    assert_eq!(raw.metadata.height, 16);
    assert!(raw.linear_rgb.is_some());
    Ok(())
}

#[test]
fn test_try_decode_x3f_true2_format30_and_prop() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"FOVb");
    buf.extend_from_slice(&[0u8; 36]);

    let prop_off: u32 = 40;
    let mut prop: Vec<u8> = Vec::new();
    prop.extend_from_slice(b"SECp");
    prop.extend_from_slice(&0u32.to_le_bytes());
    prop.extend_from_slice(&1u32.to_le_bytes());
    prop.extend_from_slice(&0u32.to_le_bytes());
    prop.extend_from_slice(&0u32.to_le_bytes());
    prop.extend_from_slice(&0u32.to_le_bytes());
    prop.extend_from_slice(&0u32.to_le_bytes());
    prop.extend_from_slice(&9u32.to_le_bytes());
    for c in "CAMMANUF\0SIGMA\0".encode_utf16() {
        prop.extend_from_slice(&c.to_le_bytes());
    }
    let prop_len = prop.len() as u32;
    buf.extend_from_slice(&prop);

    let ima_off = prop_off + prop_len;
    let mut ima: Vec<u8> = Vec::new();
    ima.extend_from_slice(b"SECi");
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&1u32.to_le_bytes());
    ima.extend_from_slice(&30u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        ima.extend_from_slice(&512u16.to_le_bytes());
    }
    ima.extend_from_slice(&[0u8; 2]);
    ima.extend_from_slice(&[1u8, 0u8, 0u8, 0u8]);
    for _ in 0..3 {
        ima.extend_from_slice(&32u32.to_le_bytes());
    }
    for _ in 0..3 {
        ima.extend_from_slice(&[0u8; 32]);
    }
    buf.extend_from_slice(&ima);

    let dir_off = ima_off + ima.len() as u32;
    buf.extend_from_slice(b"SECd");
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&2u32.to_le_bytes());
    buf.extend_from_slice(&prop_off.to_le_bytes());
    buf.extend_from_slice(&prop_len.to_le_bytes());
    buf.extend_from_slice(b"PROP");
    buf.extend_from_slice(&ima_off.to_le_bytes());
    buf.extend_from_slice(&(ima.len() as u32).to_le_bytes());
    buf.extend_from_slice(b"IMA2");
    buf.extend_from_slice(&dir_off.to_le_bytes());

    let tf = TempFile(std::env::temp_dir().join(format!("rraw_t2_{}.x3f", std::process::id())));
    std::fs::write(&tf.0, &buf)?;
    let raw = super::try_decode_x3f(&tf.0)?;
    assert_eq!(raw.metadata.camera_make, "SIGMA");
    assert_eq!(raw.metadata.width, 16);
    assert_eq!(raw.metadata.height, 16);
    assert!(raw.linear_rgb.is_some());
    Ok(())
}

#[test]
fn test_try_decode_x3f_quattro_format35() -> Result<(), Box<dyn std::error::Error>> {
    struct TempFile(std::path::PathBuf);
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"FOVb");
    buf.extend_from_slice(&[0u8; 36]);

    let ima_off: u32 = 40;
    let mut ima: Vec<u8> = Vec::new();
    ima.extend_from_slice(b"SECi");
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&1u32.to_le_bytes());
    ima.extend_from_slice(&35u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&0u32.to_le_bytes());
    for (cw, ch) in [(8u16, 8u16), (8, 8), (16, 16)] {
        ima.extend_from_slice(&cw.to_le_bytes());
        ima.extend_from_slice(&ch.to_le_bytes());
    }
    for _ in 0..3 {
        ima.extend_from_slice(&512u16.to_le_bytes());
    }
    ima.extend_from_slice(&[0u8; 2]);
    ima.extend_from_slice(&[1u8, 0u8, 0u8, 0u8]);
    ima.extend_from_slice(&0u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&16u32.to_le_bytes());
    ima.extend_from_slice(&32u32.to_le_bytes());
    ima.extend_from_slice(&[0u8; 16]);
    ima.extend_from_slice(&[0u8; 16]);
    ima.extend_from_slice(&[0u8; 32]);
    let ima_len = ima.len() as u32;
    buf.extend_from_slice(&ima);

    let dir_off = buf.len() as u32;
    buf.extend_from_slice(b"SECd");
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&ima_off.to_le_bytes());
    buf.extend_from_slice(&ima_len.to_le_bytes());
    buf.extend_from_slice(b"IMA2");
    buf.extend_from_slice(&dir_off.to_le_bytes());

    let tf = TempFile(std::env::temp_dir().join(format!("rraw_q35_{}.x3f", std::process::id())));
    std::fs::write(&tf.0, &buf)?;
    let raw = super::try_decode_x3f(&tf.0)?;
    assert_eq!(raw.metadata.width, 16);
    assert_eq!(raw.metadata.height, 16);
    assert!(raw.linear_rgb.is_some());
    Ok(())
}

#[test]
fn test_extract_camf_raw_to_srgb_synthetic() {
    let name = b"WhiteBalanceColorCorrections\0";
    let mut block = Vec::new();
    block.extend_from_slice(b"CMbM");
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&0u32.to_le_bytes());
    block.extend_from_slice(&20u32.to_le_bytes());
    block.extend_from_slice(&(20u32 + name.len() as u32).to_le_bytes());
    block.extend_from_slice(name);
    block.extend_from_slice(&3u32.to_le_bytes());
    block.extend_from_slice(&2u32.to_le_bytes());
    block.extend_from_slice(&12u32.to_le_bytes());
    for v in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        block.extend_from_slice(&v.to_le_bytes());
    }
    let entry_size = block.len() as u32;
    block[8..12].copy_from_slice(&entry_size.to_le_bytes());

    let mut key = 0u32;
    let mut enc = block;
    for b in &mut enc {
        key = (key.wrapping_mul(1597).wrapping_add(51749)) % 244_944;
        let tmp = ((u64::from(key) * 301_593_171) >> 24) as u32;
        let mask = (((((key << 8).wrapping_sub(tmp)) >> 1).wrapping_add(tmp)) >> 17) as u8;
        *b ^= mask;
    }

    let mut sec = Vec::new();
    sec.extend_from_slice(b"SECc");
    sec.extend_from_slice(&0u32.to_le_bytes());
    sec.extend_from_slice(&2u32.to_le_bytes());
    sec.extend_from_slice(&0u32.to_le_bytes());
    sec.extend_from_slice(&[0u8; 12]);
    sec.extend_from_slice(&enc);

    let mat = extract_camf_raw_to_srgb(
        &sec,
        &DirEntry {
            offset: 0,
            length: sec.len(),
            tag: *b"CAMF",
        },
        "Sunlight",
    );
    assert!(mat.is_some());
}
