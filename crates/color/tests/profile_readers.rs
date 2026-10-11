use designcraft_color::cms::{self, Cms, ColorSettings, Intent, ProofSetup, ProofTarget};
use moxcms::{ColorProfile, ProfileText};

fn bytes(mut profile: ColorProfile, name: &str) -> Vec<u8> {
    profile.description = Some(ProfileText::PlainString(name.into()));
    profile.encode().unwrap()
}

#[test]
fn canonicalizes_builtin_ids_and_legacy_serialized_names() {
    for info in cms::profiles().into_iter().filter(|info| info.builtin) {
        let mut settings = ColorSettings::default();
        match info.kind {
            cms::ProfileKind::Rgb => settings.rgb = info.id.clone(),
            cms::ProfileKind::Cmyk => settings.cmyk = info.id.clone(),
            cms::ProfileKind::Gray => panic!("no builtin gray"),
        }
        let context = Cms::new(&settings).unwrap();
        assert_eq!(context.settings(), &settings);
        match info.kind {
            cms::ProfileKind::Rgb => settings.rgb = info.name,
            cms::ProfileKind::Cmyk => settings.cmyk = info.name,
            cms::ProfileKind::Gray => panic!("no builtin gray"),
        }
        let saved = serde_json::to_string(&settings).unwrap();
        let legacy: ColorSettings = serde_json::from_str(&saved).unwrap();
        assert_eq!(Cms::new(&legacy).unwrap().settings(), context.settings());
    }
    assert_eq!(ColorSettings::default().rgb, "srgb");
    assert_eq!(ColorSettings::default().cmyk, "generic-cmyk");
}

#[test]
fn imported_ids_and_names_resolve_for_working_spaces_and_proofs() {
    let rgb = cms::register_icc(&bytes(ColorProfile::new_display_p3(), "Reader RGB"), "reader-rgb", None).unwrap();
    let press = ColorProfile::new_from_slice(cms::icc_out::generic_cmyk_icc()).unwrap();
    let cmyk = cms::register_icc(&bytes(press, "Reader CMYK"), "reader-cmyk", None).unwrap();
    let by_id = Cms::new(&ColorSettings { rgb: rgb.id.clone(), cmyk: cmyk.id.clone(), ..Default::default() }).unwrap();
    let saved = serde_json::to_string(&ColorSettings { rgb: rgb.name, cmyk: cmyk.name.clone(), ..Default::default() }).unwrap();
    let restored: ColorSettings = serde_json::from_str(&saved).unwrap();
    let by_name = Cms::new(&restored).unwrap();
    assert_eq!(by_name.settings(), by_id.settings());
    assert_eq!(by_id.rgb_to_srgb([0.4, 0.5, 0.3]), by_name.rgb_to_srgb([0.4, 0.5, 0.3]));
    let context = Cms::default();
    for (id, name) in [(cmyk.id, cmyk.name), ("device-cmyk".into(), cms::DEVICE_CMYK.into()), ("generic-cmyk".into(), cms::GENERIC_CMYK.into())] {
        let proof = ProofSetup { target: ProofTarget::Cmyk(id), ..Default::default() };
        let legacy = ProofSetup { target: ProofTarget::Cmyk(name), ..Default::default() };
        assert_eq!(context.proof_srgb([0.1, 0.6, 0.9], &proof), context.proof_srgb([0.1, 0.6, 0.9], &legacy));
        assert_eq!(context.proof_cmyk_to_srgb([0.3, 0.2, 0.1, 0.4], &proof), context.proof_cmyk_to_srgb([0.3, 0.2, 0.1, 0.4], &legacy));
    }
    assert!(by_id.srgb_to_cmyk([0.4, 0.5, 0.3], Intent::RelativeColorimetric).iter().all(|value| value.is_finite()));
}

#[test]
fn preserves_active_profile_and_conversion_when_stem_and_description_collide() {
    let original = cms::register_icc(&bytes(ColorProfile::new_display_p3(), "Active collision RGB"), "active-collision", None).unwrap();
    cms::set_active(&ColorSettings { rgb: original.name.clone(), ..Default::default() }).unwrap();
    let before = cms::active().rgb_to_srgb([0.4, 0.5, 0.3]);

    let imported = cms::register_icc(&bytes(ColorProfile::new_srgb(), &original.name), &original.id, None).unwrap();

    assert_eq!(imported.id, "active-collision-2");
    assert_eq!(cms::active_settings().rgb, original.id);
    assert_eq!(Cms::new(&cms::active_settings()).unwrap().rgb_to_srgb([0.4, 0.5, 0.3]), before);
    assert_eq!(Cms::new(&ColorSettings { rgb: original.name, ..Default::default() }).unwrap().rgb_to_srgb([0.4, 0.5, 0.3]), before);
    cms::set_active(&ColorSettings::default()).unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn suffixes_repeated_file_stems_and_accepts_padding() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    let first_path = first_dir.path().join("repeated-stem.icc");
    let second_path = second_dir.path().join("repeated-stem.icc");
    std::fs::write(&first_path, bytes(ColorProfile::new_srgb(), "First stem RGB")).unwrap();
    let mut padded = bytes(ColorProfile::new_display_p3(), "Second stem RGB");
    padded.extend_from_slice(&[255; 64]);
    std::fs::write(&second_path, padded).unwrap();
    let first = cms::load_icc_file(&first_path).unwrap();

    let second = cms::load_icc_file(&second_path).unwrap();

    assert_eq!(second.id, "repeated-stem-2");
    assert!(cms::profiles().contains(&first));
    assert!(Cms::new(&ColorSettings { rgb: second.id, ..Default::default() }).is_ok());
}

#[test]
fn id_lookup_takes_precedence_over_legacy_name() {
    let info = cms::register_icc(&bytes(ColorProfile::new_srgb(), "display-p3"), "ambiguous-description", None).unwrap();
    let resolved = Cms::new(&ColorSettings { rgb: "display-p3".into(), ..Default::default() }).unwrap();
    let builtin = Cms::new(&ColorSettings { rgb: cms::DISPLAY_P3.into(), ..Default::default() }).unwrap();
    assert_eq!(resolved.rgb_to_srgb([0.4, 0.5, 0.3]), builtin.rgb_to_srgb([0.4, 0.5, 0.3]));
    assert_ne!(
        resolved.rgb_to_srgb([0.4, 0.5, 0.3]),
        Cms::new(&ColorSettings { rgb: info.id, ..Default::default() }).unwrap().rgb_to_srgb([0.4, 0.5, 0.3])
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn suffixes_file_stem_when_it_collides_with_builtin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("srgb.icc");
    std::fs::write(&path, bytes(ColorProfile::new_display_p3(), "Builtin stem RGB")).unwrap();
    let original = Cms::new(&ColorSettings { rgb: "srgb".into(), ..Default::default() }).unwrap();

    let imported = cms::load_icc_file(&path).unwrap();

    assert_eq!(imported.id, "srgb-2");
    assert_eq!(Cms::new(&ColorSettings { rgb: "srgb".into(), ..Default::default() }).unwrap().settings(), original.settings());
    assert_ne!(
        Cms::new(&ColorSettings { rgb: imported.id, ..Default::default() }).unwrap().rgb_to_srgb([0.4, 0.5, 0.3]),
        original.rgb_to_srgb([0.4, 0.5, 0.3])
    );
}
