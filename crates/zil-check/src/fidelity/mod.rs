//! Source→IR fidelity checker (`spec/semantic-kernel.md`, P5).
//!
//! `SourceMap` produced by `zil-typeck` is candidate evidence only. This module
//! re-derives nominal field declarations directly from the original parsed AST
//! (`zil_syntax::ast::Module`) and verifies that every claimed relation in the
//! `SourceMap` matches the actual Body IR operations (via `OpSpec`) and the
//! source declarations.

use std::collections::{HashMap, HashSet};
use zil_ir::body::{
    Body, Callee, FieldIdx, FnId, InstructionKind, Module, OpLocation, Projection, SourceMap,
    SourceRelation, Terminator,
};
use zil_syntax::ast;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityError {
    pub function: FnId,
    pub reason: &'static str,
}

/// Re-derives nominal field layouts from the original AST and checks that
/// `source_map` faithfully corresponds to `module`.
pub fn check_fidelity(
    module: &Module,
    source_map: &SourceMap,
    sources: &[&ast::Module],
) -> Result<(), FidelityError> {
    let ast_fields = collect_ast_field_indices(sources);
    let mut seen_fns = HashSet::with_capacity(source_map.functions.len());

    for fn_map in &source_map.functions {
        let fn_id = fn_map.function;
        let fail = |reason| FidelityError {
            function: fn_id,
            reason,
        };
        if !seen_fns.insert(fn_id) {
            return Err(fail("duplicate function entry in source map"));
        }
        let Some(func) = module.functions.get(fn_id.index()) else {
            return Err(fail("source map references unknown function"));
        };
        let body = &func.body;

        for (at, _) in &fn_map.spans {
            if !valid_op_location(body, *at) {
                return Err(fail("source map span references invalid op location"));
            }
        }

        let mut eval_order: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        for rel in &fn_map.relations {
            match rel {
                SourceRelation::Binding { local, .. } => {
                    if local.index() >= body.locals.len() {
                        return Err(fail("binding relation references unknown local"));
                    }
                }
                SourceRelation::FieldSelect { field, at, .. } => {
                    if !op_projects_field(body, *at, *field) {
                        return Err(fail(
                            "field select relation does not match IR operation place",
                        ));
                    }
                    let Some(adt) = module.tys.get_adt(field.adt) else {
                        return Err(fail("field select references unknown ADT"));
                    };
                    let Some(fdef) = adt.field(field.variant, field.field) else {
                        return Err(fail("field select references out-of-range field"));
                    };
                    // Re-derive field ordinal from original AST when declared there.
                    let bare_name = adt.name.rsplit('.').next().unwrap_or(&adt.name);
                    if let Some(expected_idx) = ast_fields
                        .get(&(adt.name.as_str(), fdef.name.as_str()))
                        .or_else(|| ast_fields.get(&(bare_name, fdef.name.as_str())))
                        && *expected_idx != field.field
                    {
                        return Err(fail(
                            "field ordinal disagrees with original AST declaration",
                        ));
                    }
                }
                SourceRelation::Evaluation { node, ordinal, at } => {
                    if !valid_op_location(body, *at) {
                        return Err(fail("evaluation relation references invalid op location"));
                    }
                    let inst_pos = at.instruction.unwrap_or(u32::MAX);
                    let key = (node.0, at.block.0);
                    if let Some(&(prev_ord, prev_pos)) = eval_order.get(&key)
                        && *ordinal > prev_ord
                        && inst_pos < prev_pos
                    {
                        return Err(fail("evaluation order violates source order"));
                    }
                    eval_order.insert(key, (*ordinal, inst_pos));
                }
                SourceRelation::ResourceOp { at, .. } => {
                    if !op_is_resource_op(body, *at) {
                        return Err(fail("resource op relation does not match a resource IR op"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn valid_op_location(body: &Body, at: OpLocation) -> bool {
    let Some(block) = body.block(at.block) else {
        return false;
    };
    match at.instruction {
        Some(i) => (i as usize) < block.instructions.len(),
        None => true,
    }
}

fn op_projects_field(body: &Body, at: OpLocation, field: zil_ir::body::FieldRef) -> bool {
    let Some(block) = body.block(at.block) else {
        return false;
    };
    let accesses = match at.instruction {
        Some(i) => {
            let Some(inst) = block.instructions.get(i as usize) else {
                return false;
            };
            inst.kind.accesses()
        }
        None => block.terminator.accesses(),
    };
    accesses.iter().any(|a| {
        a.place
            .projections
            .iter()
            .any(|p| matches!(*p, Projection::Field(f) if f == field))
    })
}

fn op_is_resource_op(body: &Body, at: OpLocation) -> bool {
    let Some(block) = body.block(at.block) else {
        return false;
    };
    match at.instruction {
        Some(i) => block
            .instructions
            .get(i as usize)
            .is_some_and(|inst| matches!(inst.kind, InstructionKind::Drop { .. })),
        None => matches!(
            block.terminator,
            Terminator::Call {
                callee: Callee::Intrinsic(_),
                ..
            }
        ),
    }
}

/// Extracts `(type_name, field_name) -> FieldIdx` directly from parsed AST
/// nominal declarations without consulting typeck.
fn collect_ast_field_indices<'a>(
    sources: &[&'a ast::Module],
) -> HashMap<(&'a str, &'a str), FieldIdx> {
    let mut map = HashMap::new();
    for m in sources {
        for item in &m.items {
            match &item.kind {
                ast::ItemKind::Aggregate(a) => {
                    for (i, f) in a.fields.iter().enumerate() {
                        map.insert((a.name.as_str(), f.name.as_str()), FieldIdx::from_index(i));
                    }
                }
                ast::ItemKind::Enum(_)
                | ast::ItemKind::Function(_)
                | ast::ItemKind::Extern { .. }
                | ast::ItemKind::Impl(_)
                | ast::ItemKind::Trait(_)
                | ast::ItemKind::TypeAlias(_)
                | ast::ItemKind::Const(_)
                | ast::ItemKind::Import(_)
                | ast::ItemKind::Meta(_) => {}
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, NodeId, SourceUnit, Span};
    use zil_ir::body::{
        AdtDef, AdtKind, Block, BlockId, Body, FieldDef, FieldRef, FnRole, FnSig, Function,
        FunctionSourceMap, GenericArgs, Instruction, InstructionKind, IntTy, Local, LocalDecl,
        Module, Operand, ParamEnv, Place, Rvalue, SourceMap, SourceRelation, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx,
    };

    #[test]
    fn ast_field_ordinal_mismatch_is_rejected() {
        let unit = SourceUnit {
            file_id: FileId(0),
            module_id: ModuleId(0),
            path: "ast_fidelity.zil".into(),
            text: "struct Pair { a: i32; b: i32 }\nfn main() -> i32 { return 0 }\n".into(),
        };
        let parsed = zil_syntax::parse(&unit).into_result().expect("parses");

        let mut tys = TyTable::new();
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        // Deliberately swap field names in the IR ADT def compared to the AST!
        let adt = tys.add_adt(AdtDef {
            name: "Pair".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Pair".into(),
                fields: vec![
                    FieldDef {
                        name: "b".into(),
                        ty: i32_ty,
                    },
                    FieldDef {
                        name: "a".into(),
                        ty: i32_ty,
                    },
                ],
            }],
            user_drop: false,
        });
        let args = tys.intern_args(GenericArgs {
            tys: Vec::new(),
            regions: Vec::new(),
        });
        let pair_ty = tys.intern(TyKind::Adt { adt, args });
        let field_ref = FieldRef {
            adt,
            variant: VariantIdx(0),
            field: FieldIdx(0),
            args,
        };
        let body = Body {
            param_count: 1,
            locals: vec![
                LocalDecl {
                    ty: pair_ty,
                    name: Some("p".into()),
                },
                LocalDecl {
                    ty: i32_ty,
                    name: None,
                },
            ],
            captures: Vec::new(),
            closure: None,
            result: i32_ty,
            param_env: ParamEnv::default(),
            drop_flags: Vec::new(),
            blocks: vec![Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(1)),
                        value: Rvalue::Use(Operand::Copy(Place::local(Local(0)).field(field_ref))),
                    },
                    span: Span::new(FileId(0), 0, 0),
                }],
                terminator: Terminator::Return {
                    value: Operand::Copy(Place::local(Local(1))),
                },
                terminator_span: Span::new(FileId(0), 0, 0),
            }],
        };
        let module = Module {
            tys,
            functions: vec![Function {
                name: "main".into(),
                sig: FnSig {
                    params: vec![pair_ty],
                    result: i32_ty,
                    type_params: 0,
                    region_params: 0,
                    role: FnRole::Normal,
                },
                body,
            }],
            foreigns: Vec::new(),
            statics: Vec::new(),
        };
        let smap = SourceMap {
            functions: vec![FunctionSourceMap {
                function: FnId(0),
                declaration: NodeId(0),
                relations: vec![SourceRelation::FieldSelect {
                    node: NodeId(1),
                    field: field_ref,
                    at: OpLocation {
                        block: BlockId(0),
                        instruction: Some(0),
                    },
                }],
                spans: Vec::new(),
            }],
        };
        let err = check_fidelity(&module, &smap, &[&parsed])
            .expect_err("swapped field ordinal vs AST must be rejected");
        assert!(err.reason.contains("AST"));
    }
}
