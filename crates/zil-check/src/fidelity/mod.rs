//! Source→IR fidelity checker (`spec/semantic-kernel.md`, P0-A / P5).
//!
//! `SourceMap` produced by `zil-typeck` is candidate evidence only. This module
//! re-derives nominal field declarations and field-selection expressions
//! directly from the original parsed AST (`zil_syntax::ast::Module`), verifies
//! complete coverage of every function and every resource operation, checks
//! evaluation ordering, and rejects missing or forged relations.

use std::collections::{HashMap, HashSet};
use zil_ir::body::{
    BlockId, Body, Callee, FieldIdx, FnId, FnRole, InstructionKind, Module, OpLocation, Projection,
    SourceMap, SourceRelation, Terminator,
};
use zil_syntax::ast;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityError {
    pub function: FnId,
    pub reason: &'static str,
}

/// Re-derives nominal field layouts, field-selection expressions, and trait
/// impl method declarations from the original AST and checks that `source_map`
/// faithfully and completely corresponds to `module`.
pub fn check_fidelity(
    module: &Module,
    source_map: &SourceMap,
    sources: &[&ast::Module],
) -> Result<(), FidelityError> {
    let ast_fields = collect_ast_field_indices(sources);
    let ast_selects = collect_ast_field_selects(sources);
    let ast_impl_methods = collect_ast_impl_methods(sources);
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
        if !sources.is_empty()
            && let FnRole::DynSlotImpl { dyn_id, slot, .. } = func.sig.role
        {
            let Some(dyn_def) = module.tys.get_dyn(dyn_id) else {
                return Err(fail("dyn slot impl references unknown DynId"));
            };
            let Some(slot_def) = dyn_def.slots.get(slot as usize) else {
                return Err(fail("dyn slot impl references out-of-range slot"));
            };
            let expected_trait = dyn_def.name.rsplit('.').next().unwrap_or(&dyn_def.name);
            let expected_method = slot_def.name.as_str();
            let decl_start = fn_map.declaration.0;
            let fn_file = fn_map.spans.first().map(|(_, sp)| sp.file);
            let matched = ast_impl_methods.iter().any(|&(file_id, start, tr, m)| {
                start == decl_start
                    && fn_file.is_none_or(|f| f == file_id)
                    && tr == expected_trait
                    && m == expected_method
            });
            if !matched {
                return Err(fail(
                    "dyn slot impl function does not originate from matching AST trait impl method",
                ));
            }
        }
        let body = &func.body;

        let mut span_by_loc: HashMap<OpLocation, zil_base::Span> =
            HashMap::with_capacity(fn_map.spans.len());
        for (at, sp) in &fn_map.spans {
            if !valid_op_location(body, *at) {
                return Err(fail("source map span references invalid op location"));
            }
            span_by_loc.insert(*at, *sp);
        }

        let mut eval_order: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        let mut claimed_resource_ops: HashSet<OpLocation> = HashSet::new();
        for rel in &fn_map.relations {
            match rel {
                SourceRelation::Binding { local, .. } => {
                    if local.index() >= body.locals.len() {
                        return Err(fail("binding relation references unknown local"));
                    }
                }
                SourceRelation::FieldSelect { node, field, at } => {
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
                    // 1. Cross-check field ordinal against original AST declaration.
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
                    // 2. Cross-check source expression field name against IR field name.
                    // Enum variant payload fields (matched via pattern destructuring) and
                    // compiler-inserted partial-move Drop instructions (anchored at scope
                    // exit spans) do not correspond to `base.field` syntax.
                    let is_drop = at
                        .instruction
                        .and_then(|i| body.block(at.block)?.instructions.get(i as usize))
                        .is_some_and(|inst| matches!(inst.kind, InstructionKind::Drop { .. }));
                    if adt.kind != zil_ir::body::AdtKind::Enum
                        && !is_drop
                        && let Some(sp) = span_by_loc.get(at)
                        && let Some(file_selects) = ast_selects.get(&sp.file)
                    {
                        let mut found_any = false;
                        let mut matched = false;
                        let sp_start = u32::try_from(sp.start).unwrap_or(u32::MAX);
                        let sp_end = u32::try_from(sp.end).unwrap_or(u32::MAX);
                        for &(sel_start, sel_end, name) in file_selects {
                            let in_span = sel_start == node.0
                                || (sp_end > sp_start
                                    && sel_start >= sp_start
                                    && sel_end <= sp_end);
                            if in_span {
                                found_any = true;
                                if name == fdef.name.as_str() {
                                    matched = true;
                                    break;
                                }
                            }
                        }
                        if found_any && !matched {
                            return Err(fail(
                                "field name disagrees with original AST field selection",
                            ));
                        }
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
                    claimed_resource_ops.insert(*at);
                }
            }
        }

        // Completeness: every Drop and Intrinsic call in the IR must be accounted
        // for in SourceMap::ResourceOp so producers cannot silently omit them.
        for (bi, block) in body.blocks.iter().enumerate() {
            let block_id = BlockId::from_index(bi);
            for (ii, inst) in block.instructions.iter().enumerate() {
                if matches!(inst.kind, InstructionKind::Drop { .. }) {
                    let loc = OpLocation {
                        block: block_id,
                        instruction: Some(ii as u32),
                    };
                    if !claimed_resource_ops.contains(&loc) {
                        return Err(fail("missing ResourceOp relation for IR Drop instruction"));
                    }
                }
            }
            if matches!(
                block.terminator,
                Terminator::Call {
                    callee: Callee::Intrinsic(_),
                    ..
                }
            ) {
                let loc = OpLocation {
                    block: block_id,
                    instruction: None,
                };
                if !claimed_resource_ops.contains(&loc) {
                    return Err(fail("missing ResourceOp relation for IR Intrinsic call"));
                }
            }
        }
    }

    // Completeness: every function in the module must have a SourceMap entry.
    if seen_fns.len() != module.functions.len() {
        return Err(FidelityError {
            function: FnId(0),
            reason: "source map does not cover all functions in module",
        });
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

/// Collects `(file, decl_span_start, trait_last_segment, method_name)` for
/// every method declared inside an `impl Trait for Target` block in the
/// original parsed AST.
fn collect_ast_impl_methods<'a>(
    sources: &[&'a ast::Module],
) -> Vec<(zil_base::FileId, u32, &'a str, &'a str)> {
    let mut out = Vec::new();
    for m in sources {
        for item in &m.items {
            if let ast::ItemKind::Impl(imp) = &item.kind
                && let Some(trait_ref) = &imp.trait_type
                && let ast::TypeKind::Named { path, .. } = &trait_ref.kind
                && let Some(trait_name) = path.last()
            {
                for member in &imp.members {
                    if let ast::ItemKind::Function(f) = &member.kind {
                        let start = u32::try_from(member.span.start).unwrap_or(u32::MAX);
                        out.push((
                            member.span.file,
                            start,
                            trait_name.as_str(),
                            f.name.as_str(),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Collects `(span.start, span.end, field_name)` per file for every
/// `base.field` / `.field` expression in the original parsed AST.
type FileFieldSelects<'a> = HashMap<zil_base::FileId, Vec<(u32, u32, &'a str)>>;

fn collect_ast_field_selects<'a>(sources: &[&'a ast::Module]) -> FileFieldSelects<'a> {
    let mut map = HashMap::new();
    for m in sources {
        for item in &m.items {
            collect_item_selects(item, &mut map);
        }
    }
    map
}

fn collect_item_selects<'a>(item: &'a ast::Item, map: &mut FileFieldSelects<'a>) {
    match &item.kind {
        ast::ItemKind::Function(f) => collect_fn_selects(f, map),
        ast::ItemKind::Impl(imp) => {
            for member in &imp.members {
                collect_item_selects(member, map);
            }
        }
        ast::ItemKind::Const(c) => {
            if let Some(v) = &c.value {
                collect_expr_selects(v, map);
            }
        }
        ast::ItemKind::Aggregate(_)
        | ast::ItemKind::Enum(_)
        | ast::ItemKind::Extern { .. }
        | ast::ItemKind::Trait(_)
        | ast::ItemKind::TypeAlias(_)
        | ast::ItemKind::Import(_)
        | ast::ItemKind::Meta(_) => {}
    }
}

fn collect_fn_selects<'a>(f: &'a ast::Function, map: &mut FileFieldSelects<'a>) {
    match &f.body {
        ast::FunctionBody::Block(b) => collect_block_selects(b, map),
        ast::FunctionBody::Expression(e) => collect_expr_selects(e, map),
        ast::FunctionBody::Declaration => {}
    }
}

#[allow(clippy::wildcard_enum_match_arm)]
fn collect_block_selects<'a>(b: &'a ast::Block, map: &mut FileFieldSelects<'a>) {
    for stmt in &b.statements {
        match &stmt.kind {
            ast::StmtKind::Binding { value, .. } | ast::StmtKind::Yield(value) => {
                collect_expr_selects(value, map);
            }
            ast::StmtKind::Assign { target, value, .. } => {
                collect_expr_selects(target, map);
                collect_expr_selects(value, map);
            }
            ast::StmtKind::Expression(e) => collect_expr_selects(e, map),
            ast::StmtKind::Return(Some(e)) => collect_expr_selects(e, map),
            ast::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_cond_selects(condition, map);
                collect_block_selects(then_body, map);
                if let Some(eb) = else_body {
                    match eb {
                        ast::ElseBranch::Block(b) => collect_block_selects(b, map),
                        ast::ElseBranch::If(s) => {
                            if let ast::StmtKind::If {
                                condition,
                                then_body,
                                ..
                            } = &s.kind
                            {
                                collect_cond_selects(condition, map);
                                collect_block_selects(then_body, map);
                            }
                        }
                    }
                }
            }
            ast::StmtKind::While { condition, body } => {
                collect_cond_selects(condition, map);
                collect_block_selects(body, map);
            }
            ast::StmtKind::For { iterable, body, .. }
            | ast::StmtKind::With {
                value: iterable,
                body,
                ..
            } => {
                collect_expr_selects(iterable, map);
                collect_block_selects(body, map);
            }
            ast::StmtKind::Loop(body)
            | ast::StmtKind::Unsafe(body)
            | ast::StmtKind::Defer(body) => {
                collect_block_selects(body, map);
            }
            ast::StmtKind::Match { value, arms } => {
                collect_expr_selects(value, map);
                for arm in arms {
                    match &arm.body {
                        ast::ArrowBody::Expression(e) => collect_expr_selects(e, map),
                        ast::ArrowBody::Block(b) => collect_block_selects(b, map),
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_cond_selects<'a>(c: &'a ast::Condition, map: &mut FileFieldSelects<'a>) {
    match c {
        ast::Condition::Expression(e) | ast::Condition::Let { value: e, .. } => {
            collect_expr_selects(e, map);
        }
    }
}

#[allow(clippy::wildcard_enum_match_arm)]
fn collect_expr_selects<'a>(e: &'a ast::Expr, map: &mut FileFieldSelects<'a>) {
    match &e.kind {
        ast::ExprKind::Member { object, member } => {
            if let ast::MemberName::Name(name) = member
                && let (Ok(start), Ok(end)) =
                    (u32::try_from(e.span.start), u32::try_from(e.span.end))
            {
                map.entry(e.span.file)
                    .or_default()
                    .push((start, end, name.as_str()));
            }
            collect_expr_selects(object, map);
        }
        ast::ExprKind::ReceiverMember(name) => {
            if let (Ok(start), Ok(end)) = (u32::try_from(e.span.start), u32::try_from(e.span.end)) {
                map.entry(e.span.file)
                    .or_default()
                    .push((start, end, name.as_str()));
            }
        }
        ast::ExprKind::Binary { left, right, .. }
        | ast::ExprKind::Pipeline {
            input: left,
            target: right,
        }
        | ast::ExprKind::Index {
            object: left,
            index: right,
        } => {
            collect_expr_selects(left, map);
            collect_expr_selects(right, map);
        }
        ast::ExprKind::Unary { operand, .. }
        | ast::ExprKind::Group(operand)
        | ast::ExprKind::Try(operand)
        | ast::ExprKind::Ascribed { value: operand, .. }
        | ast::ExprKind::TypeApply {
            callee: operand, ..
        } => collect_expr_selects(operand, map),
        ast::ExprKind::Call { callee, arguments } => {
            collect_expr_selects(callee, map);
            for a in arguments {
                collect_expr_selects(&a.value, map);
            }
        }
        ast::ExprKind::Slice { object, start, end } => {
            collect_expr_selects(object, map);
            if let Some(s) = start {
                collect_expr_selects(s, map);
            }
            if let Some(en) = end {
                collect_expr_selects(en, map);
            }
        }
        ast::ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            collect_expr_selects(condition, map);
            collect_expr_selects(then_value, map);
            collect_expr_selects(else_value, map);
        }
        ast::ExprKind::Closure { body, .. } => match body {
            ast::ArrowBody::Expression(ex) => collect_expr_selects(ex, map),
            ast::ArrowBody::Block(b) => collect_block_selects(b, map),
        },
        ast::ExprKind::Tuple(xs) | ast::ExprKind::Array(xs) => {
            for x in xs {
                collect_expr_selects(x, map);
            }
        }
        ast::ExprKind::Record { base, fields, .. } => {
            if let Some(b) = base {
                collect_expr_selects(b, map);
            }
            for f in fields {
                if let Some(v) = &f.value {
                    collect_expr_selects(v, map);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, NodeId, SourceUnit, Span};
    use zil_ir::body::{
        AdtDef, AdtKind, Block, Body, FieldDef, FieldRef, FnRole, FnSig, Function,
        FunctionSourceMap, GenericArgs, Instruction, InstructionKind, IntTy, Local, LocalDecl,
        Module, Operand, ParamEnv, Place, Rvalue, SourceMap, SourceRelation, Terminator, TyKind,
        TyTable, VariantDef, VariantIdx,
    };

    fn dummy_pair_module(swapped_fields: bool) -> (Module, FieldRef) {
        let mut tys = TyTable::new();
        let i32_ty = tys.intern(TyKind::Int(IntTy::I32));
        let (f0, f1) = if swapped_fields {
            ("b", "a")
        } else {
            ("a", "b")
        };
        let adt = tys.add_adt(AdtDef {
            name: "Pair".into(),
            kind: AdtKind::Record,
            type_params: 0,
            region_params: 0,
            variants: vec![VariantDef {
                name: "Pair".into(),
                fields: vec![
                    FieldDef {
                        name: f0.into(),
                        ty: i32_ty,
                    },
                    FieldDef {
                        name: f1.into(),
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
            dyn_impls: Vec::new(),
        };
        (module, field_ref)
    }

    #[test]
    fn ast_field_ordinal_mismatch_is_rejected() {
        let unit = SourceUnit {
            file_id: FileId(0),
            module_id: ModuleId(0),
            path: "ast_fidelity.zil".into(),
            text: "struct Pair { a: i32; b: i32 }\nfn main() -> i32 { return 0 }\n".into(),
        };
        let parsed = zil_syntax::parse(&unit).into_result().expect("parses");
        let (module, field_ref) = dummy_pair_module(true);
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

    #[test]
    fn missing_function_entry_in_source_map_is_rejected() {
        let (module, _) = dummy_pair_module(false);
        let empty = SourceMap::default();
        let err = check_fidelity(&module, &empty, &[])
            .expect_err("empty source map must be rejected for non-empty module");
        assert!(err.reason.contains("cover all functions"));
    }

    #[test]
    fn inverted_evaluation_order_is_rejected() {
        let (mut module, _) = dummy_pair_module(false);
        // Add a second instruction so positions 0 and 1 exist.
        let copy_inst = module.functions[0].body.blocks[0].instructions[0].clone();
        module.functions[0].body.blocks[0]
            .instructions
            .push(copy_inst);
        let smap = SourceMap {
            functions: vec![FunctionSourceMap {
                function: FnId(0),
                declaration: NodeId(0),
                relations: vec![
                    SourceRelation::Evaluation {
                        node: NodeId(42),
                        ordinal: 0,
                        at: OpLocation {
                            block: BlockId(0),
                            instruction: Some(1),
                        },
                    },
                    SourceRelation::Evaluation {
                        node: NodeId(42),
                        ordinal: 1,
                        at: OpLocation {
                            block: BlockId(0),
                            instruction: Some(0),
                        },
                    },
                ],
                spans: Vec::new(),
            }],
        };
        let err = check_fidelity(&module, &smap, &[])
            .expect_err("inverted evaluation order must be rejected");
        assert!(err.reason.contains("evaluation order"));
    }

    #[test]
    fn omitted_resource_drop_in_source_map_is_rejected() {
        let (mut module, _) = dummy_pair_module(false);
        module.functions[0].body.blocks[0]
            .instructions
            .push(Instruction {
                kind: InstructionKind::Drop {
                    place: Place::local(Local(0)),
                },
                span: Span::new(FileId(0), 0, 0),
            });
        let smap = SourceMap {
            functions: vec![FunctionSourceMap {
                function: FnId(0),
                declaration: NodeId(0),
                relations: Vec::new(),
                spans: Vec::new(),
            }],
        };
        let err = check_fidelity(&module, &smap, &[])
            .expect_err("omitted ResourceOp for Drop must be rejected");
        assert!(err.reason.contains("missing ResourceOp"));
    }
}
