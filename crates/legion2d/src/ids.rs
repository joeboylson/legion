//! Short IDs for deployments and sessions, and a stable hash for folder paths.

use std::sync::atomic::{AtomicU64, Ordering};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// FNV-1a: stable across Rust releases, unlike the standard library's hasher,
/// so a folder keeps the same outside folder name.
pub fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME))
}

pub fn short_hash(bytes: &[u8]) -> String {
    format!("{:08x}", stable_hash(bytes) as u32)
}

/// Unique within this machine: the time, a counter and the process.
pub fn new_id() -> String {
    static ISSUED_COUNT: AtomicU64 = AtomicU64::new(0);
    let issued_number = ISSUED_COUNT.fetch_add(1, Ordering::Relaxed);
    let nanos_since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    short_hash(format!("{nanos_since_epoch}-{issued_number}-{}", std::process::id()).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        assert_eq!(stable_hash(b""), FNV_OFFSET_BASIS);
        assert_eq!(short_hash(b"/repo"), short_hash(b"/repo"));
        assert_ne!(short_hash(b"/repo"), short_hash(b"/repo2"));
    }

    #[test]
    fn short_hash_is_eight_hex_digits() {
        let hash = short_hash(b"anything");
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ids_differ() {
        assert_ne!(new_id(), new_id());
    }
}
