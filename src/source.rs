//! Source-level analysis with `syn`.
//!
//! Two things are extracted from each file: the definitions of named struct
//! fields, and the syntactic role (read, write, or initializer) of every field
//! access. The role map lets the reference locations returned by rust-analyzer
//! be sorted into counts.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use syn::visit::{self, Visit};
use syn::{BinOp, Expr, ExprAssign, ExprBinary, ExprField, ExprReference, FieldValue, Member};

use crate::model::{FieldDef, Location, ReferenceKind};
use crate::workspace::Workspace;

/// Map from a source position to the role of the field access at that position.
pub type ReferenceKindMap = HashMap<(u32, u32), ReferenceKind>;

/// Collect every named struct field defined in the workspace.
///
/// `struct_filter` restricts collection to a single struct name. When
/// `exclude_tests` is set, `#[cfg(test)]` modules are skipped.
pub fn collect_field_defs(
    workspace: &Workspace,
    struct_filter: Option<&str>,
    exclude_tests: bool,
) -> Result<Vec<FieldDef>> {
    let mut defs = Vec::new();
    for krate in &workspace.crates {
        if krate.src_root.is_dir() {
            collect_in_dir(&krate.src_root, struct_filter, exclude_tests, &mut defs)?;
        }
    }
    defs.sort_by(|a, b| {
        a.location
            .file
            .cmp(&b.location.file)
            .then(a.location.line.cmp(&b.location.line))
            .then(a.field_name.cmp(&b.field_name))
    });
    Ok(defs)
}

/// Classify every field access in `file` by its syntactic role.
pub fn reference_kinds(file: &Path) -> Result<ReferenceKindMap> {
    let content =
        fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;
    let ast =
        syn::parse_file(&content).with_context(|| format!("failed to parse {}", file.display()))?;

    let mut collector = KindCollector {
        kinds: HashMap::new(),
    };
    collector.visit_file(&ast);
    Ok(collector.kinds)
}

fn collect_in_dir(
    dir: &Path,
    struct_filter: Option<&str>,
    exclude_tests: bool,
    defs: &mut Vec<FieldDef>,
) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("failed to read directory {}", dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_in_dir(&path, struct_filter, exclude_tests, defs)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            collect_in_file(&path, struct_filter, exclude_tests, defs)?;
        }
    }
    Ok(())
}

fn collect_in_file(
    path: &Path,
    struct_filter: Option<&str>,
    exclude_tests: bool,
    defs: &mut Vec<FieldDef>,
) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ast = syn::parse_file(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    visit_items(&ast.items, path, struct_filter, exclude_tests, defs);
    Ok(())
}

fn visit_items(
    items: &[syn::Item],
    file: &Path,
    struct_filter: Option<&str>,
    exclude_tests: bool,
    defs: &mut Vec<FieldDef>,
) {
    for item in items {
        match item {
            syn::Item::Struct(item_struct) => {
                let struct_name = item_struct.ident.to_string();
                if struct_filter.is_some_and(|wanted| wanted != struct_name) {
                    continue;
                }
                if let syn::Fields::Named(named) = &item_struct.fields {
                    for field in &named.named {
                        let Some(ident) = &field.ident else { continue };
                        let start = ident.span().start();
                        defs.push(FieldDef {
                            struct_name: struct_name.clone(),
                            field_name: ident.to_string(),
                            location: Location {
                                file: file.to_path_buf(),
                                line: start.line as u32,
                                character: start.column as u32,
                            },
                        });
                    }
                }
            }
            syn::Item::Mod(item_mod) => {
                if exclude_tests && has_cfg_test(&item_mod.attrs) {
                    continue;
                }
                if let Some((_, inner)) = &item_mod.content {
                    visit_items(inner, file, struct_filter, exclude_tests, defs);
                }
            }
            _ => {}
        }
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if let syn::Meta::List(list) = &attr.meta {
            list.path.is_ident("cfg") && list.tokens.to_string().contains("test")
        } else {
            false
        }
    })
}

/// Walks an AST and records the role of every named field access.
struct KindCollector {
    kinds: ReferenceKindMap,
}

impl KindCollector {
    fn record(&mut self, ident: &proc_macro2::Ident, kind: ReferenceKind) {
        let start = ident.span().start();
        self.kinds
            .insert((start.line as u32, start.column as u32), kind);
    }

    /// Visit an expression in a write position: its outermost field access is
    /// the field being written, while its base is an ordinary read.
    fn visit_place(&mut self, expr: &Expr) {
        if let Expr::Field(field) = expr {
            if let Member::Named(ident) = &field.member {
                self.record(ident, ReferenceKind::Write);
            }
            self.visit_expr(&field.base);
        } else {
            self.visit_expr(expr);
        }
    }
}

impl<'ast> Visit<'ast> for KindCollector {
    fn visit_expr_field(&mut self, node: &'ast ExprField) {
        if let Member::Named(ident) = &node.member {
            self.record(ident, ReferenceKind::Read);
        }
        self.visit_expr(&node.base);
    }

    fn visit_expr_assign(&mut self, node: &'ast ExprAssign) {
        self.visit_place(&node.left);
        self.visit_expr(&node.right);
    }

    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        if is_compound_assignment(&node.op) {
            self.visit_place(&node.left);
            self.visit_expr(&node.right);
        } else {
            visit::visit_expr_binary(self, node);
        }
    }

    fn visit_expr_reference(&mut self, node: &'ast ExprReference) {
        if node.mutability.is_some() {
            self.visit_place(&node.expr);
        } else {
            visit::visit_expr_reference(self, node);
        }
    }

    fn visit_field_value(&mut self, node: &'ast FieldValue) {
        if let Member::Named(ident) = &node.member {
            self.record(ident, ReferenceKind::Initializer);
        }
        self.visit_expr(&node.expr);
    }
}

fn is_compound_assignment(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::AddAssign(_)
            | BinOp::SubAssign(_)
            | BinOp::MulAssign(_)
            | BinOp::DivAssign(_)
            | BinOp::RemAssign(_)
            | BinOp::BitXorAssign(_)
            | BinOp::BitAndAssign(_)
            | BinOp::BitOrAssign(_)
            | BinOp::ShlAssign(_)
            | BinOp::ShrAssign(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_names(src: &str, exclude_tests: bool) -> Vec<String> {
        let ast = syn::parse_file(src).expect("valid source");
        let mut defs = Vec::new();
        visit_items(
            &ast.items,
            Path::new("test.rs"),
            None,
            exclude_tests,
            &mut defs,
        );
        defs.into_iter().map(|d| d.display_name()).collect()
    }

    fn kinds(src: &str) -> Vec<ReferenceKind> {
        let ast = syn::parse_file(src).expect("valid source");
        let mut collector = KindCollector {
            kinds: HashMap::new(),
        };
        collector.visit_file(&ast);
        let mut entries: Vec<_> = collector.kinds.into_iter().collect();
        entries.sort_by_key(|(position, _)| *position);
        entries.into_iter().map(|(_, kind)| kind).collect()
    }

    #[test]
    fn collects_named_struct_fields() {
        let src = "struct User { id: u32, name: String }";
        assert_eq!(field_names(src, true), ["User::id", "User::name"]);
    }

    #[test]
    fn descends_into_modules() {
        let src = "mod inner { struct Inner { value: u8 } }";
        assert_eq!(field_names(src, true), ["Inner::value"]);
    }

    #[test]
    fn skips_cfg_test_modules_when_excluding() {
        let src = "#[cfg(test)] mod tests { struct Fixture { x: u8 } }";
        assert!(field_names(src, true).is_empty());
        assert_eq!(field_names(src, false), ["Fixture::x"]);
    }

    #[test]
    fn ignores_tuple_structs() {
        let src = "struct Point(u32, u32);";
        assert!(field_names(src, true).is_empty());
    }

    #[test]
    fn assignment_target_is_a_write() {
        let src = "fn f(mut x: T) { x.field = 1; }";
        assert_eq!(kinds(src), [ReferenceKind::Write]);
    }

    #[test]
    fn compound_assignment_target_is_a_write() {
        let src = "fn f(mut x: T) { x.field += 1; }";
        assert_eq!(kinds(src), [ReferenceKind::Write]);
    }

    #[test]
    fn mutable_borrow_is_a_write_and_plain_borrow_is_a_read() {
        assert_eq!(
            kinds("fn f(x: T) { let _ = &mut x.field; }"),
            [ReferenceKind::Write]
        );
        assert_eq!(
            kinds("fn f(x: T) { let _ = &x.field; }"),
            [ReferenceKind::Read]
        );
    }

    #[test]
    fn struct_literal_member_is_an_initializer() {
        let src = "fn f() -> T { T { field: 1 } }";
        assert_eq!(kinds(src), [ReferenceKind::Initializer]);
    }

    #[test]
    fn assignment_base_is_a_read() {
        // Writing `x.inner.field` writes `field` but reads `inner`.
        let src = "fn f(mut x: T) { x.inner.field = 1; }";
        let mut roles = kinds(src);
        roles.sort_by_key(|kind| format!("{kind:?}"));
        assert_eq!(roles, [ReferenceKind::Read, ReferenceKind::Write]);
    }
}
