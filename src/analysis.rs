//! Turns aggregated reference counts into a [`Classification`].

use crate::model::{Classification, FieldStats, ReferenceKind, ReferenceSite};

/// Aggregate a field's reference sites into counts.
pub fn aggregate(sites: &[ReferenceSite]) -> FieldStats {
    let mut stats = FieldStats::default();
    for site in sites {
        match site.kind {
            ReferenceKind::Initializer => stats.initializers += 1,
            ReferenceKind::Read => stats.reads += 1,
            ReferenceKind::Write => {
                stats.writes += 1;
                stats.write_lines.push(site.location.line);
            }
        }
    }
    stats.write_lines.sort_unstable();
    stats
}

/// Classify a field from its counts.
///
/// `used_by_derive` is true when the field's struct has a derive macro that
/// reads every field. Returns `None` for healthy fields, which are both read
/// and written.
pub fn classify(stats: &FieldStats, used_by_derive: bool) -> Option<Classification> {
    if stats.initializers == 0 && stats.reads == 0 && stats.writes == 0 {
        return Some(Classification::Unused);
    }
    if stats.reads == 0 {
        // The value is placed (initialized or written) but never read in code.
        if used_by_derive {
            return Some(Classification::DeriveOnly);
        }
        return Some(Classification::WriteOnly);
    }
    if stats.writes == 0 {
        return Some(Classification::ReadOnly);
    }
    None
}

/// A human-readable explanation for a classification.
pub fn message_for(classification: Classification) -> &'static str {
    match classification {
        Classification::Unused => "no references found",
        Classification::WriteOnly => "field is written but its value is never observed",
        Classification::ReadOnly => "field is set once at construction and never mutated",
        Classification::DeriveOnly => "field is read only through a derive macro",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(initializers: u32, reads: u32, writes: u32) -> FieldStats {
        FieldStats {
            initializers,
            reads,
            writes,
            write_lines: (0..writes).map(|i| i + 1).collect(),
        }
    }

    #[test]
    fn no_reference_is_unused() {
        assert_eq!(
            classify(&stats(0, 0, 0), false),
            Some(Classification::Unused)
        );
        // A derive on a never-constructed struct does not rescue the field.
        assert_eq!(
            classify(&stats(0, 0, 0), true),
            Some(Classification::Unused)
        );
    }

    #[test]
    fn value_placed_but_never_read_is_write_only() {
        assert_eq!(
            classify(&stats(0, 0, 3), false),
            Some(Classification::WriteOnly)
        );
        assert_eq!(
            classify(&stats(1, 0, 2), false),
            Some(Classification::WriteOnly)
        );
        // Initialized once and never read counts as write-only.
        assert_eq!(
            classify(&stats(1, 0, 0), false),
            Some(Classification::WriteOnly)
        );
    }

    #[test]
    fn placed_and_read_only_by_a_derive_is_derive_only() {
        assert_eq!(
            classify(&stats(1, 0, 0), true),
            Some(Classification::DeriveOnly)
        );
        assert_eq!(
            classify(&stats(0, 0, 2), true),
            Some(Classification::DeriveOnly)
        );
    }

    #[test]
    fn read_without_write_is_read_only() {
        assert_eq!(
            classify(&stats(1, 7, 0), false),
            Some(Classification::ReadOnly)
        );
        // An explicit read takes precedence over derive usage.
        assert_eq!(
            classify(&stats(0, 2, 0), true),
            Some(Classification::ReadOnly)
        );
    }

    #[test]
    fn read_and_write_is_healthy() {
        assert_eq!(classify(&stats(1, 4, 2), false), None);
    }
}
