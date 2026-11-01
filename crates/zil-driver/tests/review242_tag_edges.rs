//! Independent nested-path raw candidates: no Types or Analyze supplies Verify
//! authority. Initialization/empty Hold claims below are fixture plumbing only.
use std::{collections::BTreeSet, fs, path::PathBuf};
use zil_base::{
    Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;

fn op(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: Span::default(),
    }
}
fn payload() -> Type {
    Type::Record {
        name: "EdgeToken".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn managed() -> Type {
    Type::Managed(Box::new(payload()))
}
fn inner() -> Type {
    Type::Enum {
        name: "EdgeInner".into(),
        variants: vec![
            ("Left".into(), vec![managed(), Type::I32]),
            ("Right".into(), vec![managed(), Type::I32]),
        ],
    }
}
fn outer() -> Type {
    Type::Enum {
        name: "EdgeOuter".into(),
        variants: vec![
            ("Wrap".into(), vec![inner(), Type::I32]),
            ("Empty".into(), vec![]),
        ],
    }
}
fn projection(variant: u32, index: u32) -> VariantProjection {
    VariantProjection { variant, index }
}
fn fixture() -> TypedModule {
    let locals = [outer(), inner(), Type::I32, managed(), Type::I32, outer()]
        .into_iter()
        .enumerate()
        .map(|(n, ty)| LocalDecl {
            id: LocalId(n as u32),
            name: format!("l{n}"),
            ty,
            mutable: true,
            span: Span::default(),
        })
        .collect();
    let mut entry = vec![op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) })];
    for n in 1..5 {
        entry.push(op(None, InstructionKind::StorageLive(LocalId(n))));
    }
    entry.extend([
        op(
            Some(0),
            InstructionKind::VariantTagLocal {
                local: LocalId(0),
                path: vec![],
            },
        ),
        op(Some(1), InstructionKind::Constant(Constant::I32(0))),
        op(
            Some(2),
            InstructionKind::Binary {
                op: BinaryOp::Equal,
                left: ValueId(0),
                right: ValueId(1),
            },
        ),
    ]);
    let function = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(0),
        name: "inspect_nested".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0), LocalId(5)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals,
        values: vec![
            Type::I32,
            Type::I32,
            Type::Bool,
            Type::I32,
            Type::I32,
            Type::Bool,
            Type::I32,
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: entry,
                terminator: Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(3),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![
                    op(
                        Some(3),
                        InstructionKind::VariantTagLocal {
                            local: LocalId(0),
                            path: vec![projection(0, 0)],
                        },
                    ),
                    op(Some(4), InstructionKind::Constant(Constant::I32(1))),
                    op(
                        Some(5),
                        InstructionKind::Binary {
                            op: BinaryOp::Equal,
                            left: ValueId(3),
                            right: ValueId(4),
                        },
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(5),
                    then_block: BlockId(2),
                    else_block: BlockId(3),
                },
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![
                    op(
                        Some(6),
                        InstructionKind::ReadVariantLocal {
                            local: LocalId(0),
                            path: vec![projection(0, 0), projection(1, 1)],
                        },
                    ),
                    op(
                        None,
                        InstructionKind::UnpackVariant {
                            local: LocalId(0),
                            variant: 0,
                            fields: vec![LocalId(1), LocalId(2)],
                        },
                    ),
                    // The observation belonged to root l0. A newly initialized
                    // child l1 needs its own tag test before another Unpack;
                    // this probe instead drops that complete child normally.
                    op(None, InstructionKind::DropEnum { local: LocalId(1) }),
                ],
                terminator: Terminator::Trap,
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Trap,
            },
        ],
        entry: BlockId(0),
        span: Span::default(),
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"independent nested edge and loop candidates"),
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "EdgeToken".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
// Compute only the candidate's definite-initialization bookkeeping. This has no
// tag, origin, age, cleanup or resource inference, and calls neither safety kernel.
fn candidate(module: &TypedModule) -> CandidatePlan {
    let f = &module.functions[0];
    let n = f.blocks.len();
    let all: BTreeSet<_> = (0..f.locals.len()).map(|i| LocalId(i as u32)).collect();
    let params: BTreeSet<_> = f.parameters.iter().copied().collect();
    let mut preds = vec![vec![]; n];
    for (from, block) in f.blocks.iter().enumerate() {
        let targets = match block.terminator {
            Terminator::Jump(to) => vec![to],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            _ => vec![],
        };
        for to in targets {
            preds[to.0 as usize].push(from);
        }
    }
    let mut inputs = vec![all.clone(); n];
    let mut outputs = inputs.clone();
    loop {
        let old = (inputs.clone(), outputs.clone());
        for b in 0..n {
            let mut state = if b == 0 {
                params.clone()
            } else {
                preds[b].iter().fold(all.clone(), |set, p| {
                    set.intersection(&outputs[*p]).copied().collect()
                })
            };
            inputs[b] = state.clone();
            for i in &f.blocks[b].instructions {
                match &i.kind {
                    InstructionKind::Store { local, .. } => {
                        state.insert(*local);
                    }
                    InstructionKind::UnpackVariant { local, fields, .. } => {
                        state.remove(local);
                        state.extend(fields);
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropEnum { local } => {
                        state.remove(local);
                    }
                    _ => {}
                }
            }
            outputs[b] = state;
        }
        if old == (inputs.clone(), outputs.clone()) {
            break;
        }
    }
    let mut plan = CandidatePlan::empty_for(module);
    plan.functions.push(FunctionPlan {
        function: f.id,
        blocks: (0..n)
            .map(|b| BlockPlan {
                block: BlockId(b as u32),
                initialized_in: inputs[b].iter().copied().collect(),
                initialized_out: outputs[b].iter().copied().collect(),
            })
            .collect(),
        holds: (0..f.values.len())
            .map(|v| ValueHold {
                value: ValueId(v as u32),
                roots: vec![],
            })
            .collect(),
    });
    plan
}
fn check(label: &str, module: TypedModule, accept: bool) {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review242-edges-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    let plan = candidate(&module);
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    // Verify runs before Analyze and receives only the handmade public candidate.
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    let analyzed = zil_analyze::analyze(&module, &limits);
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    fs::write(out.join("wire.txt"), format!("{bytes:#?}")).unwrap();
    fs::write(out.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    for (kind, result) in [
        ("raw", raw.map(|_| ())),
        ("Wire", bytes.map(|_| ())),
        ("Analyze", analyzed.map(|_| ())),
    ] {
        if accept {
            result.unwrap_or_else(|e| panic!("{label}/{kind}: {e:?}"));
        } else {
            let errors = result.expect_err(&format!("{label}/{kind} forged nested tag authority"));
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "{label}/{kind}: {errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .all(|d| !d.code.contains("CLAIM") && !d.code.starts_with("E-VERIFY-PLAN")),
                "{label}/{kind}: bookkeeping-only rejection {errors:?}"
            );
        }
    }
}

#[test]
fn recursive_path_requires_both_actual_edges_and_actual_local() {
    check("nested-control", fixture(), true);
    let mut opposite = fixture();
    opposite.functions[0].blocks[1].terminator = Terminator::Branch {
        condition: ValueId(5),
        then_block: BlockId(3),
        else_block: BlockId(2),
    };
    check("opposite-child-edge", opposite, false);
    let mut same = fixture();
    same.functions[0].blocks[1].terminator = Terminator::Branch {
        condition: ValueId(5),
        then_block: BlockId(2),
        else_block: BlockId(2),
    };
    check("same-child-target", same, false);
    let mut other = fixture();
    if let InstructionKind::VariantTagLocal { local, .. } =
        &mut other.functions[0].blocks[0].instructions[5].kind
    {
        *local = LocalId(5);
    }
    check("other-same-type-local", other, false);
}

#[test]
fn a_bypass_predecessor_cannot_inherit_only_the_guarded_tag() {
    let mut m = fixture();
    m.functions[0].blocks[0].terminator = Terminator::Branch {
        condition: ValueId(2),
        then_block: BlockId(1),
        else_block: BlockId(4),
    };
    m.functions[0].blocks.push(BasicBlock {
        id: BlockId(4),
        instructions: vec![],
        terminator: Terminator::Jump(BlockId(1)),
    });
    check("outer-bypass-to-inner", m, false);
}

#[test]
fn loop_at_one_write_site_needs_a_new_observation_after_each_swap() {
    for fresh in [false, true] {
        let mut m = fixture();
        let f = &mut m.functions[0];
        let mut observation = f.blocks[0].instructions.split_off(5);
        f.values.extend([outer(), outer()]);
        let mut body = vec![
            op(Some(7), InstructionKind::TakeLocal { local: LocalId(0) }),
            op(Some(8), InstructionKind::TakeLocal { local: LocalId(5) }),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(8),
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(5),
                    value: ValueId(7),
                },
            ),
        ];
        if fresh {
            body.append(&mut observation);
        } else {
            f.blocks[0].instructions.append(&mut observation);
        }
        f.blocks[0].terminator = Terminator::Jump(BlockId(4));
        f.blocks.push(BasicBlock {
            id: BlockId(4),
            instructions: body,
            terminator: Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(4),
            },
        });
        check(
            if fresh {
                "loop-fresh-control"
            } else {
                "loop-stale-swap"
            },
            m,
            fresh,
        );
    }
}
