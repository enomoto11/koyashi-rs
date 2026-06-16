//! Fixture crate demonstrating koyashi.toml suppression.

#![allow(dead_code)]

/// `stale_entry` is write-only, but suppressed via koyashi.toml.
struct Cache {
    stale_entry: String,
}

/// Never constructed, so `orphan` is unused and reported normally.
struct Dangling {
    orphan: u8,
}

fn main() {
    let mut cache = Cache {
        stale_entry: String::new(),
    };
    cache.stale_entry = "value".to_string();
}
