//! Public frontend, including bounded local source imports. No driver/LLVM dependency.
use crate::budget::R;
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CoreImport, CoreProgram, TypedModule};
fn errors(d: Vec<zil_base::Diagnostic>) -> String {
    d.iter().map(|d| d.to_json()).collect::<Vec<_>>().join("\n")
}
pub fn text(name: &str, text: &str) -> R<TypedModule> {
    let s = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let syntax = zil_syntax::parse(&s).into_result().map_err(errors)?;
    let core = zil_expand::expand(&s, &syntax).map_err(errors)?;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).map_err(errors)
}
pub fn load(path: &Path) -> R<(TypedModule, Vec<(PathBuf, String)>)> {
    let mut paths = vec![path.to_path_buf()];
    let mut ids = BTreeMap::from([(path.to_path_buf(), 0usize)]);
    let mut todo = VecDeque::from([0]);
    let mut sources = Vec::new();
    let mut syntax = Vec::new();
    let mut imports = Vec::new();
    let mut files = Vec::new();
    let mut bytes = Vec::new();
    while let Some(id) = todo.pop_front() {
        if id != sources.len() {
            return Err("InternalError: source ordering".into());
        }
        if id >= 32 {
            return Err("ResourceLimit: source modules".into());
        }
        let path = &paths[id];
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.len() > 1_048_576 {
            return Err("ResourceLimit: source bytes".into());
        }
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        bytes.extend_from_slice(data.as_bytes());
        let source = SourceUnit::new(
            FileId(id as u32),
            ModuleId(id as u32),
            path.to_string_lossy(),
            data.as_str(),
        );
        let tree = zil_syntax::parse(&source).into_result().map_err(errors)?;
        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut edges = vec![];
        for item in &tree.items {
            if let zil_syntax::ast::ItemKind::Import(import) = &item.kind {
                if import.syntax || import.path.len() != 1 {
                    return Err(
                        "Unsupported: research loader admits one-level local module imports".into(),
                    );
                }
                let name = &import.path[0];
                if name.contains('/') || name.contains("..") {
                    return Err("Unsupported: import path".into());
                }
                let next = directory.join(format!("{name}.zil"));
                let module = if let Some(&v) = ids.get(&next) {
                    v
                } else {
                    let v = paths.len();
                    if v >= 32 {
                        return Err("ResourceLimit: source modules".into());
                    }
                    ids.insert(next.clone(), v);
                    paths.push(next);
                    todo.push_back(v);
                    v
                };
                edges.push(CoreImport {
                    alias: import.alias.clone().unwrap_or_else(|| name.clone()),
                    module: ModuleId(module as u32),
                    span: item.span,
                });
            }
        }
        files.push((paths[id].clone(), data));
        sources.push(source);
        syntax.push(tree);
        imports.push(edges);
    }
    if sources.len() == 1 && imports[0].is_empty() {
        return Ok((text(&path.to_string_lossy(), &files[0].1)?, files));
    }
    let refs = syntax.iter().collect::<Vec<_>>();
    let expanded =
        zil_expand::expand_program_with_limits(&sources, &refs, &imports, &Limits::default())
            .map_err(errors)?;
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::of(&bytes),
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    };
    let typed = zil_types::check_program(&program, &TargetProfile::host_llvm(), &Limits::default())
        .map_err(errors)?;
    Ok((typed.module, files))
}
