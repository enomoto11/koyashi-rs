//! Fixture crate with no freeloader fields; koyashi should report nothing.

struct Healthy {
    /// healthy: both read and written.
    value: u32,
}

fn main() {
    let mut healthy = Healthy { value: 0 };
    healthy.value += 1;
    println!("{}", healthy.value);
}
