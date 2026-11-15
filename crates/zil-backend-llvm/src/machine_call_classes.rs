//! Exact direct-call classes for the explicitly closed executable strategy.
//!
//! The caller must preflight the COMPLETE original module before calling this
//! pass, supply every entry/export name, and retain the ordinary later checks.
//! This is not an interposition-safe optimization for open modules. Names only
//! restrict the candidate domain; complete Function equality authorizes a class.
//! Definitions, linkage, attributes, local names and all address uses survive.
use std::collections::{BTreeSet, HashMap};
use zil_llvm_api::model::{Arg, Function, Item, Linkage, Module, Op, Ty, Val};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    pub(crate) eligible_functions: usize,
    pub(crate) equivalence_classes: usize,
    pub(crate) rewritten_calls: usize,
}

fn integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Int(1 | 8 | 16 | 32 | 64))
}

fn scalar(value: &Val) -> bool {
    matches!(
        value,
        Val::Local(_) | Val::Int(_) | Val::Bool(_) | Val::Zero
    )
}

fn integer_arg(arg: &Arg) -> bool {
    integer(&arg.ty) && scalar(&arg.value)
}

fn overflow_pair(ty: &Ty) -> bool {
    matches!(ty, Ty::Struct(fields) if fields.len() == 2
        && matches!(fields[0], Ty::Int(8 | 16 | 32 | 64)) && fields[1] == Ty::Int(1))
}

fn generated(name: &str) -> bool {
    name.strip_prefix("zil.fn.")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

fn intrinsic(function: &Function) -> bool {
    let sig = &function.signature;
    if function.blocks.is_some()
        || function.linkage != Linkage::External
        || !function.attrs.is_empty()
        || sig.variadic
        || !sig.result_attrs.is_empty()
        || sig.params.iter().any(|p| !p.attrs.is_empty())
    {
        return false;
    }
    if function.name == "llvm.trap" {
        return sig.result == Ty::Void && sig.params.is_empty();
    }
    if !overflow_pair(&sig.result) || sig.params.len() != 2 {
        return false;
    }
    let Ty::Struct(fields) = &sig.result else {
        return false;
    };
    let Ty::Int(bits) = fields[0] else {
        return false;
    };
    sig.params.iter().all(|p| p.ty == Ty::Int(bits))
        && ["sadd", "ssub", "smul", "uadd", "usub", "umul"]
            .iter()
            .any(|op| function.name == format!("llvm.{op}.with.overflow.i{bits}"))
}

// Walk addresses in unsupported operations too. In particular, a function
// passed as an argument to a direct call is an address use, not a call edge.
// Iteration avoids depending on the nesting depth of constant aggregates.
fn address_uses(
    value: &Val,
    addresses: &mut BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<()> {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        if cancelled() {
            return None;
        }
        match value {
            Val::Global(name) => {
                addresses.insert(name.clone());
            }
            Val::Array(args) => pending.extend(args.iter().map(|arg| &arg.value)),
            Val::Gep { ptr, indices, .. } => {
                pending.push(ptr);
                pending.extend(indices.iter().map(|arg| &arg.value));
            }
            _ => {}
        }
    }
    Some(())
}

fn call_abi(result: &Ty, attrs: &[zil_llvm_api::model::Attr], args: &[Arg], f: &Function) -> bool {
    !f.signature.variadic
        && *result == f.signature.result
        && attrs == f.signature.result_attrs
        && args.len() == f.signature.params.len()
        && args
            .iter()
            .zip(&f.signature.params)
            .all(|(a, p)| a.ty == p.ty && a.attrs == p.attrs && integer_arg(a))
}

fn dependencies(
    function: &Function,
    functions: &[&Function],
    names: &HashMap<&str, usize>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<BTreeSet<usize>> {
    if !integer(&function.signature.result)
        || function.signature.variadic
        || function.signature.params.iter().any(|p| !integer(&p.ty))
        || function.linkage == Linkage::AvailableExternally
    {
        return None;
    }
    let mut dependencies = BTreeSet::new();
    for block in function.blocks.as_ref()? {
        for inst in &block.instructions {
            if cancelled() {
                return None;
            }
            let allowed = match &inst.op {
                Op::Binary {
                    opcode,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty)
                        && scalar(left)
                        && scalar(right)
                        && matches!(
                            opcode.as_str(),
                            "add"
                                | "sub"
                                | "mul"
                                | "udiv"
                                | "sdiv"
                                | "urem"
                                | "srem"
                                | "shl"
                                | "lshr"
                                | "ashr"
                                | "and"
                                | "or"
                                | "xor"
                        )
                }
                Op::Compare {
                    predicate,
                    ty,
                    left,
                    right,
                } => {
                    integer(ty)
                        && scalar(left)
                        && scalar(right)
                        && matches!(
                            predicate.as_str(),
                            "eq" | "ne"
                                | "ugt"
                                | "uge"
                                | "ult"
                                | "ule"
                                | "sgt"
                                | "sge"
                                | "slt"
                                | "sle"
                        )
                }
                Op::Cast { opcode, arg, to } => {
                    matches!(opcode.as_str(), "trunc" | "zext" | "sext")
                        && integer_arg(arg)
                        && integer(to)
                }
                Op::Extract { arg, index } => {
                    overflow_pair(&arg.ty) && matches!(arg.value, Val::Local(_)) && *index < 2
                }
                Op::Call {
                    result,
                    result_attrs,
                    callee: Val::Global(name),
                    args,
                    vararg,
                } => {
                    let Some(&callee) = names.get(name.as_str()) else {
                        return None;
                    };
                    let target = functions[callee];
                    if vararg.is_some() || !call_abi(result, result_attrs, args, target) {
                        return None;
                    }
                    if !intrinsic(target) {
                        if !integer(result) || target.blocks.is_none() {
                            return None;
                        }
                        dependencies.insert(callee);
                    }
                    true
                }
                Op::Phi { ty, incoming } => integer(ty) && incoming.iter().all(|(v, _)| scalar(v)),
                Op::Branch(_) | Op::Unreachable => true,
                Op::CondBranch { condition, .. } => scalar(condition),
                Op::Return(Some(arg)) => integer_arg(arg),
                Op::Select {
                    ty,
                    condition,
                    yes,
                    no,
                } => integer(ty) && scalar(condition) && scalar(yes) && scalar(no),
                // No memory, pointer/aggregate manipulation, freeze, foreign or
                // indirect calls, unknown intrinsic, undef or poison admission.
                _ => false,
            };
            if !allowed {
                return None;
            }
        }
    }
    Some(dependencies)
}

fn normalized_key(
    function: &Function,
    functions: &[&Function],
    names: &HashMap<&str, usize>,
    representatives: &[Option<usize>],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Function> {
    let mut key = function.clone();
    key.name.clear();
    let mut stopped = false;
    let needs_rewrite = key.blocks.as_ref()?.iter().any(|block| {
        block.instructions.iter().any(|inst| {
            if cancelled() {
                stopped = true;
                return true;
            }
            if let Op::Call {
                callee: Val::Global(name),
                ..
            } = &inst.op
            {
                let Some(&target) = names.get(name.as_str()) else {
                    stopped = true;
                    return true;
                };
                return representatives[target].is_some_and(|rep| rep != target);
            }
            false
        })
    });
    if stopped {
        return None;
    }
    // Clearing the header name does not need mutable block access. Keep leaf,
    // intrinsic-only and already-canonical bodies on their original SharedVec.
    if needs_rewrite {
        for block in key.blocks.as_mut()?.iter_mut() {
            for inst in &mut block.instructions {
                if cancelled() {
                    return None;
                }
                if let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &mut inst.op
                {
                    let target = *names.get(name.as_str())?;
                    if let Some(rep) = representatives[target]
                        && rep != target
                    {
                        *name = functions[rep].name.clone();
                    }
                }
            }
        }
    }
    Some(key)
}

/// Return None on cancellation/ambiguous names, without modifying the module.
/// Pure DAGs are processed bottom-up. Cycles and all their callers remain
/// unclassified; no recursive traversal or fixed-point speculation is used.
pub(crate) fn canonicalize(
    module: &mut Module,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Stats> {
    if cancelled() {
        return None;
    }
    let mut functions = Vec::new();
    let mut names = HashMap::new();
    let mut addresses = protected.clone();
    for item in &module.items {
        if cancelled() {
            return None;
        }
        match item {
            Item::Global(global) => {
                if let Some(value) = &global.initializer {
                    address_uses(value, &mut addresses, cancelled)?;
                }
            }
            Item::Function(function) => {
                if names
                    .insert(function.name.as_str(), functions.len())
                    .is_some()
                {
                    return None;
                }
                functions.push(function);
                for block in function.blocks.iter().flatten() {
                    for inst in &block.instructions {
                        match &inst.op {
                            Op::Call {
                                callee: Val::Global(_),
                                args,
                                ..
                            } => {
                                for arg in args {
                                    address_uses(&arg.value, &mut addresses, cancelled)?;
                                }
                            }
                            _ => {
                                for value in inst.op.values() {
                                    address_uses(value, &mut addresses, cancelled)?;
                                }
                            }
                        }
                    }
                }
            }
            Item::NamedType(..) => {}
        }
    }
    let count = functions.len();
    let mut outstanding = vec![None; count];
    let mut parents = vec![Vec::new(); count];
    let mut ready = BTreeSet::new();
    for (index, function) in functions.iter().enumerate() {
        if cancelled() {
            return None;
        }
        if !generated(&function.name) || addresses.contains(&function.name) {
            continue;
        }
        let Some(deps) = dependencies(function, &functions, &names, cancelled) else {
            continue;
        };
        outstanding[index] = Some(deps.len());
        if deps.is_empty() {
            ready.insert(index);
        }
        for callee in deps {
            parents[callee].push(index);
        }
    }
    let mut representatives = vec![None; count];
    // Hash is only the selector: HashMap resolves collisions with full Function
    // Eq, including complete signature/attributes/linkage/blocks/SSA names.
    let mut classes: HashMap<Function, usize> = HashMap::new();
    let mut stats = Stats::default();
    while !ready.is_empty() {
        let mut next = BTreeSet::new();
        // Equal normalized DAGs have equal height. Finish an entire height in
        // source order, so a later function can never become an earlier one's
        // representative just because its callee was visited first.
        for index in ready {
            if cancelled() {
                return None;
            }
            let key = normalized_key(
                functions[index],
                &functions,
                &names,
                &representatives,
                cancelled,
            )?;
            let rep = *classes.entry(key).or_insert(index);
            representatives[index] = Some(rep);
            stats.eligible_functions += 1;
            for &parent in &parents[index] {
                let remaining = outstanding[parent].as_mut()?;
                *remaining -= 1;
                if *remaining == 0 {
                    next.insert(parent);
                }
            }
        }
        ready = next;
    }
    stats.equivalence_classes = classes.len();
    let replacements: HashMap<String, String> = representatives
        .iter()
        .enumerate()
        .filter_map(|(index, rep)| {
            let rep = (*rep)?;
            (rep != index).then(|| (functions[index].name.clone(), functions[rep].name.clone()))
        })
        .collect();
    if replacements.is_empty() {
        return (!cancelled()).then_some(stats);
    }
    // Stage all changes in a COW clone: cancellation at any point cannot leave
    // an incompletely transformed original module or publish partial classes.
    let mut changed = module.clone();
    for item in &mut changed.items {
        if cancelled() {
            return None;
        }
        let Item::Function(function) = item else {
            continue;
        };
        if !function.blocks.iter().flatten().any(|block| block.instructions.iter().any(|inst| {
            matches!(&inst.op, Op::Call { callee: Val::Global(name), .. } if replacements.contains_key(name))
        })) {
            continue;
        }
        for block in function.blocks.as_mut()?.iter_mut() {
            for inst in &mut block.instructions {
                if cancelled() {
                    return None;
                }
                if let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &mut inst.op
                    && let Some(rep) = replacements.get(name)
                {
                    *name = rep.clone();
                    stats.rewritten_calls += 1;
                }
            }
        }
    }
    if cancelled() {
        return None;
    }
    if stats.rewritten_calls != 0 {
        *module = changed;
    }
    Some(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zil_llvm_api::model::{Attr, Block, Global, Inst, Param, Signature};

    fn body(name: &str, instructions: Vec<Inst>) -> Function {
        Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
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

    fn ret(value: Val) -> Inst {
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), value))))
    }

    fn leaf(name: &str, value: i128) -> Function {
        body(name, vec![ret(Val::Int(value))])
    }

    fn call(name: &str, target: &str) -> Function {
        body(
            name,
            vec![
                Inst::new(
                    Some("v".into()),
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global(target.into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                ret(Val::Local("v".into())),
            ],
        )
    }

    fn module(functions: Vec<Function>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "unchanged-original-identity".into(),
            items: functions.into_iter().map(Item::Function).collect(),
        }
    }

    fn run(module: &mut Module) -> Stats {
        canonicalize(module, &BTreeSet::new(), &|| false).unwrap()
    }

    fn target(module: &Module, name: &str) -> String {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(f) if f.name == name => f.blocks.as_ref().unwrap()[0]
                    .instructions
                    .iter()
                    .find_map(|inst| match &inst.op {
                        Op::Call {
                            callee: Val::Global(name),
                            ..
                        } => Some(name.clone()),
                        _ => None,
                    }),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn nested_classes_keep_all_definitions_and_choose_source_order_not_topological_accident() {
        // The earlier parent depends on the later leaf. A simple priority-ready
        // queue could incorrectly make the later parent the representative.
        let original = module(vec![
            call("zil.fn.8", "zil.fn.2"),
            leaf("zil.fn.1", 7),
            call("zil.fn.9", "zil.fn.1"),
            leaf("zil.fn.2", 7),
            call("zil.fn.10", "zil.fn.8"),
            call("zil.fn.11", "zil.fn.9"),
            call("main", "zil.fn.11"),
        ]);
        let mut actual = original.clone();
        assert_eq!(
            run(&mut actual),
            Stats {
                eligible_functions: 6,
                equivalence_classes: 3,
                rewritten_calls: 3,
            }
        );
        assert_eq!(target(&actual, "zil.fn.8"), "zil.fn.1");
        assert_eq!(target(&actual, "zil.fn.11"), "zil.fn.8");
        assert_eq!(target(&actual, "main"), "zil.fn.10");
        // Compare complete modules after changing ONLY the three authorized
        // direct target atoms in the expected copy, not a lossy names inventory.
        let mut expected = original;
        for (item, name) in [(0, "zil.fn.1"), (5, "zil.fn.8"), (6, "zil.fn.10")] {
            let Item::Function(f) = &mut expected.items[item] else {
                panic!()
            };
            let Op::Call { callee, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op
            else {
                panic!()
            };
            *callee = Val::Global(name.into());
        }
        assert_eq!(actual, expected);
        assert_eq!(run(&mut actual).rewritten_calls, 0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn normalized_keys_share_bodies_until_a_direct_target_actually_changes() {
        let first = leaf("zil.fn.0", 7);
        let duplicate = leaf("zil.fn.1", 7);
        let canonical_call = call("zil.fn.2", "zil.fn.0");
        let changed_call = call("zil.fn.3", "zil.fn.1");
        let trap = Function {
            name: "llvm.trap".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        };
        let intrinsic_call = body(
            "zil.fn.4",
            vec![
                Inst::new(
                    None,
                    Op::Call {
                        result: Ty::Void,
                        result_attrs: vec![],
                        callee: Val::Global(trap.name.clone()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(None, Op::Unreachable),
            ],
        );
        let functions = vec![
            &first,
            &duplicate,
            &canonical_call,
            &changed_call,
            &trap,
            &intrinsic_call,
        ];
        let names = functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.as_str(), i))
            .collect();
        let representatives = [Some(0), Some(0), None, None, None, None];
        for function in [&first, &duplicate, &canonical_call, &intrinsic_call] {
            let key =
                normalized_key(function, &functions, &names, &representatives, &|| false).unwrap();
            assert!(key.name.is_empty());
            assert!(
                key.blocks
                    .as_ref()
                    .unwrap()
                    .ptr_eq(function.blocks.as_ref().unwrap())
            );
            let mut expected = function.clone();
            expected.name.clear();
            assert_eq!(key, expected);
        }
        let key = normalized_key(&changed_call, &functions, &names, &representatives, &|| {
            false
        })
        .unwrap();
        assert!(
            !key.blocks
                .as_ref()
                .unwrap()
                .ptr_eq(changed_call.blocks.as_ref().unwrap())
        );
        let mut expected = canonical_call.clone();
        expected.name.clear();
        assert_eq!(key, expected);
        assert_eq!(changed_call, call("zil.fn.3", "zil.fn.1"));
    }

    #[test]
    fn changed_constant_abi_linkage_attributes_and_local_names_do_not_collapse() {
        let base = leaf("zil.fn.0", 7);
        let mut abi = leaf("zil.fn.2", 7);
        abi.signature.result_attrs.push(Attr::ZeroExt);
        let mut linkage = leaf("zil.fn.3", 7);
        linkage.linkage = Linkage::Internal;
        let mut attrs = leaf("zil.fn.4", 7);
        attrs.attrs.push(Attr::NoInline);
        let mut local = leaf("zil.fn.5", 7);
        local.blocks.as_mut().unwrap()[0].name = "different".into();
        let mut m = module(vec![base, leaf("zil.fn.1", 8), abi, linkage, attrs, local]);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 6);
        assert_eq!(stats.equivalence_classes, 6);
        assert_eq!(m, before);
    }

    #[test]
    fn protected_entry_and_nested_global_or_argument_addresses_are_not_class_members() {
        let mut sink = call("sink", "foreign");
        let Op::Call { args, .. } = &mut sink.blocks.as_mut().unwrap()[0].instructions[0].op else {
            panic!()
        };
        args.push(Arg::new(Ty::Ptr, Val::Global("zil.fn.3".into())));
        let mut foreign = leaf("foreign", 0);
        foreign.blocks = None;
        foreign.signature.params.push(Param::unnamed(Ty::Ptr));
        let mut m = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 7),
            leaf("zil.fn.2", 7),
            leaf("zil.fn.3", 7),
            call("main", "zil.fn.1"),
            sink,
            foreign,
        ]);
        m.items.push(Item::Global(Global {
            name: "addresses".into(),
            linkage: Linkage::External,
            ty: Ty::array(1, Ty::array(1, Ty::Ptr)),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::array(1, Ty::Ptr),
                Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.2".into()))]),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        }));
        let before = m.clone();
        let protected = BTreeSet::from(["zil.fn.1".to_string()]);
        let stats = canonicalize(&mut m, &protected, &|| false).unwrap();
        assert_eq!(stats.eligible_functions, 1);
        assert_eq!(m, before);
    }

    #[test]
    fn recursion_and_callers_of_recursion_foreign_or_unknown_intrinsics_are_refused() {
        let mut foreign = leaf("foreign", 0);
        foreign.blocks = None;
        let mut unknown = leaf("llvm.readcyclecounter", 0);
        unknown.blocks = None;
        let mut m = module(vec![
            call("zil.fn.0", "zil.fn.1"),
            call("zil.fn.1", "zil.fn.0"),
            call("zil.fn.2", "zil.fn.0"),
            call("zil.fn.3", "zil.fn.3"),
            call("zil.fn.4", "foreign"),
            call("zil.fn.5", "foreign"),
            call("zil.fn.6", "llvm.readcyclecounter"),
            call("zil.fn.7", "zil.fn.4"),
            foreign,
            unknown,
            call("main", "zil.fn.5"),
        ]);
        let before = m.clone();
        assert_eq!(run(&mut m), Stats::default());
        assert_eq!(m, before);
    }

    #[test]
    fn memory_indirect_pointer_undef_and_unknown_opcodes_are_refused() {
        let operations = vec![
            Op::Load {
                ty: Ty::Int(32),
                ptr: Val::Global("data".into()),
                align: 4,
            },
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Local("callback".into()),
                args: vec![],
                vararg: None,
            },
            Op::Cast {
                opcode: "ptrtoint".into(),
                arg: Arg::new(Ty::Ptr, Val::Global("data".into())),
                to: Ty::Int(32),
            },
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Undef,
                right: Val::Int(1),
            },
            Op::Binary {
                opcode: "unknown".into(),
                ty: Ty::Int(32),
                left: Val::Int(0),
                right: Val::Int(1),
            },
            Op::Freeze(Arg::new(Ty::Int(32), Val::Poison)),
        ];
        for operation in operations {
            let a = body(
                "zil.fn.0",
                vec![
                    Inst::new(Some("v".into()), operation),
                    ret(Val::Local("v".into())),
                ],
            );
            let mut b = a.clone();
            b.name = "zil.fn.1".into();
            let mut m = module(vec![a, b, call("main", "zil.fn.1")]);
            let before = m.clone();
            assert_eq!(run(&mut m).eligible_functions, 0);
            assert_eq!(m, before);
        }
        let mut a = leaf("zil.fn.0", 0);
        a.signature.params.push(Param::new(Ty::Ptr, "p"));
        let mut b = a.clone();
        b.name = "zil.fn.1".into();
        let mut m = module(vec![a, b]);
        assert_eq!(run(&mut m).eligible_functions, 0);
    }

    #[test]
    fn checked_overflow_and_trap_are_compared_exactly_without_arithmetic_rewriting() {
        let pair = Ty::Struct(vec![Ty::Int(32), Ty::Int(1)]);
        let overflow = Function {
            name: "llvm.sadd.with.overflow.i32".into(),
            linkage: Linkage::External,
            signature: Signature::new(
                pair.clone(),
                vec![Param::unnamed(Ty::Int(32)), Param::unnamed(Ty::Int(32))],
            ),
            attrs: vec![],
            blocks: None,
        };
        let trap = Function {
            name: "llvm.trap".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: None,
        };
        let mut checked = body(
            "zil.fn.0",
            vec![
                Inst::new(
                    Some("pair".into()),
                    Op::Call {
                        result: pair.clone(),
                        result_attrs: vec![],
                        callee: Val::Global(overflow.name.clone()),
                        args: vec![
                            Arg::new(Ty::Int(32), Val::Int(i128::from(i32::MAX))),
                            Arg::new(Ty::Int(32), Val::Int(1)),
                        ],
                        vararg: None,
                    },
                ),
                Inst::new(
                    Some("value".into()),
                    Op::Extract {
                        arg: Arg::new(pair.clone(), Val::Local("pair".into())),
                        index: 0,
                    },
                ),
                Inst::new(
                    Some("flag".into()),
                    Op::Extract {
                        arg: Arg::new(pair, Val::Local("pair".into())),
                        index: 1,
                    },
                ),
                Inst::new(
                    None,
                    Op::CondBranch {
                        condition: Val::Local("flag".into()),
                        yes: "trap".into(),
                        no: "exit".into(),
                    },
                ),
            ],
        );
        checked.blocks.as_mut().unwrap().push(Block {
            name: "trap".into(),
            instructions: vec![
                Inst::new(
                    None,
                    Op::Call {
                        result: Ty::Void,
                        result_attrs: vec![],
                        callee: Val::Global(trap.name.clone()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(None, Op::Unreachable),
            ],
        });
        checked.blocks.as_mut().unwrap().push(Block {
            name: "exit".into(),
            instructions: vec![ret(Val::Local("value".into()))],
        });
        let mut duplicate = checked.clone();
        duplicate.name = "zil.fn.1".into();
        let mut m = module(vec![
            overflow,
            trap,
            checked,
            duplicate,
            call("main", "zil.fn.1"),
        ]);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 2);
        assert_eq!(stats.equivalence_classes, 1);
        assert_eq!(stats.rewritten_calls, 1);
        assert_eq!(target(&m, "main"), "zil.fn.0");
        assert_eq!(&m.items[..4], &before.items[..4]);
    }

    #[test]
    fn cancellation_even_during_final_staging_is_transactional() {
        let original = module(vec![
            leaf("zil.fn.0", 7),
            leaf("zil.fn.1", 7),
            call("main", "zil.fn.1"),
        ]);
        let polls = AtomicUsize::new(0);
        let mut measured = original.clone();
        canonicalize(&mut measured, &BTreeSet::new(), &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        })
        .unwrap();
        let total = polls.load(Ordering::Relaxed);
        for stop in [0, total / 2, total - 2, total - 1] {
            let polls = AtomicUsize::new(0);
            let mut m = original.clone();
            assert!(
                canonicalize(&mut m, &BTreeSet::new(), &|| {
                    polls.fetch_add(1, Ordering::Relaxed) >= stop
                })
                .is_none()
            );
            assert_eq!(m, original);
        }
    }

    #[test]
    fn long_call_dag_does_not_use_rust_recursion() {
        let mut functions = vec![leaf("zil.fn.0", 7)];
        for i in 1..2048 {
            functions.push(call(&format!("zil.fn.{i}"), &format!("zil.fn.{}", i - 1)));
        }
        let mut m = module(functions);
        let before = m.clone();
        let stats = run(&mut m);
        assert_eq!(stats.eligible_functions, 2048);
        assert_eq!(stats.equivalence_classes, 2048);
        assert_eq!(m, before);
    }
}
