use rand::rngs::OsRng;
use rand::Rng;
use std::collections::HashSet;
use std::sync::Mutex;

/// Generates unique device IDs in XXX-XXX-XXX format (9 digits, dash-separated).
/// Uses CSPRNG for randomness and tracks generated IDs to prevent collisions.
pub struct IdGenerator {
    known_ids: Mutex<HashSet<String>>,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self {
            known_ids: Mutex::new(HashSet::new()),
        }
    }

    /// Generate a unique ID in XXX-XXX-XXX format.
    /// Retries on collision with previously generated IDs.
    pub fn generate(&self) -> String {
        let mut rng = OsRng;
        loop {
            let d1: u32 = rng.gen_range(0..1000);
            let d2: u32 = rng.gen_range(0..1000);
            let d3: u32 = rng.gen_range(0..1000);
            let id = format!("{:03}-{:03}-{:03}", d1, d2, d3);

            let mut known = self.known_ids.lock().unwrap();
            if !known.contains(&id) {
                known.insert(id.clone());
                return id;
            }
            // Collision — retry with new random values
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_format() {
        let gen = IdGenerator::new();
        let id = gen.generate();
        assert!(regex::Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&id));
    }

    #[test]
    fn test_uniqueness() {
        let gen = IdGenerator::new();
        let mut ids = HashSet::new();
        for _ in 0..1000 {
            let id = gen.generate();
            assert!(ids.insert(id), "duplicate ID generated");
        }
    }
}