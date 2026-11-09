//! Complete Facts oracle: the legacy None lane walks every local after the
//! actual partial Meta::step. No candidate tag list is used by that traversal.
use super::*;
use crate::place_replacements::fixture;
use zil_ir::{BasicBlock, BinaryOp, BlockId, LocalDecl};

fn module() -> TypedModule {
    let mut module = fixture::module();
    let owner = module.functions[0].locals[1].ty.clone();
    let scalar = Type::Enum {
        name: "State".into(),
        variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![])],
    };
    let tagged = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(scalar.clone()),
    };
    let types = vec![
        Type::I32,
        owner.clone(),
        Type::Tuple(vec![owner.clone(), Type::I32]),
        Type::Record {
            name: "Inline".into(),
            fields: vec![("state".into(), scalar.clone())],
        },
        Type::Array {
            element: Box::new(scalar.clone()),
            length: 3,
        },
        scalar.clone(),
        Type::Tuple(vec![owner.clone(), tagged.clone()]),
        Type::Package {
            name: "Packet".into(),
            fields: vec![("owner".into(), owner), ("state".into(), tagged)],
        },
        Type::Address {
            mutable: true,
            inner: Box::new(scalar),
        },
    ];
    let f = &mut module.functions[0];
    f.locals = types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| LocalDecl {
            id: LocalId(index as u32),
            name: format!("slot{index}"),
            ty,
            mutable: true,
            span: f.span,
        })
        .collect();
    f.parameters = f.locals.iter().map(|local| local.id).collect();
    f.values = vec![Type::I32, Type::I32, Type::Bool];
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                fixture::instruction(
                    Some(ValueId(0)),
                    InstructionKind::PlaceTagLocal {
                        local: LocalId(5),
                        path: vec![],
                    },
                ),
                fixture::instruction(
                    Some(ValueId(1)),
                    InstructionKind::Constant(Constant::I32(0)),
                ),
                fixture::instruction(
                    Some(ValueId(2)),
                    InstructionKind::Binary {
                        op: BinaryOp::Equal,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                ),
            ],
            terminator: Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![fixture::instruction(
                None,
                InstructionKind::StorageLive(LocalId(0)),
            )],
            terminator: Terminator::Return(None),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Return(None),
        },
    ];
    module
}
fn plan(f: &TypedFunction) -> Plan {
    let mut b = Budget::new(&Limits::default());
    b.sum_functions = vec![true];
    Plan::new(f, &mut b).unwrap().unwrap()
}
#[test]
fn actual_static_tag_index_matches_old_full_local_walk_with_all_facts_sidecars() {
    let module = module();
    let f = &module.functions[0];
    let indexed = plan(f);
    assert_eq!(
        indexed.tagged_locals,
        Some(vec![LocalId(5), LocalId(6), LocalId(7)])
    );
    let mut full = plan(f);
    full.tagged_locals = None;
    let mut seed = indexed
        .initial(&module, f, &mut Budget::new(&Limits::default()))
        .unwrap();
    // Execute an actual observation + matching branch so Meta::step starts
    // with nonempty current reads/tags. The candidate must preserve the whole
    // observation invalidation as well as the Sum Trees.
    for i in &f.blocks[0].instructions {
        indexed
            .partial
            .as_ref()
            .unwrap()
            .step(
                seed.partial.as_mut().unwrap(),
                i,
                &mut Budget::new(&Limits::default()),
            )
            .unwrap();
    }
    assert!(
        indexed
            .edge(f, 0, 1, &mut seed, &mut Budget::new(&Limits::default()))
            .unwrap()
    );
    assert_ne!(
        seed.partial.as_ref().unwrap(),
        &partial_places::State::default()
    );
    seed.valid = BTreeSet::from([ValueId(0), ValueId(2)]);
    seed.captures.insert(
        0,
        Arc::new(Tree {
            missing: true,
            node: Node::Leaf(BTreeSet::from([Birth::Captured(3)])),
        }),
    );
    for tags in 0..16 {
        for missing in 0..32 {
            let mut input = seed.clone();
            for tree in input.locals.values_mut() {
                super::tag_forgetting_tests::vary(tree, tags, missing, &mut 0);
            }
            let frozen_input = format!("{input:?}");
            for malformed in [false, true] {
                let mut a = input.clone();
                if malformed {
                    // The static false-type proof is content identity even
                    // for this private malformed Tree; no new raw admission
                    // or permission is claimed by a component-only fixture.
                    a.locals.insert(
                        LocalId(0),
                        Arc::new(Tree {
                            missing: false,
                            node: Node::Sum(BTreeMap::from([(71, vec![])])),
                        }),
                    );
                }
                let mut old = a.clone();
                let before = a.clone();
                let instruction = &f.blocks[1].instructions[0];
                let new_result = indexed
                    .apply(
                        &module,
                        f,
                        instruction,
                        &mut a,
                        false,
                        &mut Budget::new(&Limits::default()),
                    )
                    .unwrap();
                let old_result = full
                    .apply(
                        &module,
                        f,
                        instruction,
                        &mut old,
                        false,
                        &mut Budget::new(&Limits::default()),
                    )
                    .unwrap();
                assert_eq!(new_result, old_result);
                assert_eq!(
                    a, old,
                    "tags={tags} missing={missing} malformed={malformed}"
                );
                assert_eq!(a.valid, before.valid);
                assert_eq!(a.captures, before.captures);
                assert_eq!(a.values, before.values);
                assert_eq!(a.partial, Some(partial_places::State::default()));
                let new_join = a.join(&seed, &mut Budget::new(&Limits::default()));
                let old_join = old.join(&seed, &mut Budget::new(&Limits::default()));
                assert_eq!(new_join, old_join);
                if malformed {
                    assert!(
                        new_join
                            .unwrap_err()
                            .iter()
                            .any(|e| e.message.contains("Sum fact has inconsistent shape"))
                    );
                } else {
                    new_join.unwrap();
                }
                assert_eq!(a, old, "subsequent complete join must retain every fact");
                // Check content, rather than comparing the same Arc identity.
                assert_eq!(format!("{input:?}"), frozen_input);
            }
        }
    }
}
#[test]
fn selected_tagged_slot_absence_keeps_the_actual_missing_fact_rejection() {
    let module = module();
    let f = &module.functions[0];
    let p = plan(f);
    let mut facts = p
        .initial(&module, f, &mut Budget::new(&Limits::default()))
        .unwrap();
    facts.locals.remove(&LocalId(5));
    let errors = p
        .apply(
            &module,
            f,
            &f.blocks[1].instructions[0],
            &mut facts,
            false,
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("tagged local fact is absent")),
        "{errors:?}"
    );
}

fn guard(payload: Type) -> Type {
    Type::MutexGuard {
        payload: Box::new(payload),
        retained_life: zil_ir::RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }
}
#[test]
fn excluded_types_keep_request_presence_and_complete_closed_dyn_qualification() {
    let mut module = module();
    let f = &mut module.functions[0];
    f.locals[0].ty = guard(Type::Registration {
        environment: Box::new(Type::TaskI32),
    });
    let mut b = Budget::new(&Limits::default());
    b.sum_functions = vec![true];
    assert!(!b.tasks && !b.registrations && !b.generic_mutex);
    let p = Plan::new(f, &mut b).unwrap().unwrap();
    assert!(b.tasks && b.registrations && b.generic_mutex);
    assert!(!p.tagged_locals.as_ref().unwrap().contains(&LocalId(0)));
    // The same static flag effects are performed by the old full traversal.
    let mut old_b = Budget::new(&Limits::default());
    let mut old = Tree::unknown(&f.locals[0].ty, &mut old_b).unwrap();
    old.forget_addressed_copy_tags(&f.locals[0].ty, &mut old_b)
        .unwrap();
    assert_eq!(
        (b.tasks, b.registrations, b.generic_mutex),
        (old_b.tasks, old_b.registrations, old_b.generic_mutex)
    );
    // Each new request must independently reject context hidden beneath a
    // tagless guard. A completed Plan is never a cross-request permission.
    let mut bad_task = if let Type::MutexGuard { retained_life, .. } = guard(Type::Unit) {
        retained_life
    } else {
        unreachable!()
    };
    bad_task.binder_count = 1;
    for payload in [
        Type::Dyn {
            interface: zil_ir::DynInterfaceId {
                module: zil_base::ModuleId(0),
                index: 71,
            },
        },
        Type::Task {
            result: Box::new(Type::I32),
            retained_life: bad_task,
        },
    ] {
        f.locals[0].ty = guard(payload);
        let mut fresh = Budget::new(&Limits::default());
        fresh.sum_functions = vec![true];
        let error = Plan::new(f, &mut fresh)
            .err()
            .expect("bad excluded context must fail before Plan publication");
        let expected = Budget::new(&Limits::default())
            .type_work(&f.locals[0].ty, true, "mutable local Enum tag knowledge")
            .unwrap_err();
        assert_eq!(error, expected);
    }
}
