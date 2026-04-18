//! HScript — a Haxe-subset tree-walking interpreter used for Psych Engine mod
//! compatibility. We re-use Rayzor's Haxe parser for the front-end (lexer +
//! AST) and implement a minimal evaluator over [`rayzor_parser::ExprKind`].
//!
//! HScript as shipped with Psych Engine accepts raw top-level statements, not
//! full Haxe files. We parse by wrapping the source in a dummy class so the
//! full Haxe parser is happy, then lift function/var declarations into globals
//! on the interpreter.

pub mod host;
pub mod interp;
pub mod scope;
pub mod value;

use rayzor_parser::haxe_ast::{Function, HaxeFile, ModuleFieldKind, TypeDeclaration};
use thiserror::Error;

pub use host::{HostBridge, NoopHost};
pub use interp::Interp;
pub use scope::Scope;
pub use value::Value;

#[derive(Debug, Error)]
pub enum HScriptError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("unsupported construct: {0}")]
    Unsupported(&'static str),
}

pub type HResult<T> = Result<T, HScriptError>;

/// Parse a Psych-style HScript source (top-level statements) into a [`HaxeFile`].
///
/// Rayzor's parser wants a real Haxe file; HScript sources don't have a class
/// wrapper, so we try parsing twice — raw first, and if that fails, wrapped in
/// a synthetic class.
pub fn parse(name: &str, source: &str) -> HResult<HaxeFile> {
    if let Ok(file) = rayzor_parser::parse_haxe_file(name, source, true) {
        return Ok(file);
    }

    let wrapped = format!("class __HScriptMain__ {{\n{}\n}}\n", source);
    rayzor_parser::parse_haxe_file(name, &wrapped, true).map_err(HScriptError::Parse)
}

/// Extract top-level function definitions from a parsed HScript file. Walks
/// both module-level fields (`function foo()` at file scope) and the synthetic
/// wrapper class's fields.
pub fn collect_top_level_functions(file: &HaxeFile) -> Vec<Function> {
    let mut out = Vec::new();

    for mf in &file.module_fields {
        if let ModuleFieldKind::Function(f) = &mf.kind {
            out.push(f.clone());
        }
    }

    for decl in &file.declarations {
        if let TypeDeclaration::Class(class) = decl {
            for field in &class.fields {
                if let rayzor_parser::haxe_ast::ClassFieldKind::Function(f) = &field.kind {
                    out.push(f.clone());
                }
            }
        }
    }

    out
}

/// Extract top-level variable declarations (name + initializer) from a parsed
/// HScript file. Mirrors `collect_top_level_functions` for `var`/`final`.
pub fn collect_top_level_vars(
    file: &HaxeFile,
) -> Vec<(String, Option<rayzor_parser::haxe_ast::Expr>)> {
    let mut out = Vec::new();

    for mf in &file.module_fields {
        match &mf.kind {
            ModuleFieldKind::Var { name, expr, .. } | ModuleFieldKind::Final { name, expr, .. } => {
                out.push((name.clone(), expr.clone()));
            }
            _ => {}
        }
    }

    for decl in &file.declarations {
        if let TypeDeclaration::Class(class) = decl {
            for field in &class.fields {
                match &field.kind {
                    rayzor_parser::haxe_ast::ClassFieldKind::Var { name, expr, .. }
                    | rayzor_parser::haxe_ast::ClassFieldKind::Final { name, expr, .. } => {
                        out.push((name.clone(), expr.clone()));
                    }
                    _ => {}
                }
            }
        }
    }

    out
}
