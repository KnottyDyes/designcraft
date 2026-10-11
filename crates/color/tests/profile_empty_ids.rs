use designcraft_color::cms::{self, CmsError};
use moxcms::{ColorProfile, ProfileText};

fn rejects_blank_id(id: &str) {
    let mut source = ColorProfile::new_srgb();
    source.description = Some(ProfileText::PlainString("Blank ID regression".into()));
    let bytes = source.encode().unwrap();
    let before = cms::profiles();

    let result = cms::register_icc(&bytes, id, None);

    assert!(matches!(result, Err(CmsError::Unsupported(_))), "id {id:?}: {result:?}");
    assert_eq!(cms::profiles(), before, "rejected registration must leave the registry unchanged");
}

#[test]
fn rejects_registration_when_id_is_empty() {
    rejects_blank_id("");
}

#[test]
fn rejects_registration_when_id_is_whitespace_only() {
    rejects_blank_id(" \t\r\n ");
}
