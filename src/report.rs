//! Rendering of a [`Report`] as text or JSON.

use anyhow::Result;
use colored::Colorize;

use crate::cli::OutputFormat;
use crate::model::{Classification, Finding, Report};

/// Render a report in the requested format.
pub fn render(report: &Report, format: OutputFormat, use_color: bool) -> Result<String> {
    match format {
        OutputFormat::Text => Ok(render_text(report, use_color)),
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
    }
}

fn render_text(report: &Report, use_color: bool) -> String {
    if report.findings.is_empty() {
        return "no koyashi fields found".to_string();
    }
    report
        .findings
        .iter()
        .map(|finding| render_finding(finding, use_color))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_finding(finding: &Finding, use_color: bool) -> String {
    let label = label(finding.classification, use_color);
    let location = format!("{}:{}", finding.file.display(), finding.line);
    let mut lines = vec![format!("{label} {location:<32} {}", finding.field)];
    if let Some(counts) = counts_line(finding) {
        lines.push(format!("             ↳ {counts}"));
    }
    lines.push(format!("             ↳ {}", finding.message));
    lines.join("\n")
}

/// The counts line for a finding (`unused` has none).
fn counts_line(finding: &Finding) -> Option<String> {
    match finding.classification {
        Classification::Unused => None,
        Classification::WriteOnly => Some(if finding.write_lines.is_empty() {
            format!("{} reads, {} writes", finding.reads, finding.writes)
        } else {
            let lines = finding
                .write_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} reads, {} writes (lines {lines})",
                finding.reads, finding.writes
            )
        }),
        Classification::ReadOnly => {
            let initializer = if finding.initializers == 1 {
                "initializer"
            } else {
                "initializers"
            };
            Some(format!(
                "{} {initializer}, {} reads, {} writes",
                finding.initializers, finding.reads, finding.writes
            ))
        }
    }
}

/// The classification label, padded and optionally colored.
fn label(classification: Classification, use_color: bool) -> String {
    let text = format!("{:<12}", classification.label());
    if !use_color {
        return text;
    }
    let colored = match classification {
        Classification::Unused | Classification::WriteOnly => text.as_str().red(),
        Classification::ReadOnly => text.as_str().yellow(),
    };
    colored.to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn sample_report() -> Report {
        Report::new(
            PathBuf::from("/ws"),
            vec![
                Finding {
                    classification: Classification::WriteOnly,
                    field: "Repo::last_sync_at".to_string(),
                    file: PathBuf::from("src/repo.rs"),
                    line: 22,
                    initializers: 0,
                    reads: 0,
                    writes: 3,
                    write_lines: vec![41, 58, 73],
                    message: "field is written but its value is never observed".to_string(),
                },
                Finding {
                    classification: Classification::Unused,
                    field: "State::pending".to_string(),
                    file: PathBuf::from("src/state.rs"),
                    line: 5,
                    initializers: 0,
                    reads: 0,
                    writes: 0,
                    write_lines: vec![],
                    message: "no references found".to_string(),
                },
            ],
        )
    }

    #[test]
    fn text_output_includes_counts_and_message() {
        let text = render_text(&sample_report(), false);
        assert!(text.contains("write-only"));
        assert!(text.contains("0 reads, 3 writes (lines 41, 58, 73)"));
        assert!(!text.contains("0 initializers"));
    }

    #[test]
    fn json_output_carries_schema_version_and_summary() {
        let json = render(&sample_report(), OutputFormat::Json, false).unwrap();
        assert!(json.contains("\"schema_version\": \"0.1\""));
        assert!(json.contains("\"write_only\": 1"));
        assert!(json.contains("\"classification\": \"write-only\""));
    }

    #[test]
    fn empty_report_renders_placeholder() {
        let report = Report::new(PathBuf::from("/ws"), vec![]);
        assert_eq!(render_text(&report, false), "no koyashi fields found");
    }
}
