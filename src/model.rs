//! Core data model shared across the analysis pipeline.

use std::path::PathBuf;

use serde::Serialize;

/// How a struct field is used across the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    /// The field has no references at all.
    Unused,
    /// The field's value is placed but never read.
    WriteOnly,
    /// The field is read but never reassigned after construction.
    ReadOnly,
    /// The field is read only through a derive macro, never in hand-written code.
    DeriveOnly,
}

impl Classification {
    /// The label used in textual output.
    pub fn label(self) -> &'static str {
        match self {
            Classification::Unused => "unused",
            Classification::WriteOnly => "write-only",
            Classification::ReadOnly => "read-only",
            Classification::DeriveOnly => "derive-only",
        }
    }
}

/// The syntactic role of a single reference to a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A struct literal initializer (`Foo { field: value }`).
    Initializer,
    /// A read of the field's value.
    Read,
    /// An assignment or compound assignment of the field.
    Write,
    /// A mutable borrow (`&mut field`); the value may be read and/or written
    /// through it, so it counts as both a read and a write.
    MutBorrow,
}

impl ReferenceKind {
    /// A short label for the reference kind.
    pub fn label(self) -> &'static str {
        match self {
            ReferenceKind::Initializer => "initializer",
            ReferenceKind::Read => "read",
            ReferenceKind::Write => "write",
            ReferenceKind::MutBorrow => "mut-borrow",
        }
    }
}

/// A position in a source file (one-based line, zero-based character).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub line: u32,
    pub character: u32,
}

/// A single reference to a field, with its syntactic role.
#[derive(Debug, Clone)]
pub struct ReferenceSite {
    pub location: Location,
    pub kind: ReferenceKind,
}

/// A named struct field discovered in the workspace.
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub struct_name: String,
    pub field_name: String,
    pub location: Location,
    /// Whether the field's struct has a derive macro that reads every field
    /// (such as `Debug`, `Serialize`, or `PartialEq`).
    pub used_by_derive: bool,
}

impl FieldDef {
    /// The `Struct::field` display form.
    pub fn display_name(&self) -> String {
        format!("{}::{}", self.struct_name, self.field_name)
    }
}

/// Reference counts aggregated for a single field.
#[derive(Debug, Clone, Default)]
pub struct FieldStats {
    pub initializers: u32,
    pub reads: u32,
    pub writes: u32,
    /// Line numbers at which writes occur.
    pub write_lines: Vec<u32>,
}

/// A single reference site, in a form suitable for reporting.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceEntry {
    pub kind: &'static str,
    pub file: PathBuf,
    pub line: u32,
}

/// One detected field, ready for reporting.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub classification: Classification,
    pub field: String,
    pub file: PathBuf,
    pub line: u32,
    pub initializers: u32,
    pub reads: u32,
    pub writes: u32,
    pub write_lines: Vec<u32>,
    pub message: String,
    /// Every reference site; populated only when `--explain` is set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ReferenceEntry>,
}

/// Per-classification counts.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub unused: u32,
    pub write_only: u32,
    pub read_only: u32,
    pub derive_only: u32,
}

impl Summary {
    fn from_findings(findings: &[Finding]) -> Self {
        let mut summary = Summary::default();
        for finding in findings {
            match finding.classification {
                Classification::Unused => summary.unused += 1,
                Classification::WriteOnly => summary.write_only += 1,
                Classification::ReadOnly => summary.read_only += 1,
                Classification::DeriveOnly => summary.derive_only += 1,
            }
        }
        summary
    }
}

/// The full result of a `check` run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub workspace: PathBuf,
    pub findings: Vec<Finding>,
    pub summary: Summary,
}

impl Report {
    /// Version of the JSON output schema.
    pub const SCHEMA_VERSION: &'static str = "0.1";

    /// Build a report from an already-ordered list of findings.
    pub fn new(workspace: PathBuf, findings: Vec<Finding>) -> Self {
        let summary = Summary::from_findings(&findings);
        Report {
            schema_version: Self::SCHEMA_VERSION,
            workspace,
            findings,
            summary,
        }
    }
}
