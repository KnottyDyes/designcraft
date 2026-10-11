use designcraft_color::cms::{self, Cms, CmsError, ColorSettings, IccProfile, Intent, ProfileInfo, ProfileKind};
use moxcms::{ColorProfile, ProfileText};

fn rgb_bytes(label: &str) -> Vec<u8> {
    let mut profile = ColorProfile::new_srgb();
    profile.description = Some(ProfileText::PlainString(label.into()));
    profile.encode().unwrap()
}

#[test]
fn uses_explicit_id_when_registering_valid_rgb() {
    let bytes = rgb_bytes("Registered RGB description");

    let info = cms::register_icc(&bytes, "registration-rgb", None).unwrap();

    assert_eq!(info.id, "registration-rgb");
    assert_eq!(info.name, "Registered RGB description");
    assert_eq!(info.kind, ProfileKind::Rgb);
    assert!(!info.builtin);
    let restored: ProfileInfo = serde_json::from_str(&serde_json::to_string(&info).unwrap()).unwrap();
    assert_eq!(restored, info);
    let cms = Cms::new(&ColorSettings { rgb: info.name, ..Default::default() }).unwrap();
    let rgb = cms.rgb_to_srgb([0.2, 0.4, 0.6]);
    assert!(rgb.iter().zip([0.2, 0.4, 0.6]).all(|(actual, expected)| (actual - expected).abs() < 0.01));
}

#[test]
fn preserves_id_when_nonempty_id_has_surrounding_whitespace() {
    let bytes = rgb_bytes("Whitespace-preserved RGB description");

    let info = cms::register_icc(&bytes, " registration-with-spaces ", None).unwrap();

    assert_eq!(info.id, " registration-with-spaces ");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn preserves_file_stem_when_filename_has_surrounding_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(" file-stem-with-spaces .icc");
    std::fs::write(&path, rgb_bytes("Whitespace-preserved file description")).unwrap();

    let info = cms::load_icc_file(&path).unwrap();

    assert_eq!(info.id, " file-stem-with-spaces ");
}

#[test]
fn rejects_registration_when_bytes_are_invalid() {
    let result = cms::register_icc(b"not an ICC", "invalid-regression", None);

    assert!(matches!(result, Err(CmsError::BadProfile(_))));
    assert!(cms::profiles().iter().all(|profile| profile.id != "invalid-regression"));
}

#[test]
fn rejects_registration_when_declared_length_is_zero() {
    let mut bytes = rgb_bytes("Invalid registration length");
    bytes[..4].fill(0);

    let result = cms::register_icc(&bytes, "invalid-length-registration", None);

    assert!(matches!(result, Err(CmsError::BadProfile(_))));
}

#[test]
fn rejects_registration_when_name_collides_with_builtin() {
    let bytes = cms::icc_out::generic_cmyk_icc();

    let result = cms::register_icc(bytes, "builtin-name-collision", None);

    assert!(matches!(result, Err(CmsError::Unsupported(_))));
}

#[test]
fn suffixes_registration_when_id_collides_with_builtin() {
    let bytes = rgb_bytes("Distinct collision description");
    for id in ["srgb", "adobe-rgb", "display-p3", "prophoto-rgb", "generic-cmyk", "device-cmyk"] {
        let info = cms::register_icc(&bytes, id, None).unwrap();

        assert_eq!(info.id, format!("{id}-2"));
        assert!(cms::profiles().iter().any(|profile| profile.builtin && profile.id == id));
    }
}

#[test]
fn rejects_registration_when_explicit_name_collides_with_builtin() {
    let bytes = rgb_bytes("Distinct explicit name description");
    for name in [cms::SRGB, cms::ADOBE_RGB, cms::DISPLAY_P3, cms::PROPHOTO_RGB, cms::GENERIC_CMYK, cms::DEVICE_CMYK] {
        let result = cms::register_icc(&bytes, "explicit-name-collision", Some(name.into()));

        assert!(matches!(result, Err(CmsError::Unsupported(_))), "{name}: {result:?}");
    }
}

#[test]
fn preserves_profile_when_imported_id_is_registered_again() {
    let original = cms::register_icc(&rgb_bytes("Replaced description"), "replacement-id", None).unwrap();
    let bytes = rgb_bytes("Replacement description");

    let info = cms::register_icc(&bytes, "replacement-id", None).unwrap();

    let matches: Vec<_> = cms::profiles().into_iter().filter(|profile| profile.id == "replacement-id").collect();
    assert_eq!(matches, [original]);
    assert_eq!(info.id, "replacement-id-2");
    assert!(Cms::new(&ColorSettings { rgb: "Replaced description".into(), ..Default::default() }).is_ok());
    let third = cms::register_icc(&bytes, "replacement-id", None).unwrap();
    assert_eq!(third.id, "replacement-id-3");
}

#[test]
fn preserves_profile_when_imported_name_is_registered_again() {
    let bytes = rgb_bytes("Shared imported description");
    let original = cms::register_icc(&bytes, "old-name-id", None).unwrap();

    let info = cms::register_icc(&bytes, "new-name-id", None).unwrap();

    let matches: Vec<_> = cms::profiles().into_iter().filter(|profile| profile.name == "Shared imported description").collect();
    assert_eq!(matches, [original, info]);
    let legacy = Cms::new(&ColorSettings { rgb: "Shared imported description".into(), ..Default::default() }).unwrap();
    assert_eq!(legacy.settings().rgb, "old-name-id");
}

#[test]
fn suffixes_id_when_it_would_shadow_an_existing_legacy_name() {
    let original = cms::register_icc(&rgb_bytes("legacy-id-shadow"), "legacy-shadow-original", None).unwrap();
    cms::register_icc(&rgb_bytes("Reserved suffix description"), "legacy-id-shadow-2", None).unwrap();

    let imported = cms::register_icc(&rgb_bytes("New shadow description"), "legacy-id-shadow", None).unwrap();

    assert_eq!(imported.id, "legacy-id-shadow-3");
    let legacy = Cms::new(&ColorSettings { rgb: original.name, ..Default::default() }).unwrap();
    assert_eq!(legacy.settings().rgb, original.id);
}

#[test]
fn rejects_registration_when_gray_is_not_a_working_space() {
    let bytes = ColorProfile::new_gray_with_gamma(2.2).encode().unwrap();

    let result = cms::register_icc(&bytes, "gray-registration", Some("Distinct gray".into()));

    assert!(matches!(result, Err(CmsError::Unsupported(_))));
}

#[test]
fn rejects_registration_when_transforms_are_unusable() {
    let mut source = ColorProfile::new_srgb();
    source.red_trc = None;
    source.green_trc = None;
    source.blue_trc = None;
    let bytes = source.encode().unwrap();
    assert!(!IccProfile::from_bytes(None, &bytes).unwrap().usable());

    let result = cms::register_icc(&bytes, "unusable-registration", Some("Unusable RGB".into()));

    assert!(matches!(result, Err(CmsError::Unsupported(_))));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn uses_file_stem_and_description_when_importing_cmyk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("distinct-press.icc");
    let mut source = ColorProfile::new_from_slice(cms::icc_out::generic_cmyk_icc()).unwrap();
    source.description = Some(ProfileText::PlainString("Distinct imported press".into()));
    std::fs::write(&path, source.encode().unwrap()).unwrap();

    let info = cms::load_icc_file(&path).unwrap();

    assert_eq!(info.id, "distinct-press");
    assert_eq!(info.name, "Distinct imported press");
    assert_eq!(info.kind, ProfileKind::Cmyk);
    let restored: ProfileInfo = serde_json::from_str(&serde_json::to_string(&info).unwrap()).unwrap();
    assert_eq!(restored, info);
    let context = Cms::new(&ColorSettings { cmyk: info.name, ..Default::default() }).unwrap();
    let inks = context.srgb_to_cmyk([0.7, 0.5, 0.4], Intent::RelativeColorimetric);
    assert!(inks.iter().all(|value| value.is_finite() && (0.0..=1.0).contains(value)));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rejects_disk_import_before_reading_when_file_exceeds_cap() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(u64::try_from(cms::icc::MAX_ICC_BYTES).unwrap() + 1).unwrap();

    let result = cms::load_icc_file(file.path());

    assert!(matches!(result, Err(CmsError::BadProfile(message)) if message.contains("size limit")));
}
