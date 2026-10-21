//! Device ID generation in XXX-XXX-XXX format.

use rand::Rng;

/// Generate a device ID in the format XXX-XXX-XXX where X is a digit 0-9.
pub fn generate_device_id() -> String {
    let mut rng = rand::thread_rng();
    let seg1: u16 = rng.gen_range(0..1000);
    let seg2: u16 = rng.gen_range(0..1000);
    let seg3: u16 = rng.gen_range(0..1000);
    format!("{:03}-{:03}-{:03}", seg1, seg2, seg3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format() {
        let id = generate_device_id();
        assert_eq!(id.len(), 11);
        assert_eq!(&id[3..4], "-");
        assert_eq!(&id[7..8], "-");
        for c in id.chars() {
            assert!(c.is_ascii_digit() || c == '-');
        }
    }

    #[test]
    fn test_uniqueness() {
        let ids: std::collections::HashSet<String> =
            (0..100).map(|_| generate_device_id()).collect();
        assert!(ids.len() > 90, "IDs should be mostly unique");
    }
}