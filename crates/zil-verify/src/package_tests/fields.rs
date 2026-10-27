//! Independent public raw and wire candidates for the partial-local slice.
//! The existing helper supplies only initialization/Hold claims; field states,
//! description roots and cleanup debts are always reconstructed by Verify.
use super::*;

fn take(b: &mut Body, local: LocalId, index: u32) -> ValueId {
    b.value(owned(), InstructionKind::TakePackageField { local, index })
}

fn dispose(b: &mut Body, value: ValueId) {
    let local = b.storage(owned());
    b.op(InstructionKind::Store { local, value });
    b.op(InstructionKind::DropManaged { local });
}

fn replace(b: &mut Body, local: LocalId, index: u32) {
    let value = b.token(99);
    b.op(InstructionKind::DropPackageField { local, index });
    b.op(InstructionKind::StorePackageField {
        local,
        index,
        value,
    });
}

fn replaced() -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let local = b.package_local();
    replace(&mut b, local, 0);
    b.op(InstructionKind::DropPackage { local });
    module(b.finish(None, CleanupExitKind::Return))
}

fn insert_replacement(m: &mut TypedModule, block: usize, at: usize, local: LocalId, index: u32) {
    let f = &mut m.functions[0];
    let code = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let value = ValueId(f.values.len() as u32);
    f.values.push(owned());
    f.blocks[block].instructions.splice(
        at..at,
        [
            ins(Some(code), InstructionKind::Constant(Constant::I32(99))),
            ins(
                Some(value),
                InstructionKind::MakeManaged { fields: vec![code] },
            ),
            ins(None, InstructionKind::DropPackageField { local, index }),
            ins(
                None,
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                },
            ),
        ],
    );
}

#[test]
fn take_refill_and_complete_return_pass_both_boundaries() {
    let mut b = Body::new(package());
    let local = b.package_local();
    let value = take(&mut b, local, 0);
    // The explicit conditional old cleanup is harmless after RHS took it.
    b.op(InstructionKind::DropPackageField { local, index: 0 });
    b.op(InstructionKind::StorePackageField {
        local,
        index: 0,
        value,
    });
    let whole = b.value(package(), InstructionKind::TakeLocal { local });
    accepted(module(b.finish(Some(whole), CleanupExitKind::Return)));
}

#[test]
fn partial_shell_keeps_copy_fields_and_sibling_descriptions_available() {
    let mut b = Body::new(Type::I32);
    let local = b.package_local();
    let sibling = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 1 },
    );
    let value = take(&mut b, local, 0);
    dispose(&mut b, value);
    let number = b.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    b.op(InstructionKind::StorePackageField {
        local,
        index: 2,
        value: number,
    });
    let result = b.value(
        Type::I32,
        InstructionKind::ReadDesc {
            desc: sibling,
            index: 0,
        },
    );
    b.op(InstructionKind::DropPackage { local });
    accepted(module(b.finish(Some(result), CleanupExitKind::Return)));
}

#[test]
fn even_an_empty_package_shell_requires_final_cleanup() {
    let mut b = Body::new(Type::Unit);
    let local = b.package_local();
    for index in 0..2 {
        let value = take(&mut b, local, index);
        dispose(&mut b, value);
    }
    b.op(InstructionKind::DropPackage { local });
    let m = module(b.finish(None, CleanupExitKind::Return));
    accepted(m.clone());
    let mut missing = m;
    missing.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    rejected(missing, "outstanding resource responsibility");
}

#[test]
fn let_parameter_can_transfer_a_field_but_cannot_refill_or_change_copy_content() {
    let mut b = Body::new(Type::Unit);
    let local = b.parameter(package());
    b.0.locals[local.0 as usize].mutable = false;
    let value = take(&mut b, local, 0);
    dispose(&mut b, value);
    b.op(InstructionKind::DropPackage { local });
    accepted(module(b.finish(None, CleanupExitKind::Return)));
    for index in [0, 2] {
        let mut b = Body::new(Type::Unit);
        let local = b.package_local();
        b.0.locals[local.0 as usize].mutable = false;
        let value = if index == 0 {
            take(&mut b, local, index)
        } else {
            b.value(Type::I32, InstructionKind::Constant(Constant::I32(1)))
        };
        b.op(InstructionKind::StorePackageField {
            local,
            index,
            value,
        });
        b.op(InstructionKind::DropPackage { local });
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "requires mutable storage",
        );
    }
}

#[test]
fn repeated_take_and_description_of_a_missing_field_are_rejected() {
    for describe in [false, true] {
        let mut b = Body::new(Type::Unit);
        let local = b.package_local();
        let value = take(&mut b, local, 0);
        dispose(&mut b, value);
        if describe {
            b.value(
                desc(),
                InstructionKind::DescribePackageField { local, index: 0 },
            );
        } else {
            let value = take(&mut b, local, 0);
            dispose(&mut b, value);
        }
        b.op(InstructionKind::DropPackage { local });
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "requires a live field",
        );
    }
}

#[test]
fn partial_package_cannot_manufacture_a_complete_ssa_or_escape_by_return() {
    let mut b = Body::new(package());
    let local = b.package_local();
    let value = take(&mut b, local, 1);
    dispose(&mut b, value);
    let whole = b.value(package(), InstructionKind::TakeLocal { local });
    rejected(
        module(b.finish(Some(whole), CleanupExitKind::Return)),
        "requires every managed field live",
    );
}

#[test]
fn live_field_overwrite_and_reused_ssa_are_rejected() {
    let mut m = replaced();
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackageField { .. }));
    rejected(
        m,
        "field store would overwrite an outstanding responsibility",
    );
    let mut b = Body::new(Type::Unit);
    let local = b.package_local();
    for index in 0..2 {
        b.op(InstructionKind::DropPackageField { local, index });
    }
    let value = b.token(67);
    for index in 0..2 {
        b.op(InstructionKind::StorePackageField {
            local,
            index,
            value,
        });
    }
    b.op(InstructionKind::DropPackage { local });
    rejected(
        module(b.finish(None, CleanupExitKind::Return)),
        "SSA value was not available exactly once",
    );
}

#[test]
fn conditional_field_cleanup_is_idempotent_without_ending_the_shell() {
    let mut b = Body::new(Type::Unit);
    let local = b.package_local();
    b.op(InstructionKind::DropPackageField { local, index: 0 });
    b.op(InstructionKind::DropPackageField { local, index: 0 });
    let replacement = b.token(67);
    b.op(InstructionKind::StorePackageField {
        local,
        index: 0,
        value: replacement,
    });
    b.op(InstructionKind::DropPackage { local });
    accepted(module(b.finish(None, CleanupExitKind::Return)));
}

#[test]
fn field_operations_recheck_local_kind_index_and_operand_presence() {
    for operation in 0..3 {
        for (local, index, needle) in [
            (LocalId(999), 0, "absent local"),
            (LocalId(0), 99, "outside its declaration"),
        ] {
            let mut b = Body::new(Type::Unit);
            b.package_local();
            match operation {
                0 => {
                    take(&mut b, local, index);
                }
                1 => b.op(InstructionKind::DropPackageField { local, index }),
                _ => {
                    let value = b.token(3);
                    b.op(InstructionKind::StorePackageField {
                        local,
                        index,
                        value,
                    });
                }
            }
            rejected(module(b.finish(None, CleanupExitKind::Return)), needle);
        }
        let mut b = Body::new(Type::Unit);
        let local = b.storage(Type::I32);
        let number = b.value(Type::I32, InstructionKind::Constant(Constant::I32(0)));
        b.op(InstructionKind::Store {
            local,
            value: number,
        });
        match operation {
            0 => {
                take(&mut b, local, 0);
            }
            1 => b.op(InstructionKind::DropPackageField { local, index: 0 }),
            _ => b.op(InstructionKind::StorePackageField {
                local,
                index: 0,
                value: number,
            }),
        }
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "requires a package local",
        );
    }
    let mut m = replaced();
    for i in &mut m.functions[0].blocks[0].instructions {
        if let InstructionKind::StorePackageField { value, .. } = &mut i.kind {
            *value = ValueId(999);
        }
    }
    rejected(m, "absent SSA value");
}

#[test]
fn copy_fields_cannot_be_taken_or_dropped_as_managed_values() {
    for drop in [false, true] {
        let mut b = Body::new(Type::Unit);
        let local = b.package_local();
        if drop {
            b.op(InstructionKind::DropPackageField { local, index: 2 });
        } else {
            take(&mut b, local, 2);
        }
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "requires an owning field",
        );
    }
}

#[test]
fn result_shape_and_exact_store_type_are_rechecked() {
    for kind in [0, 1, 2] {
        let mut b = Body::new(Type::Unit);
        let local = b.package_local();
        match kind {
            0 => b.op(InstructionKind::TakePackageField { local, index: 0 }),
            1 => {
                b.value(
                    Type::Unit,
                    InstructionKind::DropPackageField { local, index: 0 },
                );
            }
            _ => {
                let value = b.token(99);
                b.value(
                    Type::Unit,
                    InstructionKind::StorePackageField {
                        local,
                        index: 0,
                        value,
                    },
                );
            }
        }
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "result presence or type is incorrect",
        );
    }
    for index in [0, 2] {
        let mut b = Body::new(Type::Unit);
        let local = b.package_local();
        let value = if index == 0 {
            b.value(Type::I32, InstructionKind::Constant(Constant::I32(1)))
        } else {
            b.token(2)
        };
        b.op(InstructionKind::StorePackageField {
            local,
            index,
            value,
        });
        rejected(
            module(b.finish(None, CleanupExitKind::Return)),
            "differs from its declared field type",
        );
    }
}

#[test]
fn store_rejects_a_different_but_individually_valid_nominal_resource() {
    let mut m = replaced();
    let other = Type::Record {
        name: "OtherToken".into(),
        fields: vec![("code".into(), Type::I32)],
    };
    m.resources.push(ResourceDefinition {
        name: "OtherToken".into(),
        payload: other.clone(),
        teardown: None,
    });
    let value = m.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            if let InstructionKind::StorePackageField { value, .. } = i.kind {
                Some(value)
            } else {
                None
            }
        })
        .unwrap();
    m.functions[0].values[value.0 as usize] = Type::Managed(Box::new(other));
    rejected(m, "differs from its declared field type");
}

#[test]
fn neither_field_drop_nor_store_can_reactivate_an_uninitialized_or_ended_shell() {
    for ended in [false, true] {
        for operation in 0..3 {
            let mut b = Body::new(Type::Unit);
            let local = if ended {
                b.package_local()
            } else {
                b.storage(package())
            };
            if ended {
                b.op(InstructionKind::DropPackage { local });
            }
            match operation {
                0 => {
                    take(&mut b, local, 0);
                }
                1 => b.op(InstructionKind::DropPackageField { local, index: 0 }),
                _ => {
                    let value = b.token(99);
                    b.op(InstructionKind::StorePackageField {
                        local,
                        index: 0,
                        value,
                    });
                }
            }
            rejected(
                module(b.finish(None, CleanupExitKind::Return)),
                "before initialization",
            );
        }
    }
}

#[test]
fn field_transfer_keeps_input_provenance_at_create_returns() {
    let mut b = Body::new(owned());
    b.0.result_mode = PassingMode::Give;
    let local = b.package_local();
    let value = take(&mut b, local, 0);
    b.op(InstructionKind::DropPackage { local });
    let m = module(b.finish(Some(value), CleanupExitKind::Return));
    accepted(m.clone());
    let mut forged = m;
    forged.functions[0].result_mode = PassingMode::Create;
    rejected(forged, "create result is not independently proven fresh");
}

#[test]
fn old_description_cannot_be_redirected_by_same_slot_replacement_or_refill() {
    for transfer in [false, true] {
        let mut b = Body::new(Type::I32);
        let local = b.package_local();
        let old = b.value(
            desc(),
            InstructionKind::DescribePackageField { local, index: 0 },
        );
        if transfer {
            let value = take(&mut b, local, 0);
            b.op(InstructionKind::StorePackageField {
                local,
                index: 0,
                value,
            });
        } else {
            replace(&mut b, local, 0);
        }
        let result = b.value(
            Type::I32,
            InstructionKind::ReadDesc {
                desc: old,
                index: 0,
            },
        );
        b.op(InstructionKind::DropPackage { local });
        rejected(
            module(b.finish(Some(result), CleanupExitKind::Return)),
            "invalidate a live description",
        );
    }
}

#[test]
fn old_description_alias_and_captured_description_keep_the_precise_field() {
    let mut b = Body::new(Type::I32);
    let local = b.package_local();
    let alias = b.storage(desc());
    let old = b.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
    );
    b.op(InstructionKind::Store {
        local: alias,
        value: old,
    });
    replace(&mut b, local, 0);
    let old = b.value(desc(), InstructionKind::Load(alias));
    let result = b.value(
        Type::I32,
        InstructionKind::ReadDesc {
            desc: old,
            index: 0,
        },
    );
    b.op(InstructionKind::DropPackage { local });
    rejected(
        module(b.finish(Some(result), CleanupExitKind::Return)),
        "invalidate a live description",
    );
    for index in [0, 1] {
        let mut m = description_closure(false);
        let at = m.functions[0].blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
            .unwrap();
        insert_replacement(&mut m, 0, at, LocalId(0), index);
        if index == 0 {
            accepted(m);
        } else {
            rejected(m, "invalidate a live description");
        }
    }
}

#[test]
fn merged_description_origins_block_only_their_actual_fields() {
    for index in [0, 1] {
        let mut m = branch_description(false);
        // Alias may name first.0 or second.1; second.0 is independent.
        insert_replacement(&mut m, 3, 0, LocalId(2), index);
        if index == 0 {
            accepted(m);
        } else {
            rejected(m, "invalidate a live description");
        }
    }
}

#[test]
fn field_replacement_does_not_change_shell_cleanup_order() {
    let mut b = Body::new(Type::Unit);
    let package = b.package_local();
    let later = b.storage(owned());
    let value = b.token(67);
    b.op(InstructionKind::Store {
        local: later,
        value,
    });
    replace(&mut b, package, 0);
    b.op(InstructionKind::DropManaged { local: later });
    b.op(InstructionKind::DropPackage { local: package });
    let m = module(b.finish(None, CleanupExitKind::Return));
    accepted(m.clone());
    let mut wrong = m;
    let ops = &mut wrong.functions[0].blocks[0].instructions;
    let at = ops
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    ops.swap(at, at + 1);
    rejected(wrong, "not in reverse successful initialization order");
}

fn conditional_field(replace_after: bool, drop_old: bool, whole_return: bool) -> TypedModule {
    let mut b = Body::new(if whole_return { package() } else { Type::Unit });
    let local = b.parameter(package());
    let flag = b.parameter(Type::Bool);
    let condition = b.value(Type::Bool, InstructionKind::Load(flag));
    let prefix = b.0.blocks[0].instructions.len();
    let value = take(&mut b, local, 0);
    // A temporary exists only on this edge and its lexical scope closes here.
    b.op(InstructionKind::ScopeEnter { scope: ScopeId(1) });
    dispose(&mut b, value);
    let temporary = LocalId((b.0.locals.len() - 1) as u32);
    b.op(InstructionKind::StorageDead(temporary));
    b.op(InstructionKind::ScopeExit {
        scope: ScopeId(1),
        kind: CleanupExitKind::Fallthrough,
    });
    let left = b.0.blocks[0].instructions.split_off(prefix);
    if replace_after {
        let value = b.token(99);
        if drop_old {
            b.op(InstructionKind::DropPackageField { local, index: 0 });
        }
        b.op(InstructionKind::StorePackageField {
            local,
            index: 0,
            value,
        });
    }
    let result = if whole_return {
        Some(b.value(package(), InstructionKind::TakeLocal { local }))
    } else {
        b.op(InstructionKind::DropPackage { local });
        None
    };
    let mut f = b.0;
    for local in [flag, local] {
        f.blocks[0]
            .instructions
            .push(ins(None, InstructionKind::StorageDead(local)));
    }
    f.blocks[0].instructions.push(ins(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    ));
    let merge = f.blocks[0].instructions.split_off(prefix);
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: left,
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: merge,
            terminator: Terminator::Return(result),
        },
    ]);
    module(f)
}

#[test]
fn live_moved_join_requires_conditional_old_cleanup_before_refill() {
    accepted(conditional_field(true, true, true));
    accepted(conditional_field(false, false, false));
    rejected(
        conditional_field(true, false, true),
        "field store would overwrite an outstanding responsibility",
    );
    rejected(
        conditional_field(false, false, true),
        "requires every managed field live",
    );
}

fn field_loop(fresh_description: bool, old_description: bool) -> TypedModule {
    let mut b = Body::new(Type::Unit);
    let local = b.parameter(package());
    let flag = b.parameter(Type::Bool);
    let old = old_description.then(|| {
        b.value(
            desc(),
            InstructionKind::DescribePackageField { local, index: 0 },
        )
    });
    let prefix = b.0.blocks[0].instructions.len();
    let condition = b.value(Type::Bool, InstructionKind::Load(flag));
    let header = b.0.blocks[0].instructions.split_off(prefix);
    let current = if fresh_description {
        Some(b.value(
            desc(),
            InstructionKind::DescribePackageField { local, index: 0 },
        ))
    } else {
        old
    };
    if let Some(desc) = current {
        b.value(Type::I32, InstructionKind::ReadDesc { desc, index: 0 });
    }
    replace(&mut b, local, 0);
    let body = b.0.blocks[0].instructions.split_off(prefix);
    b.op(InstructionKind::DropPackage { local });
    let mut f = b.finish(None, CleanupExitKind::Return);
    let exit = f.blocks[0].instructions.split_off(prefix);
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: header,
            terminator: Terminator::Branch {
                condition,
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: body,
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: exit,
            terminator: Terminator::Return(None),
        },
    ]);
    module(f)
}

#[test]
fn loop_replacement_gets_fresh_instances_without_retargeting_old_descriptions() {
    accepted(field_loop(false, false));
    accepted(field_loop(true, false));
    // The old use is before this iteration's replacement, but the backedge
    // keeps that same old instance live for the following iteration.
    rejected(field_loop(false, true), "invalidate a live description");
}

#[test]
fn loop_cannot_redefine_unconsumed_rhs_or_omit_old_field_cleanup() {
    let mut m = field_loop(false, false);
    m.functions[0].blocks[2]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorePackageField { .. }));
    rejected(m, "loop redefines a managed SSA result");
    let mut m = field_loop(false, false);
    m.functions[0].blocks[2]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackageField { .. }));
    rejected(
        m,
        "field store would overwrite an outstanding responsibility",
    );
}

#[test]
fn field_accesses_require_active_lexical_storage_even_with_initialized_claims() {
    let mut m = replaced();
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorageLive(_)));
    rejected(m, "local operation occurs outside its active storage scope");
}

#[test]
fn rhs_error_cleans_prepared_temporary_and_remaining_package_responsibility() {
    let mut b = Body::new(error_type());
    let local = b.package_local();
    // A prior field transfer is allowed; a later RHS fails before any commit.
    let taken = take(&mut b, local, 1);
    dispose(&mut b, taken);
    let temporary = b.storage(owned());
    let value = b.token(99);
    b.op(InstructionKind::Store {
        local: temporary,
        value,
    });
    let error = error_value(&mut b);
    b.op(InstructionKind::DropManaged { local: temporary });
    b.op(InstructionKind::DropPackage { local });
    let m = module(b.finish(Some(error), CleanupExitKind::Error));
    accepted(m.clone());
    let mut lost = m;
    lost.functions[0].blocks[0].instructions.retain(
        |i| !matches!(i.kind, InstructionKind::DropManaged { local } if local == temporary),
    );
    rejected(lost, "reverse successful initialization order");
}

fn wide_parameter(width: usize) -> TypedModule {
    let ty = Type::Package {
        name: "Wide".into(),
        fields: (0..width).map(|i| (format!("r{i}"), owned())).collect(),
    };
    let mut b = Body::new(Type::Unit);
    let local = b.parameter(ty);
    b.op(InstructionKind::DropPackage { local });
    module(b.finish(None, CleanupExitKind::Return))
}

fn responsibility_cost(m: &TypedModule, limit: usize) -> Check<usize> {
    let graphs = m
        .functions
        .iter()
        .map(|_| FunctionGraph {
            successors: vec![vec![]],
            predecessors: vec![vec![]],
            reachable: vec![true],
            package_views: packages::CaptureViews::default(),
        })
        .collect::<Vec<_>>();
    let mut budget = Budget { remaining: limit };
    resources::check_module(m, &graphs, &mut budget)?;
    Ok(limit - budget.remaining)
}

#[test]
fn per_field_responsibility_states_and_scans_consume_the_work_budget() {
    let small = wide_parameter(1);
    let large = wide_parameter(64);
    accepted(small.clone());
    accepted(large.clone());
    let small_cost = responsibility_cost(&small, 1_000_000).unwrap();
    let large_cost = responsibility_cost(&large, 1_000_000).unwrap();
    assert!(
        large_cost > small_cost + 64,
        "field states must incur real responsibility work"
    );
    assert_eq!(
        responsibility_cost(&large, small_cost)
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
    let limits = Limits {
        max_analysis_steps: small_cost,
        ..Limits::default()
    };
    let plan = resource_tests::plan(&large);
    let wire = zil_ir::wire::encode_module_candidate(&large, &plan, &Limits::default()).unwrap();
    for result in [
        verify_module(large.clone(), plan, &large.target, &limits),
        verify_module_bytes(&wire, &large.target, &limits),
    ] {
        assert_eq!(
            result.unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}

#[test]
fn responsibility_pass_does_not_rescan_unrelated_nominal_resource_definitions() {
    let small = wide_parameter(2);
    let mut large = small.clone();
    for index in 0..128 {
        let name = format!("Unrelated{index}");
        large.resources.push(ResourceDefinition {
            payload: Type::Record {
                name: name.clone(),
                fields: vec![],
            },
            name,
            teardown: None,
        });
    }
    accepted(large.clone());
    assert_eq!(
        responsibility_cost(&small, 1_000_000).unwrap(),
        responsibility_cost(&large, 1_000_000).unwrap()
    );
}

#[test]
fn no_package_scalar_controls_charge_resource_frontier_setup_exactly() {
    // 080 adds the actually executed dependency scan/queue to the existing
    // no-Package control. Frozen complete responsibility-pass N values include
    // both final validation and setup; the prior 10/258 failure is retained.
    // This fixture deliberately registers a resource despite having no owning
    // or description locals, so it reaches the resource pass's scalar path.
    for (count, expected_cost) in [(1, 22), (32, 394)] {
        let mut b = Body::new(Type::Unit);
        for value in 0..count {
            let local = b.storage(Type::I32);
            let value = b.value(Type::I32, InstructionKind::Constant(Constant::I32(value)));
            b.op(InstructionKind::Store { local, value });
        }
        let m = module(b.finish(None, CleanupExitKind::Return));
        accepted(m.clone());
        assert_eq!(
            responsibility_cost(&m, expected_cost).unwrap(),
            expected_cost
        );
        assert_eq!(
            responsibility_cost(&m, expected_cost - 1)
                .unwrap_err()
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}

#[test]
fn existing_normal_return_debt_scan_is_charged_at_the_exact_budget_boundary() {
    let mut b = Body::new(Type::Unit);
    let local = b.storage(owned());
    let value = b.token(65);
    b.op(InstructionKind::Store { local, value });
    b.op(InstructionKind::DropManaged { local });
    let normal = module(b.finish(None, CleanupExitKind::Return));
    let mut abort = normal.clone();
    abort.functions[0].blocks[0].terminator = Terminator::Trap;
    for instruction in &mut abort.functions[0].blocks[0].instructions {
        if let InstructionKind::ScopeExit { kind, .. } = &mut instruction.kind {
            *kind = CleanupExitKind::Fallthrough;
        }
    }
    accepted(normal.clone());
    accepted(abort.clone());
    let cost = responsibility_cost(&normal, 1_000_000).unwrap();
    let abort_cost = responsibility_cost(&abort, 1_000_000).unwrap();
    // The normal-return scan inspects the one ordinary owned local and the
    // already-consumed Managed SSA. Neither function contains a Package.
    assert_eq!(cost, abort_cost + 2);
    assert_eq!(responsibility_cost(&normal, cost).unwrap(), cost);
    assert_eq!(
        responsibility_cost(&normal, cost - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
}
