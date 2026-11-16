//! Prove total, memory-free current generated call DAGs after CFG compaction.
//! Unknown effects, traps, recursion and undef/poison are never declarations of
//! purity. Attributes travel with current definitions into every declaration.
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use zil_llvm_api::model::{Attr, Function, Item, Linkage, Module, Op, Ty, Val};
fn scalar(v: &Val) -> bool {
    matches!(v, Val::Local(_) | Val::Int(_) | Val::Bool(_) | Val::Zero)
}
fn successor(op: &Op) -> Vec<&str> {
    match op {
        Op::Branch(n) => vec![n],
        Op::CondBranch { yes, no, .. } => vec![yes, no],
        _ => vec![],
    }
}
fn dependencies(f: &Function) -> Option<BTreeSet<String>> {
    let blocks = f.blocks.as_ref()?;
    if blocks.is_empty() {
        return None;
    }
    let labels: HashMap<_, _> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect();
    let mut reachable = BTreeSet::new();
    let mut pending = vec![0];
    let mut deps = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if !reachable.insert(index) {
            continue;
        }
        let block = blocks.get(index)?;
        match &block.instructions.last()?.op {
            Op::Return(Some(arg)) if arg.ty == Ty::Int(32) => {}
            Op::Branch(_) | Op::CondBranch { .. } => {}
            _ => return None,
        }
        for inst in &block.instructions {
            if !inst
                .op
                .values()
                .all(|v| matches!(v, Val::Global(_)) || scalar(v))
            {
                return None;
            }
            match &inst.op {
                Op::Binary {
                    opcode,
                    ty: Ty::Int(bits @ (1 | 32)),
                    right,
                    ..
                } => match opcode.as_str() {
                    "add" | "sub" | "mul" | "and" | "or" | "xor" => {}
                    "srem" | "urem" | "sdiv" | "udiv" => {
                        if *bits != 32
                            || !matches!(right,Val::Int(n)if *n>0 && *n<=i32::MAX as i128)
                        {
                            return None;
                        }
                    }
                    "shl" | "lshr" | "ashr" => {
                        if !matches!(right,Val::Int(n)if (0..i128::from(*bits)).contains(n)) {
                            return None;
                        }
                    }
                    _ => return None,
                },
                Op::Compare {
                    ty: Ty::Int(32) | Ty::Int(1),
                    ..
                }
                | Op::Phi {
                    ty: Ty::Int(32) | Ty::Int(1),
                    ..
                }
                | Op::Select {
                    ty: Ty::Int(32) | Ty::Int(1),
                    ..
                }
                | Op::Branch(_)
                | Op::CondBranch { .. }
                | Op::Return(Some(_)) => {}
                Op::Call {
                    result: Ty::Int(32),
                    callee: Val::Global(name),
                    args,
                    vararg: None,
                    result_attrs,
                } if result_attrs.is_empty()
                    && args
                        .iter()
                        .all(|a| a.ty == Ty::Int(32) && a.attrs.is_empty() && scalar(&a.value)) =>
                {
                    deps.insert(name.clone());
                }
                _ => return None,
            }
        }
        for name in successor(&block.instructions.last()?.op) {
            pending.push(*labels.get(name)?);
        }
    }
    let mut indegree = vec![0usize; blocks.len()];
    for &index in &reachable {
        for name in successor(&blocks[index].instructions.last()?.op) {
            let &next = labels.get(name)?;
            indegree[next] += 1;
        }
    }
    let mut ready: VecDeque<_> = reachable
        .iter()
        .copied()
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut seen = 0;
    while let Some(index) = ready.pop_front() {
        seen += 1;
        for name in successor(&blocks[index].instructions.last()?.op) {
            let &next = labels.get(name)?;
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push_back(next);
            }
        }
    }
    (seen == reachable.len()).then_some(deps)
}
pub(crate) fn mark(
    module: &mut Module,
    protected: &BTreeSet<String>,
    preflight: &mut crate::machine_preflight::CurrentPreflight,
) -> usize {
    // These three facts are minted here, never inherited from an older graph.
    // The normal/fallback pipelines start unattributed, but clearing them also
    // makes a future recheck after a callee edit conservative by construction.
    let ours = |a: &Attr| matches!(a, Attr::NoUnwind | Attr::WillReturn | Attr::MemoryNone);
    if module.items.iter().any(|item| {
        matches!(item, Item::Function(f)
        if (f.name.starts_with("zil.fn.") || f.name.starts_with("zil.class."))
            && f.attrs.iter().any(ours))
    }) {
        for item in module.items.iter_mut() {
            if let Item::Function(f) = item
                && (f.name.starts_with("zil.fn.") || f.name.starts_with("zil.class."))
            {
                f.attrs.retain(|a| !ours(a));
            }
        }
    }
    if preflight.check(module).is_err() {
        return 0;
    }
    // Effect facts are universal for the current body. Address-taken functions
    // received no narrowed argument assumptions, so their actual pure bodies
    // may still be annotated; no caller range is imported as an effect proof.
    let mut body_cache: HashMap<usize, Vec<(&Function, Option<BTreeSet<String>>)>> = HashMap::new();
    let candidates: BTreeMap<_, _> = module
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let Item::Function(f) = item else {
                return None;
            };
            if f.blocks.is_none()
                || f.linkage == Linkage::AvailableExternally
                || f.attrs.contains(&Attr::NoInline)
                || protected.contains(&f.name)
                || !(f.name.starts_with("zil.fn.") || f.name.starts_with("zil.class."))
                || f.signature.result != Ty::Int(32)
                || f.signature.variadic
                || !f.signature.result_attrs.is_empty()
                || f.signature
                    .params
                    .iter()
                    .any(|p| p.ty != Ty::Int(32) || !p.attrs.is_empty())
            {
                return None;
            }
            let blocks = f.blocks.as_ref().unwrap();
            let key = blocks.as_ptr() as usize;
            let cached = body_cache
                .get(&key)
                .and_then(|entries| {
                    entries.iter().find(|(old, _)| {
                        old.signature == f.signature && old.blocks.as_ref().unwrap().ptr_eq(blocks)
                    })
                })
                .map(|(_, deps)| deps.clone());
            let deps = if let Some(deps) = cached {
                deps
            } else {
                let deps = dependencies(f);
                body_cache.entry(key).or_default().push((f, deps.clone()));
                deps
            };
            deps.map(|deps| (f.name.clone(), (index, deps)))
        })
        .collect();
    let mut proved = BTreeSet::new();
    for _ in 0..4 {
        let next: Vec<_> = candidates
            .iter()
            .filter(|(name, (_, deps))| {
                !proved.contains(*name) && deps.iter().all(|n| proved.contains(n))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if next.is_empty() {
            break;
        }
        proved.extend(next);
    }
    let mut changed = 0;
    for name in proved {
        let Item::Function(f) = &mut module.items[candidates[&name].0] else {
            unreachable!()
        };
        for attr in [Attr::NoUnwind, Attr::WillReturn, Attr::MemoryNone] {
            if !f.attrs.contains(&attr) {
                f.attrs.push(attr);
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Inst, Param, Signature};
    fn function(name: &str, call: Option<&str>) -> Function {
        let mut instructions = vec![];
        let returned = if let Some(callee) = call {
            instructions.push(Inst::new(
                Some("value".into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(callee.into()),
                    args: vec![Arg::new(Ty::Int(32), Val::Local("x".into()))],
                    vararg: None,
                },
            ));
            Val::Local("value".into())
        } else {
            Val::Local("x".into())
        };
        instructions.push(Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), returned))),
        ));
        Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(32), "x")]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions,
                }]
                .into(),
            ),
        }
    }
    #[test]
    fn current_call_dag_attributes_do_not_survive_a_callee_effect_or_cycle() {
        let raw = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "pure".into(),
            items: vec![
                Item::Function(function("zil.fn.0", Some("zil.fn.1"))),
                Item::Function(function("zil.fn.1", None)),
            ]
            .into(),
        };
        let mut valid = raw.clone();
        assert_eq!(
            mark(&mut valid, &BTreeSet::new(), &mut Default::default()),
            6
        );
        zil_llvm_api::validate_module(&valid).unwrap();
        for item in &valid.items {
            let Item::Function(f) = item else {
                unreachable!()
            };
            assert_eq!(
                f.attrs,
                vec![Attr::NoUnwind, Attr::WillReturn, Attr::MemoryNone]
            );
            assert_eq!(f.declaration().attrs, f.attrs);
        }
        let mut changed_after_proof = valid.clone();
        changed_after_proof.items[1] = Item::Function(function("zil.fn.1", Some("zil.fn.0")));
        assert_eq!(
            mark(
                &mut changed_after_proof,
                &BTreeSet::new(),
                &mut Default::default()
            ),
            0
        );
        for item in &changed_after_proof.items {
            if let Item::Function(f) = item {
                assert!(f.attrs.is_empty());
            }
        }
        let mut recursive = raw.clone();
        recursive.items[1] = Item::Function(function("zil.fn.1", Some("zil.fn.0")));
        let before = recursive.clone();
        assert_eq!(
            mark(&mut recursive, &BTreeSet::new(), &mut Default::default()),
            0
        );
        assert_eq!(recursive, before);
        for poison in [false, true] {
            let mut bad = raw.clone();
            let Item::Function(f) = &mut bad.items[1] else {
                unreachable!()
            };
            let block = &mut f.blocks.as_mut().unwrap()[0];
            if poison {
                block.instructions[0] =
                    Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), Val::Undef))));
            } else {
                block.instructions[0] = Inst::new(None, Op::Branch("entry".into()));
            }
            let before = bad.clone();
            assert_eq!(mark(&mut bad, &BTreeSet::new(), &mut Default::default()), 0);
            assert_eq!(bad, before);
        }
        let mut unknown = raw;
        let mut callee = function("external", None);
        callee.blocks = None;
        unknown.items.push(Item::Function(callee));
        unknown.items[1] = Item::Function(function("zil.fn.1", Some("external")));
        let before = unknown.clone();
        assert_eq!(
            mark(&mut unknown, &BTreeSet::new(), &mut Default::default()),
            0
        );
        assert_eq!(unknown, before);
    }
}
