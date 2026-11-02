use super::*;
use zil_base::{ModuleId, Span, TargetProfile};
use zil_ir::{BlockId, LirModule, PassingMode, Type, TypedFunction};
use zil_llvm_api::model::{Arg, Attr, Block, Inst, Signature};

fn definition(name: String, value: i128) -> Item {
    Item::Function(Function {
        name,
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(vec![Block {
            name: "entry".into(),
            instructions: vec![Inst::new(
                None,
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value)))),
            )],
        }]),
    })
}

// As in the existing partition model tests, the LIR function headers here are
// grouping inputs, not a forged VerifiedModule or a source-language proof.
fn fixture(order: &[String]) -> (Module, LirModule, Vec<(FunctionId, String)>) {
    let mut source = LirModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: ModuleId(0),
        verified_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![],
        closures: vec![],
        resources: vec![],
        layouts: vec![],
    };
    let mut items = vec![];
    let mut hints = vec![];
    let mut entry = None;
    for (index, name) in order.iter().enumerate() {
        let id = FunctionId(index as u32);
        source.functions.push(TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id,
            name: name.clone(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![],
            blocks: vec![],
            entry: BlockId(0),
            span: Span::default(),
        });
        let value = name
            .strip_prefix('h')
            .and_then(|number| number.parse::<i128>().ok())
            .unwrap_or(0);
        items.push(definition(source_symbol(id), value));
        hints.push((id, format!("fixed-project/module/function/{name}")));
        if name == "main" {
            entry = Some(id);
        }
    }
    items.push(Item::Function(Function {
        name: "main".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(vec![Block {
            name: "entry".into(),
            instructions: vec![
                Inst::new(
                    Some("result".into()),
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global(source_symbol(entry.unwrap())),
                        args: vec![],
                        vararg: None,
                    },
                ),
                Inst::new(
                    None,
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("result".into())))),
                ),
            ],
        }]),
    }));
    (
        Module {
            triple: source.target.triple.clone(),
            fingerprint: "same-model-comment".into(),
            items,
        },
        source,
        hints,
    )
}

fn order() -> Vec<String> {
    (0..64)
        .map(|i| format!("h{i}"))
        .chain(["main".into()])
        .collect()
}

fn commit(
    module: Module,
    source: &LirModule,
    hints: &[(FunctionId, String)],
) -> BTreeMap<usize, Module> {
    let prepared =
        prepare(&module, &source.as_view(), hints, 8, &|| false).expect("eligible flat model");
    let owners = prepared.owners.values().copied().collect::<BTreeSet<_>>();
    let mut renamed = module;
    prepared.rename(&mut renamed);
    zil_llvm_api::validate_module(&renamed).unwrap();
    let (mut plan, candidates, diagnostic) = prepared.partition(renamed, None);
    assert!(diagnostic.is_none());
    crate::machine_cgu::internalize_closed_executable_names(&mut plan.units, &candidates);
    for unit in &plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    assert_eq!(owners.len(), plan.units.len());
    owners.into_iter().zip(plan.units).collect()
}

#[test]
fn names_owners_and_actual_unit_order_survive_dense_reordering() {
    let first = order();
    let mut second = first.clone();
    second.reverse();
    let (a, al, ah) = fixture(&first);
    let (b, bl, bh) = fixture(&second);
    assert_ne!(a, b);
    assert_eq!(commit(a, &al, &ah), commit(b, &bl, &bh));
}

#[test]
fn insertion_and_deletion_change_only_their_real_owner_bucket() {
    let original = order();
    let (module, source, hints) = fixture(&original);
    let baseline = commit(module, &source, &hints);
    let mut inserted = original.clone();
    inserted.insert(0, "inserted".into());
    let mut deleted = original.clone();
    deleted.remove(0);
    for edited in [inserted, deleted] {
        let (module, source, hints) = fixture(&edited);
        let current = commit(module, &source, &hints);
        let buckets = baseline
            .keys()
            .chain(current.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            buckets
                .into_iter()
                .filter(|bucket| baseline.get(bucket) != current.get(bucket))
                .count(),
            1
        );
    }
}

#[test]
fn sparse_new_bucket_moves_vector_positions_without_changing_existing_modules() {
    let names = order();
    let (module, source, hints) = fixture(&names);
    let prepared = prepare(&module, &source.as_view(), &hints, 8, &|| false).unwrap();
    let entry_owner = prepared.owners[&(names.len() - 1)];
    let mut examples = BTreeMap::<usize, String>::new();
    for (index, name) in names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_str() != "main")
    {
        examples
            .entry(prepared.owners[&index])
            .or_insert_with(|| name.clone());
    }
    let (&other_owner, other) = examples
        .iter()
        .rev()
        .find(|(owner, _)| **owner != entry_owner)
        .unwrap();
    let (&insert_owner, inserted) = examples
        .iter()
        .find(|(owner, _)| **owner != entry_owner && **owner != other_owner)
        .unwrap();
    assert!(insert_owner < entry_owner.max(other_owner));
    let (a, al, ah) = fixture(&[other.clone(), "main".into()]);
    let before = commit(a, &al, &ah);
    let (b, bl, bh) = fixture(&[inserted.clone(), other.clone(), "main".into()]);
    let after = commit(b, &bl, &bh);
    assert_eq!(before.len(), 2);
    assert_eq!(after.len(), 3);
    for (owner, module) in &before {
        assert_eq!(after.get(owner), Some(module));
    }
    let high = entry_owner.max(other_owner);
    assert_ne!(
        before.keys().position(|owner| *owner == high),
        after.keys().position(|owner| *owner == high)
    );
    // Real object keys hash namespace/triple/items, not this compact position.
}

#[test]
fn identity_misses_leave_the_complete_original_model_unchanged() {
    let (module, source, hints) = fixture(&order());
    for invalid in [
        vec![],
        hints[..hints.len() - 1].to_vec(),
        {
            let mut duplicate = hints.clone();
            duplicate[1].1 = duplicate[0].1.clone();
            duplicate
        },
        {
            let mut duplicate = hints.clone();
            duplicate[1].0 = duplicate[0].0;
            duplicate
        },
    ] {
        let before = module.clone();
        assert!(prepare(&module, &source.as_view(), &invalid, 8, &|| false).is_none());
        assert_eq!(module, before);
    }
    assert!(prepare(&module, &source.as_view(), &hints, 1, &|| false).is_none());
    assert!(prepare(&module, &source.as_view(), &hints, 8, &|| true).is_none());
}

#[test]
fn source_calls_and_addresses_require_the_original_policy() {
    let (module, source, hints) = fixture(&order());
    for op in [
        Op::Call {
            result: Ty::Int(32),
            result_attrs: vec![],
            callee: Val::Global("zil.fn.1".into()),
            args: vec![],
            vararg: None,
        },
        Op::Freeze(Arg::new(Ty::Ptr, Val::Global("zil.fn.1".into()))),
    ] {
        let mut candidate = module.clone();
        let Item::Function(function) = &mut candidate.items[0] else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0]
            .instructions
            .insert(0, Inst::new(Some("extra".into()), op));
        zil_llvm_api::validate_module(&candidate).unwrap();
        let before = candidate.clone();
        assert!(prepare(&candidate, &source.as_view(), &hints, 8, &|| false).is_none());
        assert_eq!(candidate, before);
    }
}

#[test]
fn unused_invalid_declaration_cannot_be_hidden_by_stable_partition() {
    let (mut module, source, hints) = fixture(&order());
    module.items.push(Item::Function(Function {
        name: "llvm.trap".into(),
        linkage: Linkage::External,
        signature: Signature {
            result: Ty::Void,
            result_attrs: vec![Attr::NonLazyBind],
            params: vec![],
            variadic: false,
        },
        attrs: vec![],
        blocks: None,
    }));
    let error = zil_llvm_api::validate_module(&module).unwrap_err();
    let before = module.clone();
    assert!(prepare(&module, &source.as_view(), &hints, 8, &|| false).is_none());
    assert_eq!(module, before);
    assert_eq!(zil_llvm_api::validate_module(&module).unwrap_err(), error);
}

#[test]
fn generic_symbol_visitor_covers_nested_address_operands() {
    let mut value = Val::Gep {
        ty: Ty::Int(8),
        ptr: Box::new(Val::Global("old".into())),
        indices: vec![Arg::new(
            Ty::Int(32),
            Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("old".into()))]),
        )],
    };
    let mut names = BTreeMap::new();
    names.insert("old".into(), "new".into());
    rename_value(&mut value, &names);
    let mut symbols = BTreeSet::new();
    value.symbols(&mut symbols);
    assert_eq!(symbols, BTreeSet::from(["new".into()]));
    // This is a traversal control, not an admitted nested-address model.
}

// Persistent controls for the narrow closed reachability pruning policy.
// The existing fixture is an untrusted MachineIR/grouping input, not a forged
// VerifiedModule. Current source verification remains a separate driver test.
fn dce_unit(mut module: Module, source: &LirModule, hints: &[(FunctionId, String)]) -> Module {
    let mut prepared = prepare(&module, &source.as_view(), hints, 8, &|| false).unwrap();
    prepared.prune_unreachable_closed_definitions(&mut module);
    zil_llvm_api::validate_module(&module).unwrap();
    prepared.rename(&mut module);
    let (mut plan, names, trace) = prepared.partition(module, None);
    assert_eq!(plan.units.len(), 1);
    assert!(plan.fallback.is_none() && trace.is_none());
    crate::machine_cgu::internalize_closed_executable_names(&mut plan.units, &names);
    let unit = plan.units.pop().unwrap();
    zil_llvm_api::validate_module(&unit).unwrap();
    assert_eq!(
        unit.items
            .iter()
            .filter(|item| matches!(item, Item::Function(f) if f.blocks.is_some()))
            .count(),
        2
    );
    unit
}

fn dce_external(module: &mut Module, source: &mut LirModule, trap: bool) {
    let name = if trap { "llvm.trap" } else { "effect" };
    if !trap {
        let mut foreign = source.functions[0].clone();
        foreign.id = FunctionId(source.functions.len() as u32);
        foreign.name = name.into();
        foreign.foreign = Some(zil_ir::ForeignDeclaration {
            abi: "C".into(),
            symbol: name.into(),
            noescape_parameters: vec![],
            static_callbacks: false,
        });
        source.functions.push(foreign);
    }
    module.items.push(Item::Function(Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(if trap { Ty::Void } else { Ty::Int(32) }, vec![]),
        attrs: vec![],
        blocks: None,
    }));
}

fn dce_insert_call(module: &mut Module, index: usize, trap: bool) {
    let Item::Function(function) = &mut module.items[index] else {
        unreachable!()
    };
    let body = &mut function.blocks.as_mut().unwrap()[0].instructions;
    let call = Inst::new(
        if trap {
            None
        } else {
            Some("effect_result".into())
        },
        Op::Call {
            result: if trap { Ty::Void } else { Ty::Int(32) },
            result_attrs: vec![],
            callee: Val::Global(if trap { "llvm.trap" } else { "effect" }.into()),
            args: vec![],
            vararg: None,
        },
    );
    if trap {
        *body = vec![call, Inst::new(None, Op::Unreachable)];
    } else {
        body.insert(0, call);
    }
}

#[test]
fn closed_pruning_preserves_the_actual_entry_unit_across_reorder_insert_and_restore() {
    let original = order();
    let (module, source, hints) = fixture(&original);
    let baseline = dce_unit(module, &source, &hints);
    let mut reversed = original.clone();
    reversed.reverse();
    let mut inserted = original.clone();
    inserted.insert(0, "unused".into());
    for current in [reversed, inserted, original] {
        let (module, source, hints) = fixture(&current);
        assert_eq!(dce_unit(module, &source, &hints), baseline);
    }
}

#[test]
fn closed_pruning_removes_dead_effects_and_traps_but_preserves_live_bodies() {
    let names = order();
    let (module, source, hints) = fixture(&names);
    let baseline = dce_unit(module, &source, &hints);
    let (mut module, mut source, hints) = fixture(&names);
    for trap in [false, true] {
        dce_external(&mut module, &mut source, trap);
        dce_insert_call(&mut module, usize::from(trap), trap);
    }
    zil_llvm_api::validate_module(&module).unwrap();
    assert_eq!(dce_unit(module, &source, &hints), baseline);
    for trap in [false, true] {
        let (mut module, mut source, hints) = fixture(&names);
        dce_external(&mut module, &mut source, trap);
        dce_insert_call(&mut module, names.len() - 1, trap);
        let Item::Function(original_entry) = &module.items[names.len() - 1] else {
            unreachable!()
        };
        let original_body = original_entry.blocks.clone();
        let unit = dce_unit(module, &source, &hints);
        let callee = if trap { "llvm.trap" } else { "effect" };
        assert!(unit.items.iter().any(
            |item| matches!(item, Item::Function(f) if f.name == callee && f.blocks.is_none())
        ));
        let entry = unit
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(f) if f.name.starts_with("zil.stable.") => Some(f),
                _ => None,
            })
            .unwrap();
        assert_eq!(entry.blocks, original_body);
    }
}

#[test]
fn closed_pruning_cannot_hide_an_invalid_dead_definition_or_declaration() {
    for declaration in [false, true] {
        let (mut module, source, hints) = fixture(&order());
        if declaration {
            module.items.push(Item::Function(Function {
                name: "llvm.trap".into(),
                linkage: Linkage::External,
                signature: Signature {
                    result: Ty::Void,
                    result_attrs: vec![Attr::NonLazyBind],
                    params: vec![],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }));
        } else {
            let Item::Function(function) = &mut module.items[0] else {
                unreachable!()
            };
            function.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
                None,
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("undefined".into())))),
            )];
        }
        let before = module.clone();
        let first_error = zil_llvm_api::validate_module(&module).unwrap_err();
        assert!(prepare(&module, &source.as_view(), &hints, 8, &|| false).is_none());
        assert_eq!(module, before);
        assert_eq!(
            zil_llvm_api::validate_module(&module).unwrap_err(),
            first_error
        );
    }
}
