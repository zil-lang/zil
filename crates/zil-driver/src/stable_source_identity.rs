//! Request-local spelling candidates for the isolated closed-executable trial.
//! These hints grant no authority and are never stored in frontend caches.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zil_base::{Fingerprint, ModuleId, SourceUnit, Span};
use zil_ir::{FunctionId, PassingMode, Type, TypedModule};
use zil_syntax::ast;

/// Share the cheap option gate at discovery and capture. Unsupported modes
/// must not pay for a whole-project source identity index they cannot use.
pub(super) fn eligible(options: &super::CompileOptions) -> bool {
    options.profile == super::ExecutionProfile::Native
        && options.stop_after == super::Stage::Build
        && options.closed_executable
        && options.target.triple == "x86_64-unknown-linux-gnu"
        && options.backend == super::NativeBackend::LlvmApi
        && options.optimization == super::Optimization::O2
        && options.incremental_o2
        && options.entry.is_none()
        && (2..=64).contains(&options.codegen.units)
}

// Separate optimization bounds: a miss preserves the ordinary pipeline and
// never consumes or changes its acceptance budgets.
const MAX_ITEMS: usize = 65_536;
const MAX_DECLARATIONS: usize = 10_000;
const MAX_MODULES: usize = 10_000;
const MAX_SIGNATURE_ENTRIES: usize = 100_000;
const MAX_IDENTITY_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 16 * 1024;

type SpanKey = (u32, usize, usize);

struct Declaration {
    module: ModuleId,
    name: String,
    key: String,
    parameters: Vec<Type>,
    result: Type,
}

pub(super) struct SourceIdentities {
    root: ModuleId,
    fingerprint: Fingerprint,
    declarations: BTreeMap<SpanKey, Declaration>,
}

/// Bounded discovery-only ownership. Parsed syntax shares these strings rather
/// than cloning a potentially long canonical namespace for every module twice.
pub(super) struct CanonicalModules {
    root: PathBuf,
    keys: Vec<Arc<str>>,
    bytes_left: usize,
}

impl CanonicalModules {
    pub(super) fn new(root: &Path) -> Option<Self> {
        let key = module_key(root, root)?;
        Some(Self {
            root: root.to_owned(),
            bytes_left: MAX_IDENTITY_BYTES.checked_sub(key.len())?,
            keys: vec![Arc::from(key)],
        })
    }

    pub(super) fn push(&mut self, module: &Path) -> Option<()> {
        if self.keys.len() >= MAX_MODULES {
            return None;
        }
        let key = module_key(&self.root, module)?;
        let bytes_left = self.bytes_left.checked_sub(key.len())?;
        self.keys.push(Arc::from(key));
        self.bytes_left = bytes_left;
        Some(())
    }

    pub(super) fn get(&self, index: usize) -> Option<Arc<str>> {
        self.keys.get(index).cloned()
    }
}

/// Only the filesystem loader calls this, with its actual canonical paths.
/// Requiring UTF-8 and a root-relative module is a conservative trial gate.
fn module_key(root: &Path, module: &Path) -> Option<String> {
    let namespace = root.to_str()?;
    let relative = module.strip_prefix(root.parent()?).ok()?.to_str()?;
    let mut key = String::from("zil-closed-source-v1;");
    append(&mut key, namespace)?;
    append(&mut key, relative)?;
    Some(key)
}

/// Capture original names before Types replaces imported diagnostic labels.
/// Syntax belongs to this request's discovery; a public SourceUnit path alone
/// is not sufficient filesystem provenance.
pub(super) fn capture<'a>(
    root: ModuleId,
    fingerprint: Fingerprint,
    max_functions: usize,
    sources: &[SourceUnit],
    syntax: impl IntoIterator<Item = (&'a ast::Module, Option<&'a str>)>,
) -> Option<SourceIdentities> {
    if sources.is_empty() || sources.len() > MAX_MODULES {
        return None;
    }
    let mut syntax = syntax.into_iter();
    let mut declarations = BTreeMap::new();
    let mut module_keys = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut items_left = MAX_ITEMS;
    let declaration_limit = max_functions.min(MAX_DECLARATIONS);
    let mut signature_entries_left = MAX_SIGNATURE_ENTRIES;
    let mut bytes_left = MAX_IDENTITY_BYTES;
    for source in sources {
        let (parsed, key) = syntax.next()?;
        let key = key?;
        if parsed.id != source.module_id
            || !parsed.span.is_valid_for(source)
            || !module_keys.insert(key)
            || key.len() > MAX_KEY_BYTES
        {
            return None;
        }
        bytes_left = bytes_left.checked_sub(key.len())?;
        for item in &parsed.items {
            items_left = items_left.checked_sub(1)?;
            if !item.attributes.is_empty() {
                return None;
            }
            let function = match &item.kind {
                ast::ItemKind::Import(import) if !import.syntax => continue,
                // Foreign symbols keep their actual ABI spelling. The backend
                // independently gates the complete graph (including putchar).
                ast::ItemKind::Extern { .. } => continue,
                ast::ItemKind::Function(function) => function,
                _ => return None,
            };
            if !function.generics.is_empty()
                || !function.where_clause.is_empty()
                || !function.contract.is_empty()
                || matches!(&function.body, ast::FunctionBody::Declaration)
                || !function.span.is_valid_for(source)
                || !names.insert((key, function.name.as_str()))
                || declarations.len() >= declaration_limit
            {
                return None;
            }
            signature_entries_left =
                signature_entries_left.checked_sub(function.params.len().checked_add(1)?)?;
            let result = primitive(function.return_type.as_ref()?)?;
            let mut parameters = Vec::with_capacity(function.params.len());
            for parameter in &function.params {
                if parameter.give || !parameter.attributes.is_empty() {
                    return None;
                }
                parameters.push(primitive(&parameter.ty)?);
            }
            let mut stable_key = key.to_owned();
            append(&mut stable_key, "function")?;
            append(&mut stable_key, &function.name)?;
            bytes_left = bytes_left.checked_sub(stable_key.len())?;
            let declaration = Declaration {
                module: source.module_id,
                name: function.name.clone(),
                key: stable_key,
                parameters,
                result,
            };
            if declarations
                .insert(span_key(function.span), declaration)
                .is_some()
            {
                return None;
            }
        }
    }
    if syntax.next().is_some() || declarations.is_empty() {
        return None;
    }
    Some(SourceIdentities {
        root,
        fingerprint,
        declarations,
    })
}

impl SourceIdentities {
    /// Called only after this request's normal Analyze and Verify succeed.
    /// The complete bijection is rebound even for restored raw TypedProgram.
    pub(super) fn bind(&self, module: &TypedModule) -> Option<Vec<(FunctionId, String)>> {
        if module.module_id != self.root
            || module.source_fingerprint != self.fingerprint
            || !module.closures.is_empty()
            || !module.resources.is_empty()
            || !module.unsupported.is_empty()
            || module.functions.len() > MAX_ITEMS
        {
            return None;
        }
        let mut bound = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut names = Vec::with_capacity(self.declarations.len());
        for function in &module.functions {
            if function.foreign.is_some() {
                continue;
            }
            let span = span_key(function.span);
            let declaration = self.declarations.get(&span)?;
            if !bound.insert(span)
                || !ids.insert(function.id)
                || function.environment.is_some()
                || function.callable_return_contract.is_some()
                || function.callable_contract.is_some()
                || function.return_type != declaration.result
                // Expand retains Infer for an ordinary declaration with no
                // result contract; primitive Copy is not stored as Copy here.
                || function.result_mode != PassingMode::Infer
                || function.parameters.len() != declaration.parameters.len()
            {
                return None;
            }
            let expected_name = if declaration.module == self.root {
                declaration.name.clone()
            } else {
                format!("@module{}:function{}", declaration.module.0, function.id.0)
            };
            if function.name != expected_name {
                return None;
            }
            for (id, expected) in function.parameters.iter().zip(&declaration.parameters) {
                let local = function.locals.get(id.0 as usize)?;
                // Plain primitive source parameters have Copy passing mode;
                // Typed LocalDecl has no separate passing-mode field.
                if local.id != *id || local.ty != *expected {
                    return None;
                }
            }
            names.push((function.id, declaration.key.clone()));
        }
        (bound.len() == self.declarations.len()).then_some(names)
    }
}

fn span_key(span: Span) -> SpanKey {
    (span.file.0, span.start, span.end)
}

fn append(key: &mut String, value: &str) -> Option<()> {
    let length = value.len().to_string();
    let added = length.len().checked_add(1)?.checked_add(value.len())?;
    if key.len().checked_add(added)? > MAX_KEY_BYTES {
        return None;
    }
    key.push_str(&length);
    key.push(':');
    key.push_str(value);
    Some(())
}

fn primitive(ty: &ast::TypeRef) -> Option<Type> {
    match &ty.kind {
        ast::TypeKind::Tuple(types) if types.is_empty() => Some(Type::Unit),
        ast::TypeKind::Named { path, args } if args.is_empty() && path.len() == 1 => {
            Some(match path[0].as_str() {
                "bool" => Type::Bool,
                "i32" => Type::I32,
                "i64" => Type::I64,
                "u8" => Type::U8,
                "u16" => Type::U16,
                "void" => Type::Unit,
                _ => return None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, TargetProfile};
    use zil_ir::{BlockId, ClosureId, ForeignDeclaration, LocalDecl, LocalId, TypedFunction};
    use zil_syntax::ParseResult;

    const CURRENT: Fingerprint = Fingerprint([7; 32]);

    fn options() -> crate::CompileOptions {
        let mut options = crate::CompileOptions {
            stop_after: crate::Stage::Build,
            backend: crate::NativeBackend::LlvmApi,
            optimization: crate::Optimization::O2,
            closed_executable: true,
            incremental_o2: true,
            ..crate::CompileOptions::default()
        };
        options.codegen.units = 2;
        options.target.triple = "x86_64-unknown-linux-gnu".into();
        options
    }

    fn parse_fixture(file: u32, module: u32, text: &str) -> (SourceUnit, ParseResult) {
        let source = SourceUnit::new(FileId(file), ModuleId(module), "/p/main.zil", text);
        let parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        (source, parsed)
    }

    fn function(parsed: &ParseResult, index: usize) -> &ast::Function {
        let ast::ItemKind::Function(function) = &parsed.module.items[index].kind else {
            panic!("ordinary function fixture expected")
        };
        function
    }

    fn one(source: &SourceUnit, parsed: &ParseResult) -> SourceIdentities {
        let key = module_key(Path::new("/p/main.zil"), Path::new("/p/main.zil")).unwrap();
        capture(
            ModuleId(0),
            CURRENT,
            10_000,
            std::slice::from_ref(source),
            [(&parsed.module, Some(key.as_str()))],
        )
        .unwrap()
    }

    // These are metadata-only fixtures for the private binder, not Verified
    // capabilities or claims that an empty CFG is acceptable to the compiler.
    fn typed_function(function: &ast::Function, id: u32, module: u32) -> TypedFunction {
        TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(id),
            name: if module == 0 {
                function.name.clone()
            } else {
                format!("@module{module}:function{id}")
            },
            foreign: None,
            environment: None,
            parameters: (0..function.params.len())
                .map(|i| LocalId(i as u32))
                .collect(),
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: function
                .params
                .iter()
                .enumerate()
                .map(|(i, parameter)| LocalDecl {
                    id: LocalId(i as u32),
                    name: parameter.name.clone(),
                    ty: Type::I32,
                    mutable: false,
                    span: parameter.span,
                })
                .collect(),
            values: vec![],
            blocks: vec![],
            entry: BlockId(0),
            span: function.span,
        }
    }

    fn typed(functions: Vec<TypedFunction>) -> TypedModule {
        TypedModule {
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: CURRENT,
            target: TargetProfile::host_llvm(),
            functions,
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        }
    }

    #[test]
    fn option_gate_requires_every_native_trial_axis() {
        let options = options();
        assert!(eligible(&options));
        assert!(!eligible(&crate::CompileOptions::default()));
        let misses: &[fn(&mut crate::CompileOptions)] = &[
            |o| o.profile = crate::ExecutionProfile::Managed,
            |o| o.stop_after = crate::Stage::Verify,
            |o| o.closed_executable = false,
            |o| o.target.triple = "x86_64-pc-windows-msvc".into(),
            |o| o.backend = crate::NativeBackend::Clang,
            |o| o.optimization = crate::Optimization::O0,
            |o| o.incremental_o2 = false,
            |o| o.entry = Some("main".into()),
            |o| o.codegen.units = 1,
            |o| o.codegen.units = 65,
        ];
        for miss in misses {
            let mut rejected = options.clone();
            miss(&mut rejected);
            assert!(!eligible(&rejected));
        }
    }

    #[test]
    fn canonical_keys_are_framed_root_relative_and_shared_for_aliases() {
        let root = Path::new("/p/main.zil");
        assert_eq!(
            module_key(root, root).unwrap(),
            "zil-closed-source-v1;11:/p/main.zil8:main.zil"
        );
        let mut modules = CanonicalModules::new(root).unwrap();
        modules.push(Path::new("/p/dep.zil")).unwrap();
        // Discovery deduplicates canonical aliases before push. Both logical
        // aliases then use the one existing module index and the same Arc.
        let first_alias = modules.get(1).unwrap();
        let second_alias = modules.get(1).unwrap();
        assert!(Arc::ptr_eq(&first_alias, &second_alias));
        assert_ne!(modules.get(0).unwrap(), first_alias);
        assert!(module_key(root, Path::new("/outside/dep.zil")).is_none());
        assert!(CanonicalModules::new(Path::new("/p/主.zil")).is_some());
        assert_ne!(
            module_key(root, root),
            module_key(Path::new("/q/main.zil"), Path::new("/q/main.zil"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_canonical_paths_decline_without_lossy_identity() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let invalid_root = PathBuf::from(OsString::from_vec(b"/p/\xff.zil".to_vec()));
        assert!(CanonicalModules::new(&invalid_root).is_none());
        assert!(module_key(Path::new("/p/main.zil"), &invalid_root).is_none());
    }

    #[test]
    fn capture_rejects_unknown_duplicate_generic_or_unbounded_origins() {
        let (source, parsed) = parse_fixture(0, 0, "fn h(x:i32)->i32=>x+1\nfn main()->i32=>0");
        let key = module_key(Path::new("/p/main.zil"), Path::new("/p/main.zil")).unwrap();
        let capture_with = |parsed: &ParseResult, key: Option<&str>, maximum| {
            capture(
                ModuleId(0),
                CURRENT,
                maximum,
                std::slice::from_ref(&source),
                [(&parsed.module, key)],
            )
        };
        assert!(capture_with(&parsed, None, 10_000).is_none());
        assert!(capture_with(&parsed, Some(&key), 1).is_none());
        assert!(capture_with(&parsed, Some(&"x".repeat(MAX_KEY_BYTES + 1)), 10_000).is_none());
        let mut generic = parsed.clone();
        let ast::ItemKind::Function(f) = &mut generic.module.items[0].kind else {
            unreachable!()
        };
        f.generics.push(ast::GenericParam {
            name: "T".into(),
            kind: ast::GenericParamKind::Type(vec![]),
            span: f.span,
        });
        assert!(capture_with(&generic, Some(&key), 10_000).is_none());
        let mut bad_span = parsed.clone();
        let ast::ItemKind::Function(f) = &mut bad_span.module.items[0].kind else {
            unreachable!()
        };
        f.span.end = source.text.len() + 1;
        assert!(capture_with(&bad_span, Some(&key), 10_000).is_none());
        let mut duplicate = parsed.clone();
        let first = function(&duplicate, 0).clone();
        {
            let ast::ItemKind::Function(second) = &mut duplicate.module.items[1].kind else {
                unreachable!()
            };
            second.span = first.span;
        }
        assert!(capture_with(&duplicate, Some(&key), 10_000).is_none());
        let mut duplicate_name = parsed.clone();
        let ast::ItemKind::Function(second) = &mut duplicate_name.module.items[1].kind else {
            unreachable!()
        };
        second.name = first.name;
        assert!(capture_with(&duplicate_name, Some(&key), 10_000).is_none());
        let (unknown_source, unknown) = parse_fixture(0, 0, "fn h(x:Custom)->i32=>0");
        assert!(
            capture(
                ModuleId(0),
                CURRENT,
                10_000,
                &[unknown_source],
                [(&unknown.module, Some(key.as_str()))]
            )
            .is_none()
        );
    }

    #[test]
    fn bind_requires_the_whole_current_span_name_and_signature_bijection() {
        let (source, parsed) = parse_fixture(0, 0, "fn h(x:i32)->i32=>x+1\nfn main()->i32=>0");
        let origins = one(&source, &parsed);
        let module = typed(vec![
            typed_function(function(&parsed, 0), 0, 0),
            typed_function(function(&parsed, 1), 1, 0),
        ]);
        assert_eq!(origins.bind(&module).unwrap().len(), 2);
        let misses: &[fn(&mut TypedModule)] = &[
            |m| {
                m.functions.pop();
            },
            |m| {
                let mut extra = m.functions[0].clone();
                extra.id = FunctionId(99);
                extra.name = "generated".into();
                m.functions.push(extra);
            },
            |m| m.functions[0].span.end += 1,
            |m| m.functions[0].name = "another".into(),
            |m| m.functions[0].return_type = Type::I64,
            |m| m.functions[0].locals[0].ty = Type::I64,
            |m| m.functions[0].parameters[0] = LocalId(99),
            |m| m.functions[0].result_mode = PassingMode::Copy,
            |m| m.functions[0].environment = Some(ClosureId(3)),
            |m| m.functions[1].id = FunctionId(0),
            |m| m.source_fingerprint = Fingerprint([8; 32]),
            |m| m.module_id = ModuleId(1),
        ];
        for miss in misses {
            let mut rejected = module.clone();
            miss(&mut rejected);
            assert!(origins.bind(&rejected).is_none());
        }
    }

    #[test]
    fn dense_ids_file_ids_spans_and_import_labels_do_not_enter_stable_keys() {
        let root_key = module_key(Path::new("/p/main.zil"), Path::new("/p/main.zil")).unwrap();
        let dep_key = module_key(Path::new("/p/main.zil"), Path::new("/p/dep.zil")).unwrap();
        let (root, root_ast) = parse_fixture(0, 0, "fn main()->i32=>0");
        let (dep, dep_ast) = parse_fixture(1, 1, "fn h(x:i32)->i32=>x+1");
        let before = capture(
            ModuleId(0),
            CURRENT,
            10_000,
            &[root, dep],
            [
                (&root_ast.module, Some(root_key.as_str())),
                (&dep_ast.module, Some(dep_key.as_str())),
            ],
        )
        .unwrap();
        let before_names = before
            .bind(&typed(vec![
                typed_function(function(&root_ast, 0), 0, 0),
                typed_function(function(&dep_ast, 0), 1, 1),
            ]))
            .unwrap();
        let (root, root_ast) = parse_fixture(0, 0, "fn inserted()->i32=>7\nfn main()->i32=>0");
        let (dep, dep_ast) = parse_fixture(2, 2, "// shifted source span\nfn h(x:i32)->i32=>x+1");
        let changed_fingerprint = Fingerprint([8; 32]);
        let after = capture(
            ModuleId(0),
            changed_fingerprint,
            10_000,
            &[root, dep],
            [
                (&root_ast.module, Some(root_key.as_str())),
                (&dep_ast.module, Some(dep_key.as_str())),
            ],
        )
        .unwrap();
        let mut after_module = typed(vec![
            typed_function(function(&root_ast, 0), 0, 0),
            typed_function(function(&root_ast, 1), 1, 0),
            typed_function(function(&dep_ast, 0), 2, 2),
        ]);
        after_module.source_fingerprint = changed_fingerprint;
        let after_names = after.bind(&after_module).unwrap();
        assert_eq!(before_names[0].1, after_names[1].1);
        assert_eq!(before_names[1].1, after_names[2].1);
        assert_ne!(after_names[0].1, after_names[1].1);
    }

    #[test]
    fn foreign_declarations_are_not_stable_source_definitions() {
        let (source, parsed) = parse_fixture(
            0,
            0,
            "extern \"C\" fn putchar(x:i32)->i32;\nfn main()->i32=>0",
        );
        let origins = one(&source, &parsed);
        let main = typed_function(function(&parsed, 1), 1, 0);
        let mut foreign = main.clone();
        foreign.id = FunctionId(0);
        foreign.name = "putchar".into();
        foreign.foreign = Some(ForeignDeclaration {
            abi: "C".into(),
            symbol: "putchar".into(),
            noescape_parameters: vec![],
            static_callbacks: false,
        });
        let mut module = typed(vec![foreign.clone(), main]);
        let names = origins.bind(&module).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].0, FunctionId(1));
        // A source definition cannot silently disappear into the skipped set.
        module.functions[1].foreign = foreign.foreign;
        assert!(origins.bind(&module).is_none());
    }
}
