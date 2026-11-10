#!/usr/bin/env python3
"""Apply the bounded signature-facet experiment to the frozen main source."""
from pathlib import Path
import hashlib
p=Path('crates/zil-types/src/incremental.rs'); s=p.read_text()
assert 'ZIL-FUNCTION-TYPES-2\\0' in s and 'declarations: &declarations' not in s
s=s.replace('context.update(b"ZIL-FUNCTION-TYPES-2\\0");','context.update(b"ZIL-FUNCTION-TYPES-3\\0");',1)
s=s.replace('    for module in &program.modules {\n        let core = &module.core;','''    // Keep the complete name/ID/import namespace in the context. Its size and
    // order affect resolution and accounting, even for a body that uses only
    // some declarations. Signature contents are a separate observed facet.
    let mut declarations = BTreeMap::new();
    for module in &program.modules {
        let core = &module.core;''',1)
s=s.replace('            if !f.type_parameters.is_empty()\n','''            if f.storage_life.is_some()
                || f.protocol_contract.is_some()
                || !f.type_parameters.is_empty()
''',1)
a=s.index('            let signature = tables.get(&core.module_id)?.get(&f.name)?;'); b=s.index('\n        }\n    }\n    let context = context.digest();',a)
s=s[:a]+'''            let signature = tables.get(&core.module_id)?.get(&f.name)?;
            // Adding, removing or renumbering a declaration still invalidates
            // the epoch; a signature-only edit need not invalidate non-users.
            signature.id.hash(&mut context);
            f.name.hash(&mut context);
            let mut declaration = FingerprintBuilder::new();
            signature.id.hash(&mut declaration);
            signature.public.hash(&mut declaration);
            f.name.hash(&mut declaration);
            f.call_kind.hash(&mut declaration);
            f.life.is_some().hash(&mut declaration);
            if let Some(life) = &f.life {
                life.binders.hash(&mut declaration);
                life.parameter_lives.hash(&mut declaration);
                life.result_life.hash(&mut declaration);
            }
            f.foreign.is_some().hash(&mut declaration);
            if let Some(foreign) = &f.foreign {
                foreign.abi.hash(&mut declaration);
                foreign.symbol.hash(&mut declaration);
                foreign.noescape_parameters.hash(&mut declaration);
                foreign.static_callbacks.hash(&mut declaration);
            }
            f.return_type.hash(&mut declaration);
            f.result_mode.hash(&mut declaration);
            f.parameters.len().hash(&mut declaration);
            for p in &f.parameters {
                p.name.hash(&mut declaration);
                p.ty.hash(&mut declaration);
                p.mode.hash(&mut declaration);
            }
            declarations
                .entry(core.module_id)
                .or_insert_with(BTreeMap::new)
                .insert(f.name.clone(), declaration.digest());'''+s[b:]
s=s.replace('''                module,
                tables,
            };
            encoder.hash.update(&context.0);''','''                module,
                tables,
                declarations: &declarations,
            };
            encoder.hash.update(&context.0);
            encoder.hash.update(&declarations.get(&module.core.module_id)?.get(&f.name)?.0);''',1)
s=s.replace('''    tables: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
}
impl BodyKey''','''    tables: &'a BTreeMap<ModuleId, BTreeMap<String, Signature>>,
    declarations: &'a BTreeMap<ModuleId, BTreeMap<String, Fingerprint>>,
}
impl BodyKey''',1)
a=s.index('    fn call(&self, callee: &CoreExpr) -> Option<()> {'); b=s.index('    fn expr(',a)
s=s[:a]+'''    fn call(&mut self, callee: &CoreExpr) -> Option<()> {
        let (module, name) = match &callee.kind {
            CoreExprKind::Name(name) => (self.module.core.module_id, name.as_str()),
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return None;
                };
                let import = self.module.imports.iter().find(|i| &i.alias == alias)?;
                (import.module, member.as_str())
            }
            _ => return None,
        };
        if self.tables.get(&module)?.get(name)?.foreign {
            return None;
        }
        // Bind the actual current declaration selected through the current
        // import/name tables, including visibility, ID, types and life header.
        // A callee body edit is not a Types dependency of its scalar caller;
        // Analyze and independent Verify still observe the entire current CFG.
        self.hash.update(&self.declarations.get(&module)?.get(name)?.0);
        Some(())
    }
'''+s[b:]
p.write_text(s)
p=Path('crates/zil-driver/src/function_incremental_tests.rs'); s=p.read_text()
old='''    for library in [
        format!("fn inserted()->i32{{return 5;}} {LIBRARY}"),
        format!("fn inserted(x:bool)->bool{{return x;}} {LIBRARY}"),
        LIBRARY.replace("pub fn answer", "fn answer"),
    ] {'''
new='''    for (library, reused) in [
        (format!("fn inserted()->i32{{return 5;}} {LIBRARY}"), 0),
        (format!("fn inserted(x:bool)->bool{{return x;}} {LIBRARY}"), 3),
        (LIBRARY.replace("pub fn answer", "fn answer"), 0),
    ] {'''
assert s.count(old)==1; s=s.replace(old,new)
s=s.replace('''            assert_eq!(s.function_type_stats().reused_functions, 0);
        } else {''','''            assert_eq!(s.function_type_stats().reused_functions, reused);
        } else {''',1)
s+='''

#[test]
fn function_types_signature_facets_only_invalidate_current_direct_users() {
    let d = Directory::new();
    let options = CompileOptions::default();
    let extra = " fn unrelated(x:i32)->i32{if x<3{return x+1;}else{return x+2;}}";
    let mut session = CompilationSession::default();
    success(&session.compile_project(&d.project(MAIN, &(LIBRARY.to_owned()+extra)), &options));
    let changed = extra.replace("unrelated(x:i32)", "unrelated(x:i64)")
        .replace("return x+1;", "return i32(x)+1;")
        .replace("return x+2;", "return i32(x)+2;");
    let changed = LIBRARY.to_owned()+&changed;
    let project = d.project(MAIN, &changed);
    let report = session.compile_project(&project, &options);
    success(&report);
    assert_eq!(session.function_type_stats().reused_functions, 3);
    assert_eq!(session.function_type_stats().executed_functions, 1);
    compare_fresh(&session, &report, &project, &options);

    let library = changed.replace("leaf(x:i32)", "leaf(x:i32,bias:i32)")
        .replace("leaf(41)", "leaf(41,0)");
    let project = d.project(MAIN, &library);
    let report = session.compile_project(&project, &options);
    success(&report);
    assert_eq!(session.function_type_stats().reused_functions, 2);
    assert_eq!(session.function_type_stats().executed_functions, 2);
    compare_fresh(&session, &report, &project, &options);
}

#[test]
fn function_types_signature_facets_recheck_unchanged_callers_and_current_errors() {
    let d = Directory::new();
    let options = CompileOptions::default();
    let mut session = CompilationSession::default();
    success(&session.compile_project(&d.project(MAIN, LIBRARY), &options));
    for library in [
        LIBRARY.replace("leaf(x:i32)", "leaf(x:bool)"),
        LIBRARY.replace("pub fn answer()->i32", "pub fn answer(extra:i32)->i32"),
        LIBRARY.replace("pub fn answer", "fn answer"),
    ] {
        let project = d.project(MAIN, &library);
        let report = session.compile_project(&project, &options);
        assert_ne!(report.status, ResultCategory::Success, "{}", report.to_json());
        compare_fresh(&session, &report, &project, &options);
        success(&session.compile_project(&d.project(MAIN, LIBRARY), &options));
    }
}

#[test]
fn function_types_signature_facets_survive_a_new_disk_cache_session() {
    let d = Directory::new();
    let options = CompileOptions { semantic_cache: Some(d.0.join("cache")), ..Default::default() };
    let extra = " fn unused(x:i32)->i32{if x<2{return x;}else{return x+1;}}";
    let library = LIBRARY.to_owned()+extra;
    success(&CompilationSession::default().compile_project(&d.project(MAIN, &library), &options));
    let changed = LIBRARY.to_owned()+&extra.replace("x:i32", "x:i64")
        .replace("return x;", "return i32(x);").replace("return x+1;", "return i32(x)+1;");
    let project = d.project(MAIN, &changed);
    let mut session = CompilationSession::default();
    let report = session.compile_project(&project, &options);
    success(&report);
    assert_eq!(session.function_type_stats().reused_functions, 3);
    assert_eq!(session.function_type_stats().executed_functions, 1);
    compare_fresh(&session, &report, &project, &options);
}
'''
p.write_text(s)
for name in ('crates/zil-types/src/incremental.rs','crates/zil-driver/src/function_incremental_tests.rs'):
    print(name, hashlib.sha256(Path(name).read_bytes()).hexdigest())
