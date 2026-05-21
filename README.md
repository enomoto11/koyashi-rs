# koyashi-rs

Detect freeloader struct fields in Rust workspaces — without running `cargo build`.

A *freeloader* field (Japanese: *koyashi*, 肥やし — "manure") looks productive but
carries no real weight: it is assigned but never read, set once and never used
again, or simply never referenced at all. `koyashi` finds these fields by asking
rust-analyzer where each field is used and classifying every reference.

## Why not just `dead_code`?

The compiler's `dead_code` lint only flags fields with *zero* references. It says
nothing about a field that is written on every request but read by no one, or one
that is initialized once and never touched again. `koyashi` looks at *how* each
field is used, not just *whether* it is used.

## Classifications

| Classification | Meaning |
| --- | --- |
| `unused` | The field has no references at all. |
| `write-only` | The field's value is placed (assigned or initialized) but never read. |
| `read-only` | The field is read but never reassigned after construction. |

Fields that are both read and written are considered healthy and are not reported.

## Requirements

- [`rust-analyzer`](https://rust-analyzer.github.io) on your `PATH`
  (or pointed to by the `KOYASHI_RUST_ANALYZER` environment variable).
- The target must be a Cargo project (it has a `Cargo.toml`).

## Installation

```bash
cargo install --path .
```

This installs the `koyashi` binary into `~/.cargo/bin`.

## Usage

```bash
koyashi check --workspace <path>
```

| Option | Description | Default |
| --- | --- | --- |
| `--workspace <dir>` | Workspace or crate root to analyze. | `.` |
| `--include <classes>` | Restrict output to comma-separated classifications. | all |
| `--exclude-tests <bool>` | Skip `#[cfg(test)]` modules. | `true` |
| `--struct <name>` | Restrict analysis to a single struct. | all |
| `--format <text\|json>` | Output format. | `text` |
| `--severity <info\|warn\|error>` | Severity threshold for the exit code. | `warn` |

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Nothing found above the `--severity` threshold. |
| `1` | A finding at or above the threshold was reported. |
| `2` | A runtime error (no Cargo project, rust-analyzer missing, etc.). |

## Example

Given a crate with one field of each kind:

```bash
koyashi check --workspace tests/fixtures/freeloaders
```

```
read-only    tests/fixtures/freeloaders/src/main.rs:8  Settings::label
             ↳ 1 initializer, 1 reads, 0 writes
             ↳ field is set once at construction and never mutated

write-only   tests/fixtures/freeloaders/src/main.rs:12 Settings::last_message
             ↳ 0 reads, 1 writes (lines 32)
             ↳ field is written but its value is never observed

unused       tests/fixtures/freeloaders/src/main.rs:18 Orphan::forgotten
             ↳ no references found
```

JSON output is available for CI integration:

```bash
koyashi check --workspace . --format json
```

## Limitations

- Fields read only through derive macros (for example a `#[derive(Serialize)]`
  field that is serialized but never read in Rust code) are currently reported as
  `write-only`.
- Taking a mutable borrow (`&mut x.field`) is counted as a write even when the
  borrow is only read from afterwards.
- References inside macro invocations are counted as plain reads.

## License

MIT
