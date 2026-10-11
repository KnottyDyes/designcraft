use designcraft_color::cms::icc::{MAX_ICC_BYTES, ProfileClassTag};
use designcraft_color::cms::{self, CmsError, IccProfile, ProfileInfo};
use moxcms::{ColorProfile, ProfileClass, ProfileText};

fn rgb_bytes(label: &str) -> Vec<u8> {
    let mut profile = ColorProfile::new_srgb();
    profile.description = Some(ProfileText::PlainString(label.into()));
    profile.encode().unwrap()
}

fn rejects_declared_length(declared: u32) {
    let mut bytes = rgb_bytes("Length regression");
    bytes[..4].copy_from_slice(&declared.to_be_bytes());

    let result = IccProfile::from_bytes(None, &bytes);

    assert!(matches!(result, Err(CmsError::BadProfile(_))), "declared {declared}, actual {}: {result:?}", bytes.len());
}

#[test]
fn retains_declared_bytes_when_input_has_padding() {
    let declared = rgb_bytes("Padded profile");
    let mut bytes = declared.clone();
    bytes.extend_from_slice(b"padding is not ICC data");

    let profile = IccProfile::from_bytes(None, &bytes).unwrap();

    assert_eq!(&*profile.raw, declared);
    assert_eq!(profile.name, "Padded profile");
    assert!(profile.usable());
}

#[test]
fn rejects_profile_when_declared_length_is_below_header_or_absurd() {
    for declared in [1, 127, u32::MAX] {
        rejects_declared_length(declared);
    }
}

#[test]
fn rejects_tag_payload_when_it_points_into_padding() {
    let mut bytes = rgb_bytes("Padding tag payload");
    let declared = bytes.len();
    let count = u32::from_be_bytes(bytes[128..132].try_into().unwrap());
    let record = (0..usize::try_from(count).unwrap()).map(|index| 132 + index * 12).find(|offset| &bytes[*offset..*offset + 4] == b"desc").unwrap();
    let offset = usize::try_from(u32::from_be_bytes(bytes[record + 4..record + 8].try_into().unwrap())).unwrap();
    let length = usize::try_from(u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap())).unwrap();
    let payload = bytes[offset..offset + length].to_vec();
    bytes.extend_from_slice(&payload);
    bytes[record + 4..record + 8].copy_from_slice(&u32::try_from(declared).unwrap().to_be_bytes());

    let result = IccProfile::from_bytes(None, &bytes);

    assert!(matches!(result, Err(CmsError::BadProfile(_))), "{result:?}");
}

#[test]
fn rejects_profile_when_declared_length_is_larger() {
    let length = u32::try_from(rgb_bytes("Length regression").len()).unwrap();
    rejects_declared_length(length + 1);
}

#[test]
fn rejects_profile_when_declared_length_is_zero() {
    rejects_declared_length(0);
}

#[test]
fn serializes_stable_ids_when_listing_builtin_profiles() {
    let profiles: Vec<_> = cms::profiles().into_iter().filter(|profile| profile.builtin).collect();

    let json = serde_json::to_value(&profiles).unwrap();

    let ids: Vec<_> = json.as_array().unwrap().iter().map(|profile| profile.get("id").and_then(serde_json::Value::as_str)).collect();
    assert_eq!(ids, ["srgb", "adobe-rgb", "display-p3", "prophoto-rgb", "generic-cmyk", "device-cmyk"].map(Some));
}

#[test]
fn preserves_profile_info_when_serializing_builtins() {
    let profiles: Vec<_> = cms::profiles().into_iter().filter(|profile| profile.builtin).collect();

    let restored: Vec<ProfileInfo> = serde_json::from_str(&serde_json::to_string(&profiles).unwrap()).unwrap();

    assert_eq!(restored, profiles);
    let ids: std::collections::HashSet<_> = restored.iter().map(|profile| &profile.id).collect();
    assert_eq!(ids.len(), 6);
}

#[test]
fn retains_exact_bytes_when_parsing_rgb() {
    let bytes = rgb_bytes("Retained RGB");

    let profile = IccProfile::from_bytes(None, &bytes).unwrap();

    let raw: std::sync::Arc<[u8]> = profile.raw.clone();
    assert_eq!(&*raw, bytes);
    assert_eq!(profile.name, "Retained RGB");
    assert_eq!(profile.channels, 3);
    assert!(format!("{profile:?}").len() < 256);
}

#[test]
fn retains_class_when_parsing_each_header_tag() {
    for class in [
        ProfileClass::InputDevice,
        ProfileClass::DisplayDevice,
        ProfileClass::OutputDevice,
        ProfileClass::DeviceLink,
        ProfileClass::ColorSpace,
        ProfileClass::Abstract,
        ProfileClass::Named,
    ] {
        let mut source = ColorProfile::new_srgb();
        source.profile_class = class;
        source.cicp = None;
        let bytes = source.encode().unwrap();

        let profile = IccProfile::from_bytes(None, &bytes).unwrap();

        let tag: ProfileClassTag = profile.class;
        assert_eq!(tag, class);
    }
}

#[test]
fn reports_four_channels_when_parsing_cmyk() {
    let bytes = cms::icc_out::generic_cmyk_icc();

    let profile = IccProfile::from_bytes(None, bytes).unwrap();

    assert_eq!(profile.channels, 4);
    assert_eq!(profile.class, ProfileClass::OutputDevice);
    assert_eq!(&*profile.raw, bytes);
}

#[test]
fn reports_one_channel_when_parsing_gray() {
    let bytes = ColorProfile::new_gray_with_gamma(2.2).encode().unwrap();

    let profile = IccProfile::from_bytes(None, &bytes).unwrap();

    assert_eq!(profile.channels, 1);
    assert_eq!(&*profile.raw, bytes);
}

#[test]
fn has_empty_raw_when_constructing_builtin_rgb() {
    let profile = cms::icc::builtin_rgb(cms::SRGB).unwrap();

    assert!(profile.raw.is_empty());
    assert_eq!(profile.class, ProfileClass::DisplayDevice);
    assert_eq!(profile.channels, 3);
}

#[test]
fn rejects_profile_when_header_is_short() {
    for length in [0, 1, 3, 4, 127] {
        let bytes = vec![0; length];

        let result = IccProfile::from_bytes(None, &bytes);

        assert!(matches!(result, Err(CmsError::BadProfile(_))));
    }
}

#[test]
fn rejects_profile_when_input_exceeds_cap() {
    let bytes = vec![0; MAX_ICC_BYTES + 1];

    let result = IccProfile::from_bytes(None, &bytes);

    assert!(matches!(result, Err(CmsError::BadProfile(message)) if message.contains("size limit")));
}

#[test]
fn keeps_parser_cap_when_input_is_below_outer_cap() {
    let mut bytes = rgb_bytes("Parser cap");
    bytes.resize(10 * 1024 * 1024, 0);
    let length = u32::try_from(bytes.len()).unwrap();
    bytes[..4].copy_from_slice(&length.to_be_bytes());

    let result = IccProfile::from_bytes(None, &bytes);

    assert!(matches!(result, Err(CmsError::BadProfile(_))));
}
