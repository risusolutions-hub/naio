//! Module / declaration scanning via `niao_parser`.

use niao_ast::{FnDef, Program, StructDef, TopLevel, TypeName};
use niao_parser::{parse, ParseError};

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub type_name: Option<String>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct SignatureInfo {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub return_type: Option<String>,
    pub arity: usize,
    pub line: usize,
    pub col: usize,
    pub span_start: usize,
    pub span_end: usize,
}

#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub functions: Vec<SignatureInfo>,
    pub structs: Vec<MemberInfo>,
    pub classes: Vec<MemberInfo>,
    pub traits: Vec<MemberInfo>,
    pub imports: Vec<String>,
    pub errors: Vec<String>,
}

pub fn format_type_name(ty: &TypeName) -> String {
    match ty {
        TypeName::Int => "int".into(),
        TypeName::Float => "float".into(),
        TypeName::String => "string".into(),
        TypeName::Bool => "bool".into(),
        TypeName::Void => "void".into(),
        TypeName::Array => "array".into(),
        TypeName::Error => "error".into(),
        TypeName::Named(n) => n.clone(),
    }
}

fn params_from_fn(def: &FnDef) -> Vec<ParamInfo> {
    def.params
        .iter()
        .map(|p| ParamInfo {
            name: p.name.clone(),
            type_name: p.ty.as_ref().map(format_type_name),
            line: p.span.line,
            col: p.span.col,
        })
        .collect()
}

fn signature_from_fn(def: &FnDef) -> SignatureInfo {
    SignatureInfo {
        name: def.name.clone(),
        params: params_from_fn(def),
        return_type: def.return_type.as_ref().map(format_type_name),
        arity: def.params.len(),
        line: def.span.line,
        col: def.span.col,
        span_start: def.span.start,
        span_end: def.span.end,
    }
}

pub fn module_members(program: &Program) -> ModuleInfo {
    let mut info = ModuleInfo {
        functions: Vec::new(),
        structs: Vec::new(),
        classes: Vec::new(),
        traits: Vec::new(),
        imports: Vec::new(),
        errors: Vec::new(),
    };

    for item in &program.items {
        match item {
            TopLevel::Fn(f) => info.functions.push(signature_from_fn(f)),
            TopLevel::Struct(s) => info.structs.push(member_from_struct(s)),
            TopLevel::Class(c) => {
                info.classes.push(MemberInfo {
                    name: c.name.clone(),
                    kind: "class".into(),
                    line: c.span.line,
                });
                for m in &c.members {
                    match m {
                        niao_ast::ClassMember::Method { def, .. }
                        | niao_ast::ClassMember::StaticMethod { def, .. } => {
                            info.functions.push(signature_from_fn(def));
                        }
                        _ => {}
                    }
                }
            }
            TopLevel::Trait(t) => {
                info.traits.push(MemberInfo {
                    name: t.name.clone(),
                    kind: "trait".into(),
                    line: t.span.line,
                });
            }
            TopLevel::Import(imp) => info.imports.push(imp.path.clone()),
            _ => {}
        }
    }
    info
}

fn member_from_struct(s: &StructDef) -> MemberInfo {
    MemberInfo {
        name: s.name.clone(),
        kind: "struct".into(),
        line: s.span.line,
    }
}

pub fn parse_module_info(source: &str) -> ModuleInfo {
    match parse(source) {
        Ok(program) => module_members(&program),
        Err(e) => ModuleInfo {
            functions: Vec::new(),
            structs: Vec::new(),
            classes: Vec::new(),
            traits: Vec::new(),
            imports: Vec::new(),
            errors: vec![format_parse_error(&e)],
        },
    }
}

fn format_parse_error(e: &ParseError) -> String {
    match e {
        ParseError::Eof => "unexpected end of file".into(),
        ParseError::Unexpected {
            found,
            expected,
            line,
            col,
        } => format!("line {line}, col {col}: expected {expected}, found {found}"),
        ParseError::Lex(le) => format!("lexer error: {le}"),
    }
}

pub fn find_decl_by_name(source: &str, name: &str) -> Option<SignatureInfo> {
    let program = parse(source).ok()?;
    for item in &program.items {
        if let TopLevel::Fn(f) = item {
            if f.name == name {
                return Some(signature_from_fn(f));
            }
        }
    }
    for item in &program.items {
        if let TopLevel::Class(c) = item {
            for m in &c.members {
                match m {
                    niao_ast::ClassMember::Method { def, .. }
                    | niao_ast::ClassMember::StaticMethod { def, .. }
                        if def.name == name =>
                    {
                        return Some(signature_from_fn(def));
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

pub fn format_signature(sig: &SignatureInfo) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| {
            if let Some(ty) = &p.type_name {
                format!("{}: {}", p.name, ty)
            } else {
                p.name.clone()
            }
        })
        .collect();
    let ret = sig
        .return_type
        .as_ref()
        .map(|t| format!(" -> {t}"))
        .unwrap_or_default();
    format!("{}({}){ret}", sig.name, params.join(", "))
}

/// Parallel scan of many sources — hot path for bulk doc/signature indexing.
pub fn scan_sources_parallel(sources: &[(String, String)]) -> Vec<(String, ModuleInfo)> {
    use rayon::prelude::*;
    sources
        .par_iter()
        .map(|(path, src)| (path.clone(), parse_module_info(src)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_functions() {
        let src = r#"
fn add(a: int, b: int) -> int {
    return a + b
}
"#;
        let info = parse_module_info(src);
        assert_eq!(info.functions.len(), 1);
        assert_eq!(info.functions[0].name, "add");
        assert_eq!(info.functions[0].arity, 2);
    }
}
