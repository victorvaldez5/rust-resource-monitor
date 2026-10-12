//! SHA-256 of a program file.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Bigger files are skipped: hashing them would take long and they are rarely programs.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Hex SHA-256 of the file, or None if it can't be read or is too big.
pub fn sha256(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_a_known_string() {
        let path = std::env::temp_dir().join(format!("hash-test-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let hash = sha256(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            hash.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn missing_file_has_no_hash() {
        assert_eq!(sha256(Path::new("/nonexistent/file")), None);
    }
}
