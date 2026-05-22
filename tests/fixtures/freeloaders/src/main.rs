//! Fixture crate exercising every koyashi classification.

#![allow(dead_code)]

/// Each field is a freeloader of a different kind.
struct Settings {
    /// read-only: initialized once, only read afterwards.
    label: String,
    /// healthy: both read and written, so koyashi does not report it.
    counter: u32,
    /// write-only: assigned, but its value is never read.
    last_message: String,
}

/// Never constructed, so its field has no references at all.
struct Orphan {
    /// unused: no references anywhere.
    forgotten: i32,
}

/// Constructed and `Debug`-printed, but its field is never read directly.
#[derive(Debug)]
struct Telemetry {
    /// derive-only: read only through the `Debug` derive.
    trace_id: String,
}

/// Its field is only ever `&mut`-borrowed by a helper.
struct Budget {
    /// healthy: a mutable borrow may read or write, so it is not reported.
    remaining: u32,
}

fn spend(amount: &mut u32) {
    *amount -= 1;
}

fn main() {
    let mut settings = Settings {
        label: "demo".to_string(),
        counter: 0,
        last_message: String::new(),
    };

    println!("{}", settings.label);

    settings.counter += 1;
    if settings.counter > 0 {
        settings.last_message = format!("ran {} times", settings.counter);
    }

    let telemetry = Telemetry {
        trace_id: "trace-001".to_string(),
    };
    println!("{telemetry:?}");

    let mut budget = Budget { remaining: 10 };
    spend(&mut budget.remaining);
}
