//! Color and weight conversion utilities for DXF output.

/// Full AutoCAD Color Index (ACI) palette, indices 1-255, as RGB triples.
/// This is the standard DXF color table (same values used by AutoCAD,
/// LibreCAD, dxflib and ezdxf), not something specific to cadspec.
const ACI_PALETTE: [(u8, (u8, u8, u8)); 255] = [
    (1, (0xFF, 0x00, 0x00)),
    (2, (0xFF, 0xFF, 0x00)),
    (3, (0x00, 0xFF, 0x00)),
    (4, (0x00, 0xFF, 0xFF)),
    (5, (0x00, 0x00, 0xFF)),
    (6, (0xFF, 0x00, 0xFF)),
    (7, (0xFF, 0xFF, 0xFF)),
    (8, (0x80, 0x80, 0x80)),
    (9, (0xC0, 0xC0, 0xC0)),
    (10, (0xFF, 0x00, 0x00)),
    (11, (0xFF, 0x7F, 0x7F)),
    (12, (0xA5, 0x00, 0x00)),
    (13, (0xA5, 0x52, 0x52)),
    (14, (0x7F, 0x00, 0x00)),
    (15, (0x7F, 0x3F, 0x3F)),
    (16, (0x4C, 0x00, 0x00)),
    (17, (0x4C, 0x26, 0x26)),
    (18, (0x26, 0x00, 0x00)),
    (19, (0x26, 0x13, 0x13)),
    (20, (0xFF, 0x3F, 0x00)),
    (21, (0xFF, 0x9F, 0x7F)),
    (22, (0xA5, 0x29, 0x00)),
    (23, (0xA5, 0x67, 0x52)),
    (24, (0x7F, 0x1F, 0x00)),
    (25, (0x7F, 0x4F, 0x3F)),
    (26, (0x4C, 0x13, 0x00)),
    (27, (0x4C, 0x2F, 0x26)),
    (28, (0x26, 0x09, 0x00)),
    (29, (0x26, 0x17, 0x13)),
    (30, (0xFF, 0x7F, 0x00)),
    (31, (0xFF, 0xBF, 0x7F)),
    (32, (0xA5, 0x52, 0x00)),
    (33, (0xA5, 0x7C, 0x52)),
    (34, (0x7F, 0x3F, 0x00)),
    (35, (0x7F, 0x5F, 0x3F)),
    (36, (0x4C, 0x26, 0x00)),
    (37, (0x4C, 0x39, 0x26)),
    (38, (0x26, 0x13, 0x00)),
    (39, (0x26, 0x1C, 0x13)),
    (40, (0xFF, 0xBF, 0x00)),
    (41, (0xFF, 0xDF, 0x7F)),
    (42, (0xA5, 0x7C, 0x00)),
    (43, (0xA5, 0x91, 0x52)),
    (44, (0x7F, 0x5F, 0x00)),
    (45, (0x7F, 0x6F, 0x3F)),
    (46, (0x4C, 0x39, 0x00)),
    (47, (0x4C, 0x42, 0x26)),
    (48, (0x26, 0x1C, 0x00)),
    (49, (0x26, 0x21, 0x13)),
    (50, (0xFF, 0xFF, 0x00)),
    (51, (0xFF, 0xFF, 0x7F)),
    (52, (0xA5, 0xA5, 0x00)),
    (53, (0xA5, 0xA5, 0x52)),
    (54, (0x7F, 0x7F, 0x00)),
    (55, (0x7F, 0x7F, 0x3F)),
    (56, (0x4C, 0x4C, 0x00)),
    (57, (0x4C, 0x4C, 0x26)),
    (58, (0x26, 0x26, 0x00)),
    (59, (0x26, 0x26, 0x13)),
    (60, (0xBF, 0xFF, 0x00)),
    (61, (0xDF, 0xFF, 0x7F)),
    (62, (0x7C, 0xA5, 0x00)),
    (63, (0x91, 0xA5, 0x52)),
    (64, (0x5F, 0x7F, 0x00)),
    (65, (0x6F, 0x7F, 0x3F)),
    (66, (0x39, 0x4C, 0x00)),
    (67, (0x42, 0x4C, 0x26)),
    (68, (0x1C, 0x26, 0x00)),
    (69, (0x21, 0x26, 0x13)),
    (70, (0x7F, 0xFF, 0x00)),
    (71, (0xBF, 0xFF, 0x7F)),
    (72, (0x52, 0xA5, 0x00)),
    (73, (0x7C, 0xA5, 0x52)),
    (74, (0x3F, 0x7F, 0x00)),
    (75, (0x5F, 0x7F, 0x3F)),
    (76, (0x26, 0x4C, 0x00)),
    (77, (0x39, 0x4C, 0x26)),
    (78, (0x13, 0x26, 0x00)),
    (79, (0x1C, 0x26, 0x13)),
    (80, (0x3F, 0xFF, 0x00)),
    (81, (0x9F, 0xFF, 0x7F)),
    (82, (0x29, 0xA5, 0x00)),
    (83, (0x67, 0xA5, 0x52)),
    (84, (0x1F, 0x7F, 0x00)),
    (85, (0x4F, 0x7F, 0x3F)),
    (86, (0x13, 0x4C, 0x00)),
    (87, (0x2F, 0x4C, 0x26)),
    (88, (0x09, 0x26, 0x00)),
    (89, (0x17, 0x26, 0x13)),
    (90, (0x00, 0xFF, 0x00)),
    (91, (0x7F, 0xFF, 0x7F)),
    (92, (0x00, 0xA5, 0x00)),
    (93, (0x52, 0xA5, 0x52)),
    (94, (0x00, 0x7F, 0x00)),
    (95, (0x3F, 0x7F, 0x3F)),
    (96, (0x00, 0x4C, 0x00)),
    (97, (0x26, 0x4C, 0x26)),
    (98, (0x00, 0x26, 0x00)),
    (99, (0x13, 0x26, 0x13)),
    (100, (0x00, 0xFF, 0x3F)),
    (101, (0x7F, 0xFF, 0x9F)),
    (102, (0x00, 0xA5, 0x29)),
    (103, (0x52, 0xA5, 0x67)),
    (104, (0x00, 0x7F, 0x1F)),
    (105, (0x3F, 0x7F, 0x4F)),
    (106, (0x00, 0x4C, 0x13)),
    (107, (0x26, 0x4C, 0x2F)),
    (108, (0x00, 0x26, 0x09)),
    (109, (0x13, 0x58, 0x17)),
    (110, (0x00, 0xFF, 0x7F)),
    (111, (0x7F, 0xFF, 0xBF)),
    (112, (0x00, 0xA5, 0x52)),
    (113, (0x52, 0xA5, 0x7C)),
    (114, (0x00, 0x7F, 0x3F)),
    (115, (0x3F, 0x7F, 0x5F)),
    (116, (0x00, 0x4C, 0x26)),
    (117, (0x26, 0x4C, 0x39)),
    (118, (0x00, 0x26, 0x13)),
    (119, (0x13, 0x58, 0x1C)),
    (120, (0x00, 0xFF, 0xBF)),
    (121, (0x7F, 0xFF, 0xDF)),
    (122, (0x00, 0xA5, 0x7C)),
    (123, (0x52, 0xA5, 0x91)),
    (124, (0x00, 0x7F, 0x5F)),
    (125, (0x3F, 0x7F, 0x6F)),
    (126, (0x00, 0x4C, 0x39)),
    (127, (0x26, 0x4C, 0x42)),
    (128, (0x00, 0x26, 0x1C)),
    (129, (0x13, 0x58, 0x58)),
    (130, (0x00, 0xFF, 0xFF)),
    (131, (0x7F, 0xFF, 0xFF)),
    (132, (0x00, 0xA5, 0xA5)),
    (133, (0x52, 0xA5, 0xA5)),
    (134, (0x00, 0x7F, 0x7F)),
    (135, (0x3F, 0x7F, 0x7F)),
    (136, (0x00, 0x4C, 0x4C)),
    (137, (0x26, 0x4C, 0x4C)),
    (138, (0x00, 0x26, 0x26)),
    (139, (0x13, 0x58, 0x58)),
    (140, (0x00, 0xBF, 0xFF)),
    (141, (0x7F, 0xDF, 0xFF)),
    (142, (0x00, 0x7C, 0xA5)),
    (143, (0x52, 0x91, 0xA5)),
    (144, (0x00, 0x5F, 0x7F)),
    (145, (0x3F, 0x6F, 0x7F)),
    (146, (0x00, 0x39, 0x4C)),
    (147, (0x26, 0x42, 0x7E)),
    (148, (0x00, 0x1C, 0x26)),
    (149, (0x13, 0x58, 0x58)),
    (150, (0x00, 0x7F, 0xFF)),
    (151, (0x7F, 0xBF, 0xFF)),
    (152, (0x00, 0x52, 0xA5)),
    (153, (0x52, 0x7C, 0xA5)),
    (154, (0x00, 0x3F, 0x7F)),
    (155, (0x3F, 0x5F, 0x7F)),
    (156, (0x00, 0x26, 0x4C)),
    (157, (0x26, 0x39, 0x7E)),
    (158, (0x00, 0x13, 0x26)),
    (159, (0x13, 0x1C, 0x58)),
    (160, (0x00, 0x3F, 0xFF)),
    (161, (0x7F, 0x9F, 0xFF)),
    (162, (0x00, 0x29, 0xA5)),
    (163, (0x52, 0x67, 0xA5)),
    (164, (0x00, 0x1F, 0x7F)),
    (165, (0x3F, 0x4F, 0x7F)),
    (166, (0x00, 0x13, 0x4C)),
    (167, (0x26, 0x2F, 0x7E)),
    (168, (0x00, 0x09, 0x26)),
    (169, (0x13, 0x17, 0x58)),
    (170, (0x00, 0x00, 0xFF)),
    (171, (0x7F, 0x7F, 0xFF)),
    (172, (0x00, 0x00, 0xA5)),
    (173, (0x52, 0x52, 0xA5)),
    (174, (0x00, 0x00, 0x7F)),
    (175, (0x3F, 0x3F, 0x7F)),
    (176, (0x00, 0x00, 0x4C)),
    (177, (0x26, 0x26, 0x7E)),
    (178, (0x00, 0x00, 0x26)),
    (179, (0x13, 0x13, 0x58)),
    (180, (0x3F, 0x00, 0xFF)),
    (181, (0x9F, 0x7F, 0xFF)),
    (182, (0x29, 0x00, 0xA5)),
    (183, (0x67, 0x52, 0xA5)),
    (184, (0x1F, 0x00, 0x7F)),
    (185, (0x4F, 0x3F, 0x7F)),
    (186, (0x13, 0x00, 0x4C)),
    (187, (0x2F, 0x26, 0x7E)),
    (188, (0x09, 0x00, 0x26)),
    (189, (0x17, 0x13, 0x58)),
    (190, (0x7F, 0x00, 0xFF)),
    (191, (0xBF, 0x7F, 0xFF)),
    (192, (0x52, 0x00, 0xA5)),
    (193, (0x7C, 0x52, 0xA5)),
    (194, (0x3F, 0x00, 0x7F)),
    (195, (0x5F, 0x3F, 0x7F)),
    (196, (0x26, 0x00, 0x4C)),
    (197, (0x39, 0x26, 0x7E)),
    (198, (0x13, 0x00, 0x26)),
    (199, (0x1C, 0x13, 0x58)),
    (200, (0xBF, 0x00, 0xFF)),
    (201, (0xDF, 0x7F, 0xFF)),
    (202, (0x7C, 0x00, 0xA5)),
    (203, (0x91, 0x52, 0xA5)),
    (204, (0x5F, 0x00, 0x7F)),
    (205, (0x6F, 0x3F, 0x7F)),
    (206, (0x39, 0x00, 0x4C)),
    (207, (0x42, 0x26, 0x4C)),
    (208, (0x1C, 0x00, 0x26)),
    (209, (0x58, 0x13, 0x58)),
    (210, (0xFF, 0x00, 0xFF)),
    (211, (0xFF, 0x7F, 0xFF)),
    (212, (0xA5, 0x00, 0xA5)),
    (213, (0xA5, 0x52, 0xA5)),
    (214, (0x7F, 0x00, 0x7F)),
    (215, (0x7F, 0x3F, 0x7F)),
    (216, (0x4C, 0x00, 0x4C)),
    (217, (0x4C, 0x26, 0x4C)),
    (218, (0x26, 0x00, 0x26)),
    (219, (0x58, 0x13, 0x58)),
    (220, (0xFF, 0x00, 0xBF)),
    (221, (0xFF, 0x7F, 0xDF)),
    (222, (0xA5, 0x00, 0x7C)),
    (223, (0xA5, 0x52, 0x91)),
    (224, (0x7F, 0x00, 0x5F)),
    (225, (0x7F, 0x3F, 0x6F)),
    (226, (0x4C, 0x00, 0x39)),
    (227, (0x4C, 0x26, 0x42)),
    (228, (0x26, 0x00, 0x1C)),
    (229, (0x58, 0x13, 0x58)),
    (230, (0xFF, 0x00, 0x7F)),
    (231, (0xFF, 0x7F, 0xBF)),
    (232, (0xA5, 0x00, 0x52)),
    (233, (0xA5, 0x52, 0x7C)),
    (234, (0x7F, 0x00, 0x3F)),
    (235, (0x7F, 0x3F, 0x5F)),
    (236, (0x4C, 0x00, 0x26)),
    (237, (0x4C, 0x26, 0x39)),
    (238, (0x26, 0x00, 0x13)),
    (239, (0x58, 0x13, 0x1C)),
    (240, (0xFF, 0x00, 0x3F)),
    (241, (0xFF, 0x7F, 0x9F)),
    (242, (0xA5, 0x00, 0x29)),
    (243, (0xA5, 0x52, 0x67)),
    (244, (0x7F, 0x00, 0x1F)),
    (245, (0x7F, 0x3F, 0x4F)),
    (246, (0x4C, 0x00, 0x13)),
    (247, (0x4C, 0x26, 0x2F)),
    (248, (0x26, 0x00, 0x09)),
    (249, (0x58, 0x13, 0x17)),
    (250, (0x00, 0x00, 0x00)),
    (251, (0x65, 0x65, 0x65)),
    (252, (0x66, 0x66, 0x66)),
    (253, (0x99, 0x99, 0x99)),
    (254, (0xCC, 0xCC, 0xCC)),
    (255, (0xFF, 0xFF, 0xFF)),
];

/// ACI color index from hex string: nearest color in the full 255-entry
/// ACI palette (unparseable input falls back to 7, white).
pub fn hex_to_aci(hex: &str) -> u8 {
    let hex = hex.trim_start_matches('#');
    let Ok(rgb) = u32::from_str_radix(hex, 16) else {
        return 7;
    };
    if hex.len() != 6 {
        return 7;
    }
    let (r, g, b) = (
        (rgb >> 16) as i32,
        ((rgb >> 8) & 0xFF) as i32,
        (rgb & 0xFF) as i32,
    );
    ACI_PALETTE
        .iter()
        .min_by_key(|(_, (pr, pg, pb))| {
            let (dr, dg, db) = (r - *pr as i32, g - *pg as i32, b - *pb as i32);
            dr * dr + dg * dg + db * db
        })
        .map(|(index, _)| *index)
        .unwrap_or(7)
}

/// Hex color from an ACI color index (inverse of `hex_to_aci`). Indices
/// outside 1-255 (e.g. 0, BYBLOCK) fall back to white.
pub fn aci_to_hex(index: u8) -> String {
    ACI_PALETTE
        .iter()
        .find(|(i, _)| *i == index)
        .map(|(_, (r, g, b))| format!("#{r:02X}{g:02X}{b:02X}"))
        .unwrap_or_else(|| "#FFFFFF".to_string())
}

/// Convert hex color string to 24-bit integer for DXF true color.
pub fn hex_to_24bit(hex: &str) -> i32 {
    let hex = hex.trim_start_matches('#');
    i32::from_str_radix(hex, 16).unwrap_or(0x00FF_FFFF)
}

/// Lineweight in mm → DXF lineweight enum value (hundredths of mm).
pub fn weight_to_dxf(mm: f64) -> i16 {
    (mm * 100.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_aci_maps_standard_colors() {
        assert_eq!(hex_to_aci("#FF0000"), 1);
        assert_eq!(hex_to_aci("#00FF00"), 3);
        assert_eq!(hex_to_aci("#FFFFFF"), 7);
    }

    #[test]
    fn hex_to_aci_maps_arbitrary_colors_to_nearest() {
        assert_eq!(hex_to_aci("#FF4444"), 20); // reddish → nearest ACI red-orange shade
        assert_eq!(hex_to_aci("#00CC44"), 112); // greenish → nearest ACI green-cyan shade
        assert_eq!(hex_to_aci("#2244CC"), 152); // bluish → nearest ACI blue shade
        assert_eq!(hex_to_aci("#123456"), 146); // dark muted → nearest ACI dark blue shade
        assert_eq!(hex_to_aci("#invalid"), 7); // unparseable → white
        assert_eq!(hex_to_aci("#FFF"), 7); // wrong length → white
    }

    #[test]
    fn aci_roundtrips_standard_palette() {
        for index in 1..=9u8 {
            assert_eq!(hex_to_aci(&aci_to_hex(index)), index);
        }
        assert_eq!(aci_to_hex(0), "#FFFFFF"); // out of range (e.g. BYBLOCK) → white
    }

    #[test]
    fn aci_roundtrips_full_palette_by_color() {
        // Some ACI indices share the exact same RGB value (e.g. several dark
        // greys), so hex_to_aci may not return the original index — but it
        // must always land on an index with the identical color.
        for index in 1..=255u8 {
            let hex = aci_to_hex(index);
            let roundtripped = hex_to_aci(&hex);
            assert_eq!(
                aci_to_hex(roundtripped),
                hex,
                "index {index} ({hex}) drifted to a different color on roundtrip"
            );
        }
    }

    #[test]
    fn hex_to_24bit_parses_correctly() {
        assert_eq!(hex_to_24bit("#FF0000"), 0xFF0000);
        assert_eq!(hex_to_24bit("00FF00"), 0x00FF00);
        assert_eq!(hex_to_24bit("#invalid"), 0x00FF_FFFF);
    }

    #[test]
    fn weight_converts_mm_to_hundredths() {
        assert_eq!(weight_to_dxf(0.35), 35);
        assert_eq!(weight_to_dxf(0.50), 50);
        assert_eq!(weight_to_dxf(1.0), 100);
    }

    // ---- hex_to_aci edge cases ----

    #[test]
    fn hex_to_aci_all_zeros() {
        assert_eq!(hex_to_aci("#000000"), 250);
    }

    #[test]
    fn hex_to_aci_all_ffs() {
        assert_eq!(hex_to_aci("#FFFFFF"), 7);
    }

    #[test]
    fn hex_to_aci_without_hash_prefix() {
        assert_eq!(hex_to_aci("FF0000"), 1);
    }

    #[test]
    fn hex_to_aci_empty_string() {
        assert_eq!(hex_to_aci(""), 7);
    }

    #[test]
    fn hex_to_aci_too_short() {
        assert_eq!(hex_to_aci("#FF"), 7);
        assert_eq!(hex_to_aci("#FFF0"), 7);
    }

    #[test]
    fn hex_to_aci_too_long() {
        assert_eq!(hex_to_aci("#FF0000FF"), 7);
    }

    #[test]
    fn hex_to_aci_non_hex_chars() {
        assert_eq!(hex_to_aci("#ZZZZZZ"), 7);
        assert_eq!(hex_to_aci("#GGGGGG"), 7);
    }

    #[test]
    fn hex_to_aci_each_primary_channel_boundary() {
        // Pure red
        assert_eq!(hex_to_aci("#FF0000"), 1);
        // Pure green
        assert_eq!(hex_to_aci("#00FF00"), 3);
        // Pure blue
        assert_eq!(hex_to_aci("#0000FF"), 5);
    }

    #[test]
    fn hex_to_aci_exact_palette_entry_roundtrips() {
        // Pick a few exact palette entries and verify they roundtrip
        for index in [1, 2, 3, 4, 5, 6, 7, 8, 9, 250] {
            let hex = aci_to_hex(index);
            assert_eq!(hex_to_aci(&hex), index, "failed for ACI {index}");
        }
    }

    // ---- aci_to_hex edge cases ----

    #[test]
    fn aci_to_hex_out_of_range_returns_white() {
        assert_eq!(aci_to_hex(0), "#FFFFFF"); // BYBLOCK
                                              // 256 is out of u8 range, so test boundary at 255 and 0
        assert_eq!(aci_to_hex(0), "#FFFFFF"); // below valid range
        assert_eq!(aci_to_hex(255), "#FFFFFF"); // 255 IS valid, and happens to be white
    }

    #[test]
    fn aci_to_hex_valid_range_never_white_except_known() {
        // ACI 7 is white in AutoCAD; 0 and 255 are also white (out-of-range/BYBLOCK).
        // All other entries 1-254 (except 7) should not be pure white.
        for index in 1..=254u8 {
            if index == 7 {
                continue; // ACI 7 is legitimately white
            }
            assert_ne!(
                aci_to_hex(index),
                "#FFFFFF",
                "ACI {index} unexpectedly returned white"
            );
        }
    }

    #[test]
    fn aci_to_hex_black() {
        assert_eq!(aci_to_hex(250), "#000000");
    }

    #[test]
    fn aci_to_hex_always_has_hash_prefix() {
        for index in [1, 50, 100, 150, 200, 255] {
            assert!(
                aci_to_hex(index).starts_with('#'),
                "ACI {index} missing # prefix"
            );
        }
    }

    #[test]
    fn aci_to_hex_always_7_chars() {
        for index in 1..=255u8 {
            assert_eq!(aci_to_hex(index).len(), 7, "ACI {index} wrong length");
        }
    }

    // ---- hex_to_24bit edge cases ----

    #[test]
    fn hex_to_24bit_all_zeros() {
        assert_eq!(hex_to_24bit("#000000"), 0x000000);
    }

    #[test]
    fn hex_to_24bit_all_ones() {
        assert_eq!(hex_to_24bit("#FFFFFF"), 0xFFFFFF);
    }

    #[test]
    fn hex_to_24bit_without_hash() {
        assert_eq!(hex_to_24bit("FF0000"), 0xFF0000);
    }

    #[test]
    fn hex_to_24bit_invalid_returns_default() {
        assert_eq!(hex_to_24bit(""), 0x00FF_FFFF);
        assert_eq!(hex_to_24bit("#ZZZZZZ"), 0x00FF_FFFF);
        assert_eq!(hex_to_24bit("not_a_color"), 0x00FF_FFFF);
    }

    #[test]
    fn hex_to_24bit_short_hex() {
        // from_str_radix accepts shorter strings (pads with leading zeros)
        assert_eq!(hex_to_24bit("#FFF"), 0xFFF);
        assert_eq!(hex_to_24bit("#0F0"), 0x0F0);
    }

    #[test]
    fn hex_to_24bit_single_byte() {
        assert_eq!(hex_to_24bit("#0F"), 0x0F);
    }

    // ---- weight_to_dxf edge cases ----

    #[test]
    fn weight_to_dxf_zero() {
        assert_eq!(weight_to_dxf(0.0), 0);
    }

    #[test]
    fn weight_to_dxf_negative() {
        assert_eq!(weight_to_dxf(-0.1), -10);
    }

    #[test]
    fn weight_to_dxf_very_small() {
        // 0.01 mm = 1 hundredth
        assert_eq!(weight_to_dxf(0.01), 1);
    }

    #[test]
    fn weight_to_dxf_fractional_rounding() {
        // f64 * 100.0 then as i16 truncates toward zero
        let result = weight_to_dxf(0.355);
        assert_eq!(result, 35); // 0.355 * 100.0 = 35.5 → truncated to 35
    }

    #[test]
    fn weight_to_dxf_large_value() {
        // 100mm → 10000 hundredths
        assert_eq!(weight_to_dxf(100.0), 10000);
    }

    #[test]
    fn weight_to_dxf_typical_lineweights() {
        // Common DXF lineweights
        assert_eq!(weight_to_dxf(0.13), 13);
        assert_eq!(weight_to_dxf(0.18), 18);
        assert_eq!(weight_to_dxf(0.25), 25);
        assert_eq!(weight_to_dxf(0.30), 30);
        assert_eq!(weight_to_dxf(0.35), 35);
        assert_eq!(weight_to_dxf(0.50), 50);
        assert_eq!(weight_to_dxf(0.70), 70);
        assert_eq!(weight_to_dxf(1.00), 100);
        assert_eq!(weight_to_dxf(1.40), 140);
        assert_eq!(weight_to_dxf(2.00), 200);
    }

    // ---- roundtrip conversions ----

    #[test]
    fn aci_hex_roundtrip_all_valid_indices() {
        for index in 1..=255u8 {
            let hex = aci_to_hex(index);
            let back = hex_to_aci(&hex);
            let hex2 = aci_to_hex(back);
            assert_eq!(
                hex, hex2,
                "ACI {index}: {hex} → {back} → {hex2} — color drifted"
            );
        }
    }

    #[test]
    fn hex_to_24bit_roundtrip_with_aci() {
        // A true roundtrip: hex → ACI → hex → 24bit should be deterministic
        let original = "#FF0000";
        let aci = hex_to_aci(original);
        let hex_from_aci = aci_to_hex(aci);
        let bits = hex_to_24bit(&hex_from_aci);
        assert_eq!(bits, 0xFF0000);
    }

    #[test]
    fn hex_to_aci_consistency_across_prefixes() {
        let with_hash = hex_to_aci("#FF0000");
        let without_hash = hex_to_aci("FF0000");
        assert_eq!(with_hash, without_hash);
    }

    #[test]
    fn hex_to_24bit_consistency_across_prefixes() {
        let with_hash = hex_to_24bit("#AABBCC");
        let without_hash = hex_to_24bit("AABBCC");
        assert_eq!(with_hash, without_hash);
    }
}
