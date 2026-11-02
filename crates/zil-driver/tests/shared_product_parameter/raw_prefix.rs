//! Untrusted claims for these bounded straight-line source fixtures only.
//! This does not invoke Analyze or reuse either core's inference routine.
use zil_ir::{
    BlockPlan, CandidatePlan, FunctionPlan, HoldRoot, InstructionKind as I, Type, TypedModule,
    ValueHold,
};

pub(super) fn straight_line_claims(module: &TypedModule) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(module);
    for function in &module.functions {
        assert_eq!(function.blocks.len(), 1, "fixture requires one block");
        let block = &function.blocks[0];
        let mut local_roots = vec![Vec::new(); function.locals.len()];
        for (index, parameter) in function.parameters.iter().enumerate() {
            if matches!(
                function.locals[parameter.0 as usize].ty,
                Type::Address { .. } | Type::Desc(_)
            ) {
                local_roots[parameter.0 as usize] = vec![HoldRoot::Parameter(index as u32)];
            }
        }
        let mut values = vec![Vec::new(); function.values.len()];
        for instruction in &block.instructions {
            if let I::Store { local, value } = instruction.kind {
                local_roots[local.0 as usize] = values[value.0 as usize].clone();
            }
            let Some(result) = instruction.result else {
                continue;
            };
            let carrier = matches!(
                function.values[result.0 as usize],
                Type::Address { .. } | Type::Desc(_) | Type::Closure(_)
            );
            if !carrier {
                continue;
            }
            let mut roots = match &instruction.kind {
                I::Load(local) => local_roots[local.0 as usize].clone(),
                I::BorrowLocal { local }
                | I::BorrowLocalMut { local }
                | I::DescribeLocal { local }
                | I::DescribePackageField { local, .. }
                | I::BorrowPackageField { local, .. } => vec![HoldRoot::Local(*local)],
                I::ReborrowAddress { address, .. }
                | I::ReadAddress { address }
                | I::AddressField { address, .. }
                | I::AddressIndex { address, .. } => values[address.0 as usize].clone(),
                I::OwnedStoreBorrow { store, .. } | I::OwnedStoreBorrowMut { store, .. } => {
                    values[store.0 as usize].clone()
                }
                I::ReadCapture { index }
                | I::BorrowCapture { index, .. }
                | I::DescribeCapture { index } => vec![HoldRoot::Capture(*index)],
                I::MakeClosure { captures, .. } => captures
                    .iter()
                    .flat_map(|v| values[v.0 as usize].iter().copied())
                    .collect(),
                other => panic!("unaccounted fixture carrier: {other:?}"),
            };
            roots.sort();
            roots.dedup();
            assert!(
                !roots.is_empty(),
                "fixture address/environment needs explicit roots"
            );
            values[result.0 as usize] = roots;
        }
        plan.functions.push(FunctionPlan {
            function: function.id,
            blocks: vec![BlockPlan {
                block: block.id,
                initialized_in: function.parameters.clone(),
                initialized_out: vec![],
            }],
            holds: values
                .into_iter()
                .enumerate()
                .map(|(index, roots)| ValueHold {
                    value: zil_ir::ValueId(index as u32),
                    roots,
                })
                .collect(),
        });
    }
    plan
}
