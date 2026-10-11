use std::io::Read;
use std::path::Path;

use super::icc::MAX_ICC_BYTES;
use super::{CmsError, ProfileInfo, register_icc};

fn read_bounded(reader: impl Read) -> Result<Vec<u8>, CmsError> {
    let limit = u64::try_from(MAX_ICC_BYTES).map_err(|e| CmsError::Io(e.to_string()))?;
    let mut reader = reader.take(limit + 1);
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer).map_err(|e| CmsError::Io(e.to_string()))?;
        if count == 0 {
            break;
        }
        let chunk = buffer.get(..count).ok_or_else(|| CmsError::Io("reader exceeded buffer length".into()))?;
        let needed = bytes.len().checked_add(count).ok_or_else(|| CmsError::BadProfile("ICC length overflow".into()))?;
        if needed > MAX_ICC_BYTES {
            return Err(CmsError::BadProfile("ICC payload exceeds size limit".into()));
        }
        if needed > bytes.capacity() {
            let capacity = bytes.capacity().saturating_mul(2).max(needed).min(MAX_ICC_BYTES);
            bytes.try_reserve_exact(capacity - bytes.len()).map_err(|e| CmsError::Io(e.to_string()))?;
        }
        bytes.extend_from_slice(chunk);
    }
    Ok(bytes)
}

/// Load a `.icc`/`.icm` file, using its stem as the ID base and its description as the label.
/// Stem collisions receive numeric suffixes without replacing any existing profile.
pub fn load_icc_file(path: &Path) -> Result<ProfileInfo, CmsError> {
    let file = std::fs::File::open(path).map_err(|e| CmsError::Io(format!("{}: {e}", path.display())))?;
    let length = file.metadata().map_err(|e| CmsError::Io(format!("{}: {e}", path.display())))?.len();
    let limit = u64::try_from(MAX_ICC_BYTES).map_err(|e| CmsError::Io(e.to_string()))?;
    if length > limit {
        return Err(CmsError::BadProfile("ICC file exceeds size limit".into()));
    }
    let id = path.file_stem().and_then(|stem| stem.to_str()).ok_or_else(|| CmsError::Io("profile filename is not UTF-8".into()))?;
    let bytes = read_bounded(file)?;
    register_icc(&bytes, id, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct GrowingReader<'a>(&'a Cell<usize>);

    impl Read for GrowingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(0);
            self.0.set(self.0.get() + buffer.len());
            Ok(buffer.len())
        }
    }

    #[test]
    fn bounds_read_when_file_grows_after_metadata_check() {
        let consumed = Cell::new(0);
        let reader = GrowingReader(&consumed);

        let result = read_bounded(reader);

        assert!(matches!(result, Err(CmsError::BadProfile(_))));
        assert_eq!(consumed.get(), MAX_ICC_BYTES + 1);
    }

    #[test]
    fn retains_bytes_when_read_is_below_cap() {
        let bytes = super::super::icc_out::generic_cmyk_icc();

        let result = read_bounded(bytes).unwrap();

        assert_eq!(result, bytes);
    }
}
