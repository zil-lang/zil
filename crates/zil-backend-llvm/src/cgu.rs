//! Partition only this backend's closed, generated textual IR grammar. Unknown
//! top-level forms fail closed to the original whole-module path. This is not a
//! general LLVM parser and confers no authority: build has already bound LIR to
//! the independent verifier result.
use std::collections::{BTreeMap, BTreeSet};

use zil_base::{FileId, Fingerprint};
use zil_ir::LirModule;

use crate::EntryPoint;

pub(crate) struct Plan {
    pub units: Vec<String>,
    pub fallback: Option<&'static str>,
}

struct Entity<'a> {
    name: &'a str,
    local: bool,
    text: &'a str,
    declaration: Option<String>,
}

pub(crate) fn partition(llvm: &str, lir: &LirModule, entry: EntryPoint, count: usize) -> Plan {
    let fallback = |reason| Plan {
        units: Vec::new(),
        fallback: Some(reason),
    };
    if llvm.contains("blockaddress(")
        || llvm.contains(" asm ")
        || llvm.contains("thread_local")
        || llvm.contains("@llvm.localescape")
        || llvm.contains("@llvm.localrecover")
    {
        return fallback("unsupported module-local addressing form");
    }
    let Some((header, entities)) = parse(llvm) else {
        return fallback("unrecognized generated LLVM form; original module retained");
    };
    let mut names = BTreeMap::new();
    for (id, entity) in entities.iter().enumerate() {
        if names.insert(entity.name, id).is_some() {
            return fallback("duplicate definition; original module retained for LLVM diagnostics");
        }
    }
    let mut sets = Sets((0..entities.len()).collect());
    let mut files = BTreeMap::<FileId, usize>::new();
    let mut source_ids = Vec::new();
    for function in &lir.functions {
        if function.foreign.is_some() {
            continue;
        }
        let symbol = format!("zil.fn.{}", function.id.0);
        let Some(&id) = names.get(symbol.as_str()) else {
            return fallback("generated definition missing from source grouping");
        };
        if let Some(&first) = files.get(&function.span.file) {
            sets.join(first, id);
        } else {
            files.insert(function.span.file, id);
        }
        source_ids.push((function.span.file.0, id));
        let is_entry = match entry {
            EntryPoint::Main => function.name == "main",
            EntryPoint::ReturnValue(id) => function.id == id,
            EntryPoint::None => false,
        };
        if is_entry && let Some(&wrapper) = names.get("main") {
            sets.join(id, wrapper);
        }
    }
    if let (Some(&main), Some(&pe)) = (names.get("main"), names.get("zil.pe.entry")) {
        sets.join(main, pe);
    }
    // Every use of a module-local definition must stay with that definition.
    // This includes address-taking, global initializers and transitive calls;
    // duplicating a private object or drop-function pointer is not permitted.
    let mut references = Vec::with_capacity(entities.len());
    for (id, entity) in entities.iter().enumerate() {
        let Some(refs) = symbols(entity.text) else {
            return fallback("quoted symbol");
        };
        let mut dependencies = Vec::new();
        for name in refs {
            if let Some(&definition) = names.get(name) {
                dependencies.push(definition);
                if entities[definition].local {
                    sets.join(id, definition);
                }
            }
        }
        dependencies.sort_unstable();
        dependencies.dedup();
        references.push(dependencies);
    }
    let Some(header_refs) = symbols(&header) else {
        return fallback("quoted header symbol");
    };
    if header_refs
        .iter()
        .any(|name| names.get(name).is_some_and(|id| entities[*id].local))
    {
        return fallback("module-local symbol referenced by shared prelude");
    }
    let mut roots = Vec::with_capacity(entities.len());
    for id in 0..entities.len() {
        roots.push(sets.root(id));
    }
    let mut source_keys = BTreeMap::<usize, u32>::new();
    for (file, id) in source_ids {
        source_keys
            .entry(roots[id])
            .and_modify(|old| *old = (*old).min(file))
            .or_insert(file);
    }
    let mut buckets = BTreeMap::<usize, usize>::new();
    for (id, entity) in entities.iter().enumerate() {
        buckets.entry(roots[id]).or_insert_with(|| {
            if let Some(file) = source_keys.get(&roots[id]) {
                *file as usize % count
            } else {
                let hash = Fingerprint::of(entity.name.as_bytes()).0;
                u64::from_le_bytes(hash[..8].try_into().expect("eight bytes")) as usize % count
            }
        });
    }
    let owners = roots.iter().map(|root| buckets[root]).collect::<Vec<_>>();
    let mut members = BTreeMap::<usize, Vec<usize>>::new();
    for (id, &bucket) in owners.iter().enumerate() {
        members.entry(bucket).or_default().push(id);
    }
    let header_dependencies = header_refs
        .iter()
        .filter_map(|name| names.get(name).copied())
        .collect::<BTreeSet<_>>();
    let mut units = Vec::with_capacity(members.len());
    for (bucket, definitions) in members {
        // A declaration has no body to optimize: importing the callee's own
        // dependencies would reintroduce a spurious transitive closure. All @
        // uses count, not just calls (addresses, tables, constant expressions).
        let mut needed = header_dependencies.clone();
        for &id in &definitions {
            needed.extend(references[id].iter().copied());
        }
        let mut text = header.clone();
        for id in needed {
            if owners[id] != bucket
                && let Some(declaration) = &entities[id].declaration
            {
                text.push_str(declaration);
            }
        }
        text.push('\n');
        for id in definitions {
            text.push_str(entities[id].text);
            text.push('\n');
        }
        units.push(text);
    }
    if units.len() <= 1 {
        return fallback("one linkage-connected source group");
    }
    Plan {
        units,
        fallback: None,
    }
}

// Keep original bodies, signatures and global definitions byte-for-byte. Only
// external declarations are added. No linkage, checking or arithmetic flag is
// changed, and every definition (including unreachable functions) is retained.
fn parse(llvm: &str) -> Option<(String, Vec<Entity<'_>>)> {
    let mut header = String::new();
    let mut entities = Vec::new();
    let mut lines = llvm.split_inclusive('\n');
    let mut offset = 0;
    while let Some(line) = lines.next() {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim();
        if let Some(signature) = trimmed.strip_prefix("define ") {
            let signature = signature.strip_suffix(" {")?;
            if [
                "available_externally ",
                "linkonce ",
                "linkonce_odr ",
                "weak ",
                "weak_odr ",
                "extern_weak ",
                "appending ",
                "common ",
            ]
            .iter()
            .any(|prefix| signature.starts_with(prefix))
            {
                return None;
            }
            let names = symbols(signature)?;
            if names.len() != 1 {
                return None;
            }
            let local = signature.starts_with("internal ") || signature.starts_with("private ");
            let mut closed = false;
            for next in lines.by_ref() {
                offset += next.len();
                if next.trim() == "}" {
                    closed = true;
                    break;
                }
                if next.starts_with("define ") {
                    return None;
                }
            }
            if !closed {
                return None;
            }
            entities.push(Entity {
                name: names[0],
                local,
                text: &llvm[start..offset],
                declaration: (!local).then(|| format!("declare {signature}\n")),
            });
        } else if trimmed.starts_with('@') {
            let (lhs, rhs) = trimmed.split_once(" = ")?;
            if !(rhs.starts_with("private ") || rhs.starts_with("internal ")) {
                return None;
            }
            if rhs
                .split_whitespace()
                .any(|word| matches!(word, "alias" | "ifunc" | "thread_local"))
            {
                return None;
            }
            let names = symbols(lhs)?;
            if names.len() != 1 {
                return None;
            }
            entities.push(Entity {
                name: names[0],
                local: true,
                text: &llvm[start..offset],
                declaration: None,
            });
        } else if trimmed.is_empty()
            || trimmed.starts_with(';')
            || trimmed.starts_with("target triple = ")
            || trimmed.starts_with("target datalayout = ")
            || trimmed.starts_with("source_filename = ")
            || trimmed.starts_with("declare ")
            || (trimmed.starts_with('%') && trimmed.contains(" = type "))
        {
            header.push_str(line);
        } else {
            return None;
        }
    }
    Some((header, entities))
}

// Generated names are unquoted ASCII. Ignore comments and string constants;
// a future quoted symbol causes a conservative whole-module fallback.
fn symbols(text: &str) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                if i >= bytes.len() {
                    return None;
                }
                i += 1;
            }
            b'@' => {
                i += 1;
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || b"_.$-".contains(&bytes[i]))
                {
                    i += 1;
                }
                if start == i {
                    return None;
                }
                out.push(&text[start..i]);
            }
            _ => i += 1,
        }
    }
    Some(out)
}

struct Sets(Vec<usize>);
impl Sets {
    fn root(&mut self, mut id: usize) -> usize {
        while self.0[id] != id {
            self.0[id] = self.0[self.0[id]];
            id = self.0[id];
        }
        id
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        self.0[a.max(b)] = a.min(b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_scan_ignores_strings_and_comments_and_fails_closed() {
        assert_eq!(
            symbols("@a ; @ignored\n@b = private constant [4 x i8] c\"@x\\00\""),
            Some(vec!["a", "b"])
        );
        assert!(symbols("@\"new.name\"").is_none());
        assert!(symbols("\"unterminated").is_none());
    }
    #[test]
    fn unknown_top_level_and_incomplete_definitions_fall_back() {
        for text in [
            "module asm \"ret\"\n",
            "@g = global i32 0\n",
            "define i32 @f() {\nentry:\n",
            "attributes #0 = { nounwind }\n",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
    }
    #[test]
    fn original_function_and_global_bodies_are_not_rewritten() {
        let text = "target triple = \"x86_64-unknown-linux-gnu\"\n@g = private constant i32 1\ndefine internal i32 @helper() {\n  ret i32 1\n}\ndefine i32 @f(i32 %x) {\n  ret i32 %x\n}\n";
        let (header, entities) = parse(text).unwrap();
        assert_eq!(entities.len(), 3);
        assert!(header.starts_with("target triple"));
        assert!(entities[0].local && entities[1].local && !entities[2].local);
        assert_eq!(
            entities[2].declaration.as_deref(),
            Some("declare i32 @f(i32 %x)\n")
        );
        assert_eq!(
            entities[2].text,
            "define i32 @f(i32 %x) {\n  ret i32 %x\n}\n"
        );
    }
    #[test]
    fn union_find_is_transitive_and_deterministic() {
        let mut sets = Sets((0..8).collect());
        sets.join(7, 3);
        sets.join(3, 1);
        sets.join(6, 7);
        for id in [1, 3, 6, 7] {
            assert_eq!(sets.root(id), 1);
        }
        assert_eq!(sets.root(2), 2);
    }
    fn empty_lir() -> LirModule {
        LirModule {
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            verified_fingerprint: Fingerprint::ZERO,
            target: zil_base::TargetProfile::host_llvm(),
            functions: Vec::new(),
            resources: Vec::new(),
            closures: Vec::new(),
            layouts: Vec::new(),
        }
    }
    #[test]
    fn shared_private_storage_and_internal_function_addresses_are_never_cloned() {
        let ir = "@g = private constant i32 3\ndefine internal i32 @hidden() {\n  %v = load i32, ptr @g\n  ret i32 %v\n}\ndefine ptr @left() {\n  ret ptr @hidden\n}\ndefine ptr @right() {\n  ret ptr @hidden\n}\ndefine i32 @independent() {\n  ret i32 7\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        let shared = plan
            .units
            .iter()
            .find(|u| u.contains("define internal i32 @hidden"))
            .unwrap();
        assert!(
            shared.contains("@g = private constant")
                && shared.contains("define ptr @left")
                && shared.contains("define ptr @right")
        );
        for marker in [
            "@g = private constant",
            "define internal i32 @hidden",
            "define ptr @left",
            "define ptr @right",
            "define i32 @independent",
        ] {
            assert_eq!(
                plan.units
                    .iter()
                    .map(|unit| unit.matches(marker).count())
                    .sum::<usize>(),
                1,
                "{marker}"
            );
        }
        assert!(plan.units.iter().all(|u| !u.contains("declare internal")));
    }
    #[test]
    fn unsupported_alias_linkage_and_block_addresses_keep_original_module() {
        for ir in [
            "@g = private alias i32, ptr @other\n",
            "define linkonce_odr i32 @f() {\n ret i32 0\n}\n",
            "define ptr @f() {\n ret ptr blockaddress(@f, %a)\n}\n",
        ] {
            let plan = partition(ir, &empty_lir(), EntryPoint::None, 4);
            assert!(plan.units.is_empty());
            assert!(plan.fallback.is_some());
        }
    }
    #[test]
    fn duplicate_definitions_are_not_erased_to_make_invalid_ir_link() {
        let ir = "define i32 @a() {\n ret i32 0\n}\ndefine i32 @a() {\n ret i32 1\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 4);
        assert!(plan.units.is_empty());
        assert!(plan.fallback.unwrap().contains("duplicate"));
    }
    #[test]
    fn sparse_declarations_follow_direct_uses_not_transitive_callees() {
        let ir = "define i32 @a() {\n %x = call i32 @b()\n ret i32 %x\n}\ndefine i32 @b() {\n %x = call i32 @c()\n ret i32 %x\n}\ndefine i32 @c() {\n ret i32 7\n}\ndefine i32 @dead() {\n ret i32 9\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in &plan.units {
            let (_, definitions) = parse(unit).unwrap();
            let defined = definitions.iter().map(|e| e.name).collect::<BTreeSet<_>>();
            let used = definitions
                .iter()
                .flat_map(|e| symbols(e.text).unwrap())
                .collect::<BTreeSet<_>>();
            for symbol in ["a", "b", "c", "dead"] {
                assert_eq!(
                    unit.contains(&format!("declare i32 @{symbol}()")),
                    used.contains(symbol) && !defined.contains(symbol)
                );
            }
        }
        assert_eq!(
            plan.units
                .iter()
                .map(|u| u.matches("define i32 @dead()").count())
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn global_initializer_and_function_value_uses_keep_exact_declarations() {
        let ir = "@table = private constant [1 x ptr] [ptr @target]\ndefine ptr @reader() {\n ret ptr @table\n}\ndefine ptr @address() {\n ret ptr @target\n}\ndefine i64 @target(i64 %x) {\n ret i64 %x\n}\ndefine i64 @unrelated() {\n ret i64 2\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in &plan.units {
            let uses = unit.contains("@table =") || unit.contains("define ptr @address");
            let defines = unit.contains("define i64 @target");
            assert_eq!(
                unit.contains("declare i64 @target(i64 %x)"),
                uses && !defines
            );
        }
    }
    #[test]
    fn mutual_recursion_keeps_cross_unit_prototypes_and_no_self_declaration() {
        let ir = "define i32 @left() {\n %x = call i32 @right()\n ret i32 %x\n}\ndefine i32 @right() {\n %x = call i32 @left()\n ret i32 %x\n}\ndefine i32 @independent() {\n ret i32 0\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in plan.units {
            for (caller, callee) in [("left", "right"), ("right", "left")] {
                let needed = unit.contains(&format!("define i32 @{caller}"))
                    && !unit.contains(&format!("define i32 @{callee}"));
                assert_eq!(unit.contains(&format!("declare i32 @{callee}")), needed);
            }
        }
    }
    #[test]
    fn foreign_declarations_and_header_are_retained_without_body_rewriting() {
        let ir = "; @not_a_reference\ndeclare i32 @foreign(i32)\ndefine i32 @a() {\n %v = call i32 @foreign(i32 3)\n ret i32 %v\n}\ndefine i32 @b() {\n ret i32 2\n}\n";
        let plan = partition(ir, &empty_lir(), EntryPoint::None, 64);
        assert!(plan.units.len() > 1);
        for unit in plan.units {
            assert!(unit.starts_with("; @not_a_reference\ndeclare i32 @foreign(i32)\n"));
        }
    }
}
