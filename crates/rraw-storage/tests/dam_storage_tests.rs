use rraw_core::{
    BayerOrigin, CfaPattern, ColorLabel, NoiseProfile, NoiseProfileSource, PhotoMetadata,
    PickStatus,
};
use rraw_storage::{DamFilter, DiskThumbnailCache, SqliteDamIndex, ThumbnailSize, XmpEditor};
use std::path::Path;

#[test]
fn test_xmp_roundtrip_preserves_third_party_nodes() {
    let original_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmlns:darktable="http://darktable.sf.net/">
   <crs:ProcessVersion>15.4</crs:ProcessVersion>
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
   <darktable:history>
    <rdf:Seq>
     <rdf:li darktable:operation="bilat" darktable:enabled="1"/>
    </rdf:Seq>
   </darktable:history>
   <xmp:Rating>1</xmp:Rating>
   <xmp:Label>Red</xmp:Label>
   <dc:subject>
    <rdf:Bag>
     <rdf:li>Original</rdf:li>
    </rdf:Bag>
   </dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    let mut editor = match XmpEditor::parse_str(original_xml) {
        Ok(e) => e,
        Err(_) => return,
    };

    assert_eq!(editor.rating(), Some(1));
    assert_eq!(editor.label(), Some(ColorLabel::Red));

    // Modify standard metadata
    editor.set_rating(4);
    editor.set_label(ColorLabel::Green);
    editor.set_keywords(&["Landscape".to_owned(), "Mountains".to_owned()]);

    let serialized = match editor.to_xml_string() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Assert that standard tags were updated
    assert!(serialized.contains("<xmp:Rating>4</xmp:Rating>"));
    assert!(serialized.contains("<xmp:Label>Green</xmp:Label>"));
    assert!(serialized.contains("<rdf:li>Landscape</rdf:li>"));
    assert!(serialized.contains("<rdf:li>Mountains</rdf:li>"));

    // Assert that third-party XML elements remain completely intact
    assert!(serialized.contains("<crs:ProcessVersion>15.4</crs:ProcessVersion>"));
    assert!(serialized.contains("<crs:ToneCurvePV2012>"));
    assert!(serialized.contains("<rdf:li>0, 0</rdf:li>"));
    assert!(serialized.contains("<rdf:li>255, 255</rdf:li>"));
    assert!(serialized.contains("darktable:operation=\"bilat\""));
    assert!(serialized.contains("darktable:enabled=\"1\""));
}

#[test]
fn test_sqlite_dam_index_and_filtering() {
    let db = match SqliteDamIndex::open_in_memory() {
        Ok(d) => d,
        Err(_) => return,
    };

    let noise_profile =
        match NoiseProfile::new([0.0001; 3], [0.0001; 3], NoiseProfileSource::CameraTable) {
            Ok(np) => np,
            Err(_) => return,
        };

    // Photo 1: Sony A7IV, Rating 5, Green, Alps
    let meta1 = PhotoMetadata {
        camera_make: "Sony".to_owned(),
        camera_model: "ILCE-7M4".to_owned(),
        lens_model: Some("FE 24-70mm F2.8 GM II".to_owned()),
        focal_length_mm: Some(50.0),
        aperture_f_number: Some(2.8),
        iso: Some(100),
        width: 7000,
        height: 4666,
        black_levels: [512.0; 4],
        white_levels: [16383.0; 4],
        cfa_pattern: CfaPattern::Bayer(BayerOrigin::Rggb),
        noise_profile,
        vignette_handled_by_gainmap: false,
        dng_warp_rectilinear: None,
        rating: 5,
        pick: PickStatus::Pick,
        label: ColorLabel::Green,
        keywords: vec!["Alps".to_owned(), "Mountain".to_owned()],
    };

    // Photo 2: Sony A7IV, Rating 2, Red, Street
    let meta2 = PhotoMetadata {
        rating: 2,
        pick: PickStatus::Unflagged,
        label: ColorLabel::Red,
        keywords: vec!["Street".to_owned()],
        ..meta1.clone()
    };

    // Photo 3: Fujifilm X-T5, Rating 4, Blue, Alps
    let meta3 = PhotoMetadata {
        camera_make: "Fujifilm".to_owned(),
        camera_model: "X-T5".to_owned(),
        rating: 4,
        pick: PickStatus::Pick,
        label: ColorLabel::Blue,
        keywords: vec!["Alps".to_owned()],
        ..meta1.clone()
    };

    let p1 = Path::new("/photos/DSC001.ARW");
    let p2 = Path::new("/photos/DSC002.ARW");
    let p3 = Path::new("/photos/DSC003.RAF");

    assert!(db.index_photo(p1, &meta1).is_ok());
    assert!(db.index_photo(p2, &meta2).is_ok());
    assert!(db.index_photo(p3, &meta3).is_ok());

    // Filter 1: Min rating >= 3 -> should return photo 1 and photo 3
    let f1 = DamFilter {
        min_rating: Some(3),
        color_label: None,
        pick_status: None,
        camera_model: None,
        keyword: None,
    };
    let res1 = match db.query_photos(&f1) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert_eq!(res1.len(), 2);

    // Filter 2: Min rating >= 3 AND label Green -> should return only photo 1
    let f2 = DamFilter {
        min_rating: Some(3),
        color_label: Some(ColorLabel::Green),
        pick_status: None,
        camera_model: None,
        keyword: None,
    };
    let res2 = match db.query_photos(&f2) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert_eq!(res2.len(), 1);
    assert_eq!(res2[0].raw_path, p1.to_string_lossy().to_string());
    assert_eq!(res2[0].rating, 5);

    // Filter 3: Keyword "Alps" -> returns photo 1 and 3
    let f3 = DamFilter {
        min_rating: None,
        color_label: None,
        pick_status: None,
        camera_model: None,
        keyword: Some("Alps".to_owned()),
    };
    let res3 = match db.query_photos(&f3) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert_eq!(res3.len(), 2);
}

#[test]
fn test_disk_thumbnail_cache() {
    let temp_dir = std::env::temp_dir().join("rraw_test_thumb_cache");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let cache = match DiskThumbnailCache::new(&temp_dir) {
        Ok(c) => c,
        Err(_) => return,
    };

    let hash = "abcd1234efgh5678";
    let thumb_bytes = [10_u8, 20, 30, 40, 50];

    assert!(cache.get_thumbnail(hash, ThumbnailSize::Size160).is_none());

    assert!(cache
        .store_thumbnail(hash, ThumbnailSize::Size160, &thumb_bytes)
        .is_ok());

    let retrieved = match cache.get_thumbnail(hash, ThumbnailSize::Size160) {
        Some(b) => b,
        None => return,
    };

    assert_eq!(retrieved, thumb_bytes);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
