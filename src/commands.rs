//! Subcommand execution. Each handler returns a process exit code.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::cli::{CheckArgs, ClassFilter, Cli, Command, ExplainArgs, OutputFormat, Severity};
use crate::model::{
    Classification, FieldDef, Finding, ReferenceKind, ReferenceSite, Report, Summary,
};
use crate::rust_analyzer::Analyzer;
use crate::source::ReferenceKindMap;
use crate::{analysis, report, source, workspace};

/// Dispatch the CLI and return the process exit code.
pub fn run(cli: Cli) -> Result<u8> {
    match cli.command {
        Command::Check(args) => run_check(args),
        Command::Explain(args) => run_explain(args),
    }
}

fn run_check(args: CheckArgs) -> Result<u8> {
    let workspace = workspace::resolve(&args.workspace)?;
    let fields =
        source::collect_field_defs(&workspace, args.struct_name.as_deref(), args.exclude_tests)?;
    eprintln!(
        "koyashi: analyzing {} struct field(s) across {} crate(s)",
        fields.len(),
        workspace.crates.len()
    );

    let mut analyzer = Analyzer::start(&workspace.root)?;
    let mut kind_cache: HashMap<PathBuf, ReferenceKindMap> = HashMap::new();
    let mut findings = Vec::new();

    for field in &fields {
        let sites = collect_sites(&mut analyzer, &mut kind_cache, field)?;
        let stats = analysis::aggregate(&sites);
        let Some(classification) = analysis::classify(&stats, field.used_by_derive) else {
            continue;
        };
        if !include_allows(&args.include, classification) {
            continue;
        }
        findings.push(Finding {
            classification,
            field: field.display_name(),
            file: field.location.file.clone(),
            line: field.location.line,
            initializers: stats.initializers,
            reads: stats.reads,
            writes: stats.writes,
            write_lines: stats.write_lines,
            message: analysis::message_for(classification).to_string(),
        });
    }

    analyzer.shutdown()?;

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.field.cmp(&b.field))
    });
    let report = Report::new(workspace.root, findings);

    let use_color = args.format == OutputFormat::Text && std::io::stdout().is_terminal();
    println!("{}", report::render(&report, args.format, use_color)?);

    Ok(exit_code(args.severity, &report.summary))
}

fn run_explain(args: ExplainArgs) -> Result<u8> {
    let workspace = workspace::resolve(&args.workspace)?;
    let fields = source::collect_field_defs(&workspace, None, false)?;
    let matched: Vec<&FieldDef> = fields
        .iter()
        .filter(|field| field.display_name() == args.field)
        .collect();
    if matched.is_empty() {
        bail!(
            "no field named `{}` was found; expected the form `Struct::field`",
            args.field
        );
    }

    eprintln!(
        "koyashi: explaining {} field(s) matching `{}`",
        matched.len(),
        args.field
    );

    let mut analyzer = Analyzer::start(&workspace.root)?;
    let mut kind_cache: HashMap<PathBuf, ReferenceKindMap> = HashMap::new();
    let mut blocks = Vec::with_capacity(matched.len());
    for field in matched {
        let sites = collect_sites(&mut analyzer, &mut kind_cache, field)?;
        let stats = analysis::aggregate(&sites);
        let classification = analysis::classify(&stats, field.used_by_derive);
        blocks.push(report::render_explanation(field, &sites, classification));
    }
    analyzer.shutdown()?;

    println!("{}", blocks.join("\n\n"));
    Ok(0)
}

/// Resolve every reference to `field` and tag each with its syntactic kind.
fn collect_sites(
    analyzer: &mut Analyzer,
    kind_cache: &mut HashMap<PathBuf, ReferenceKindMap>,
    field: &FieldDef,
) -> Result<Vec<ReferenceSite>> {
    let references = analyzer.references(&field.location)?;
    let mut sites = Vec::with_capacity(references.len());
    for location in references {
        if !kind_cache.contains_key(&location.file) {
            let kinds = source::reference_kinds(&location.file)?;
            kind_cache.insert(location.file.clone(), kinds);
        }
        let kind = kind_cache[&location.file]
            .get(&(location.line, location.character))
            .copied()
            // A reference outside the expression AST (e.g. inside a macro) is
            // counted as a read so it is never mistaken for dead code.
            .unwrap_or(ReferenceKind::Read);
        sites.push(ReferenceSite { location, kind });
    }
    Ok(sites)
}

/// Whether a classification passes the `--include` filter (empty allows all).
fn include_allows(include: &[ClassFilter], classification: Classification) -> bool {
    include.is_empty()
        || include
            .iter()
            .any(|&filter| classification_of(filter) == classification)
}

fn classification_of(filter: ClassFilter) -> Classification {
    match filter {
        ClassFilter::Unused => Classification::Unused,
        ClassFilter::WriteOnly => Classification::WriteOnly,
        ClassFilter::ReadOnly => Classification::ReadOnly,
        ClassFilter::DeriveOnly => Classification::DeriveOnly,
    }
}

/// Map the severity threshold and counts to an exit code.
fn exit_code(severity: Severity, summary: &Summary) -> u8 {
    let dead = summary.unused > 0 || summary.write_only > 0;
    let advisory = summary.read_only > 0 || summary.derive_only > 0;
    let triggered = match severity {
        Severity::Error | Severity::Warn => dead,
        Severity::Info => dead || advisory,
    };
    u8::from(triggered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(unused: u32, write_only: u32, read_only: u32) -> Summary {
        Summary {
            unused,
            write_only,
            read_only,
            ..Summary::default()
        }
    }

    #[test]
    fn write_only_gates_at_warn_and_error() {
        assert_eq!(exit_code(Severity::Warn, &summary(0, 1, 0)), 1);
        assert_eq!(exit_code(Severity::Error, &summary(0, 1, 0)), 1);
        assert_eq!(exit_code(Severity::Warn, &summary(0, 0, 0)), 0);
    }

    #[test]
    fn read_only_only_gates_at_info() {
        assert_eq!(exit_code(Severity::Warn, &summary(0, 0, 3)), 0);
        assert_eq!(exit_code(Severity::Info, &summary(0, 0, 3)), 1);
    }

    #[test]
    fn derive_only_only_gates_at_info() {
        let derive_only = Summary {
            derive_only: 2,
            ..Summary::default()
        };
        assert_eq!(exit_code(Severity::Warn, &derive_only), 0);
        assert_eq!(exit_code(Severity::Info, &derive_only), 1);
    }

    #[test]
    fn empty_include_allows_every_classification() {
        assert!(include_allows(&[], Classification::Unused));
        assert!(include_allows(
            &[ClassFilter::Unused],
            Classification::Unused
        ));
        assert!(!include_allows(
            &[ClassFilter::Unused],
            Classification::ReadOnly
        ));
    }
}
