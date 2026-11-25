//! Structured `zil_llvm_api::model::Module` builder for canonical `body_emit` units.
//!
//! Converts the canonical compiler-generated unit representation from `body_emit`
//! into an owned, typed `zil_llvm_api::model::Module` (lowering `switch` into
//! `Compare` + `CondBranch` chains) so `NativeBackend::LlvmApi` compiles directly
//! through `zil_llvm_api::compile_module` (`LLVMBuilder`) without invoking any
//! LLVM assembly text parser (`LLVMParseIRInContext`).

use zil_llvm_api::model::{
    Arg, AtomicOrdering, AtomicRmwOp, Attr, Block, Function, Global, Inst, Item, Linkage, Module,
    Op, Param, Signature, Ty, Val,
};

#[derive(Clone, Debug)]
pub struct StructuredBuildError {
    pub message: String,
}

impl StructuredBuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Converts a canonical `body_emit` LLVM unit into a structured `zil_llvm_api::model::Module`.
pub fn build_structured_module(
    unit_ir: &str,
    triple: &str,
    fingerprint: &str,
) -> Result<Module, StructuredBuildError> {
    let raw_lines: Vec<&str> = unit_ir.lines().collect();
    let mut items = Vec::new();
    let mut i = 0usize;

    while i < raw_lines.len() {
        let line = raw_lines[i].trim();
        i += 1;
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with("target ")
            || line.starts_with("source_filename")
        {
            continue;
        }
        if line.starts_with('%') && line.contains(" = type ") {
            items.push(parse_named_type(line)?);
        } else if line.starts_with('@') {
            items.push(Item::Global(parse_global(line)?));
        } else if line.starts_with("declare ") {
            items.push(Item::Function(parse_declare(line)?));
        } else if line.starts_with("define ") {
            let (func, next_i) = parse_define(line, &raw_lines, i)?;
            items.push(Item::Function(func));
            i = next_i;
        } else {
            return Err(StructuredBuildError::new(format!(
                "unexpected top-level IR line: {line}"
            )));
        }
    }

    Ok(Module {
        triple: triple.to_owned(),
        fingerprint: fingerprint.to_owned(),
        items,
    })
}

fn parse_named_type(line: &str) -> Result<Item, StructuredBuildError> {
    let (lhs, rhs) = line
        .split_once(" = type ")
        .ok_or_else(|| StructuredBuildError::new(format!("malformed named type: {line}")))?;
    let name = lhs
        .trim()
        .strip_prefix('%')
        .ok_or_else(|| StructuredBuildError::new(format!("expected %type name: {line}")))?
        .to_owned();
    let ty = parse_type_str(rhs.trim())?;
    let fields = match ty {
        Ty::Struct(fields) => fields,
        other => {
            return Err(StructuredBuildError::new(format!(
                "named type %{name} must be a struct, got {other}"
            )));
        }
    };
    Ok(Item::NamedType(name, fields))
}

fn parse_global(line: &str) -> Result<ItemGlobal, StructuredBuildError> {
    let (lhs, rhs) = line
        .split_once(" = ")
        .ok_or_else(|| StructuredBuildError::new(format!("malformed global: {line}")))?;
    let name = lhs
        .trim()
        .strip_prefix('@')
        .ok_or_else(|| StructuredBuildError::new(format!("expected @global name: {line}")))?
        .to_owned();

    let mut cur = Cursor::new(rhs);
    let mut linkage = Linkage::External;
    let mut is_external_decl = false;
    let mut thread_local = false;
    let mut unnamed_addr = false;
    let constant;

    loop {
        cur.skip_ws();
        if let Some(tok) = cur.peek_word() {
            match tok {
                "linkonce_odr" => {
                    cur.advance(tok.len());
                    linkage = Linkage::LinkOnceOdr;
                }
                "internal" => {
                    cur.advance(tok.len());
                    linkage = Linkage::Internal;
                }
                "private" => {
                    cur.advance(tok.len());
                    linkage = Linkage::Private;
                }
                "available_externally" => {
                    cur.advance(tok.len());
                    linkage = Linkage::AvailableExternally;
                }
                "external" => {
                    cur.advance(tok.len());
                    is_external_decl = true;
                }
                "thread_local" => {
                    cur.advance(tok.len());
                    thread_local = true;
                }
                "unnamed_addr" | "local_unnamed_addr" => {
                    cur.advance(tok.len());
                    unnamed_addr = true;
                }
                "constant" => {
                    cur.advance(tok.len());
                    constant = true;
                    break;
                }
                "global" => {
                    cur.advance(tok.len());
                    constant = false;
                    break;
                }
                other => {
                    return Err(StructuredBuildError::new(format!(
                        "unexpected global modifier '{other}' in: {line}"
                    )));
                }
            }
        } else {
            return Err(StructuredBuildError::new(format!(
                "incomplete global declaration: {line}"
            )));
        }
    }

    let ty = cur.parse_type()?;
    cur.skip_ws();

    let mut alignment = 0u32;
    let initializer = if is_external_decl || cur.is_eof() || cur.starts_with(", align ") {
        None
    } else {
        Some(cur.parse_val(&ty)?)
    };

    cur.skip_ws();
    if cur.eat_str(",") {
        cur.skip_ws();
        if cur.eat_word("align") {
            alignment = cur.parse_u32()?;
        }
    }

    Ok(Global {
        name,
        linkage,
        ty,
        initializer,
        constant,
        unnamed_addr,
        thread_local,
        alignment,
    })
}

type ItemGlobal = Global;

fn parse_declare(line: &str) -> Result<Function, StructuredBuildError> {
    let rest = line
        .strip_prefix("declare ")
        .ok_or_else(|| StructuredBuildError::new(format!("expected declare: {line}")))?;
    let (name, linkage, signature, attrs) = parse_fn_header(rest, false)?;
    Ok(Function {
        name,
        linkage,
        signature,
        attrs,
        blocks: None,
    })
}

fn parse_define(
    header_line: &str,
    lines: &[&str],
    mut idx: usize,
) -> Result<(Function, usize), StructuredBuildError> {
    let rest = header_line
        .strip_prefix("define ")
        .ok_or_else(|| StructuredBuildError::new(format!("expected define: {header_line}")))?;
    let rest = rest
        .strip_suffix('{')
        .ok_or_else(|| {
            StructuredBuildError::new(format!("expected '{{' on define line: {header_line}"))
        })?
        .trim_end();
    let (name, linkage, signature, attrs) = parse_fn_header(rest, true)?;

    let mut blocks: Vec<Block> = Vec::new();
    let mut switch_counter = 0usize;

    while idx < lines.len() {
        let raw = lines[idx].trim();
        idx += 1;
        if raw.is_empty() || raw.starts_with(';') {
            continue;
        }
        if raw == "}" {
            break;
        }
        // Basic block label: `<name>:` (not containing whitespace or `=`).
        if let Some(label) = raw.strip_suffix(':')
            && !label.contains(' ')
            && !label.contains('=')
        {
            blocks.push(Block {
                name: label.to_owned(),
                instructions: Vec::new(),
            });
            continue;
        }

        // Handle multi-line `switch ... [ ... ]`.
        let stmt = if raw.starts_with("switch ") && !raw.ends_with(']') {
            let mut joined = raw.to_owned();
            while idx < lines.len() {
                let cont = lines[idx].trim();
                idx += 1;
                joined.push(' ');
                joined.push_str(cont);
                if cont.ends_with(']') {
                    break;
                }
            }
            joined
        } else {
            raw.to_owned()
        };

        if stmt.starts_with("switch ") {
            lower_switch(&stmt, &mut blocks, &mut switch_counter)?;
        } else {
            let inst = parse_instruction(&stmt)?;
            let bb = blocks.last_mut().ok_or_else(|| {
                StructuredBuildError::new(format!(
                    "instruction outside basic block in @{name}: {stmt}"
                ))
            })?;
            bb.instructions.push(inst);
        }
    }

    Ok((
        Function {
            name,
            linkage,
            signature,
            attrs,
            blocks: Some(blocks),
        },
        idx,
    ))
}

fn parse_fn_header(
    src: &str,
    has_param_names: bool,
) -> Result<(String, Linkage, Signature, Vec<Attr>), StructuredBuildError> {
    let mut cur = Cursor::new(src);
    let mut linkage = Linkage::External;
    let mut result_attrs = Vec::new();

    loop {
        cur.skip_ws();
        match cur.peek_word() {
            Some("linkonce_odr") => {
                cur.advance("linkonce_odr".len());
                linkage = Linkage::LinkOnceOdr;
            }
            Some("internal") => {
                cur.advance("internal".len());
                linkage = Linkage::Internal;
            }
            Some("private") => {
                cur.advance("private".len());
                linkage = Linkage::Private;
            }
            Some("available_externally") => {
                cur.advance("available_externally".len());
                linkage = Linkage::AvailableExternally;
            }
            Some("zeroext") => {
                cur.advance("zeroext".len());
                result_attrs.push(Attr::ZeroExt);
            }
            Some("signext") => {
                cur.advance("signext".len());
                result_attrs.push(Attr::SignExt);
            }
            _ => break,
        }
    }

    let result = cur.parse_type()?;
    cur.skip_ws();
    let name = cur.parse_global_ident()?;
    cur.skip_ws();
    cur.expect_char('(')?;

    let mut params = Vec::new();
    let mut variadic = false;
    cur.skip_ws();
    if !cur.eat_char(')') {
        loop {
            cur.skip_ws();
            if cur.eat_str("...") {
                variadic = true;
                cur.skip_ws();
                cur.expect_char(')')?;
                break;
            }
            let pty = cur.parse_type()?;
            let mut pattrs = Vec::new();
            loop {
                cur.skip_ws();
                match cur.peek_word() {
                    Some("zeroext") => {
                        cur.advance("zeroext".len());
                        pattrs.push(Attr::ZeroExt);
                    }
                    Some("signext") => {
                        cur.advance("signext".len());
                        pattrs.push(Attr::SignExt);
                    }
                    _ => break,
                }
            }
            cur.skip_ws();
            let pname = if has_param_names || cur.peek_char() == Some('%') {
                if cur.peek_char() == Some('%') {
                    Some(cur.parse_local_ident()?)
                } else {
                    None
                }
            } else {
                None
            };
            params.push(Param {
                ty: pty,
                name: pname,
                attrs: pattrs,
            });
            cur.skip_ws();
            if cur.eat_char(')') {
                break;
            }
            cur.expect_char(',')?;
        }
    }

    let mut fn_attrs = Vec::new();
    loop {
        cur.skip_ws();
        match cur.peek_word() {
            Some("nobuiltin") => {
                cur.advance("nobuiltin".len());
                fn_attrs.push(Attr::NoBuiltin);
            }
            Some("noreturn") => {
                cur.advance("noreturn".len());
                fn_attrs.push(Attr::NoReturn);
            }
            Some("noinline") => {
                cur.advance("noinline".len());
                fn_attrs.push(Attr::NoInline);
            }
            Some("nonlazybind") => {
                cur.advance("nonlazybind".len());
                fn_attrs.push(Attr::NonLazyBind);
            }
            None => break,
            Some(other) => {
                return Err(StructuredBuildError::new(format!(
                    "unsupported function attribute '{other}' in: {src}"
                )));
            }
        }
    }

    let sig = Signature {
        result,
        result_attrs,
        params,
        variadic,
    };
    Ok((name, linkage, sig, fn_attrs))
}

fn lower_switch(
    stmt: &str,
    blocks: &mut Vec<Block>,
    switch_counter: &mut usize,
) -> Result<(), StructuredBuildError> {
    let mut cur = Cursor::new(stmt);
    cur.expect_word("switch")?;
    let ty = cur.parse_type()?;
    let discr = cur.parse_val(&ty)?;
    cur.skip_ws();
    cur.expect_char(',')?;
    cur.expect_word("label")?;
    let default_bb = cur.parse_local_ident()?;
    cur.skip_ws();
    cur.expect_char('[')?;

    let mut arms: Vec<(Val, String)> = Vec::new();
    loop {
        cur.skip_ws();
        if cur.eat_char(']') {
            break;
        }
        let arm_ty = cur.parse_type()?;
        if arm_ty != ty {
            return Err(StructuredBuildError::new(format!(
                "switch arm type mismatch: expected {ty}, got {arm_ty}"
            )));
        }
        let arm_val = cur.parse_val(&arm_ty)?;
        cur.skip_ws();
        cur.expect_char(',')?;
        cur.expect_word("label")?;
        let target = cur.parse_local_ident()?;
        arms.push((arm_val, target));
    }

    if arms.is_empty() {
        let bb = blocks
            .last_mut()
            .ok_or_else(|| StructuredBuildError::new("switch outside basic block"))?;
        bb.instructions
            .push(Inst::new(None, Op::Branch(default_bb)));
        return Ok(());
    }

    let sw_id = *switch_counter;
    *switch_counter += 1;

    for (arm_idx, (arm_val, target)) in arms.iter().enumerate() {
        let cmp_name = format!("__sw_{sw_id}_{arm_idx}_cmp");
        let is_last = arm_idx + 1 == arms.len();
        let fallthrough = if is_last {
            default_bb.clone()
        } else {
            format!("__sw_{sw_id}_{}", arm_idx + 1)
        };

        let bb = blocks
            .last_mut()
            .ok_or_else(|| StructuredBuildError::new("switch outside basic block"))?;
        bb.instructions.push(Inst::new(
            Some(cmp_name.clone()),
            Op::Compare {
                predicate: "eq".into(),
                ty: ty.clone(),
                left: discr.clone(),
                right: arm_val.clone(),
            },
        ));
        bb.instructions.push(Inst::new(
            None,
            Op::CondBranch {
                condition: Val::Local(cmp_name),
                yes: target.clone(),
                no: fallthrough.clone(),
            },
        ));

        if !is_last {
            blocks.push(Block {
                name: fallthrough,
                instructions: Vec::new(),
            });
        }
    }

    Ok(())
}

fn parse_instruction(stmt: &str) -> Result<Inst, StructuredBuildError> {
    let mut cur = Cursor::new(stmt);
    cur.skip_ws();

    let result = if cur.peek_char() == Some('%') {
        let name = cur.parse_local_ident()?;
        cur.skip_ws();
        cur.expect_char('=')?;
        Some(name)
    } else {
        None
    };

    cur.skip_ws();
    let opcode = cur
        .take_word()
        .ok_or_else(|| StructuredBuildError::new(format!("missing opcode in: {stmt}")))?;

    let op = match opcode {
        "alloca" => {
            let ty = cur.parse_type()?;
            let align = parse_optional_align(&mut cur)?;
            Op::Alloca { ty, align }
        }
        "load" => {
            cur.skip_ws();
            if cur.eat_word("atomic") {
                let ty = cur.parse_type()?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("ptr")?;
                let ptr = cur.parse_val(&Ty::Ptr)?;
                let ordering = parse_ordering(&mut cur)?;
                let align = parse_optional_align(&mut cur)?;
                let align = if align == 0 {
                    natural_align(&ty)
                } else {
                    align
                };
                Op::AtomicLoad {
                    ty,
                    ptr,
                    align,
                    ordering,
                }
            } else {
                let ty = cur.parse_type()?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("ptr")?;
                let ptr = cur.parse_val(&Ty::Ptr)?;
                let align = parse_optional_align(&mut cur)?;
                Op::Load { ty, ptr, align }
            }
        }
        "store" => {
            cur.skip_ws();
            if cur.eat_word("atomic") {
                let ty = cur.parse_type()?;
                let val = cur.parse_val(&ty)?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("ptr")?;
                let ptr = cur.parse_val(&Ty::Ptr)?;
                let ordering = parse_ordering(&mut cur)?;
                let align = parse_optional_align(&mut cur)?;
                let align = if align == 0 {
                    natural_align(&ty)
                } else {
                    align
                };
                Op::AtomicStore {
                    arg: Arg::new(ty, val),
                    ptr,
                    align,
                    ordering,
                }
            } else {
                let ty = cur.parse_type()?;
                let val = cur.parse_val(&ty)?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("ptr")?;
                let ptr = cur.parse_val(&Ty::Ptr)?;
                let align = parse_optional_align(&mut cur)?;
                Op::Store {
                    arg: Arg::new(ty, val),
                    ptr,
                    align,
                }
            }
        }
        "atomicrmw" => {
            cur.skip_ws();
            let op_word = cur.take_word().ok_or_else(|| {
                StructuredBuildError::new(format!("missing atomicrmw op: {stmt}"))
            })?;
            let operation = match op_word {
                "xchg" => AtomicRmwOp::Exchange,
                "add" => AtomicRmwOp::Add,
                "sub" => AtomicRmwOp::Subtract,
                "and" => AtomicRmwOp::And,
                "or" => AtomicRmwOp::Or,
                "xor" => AtomicRmwOp::Xor,
                other => {
                    return Err(StructuredBuildError::new(format!(
                        "unsupported atomicrmw op '{other}': {stmt}"
                    )));
                }
            };
            cur.expect_word("ptr")?;
            let ptr = cur.parse_val(&Ty::Ptr)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let ty = cur.parse_type()?;
            let val = cur.parse_val(&ty)?;
            let ordering = parse_ordering(&mut cur)?;
            let align = parse_optional_align(&mut cur)?;
            let align = if align == 0 {
                natural_align(&ty)
            } else {
                align
            };
            Op::AtomicRmw {
                operation,
                arg: Arg::new(ty, val),
                ptr,
                align,
                ordering,
            }
        }
        "cmpxchg" => {
            cur.expect_word("ptr")?;
            let ptr = cur.parse_val(&Ty::Ptr)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let ty = cur.parse_type()?;
            let expected = cur.parse_val(&ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let ty2 = cur.parse_type()?;
            if ty2 != ty {
                return Err(StructuredBuildError::new(format!(
                    "cmpxchg type mismatch: {ty} != {ty2}"
                )));
            }
            let desired = cur.parse_val(&ty)?;
            let success = parse_ordering(&mut cur)?;
            let failure = parse_ordering(&mut cur)?;
            let align = parse_optional_align(&mut cur)?;
            let align = if align == 0 {
                natural_align(&ty)
            } else {
                align
            };
            Op::AtomicCompareExchange {
                result: Ty::Struct(vec![ty.clone(), Ty::Int(1)]),
                expected: Arg::new(ty, expected),
                desired,
                ptr,
                align,
                success,
                failure,
            }
        }
        "add" | "sub" | "mul" | "sdiv" | "udiv" | "srem" | "urem" | "and" | "or" | "xor"
        | "shl" | "lshr" | "ashr" | "fadd" | "fsub" | "fmul" | "fdiv" | "frem" => {
            let ty = cur.parse_type()?;
            let left = cur.parse_val(&ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let right = cur.parse_val(&ty)?;
            Op::Binary {
                opcode: opcode.to_owned(),
                ty,
                left,
                right,
            }
        }
        "fneg" => {
            let ty = cur.parse_type()?;
            let right = cur.parse_val(&ty)?;
            let left = match ty {
                Ty::F32 => Val::F32(0x8000_0000),
                Ty::F64 => Val::F64(0x8000_0000_0000_0000),
                _ => {
                    return Err(StructuredBuildError::new(format!(
                        "fneg on non-float type {ty}: {stmt}"
                    )));
                }
            };
            Op::Binary {
                opcode: "fsub".into(),
                ty,
                left,
                right,
            }
        }
        "icmp" | "fcmp" => {
            cur.skip_ws();
            let predicate = cur
                .take_word()
                .ok_or_else(|| StructuredBuildError::new(format!("missing predicate: {stmt}")))?
                .to_owned();
            let ty = cur.parse_type()?;
            let left = cur.parse_val(&ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let right = cur.parse_val(&ty)?;
            Op::Compare {
                predicate,
                ty,
                left,
                right,
            }
        }
        "zext" | "sext" | "trunc" | "ptrtoint" | "inttoptr" | "sitofp" | "uitofp" | "fptosi"
        | "fptoui" | "fpext" | "fptrunc" | "bitcast" => {
            let from_ty = cur.parse_type()?;
            let val = cur.parse_val(&from_ty)?;
            cur.expect_word("to")?;
            let to = cur.parse_type()?;
            Op::Cast {
                opcode: opcode.to_owned(),
                arg: Arg::new(from_ty, val),
                to,
            }
        }
        "getelementptr" => {
            cur.skip_ws();
            let _ = cur.eat_word("inbounds");
            let ty = cur.parse_type()?;
            cur.skip_ws();
            cur.expect_char(',')?;
            cur.expect_word("ptr")?;
            let ptr = cur.parse_val(&Ty::Ptr)?;
            let mut indices = Vec::new();
            while {
                cur.skip_ws();
                cur.eat_char(',')
            } {
                let ity = cur.parse_type()?;
                let ival = cur.parse_val(&ity)?;
                indices.push(Arg::new(ity, ival));
            }
            Op::Gep { ty, ptr, indices }
        }
        "extractvalue" => {
            let ty = cur.parse_type()?;
            let val = cur.parse_val(&ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let index = cur.parse_u32()?;
            Op::Extract {
                arg: Arg::new(ty, val),
                index,
            }
        }
        "insertvalue" => {
            let agg_ty = cur.parse_type()?;
            let agg = cur.parse_val(&agg_ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let field_ty = cur.parse_type()?;
            let field = cur.parse_val(&field_ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let index = cur.parse_u32()?;
            Op::Insert {
                arg: Arg::new(agg_ty, agg),
                field: Arg::new(field_ty, field),
                index,
            }
        }
        "select" => {
            cur.expect_word("i1")?;
            let condition = cur.parse_val(&Ty::Int(1))?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let yes_ty = cur.parse_type()?;
            let yes_val = cur.parse_val(&yes_ty)?;
            cur.skip_ws();
            cur.expect_char(',')?;
            let no_ty = cur.parse_type()?;
            let no_val = cur.parse_val(&no_ty)?;
            Op::Select {
                condition,
                yes: Arg::new(yes_ty, yes_val),
                no: Arg::new(no_ty, no_val),
            }
        }
        "freeze" => {
            let ty = cur.parse_type()?;
            let val = cur.parse_val(&ty)?;
            Op::Freeze(Arg::new(ty, val))
        }
        "phi" => {
            let ty = cur.parse_type()?;
            let mut incoming = Vec::new();
            loop {
                cur.skip_ws();
                cur.expect_char('[')?;
                let val = cur.parse_val(&ty)?;
                cur.skip_ws();
                cur.expect_char(',')?;
                let bb = cur.parse_local_ident()?;
                cur.skip_ws();
                cur.expect_char(']')?;
                incoming.push((val, bb));
                cur.skip_ws();
                if !cur.eat_char(',') {
                    break;
                }
            }
            Op::Phi { ty, incoming }
        }
        "call" => {
            let mut result_attrs = Vec::new();
            loop {
                cur.skip_ws();
                match cur.peek_word() {
                    Some("zeroext") => {
                        cur.advance("zeroext".len());
                        result_attrs.push(Attr::ZeroExt);
                    }
                    Some("signext") => {
                        cur.advance("signext".len());
                        result_attrs.push(Attr::SignExt);
                    }
                    _ => break,
                }
            }
            let call_result = cur.parse_type()?;
            cur.skip_ws();
            let vararg = if cur.eat_char('(') {
                let mut fixed = Vec::new();
                loop {
                    cur.skip_ws();
                    if cur.eat_str("...") {
                        cur.skip_ws();
                        cur.expect_char(')')?;
                        break;
                    }
                    fixed.push(cur.parse_type()?);
                    cur.skip_ws();
                    if cur.eat_char(')') {
                        break;
                    }
                    cur.expect_char(',')?;
                }
                Some(fixed)
            } else {
                None
            };
            let callee = cur.parse_val(&Ty::Ptr)?;
            cur.skip_ws();
            cur.expect_char('(')?;
            let mut args = Vec::new();
            cur.skip_ws();
            if !cur.eat_char(')') {
                loop {
                    let aty = cur.parse_type()?;
                    let mut aattrs = Vec::new();
                    loop {
                        cur.skip_ws();
                        match cur.peek_word() {
                            Some("zeroext") => {
                                cur.advance("zeroext".len());
                                aattrs.push(Attr::ZeroExt);
                            }
                            Some("signext") => {
                                cur.advance("signext".len());
                                aattrs.push(Attr::SignExt);
                            }
                            _ => break,
                        }
                    }
                    let aval = cur.parse_val(&aty)?;
                    args.push(Arg {
                        ty: aty,
                        value: aval,
                        attrs: aattrs,
                    });
                    cur.skip_ws();
                    if cur.eat_char(')') {
                        break;
                    }
                    cur.expect_char(',')?;
                }
            }
            Op::Call {
                result: call_result,
                result_attrs,
                callee,
                args,
                vararg,
            }
        }
        "br" => {
            cur.skip_ws();
            if cur.eat_word("label") {
                let target = cur.parse_local_ident()?;
                Op::Branch(target)
            } else {
                cur.expect_word("i1")?;
                let condition = cur.parse_val(&Ty::Int(1))?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("label")?;
                let yes = cur.parse_local_ident()?;
                cur.skip_ws();
                cur.expect_char(',')?;
                cur.expect_word("label")?;
                let no = cur.parse_local_ident()?;
                Op::CondBranch { condition, yes, no }
            }
        }
        "ret" => {
            cur.skip_ws();
            if cur.eat_word("void") {
                Op::Return(None)
            } else {
                let ty = cur.parse_type()?;
                let val = cur.parse_val(&ty)?;
                Op::Return(Some(Arg::new(ty, val)))
            }
        }
        "unreachable" => Op::Unreachable,
        other => {
            return Err(StructuredBuildError::new(format!(
                "unsupported instruction opcode '{other}': {stmt}"
            )));
        }
    };

    Ok(Inst::new(result, op))
}

fn natural_align(ty: &Ty) -> u32 {
    match ty {
        Ty::Int(bits) => (*bits / 8).max(1),
        Ty::F32 => 4,
        Ty::F64 | Ty::Ptr => 8,
        _ => 1,
    }
}

fn parse_optional_align(cur: &mut Cursor<'_>) -> Result<u32, StructuredBuildError> {
    cur.skip_ws();
    if cur.eat_char(',') {
        cur.skip_ws();
        if cur.eat_word("align") {
            return cur.parse_u32();
        }
    }
    Ok(0)
}

fn parse_ordering(cur: &mut Cursor<'_>) -> Result<AtomicOrdering, StructuredBuildError> {
    cur.skip_ws();
    let word = cur
        .take_word()
        .ok_or_else(|| StructuredBuildError::new("expected atomic ordering"))?;
    match word {
        "monotonic" => Ok(AtomicOrdering::Relaxed),
        "acquire" => Ok(AtomicOrdering::Acquire),
        "release" => Ok(AtomicOrdering::Release),
        "acq_rel" => Ok(AtomicOrdering::AcqRel),
        "seq_cst" => Ok(AtomicOrdering::SeqCst),
        other => Err(StructuredBuildError::new(format!(
            "unsupported atomic ordering '{other}'"
        ))),
    }
}

fn parse_type_str(s: &str) -> Result<Ty, StructuredBuildError> {
    let mut cur = Cursor::new(s);
    let ty = cur.parse_type()?;
    cur.skip_ws();
    if !cur.is_eof() {
        return Err(StructuredBuildError::new(format!(
            "trailing characters in type: '{s}'"
        )));
    }
    Ok(ty)
}

struct Cursor<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.s[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.s.as_bytes().get(self.pos) {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn eat_char(&mut self, ch: char) -> bool {
        if self.peek_char() == Some(ch) {
            self.pos += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect_char(&mut self, ch: char) -> Result<(), StructuredBuildError> {
        self.skip_ws();
        if self.eat_char(ch) {
            Ok(())
        } else {
            Err(StructuredBuildError::new(format!(
                "expected '{ch}', found '{}'",
                self.rest()
            )))
        }
    }

    fn eat_str(&mut self, lit: &str) -> bool {
        if self.rest().starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn peek_word(&self) -> Option<&'a str> {
        let rest = self.rest();
        let end = rest
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b'.'))
            .unwrap_or(rest.len());
        if end == 0 { None } else { Some(&rest[..end]) }
    }

    fn take_word(&mut self) -> Option<&'a str> {
        self.skip_ws();
        let w = self.peek_word()?;
        self.pos += w.len();
        Some(w)
    }

    fn eat_word(&mut self, expected: &str) -> bool {
        self.skip_ws();
        if self.peek_word() == Some(expected) {
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, expected: &str) -> Result<(), StructuredBuildError> {
        if self.eat_word(expected) {
            Ok(())
        } else {
            Err(StructuredBuildError::new(format!(
                "expected '{expected}', found '{}'",
                self.rest()
            )))
        }
    }

    fn parse_ident_after(&mut self, prefix: char) -> Result<String, StructuredBuildError> {
        self.skip_ws();
        if !self.eat_char(prefix) {
            return Err(StructuredBuildError::new(format!(
                "expected '{prefix}', found '{}'",
                self.rest()
            )));
        }
        let rest = self.rest();
        let end = rest
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || b"._$-".contains(&b)))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(StructuredBuildError::new(format!(
                "empty identifier after '{prefix}'"
            )));
        }
        let ident = rest[..end].to_owned();
        self.pos += end;
        Ok(ident)
    }

    fn parse_local_ident(&mut self) -> Result<String, StructuredBuildError> {
        self.parse_ident_after('%')
    }

    fn parse_global_ident(&mut self) -> Result<String, StructuredBuildError> {
        self.parse_ident_after('@')
    }

    fn parse_u32(&mut self) -> Result<u32, StructuredBuildError> {
        self.skip_ws();
        let rest = self.rest();
        let end = rest
            .bytes()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(StructuredBuildError::new(format!(
                "expected u32, found '{rest}'"
            )));
        }
        let v = rest[..end]
            .parse::<u32>()
            .map_err(|e| StructuredBuildError::new(format!("invalid u32: {e}")))?;
        self.pos += end;
        Ok(v)
    }

    fn parse_u64(&mut self) -> Result<u64, StructuredBuildError> {
        self.skip_ws();
        let rest = self.rest();
        let end = rest
            .bytes()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(StructuredBuildError::new(format!(
                "expected u64, found '{rest}'"
            )));
        }
        let v = rest[..end]
            .parse::<u64>()
            .map_err(|e| StructuredBuildError::new(format!("invalid u64: {e}")))?;
        self.pos += end;
        Ok(v)
    }

    fn parse_type(&mut self) -> Result<Ty, StructuredBuildError> {
        self.skip_ws();
        match self.peek_char() {
            Some('%') => Ok(Ty::Named(self.parse_local_ident()?)),
            Some('{') => {
                self.advance(1);
                let mut fields = Vec::new();
                self.skip_ws();
                if !self.eat_char('}') {
                    loop {
                        fields.push(self.parse_type()?);
                        self.skip_ws();
                        if self.eat_char('}') {
                            break;
                        }
                        self.expect_char(',')?;
                    }
                }
                Ok(Ty::Struct(fields))
            }
            Some('[') => {
                self.advance(1);
                let len = self.parse_u64()?;
                self.expect_word("x")?;
                let elem = self.parse_type()?;
                self.expect_char(']')?;
                Ok(Ty::array(len, elem))
            }
            _ => {
                let word = self.take_word().ok_or_else(|| {
                    StructuredBuildError::new(format!("expected type, found '{}'", self.rest()))
                })?;
                match word {
                    "void" => Ok(Ty::Void),
                    "ptr" => Ok(Ty::Ptr),
                    "float" => Ok(Ty::F32),
                    "double" => Ok(Ty::F64),
                    w if w.starts_with('i') => {
                        let bits = w[1..].parse::<u32>().map_err(|e| {
                            StructuredBuildError::new(format!("invalid int type '{w}': {e}"))
                        })?;
                        Ok(Ty::Int(bits))
                    }
                    other => Err(StructuredBuildError::new(format!(
                        "unsupported type '{other}'"
                    ))),
                }
            }
        }
    }

    fn parse_val(&mut self, ty: &Ty) -> Result<Val, StructuredBuildError> {
        self.skip_ws();
        match self.peek_char() {
            Some('%') => Ok(Val::Local(self.parse_local_ident()?)),
            Some('@') => Ok(Val::Global(self.parse_global_ident()?)),
            Some('[') => {
                self.advance(1);
                let mut elems = Vec::new();
                self.skip_ws();
                if !self.eat_char(']') {
                    loop {
                        let ety = self.parse_type()?;
                        let eval = self.parse_val(&ety)?;
                        elems.push(Arg::new(ety, eval));
                        self.skip_ws();
                        if self.eat_char(']') {
                            break;
                        }
                        self.expect_char(',')?;
                    }
                }
                Ok(Val::Array(elems))
            }
            Some('c') if self.rest().starts_with("c\"") => {
                self.advance(2);
                let bytes = self.parse_llvm_c_string()?;
                Ok(Val::Bytes(bytes))
            }
            _ => {
                let rest = self.rest();
                let end = rest
                    .bytes()
                    .position(|b| b.is_ascii_whitespace() || matches!(b, b',' | b')' | b']' | b'}'))
                    .unwrap_or(rest.len());
                if end == 0 {
                    return Err(StructuredBuildError::new(format!(
                        "expected value, found '{rest}'"
                    )));
                }
                let tok = &rest[..end];
                self.pos += end;
                match tok {
                    "null" => Ok(Val::Null),
                    "zeroinitializer" => Ok(Val::Zero),
                    "undef" => Ok(Val::Undef),
                    "poison" => Ok(Val::Poison),
                    "true" => Ok(Val::Bool(true)),
                    "false" => Ok(Val::Bool(false)),
                    t if t.starts_with("0x") || t.starts_with("0X") => {
                        let bits = u64::from_str_radix(&t[2..], 16).map_err(|e| {
                            StructuredBuildError::new(format!("invalid hex float '{t}': {e}"))
                        })?;
                        match ty {
                            Ty::F32 => Ok(Val::F32((f64::from_bits(bits) as f32).to_bits())),
                            Ty::F64 => Ok(Val::F64(bits)),
                            _ => Err(StructuredBuildError::new(format!(
                                "hex float '{t}' for non-float type {ty}"
                            ))),
                        }
                    }
                    t => match ty {
                        Ty::F32 => {
                            let f = t.parse::<f32>().map_err(|e| {
                                StructuredBuildError::new(format!("invalid f32 '{t}': {e}"))
                            })?;
                            Ok(Val::F32(f.to_bits()))
                        }
                        Ty::F64 => {
                            let f = t.parse::<f64>().map_err(|e| {
                                StructuredBuildError::new(format!("invalid f64 '{t}': {e}"))
                            })?;
                            Ok(Val::F64(f.to_bits()))
                        }
                        Ty::Int(_) => {
                            let v = t.parse::<i128>().map_err(|e| {
                                StructuredBuildError::new(format!("invalid int '{t}': {e}"))
                            })?;
                            Ok(Val::Int(v))
                        }
                        _ => Err(StructuredBuildError::new(format!(
                            "unexpected literal '{t}' for type {ty}"
                        ))),
                    },
                }
            }
        }
    }

    fn parse_llvm_c_string(&mut self) -> Result<Vec<u8>, StructuredBuildError> {
        let mut out = Vec::new();
        let bytes = self.s.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            self.pos += 1;
            if b == b'"' {
                return Ok(out);
            }
            if b == b'\\' {
                if self.pos + 2 > bytes.len() {
                    return Err(StructuredBuildError::new("truncated escape in c-string"));
                }
                let hex = std::str::from_utf8(&bytes[self.pos..self.pos + 2])
                    .map_err(|_| StructuredBuildError::new("non-utf8 escape in c-string"))?;
                let val = u8::from_str_radix(hex, 16).map_err(|e| {
                    StructuredBuildError::new(format!("invalid hex escape \\{hex}: {e}"))
                })?;
                self.pos += 2;
                out.push(val);
            } else {
                out.push(b);
            }
        }
        Err(StructuredBuildError::new("unterminated c-string"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_canonical_module_with_switch_tls_and_atomics() {
        let ir = r#"
; ZIL Body IR -> LLVM 23
target triple = "x86_64-unknown-linux-gnu"

declare ptr @calloc(i64, i64) nobuiltin
declare void @abort() noreturn
declare i32 @printf(ptr, ...)
@__zil_current_task = linkonce_odr thread_local global ptr null
%__zil_task_hdr = type { i32, ptr, [2 x i64] }
@__zil_fmt_i64 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1

define linkonce_odr i32 @check(i32 %x, ptr %p) {
entry:
  %v = load atomic i32, ptr %p seq_cst, align 4
  %old = atomicrmw add ptr %p, i32 1 seq_cst
  %pair = cmpxchg ptr %p, i32 %old, i32 %v seq_cst seq_cst
  %ok = extractvalue { i32, i1 } %pair, 1
  %sel = select i1 %ok, i32 %x, i32 %v
  switch i32 %sel, label %def [
    i32 0, label %z
    i32 1, label %o
  ]
z:
  ret i32 10
o:
  ret i32 20
def:
  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 42)
  ret i32 0
}
"#;
        let module = build_structured_module(ir, "x86_64-unknown-linux-gnu", "test-fp")
            .expect("structured module build should succeed");
        zil_llvm_api::validate_module(&module)
            .expect("structured module should pass zil-llvm-api preflight");

        #[cfg(feature = "llvm-api")]
        {
            let config = zil_llvm_api::Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: zil_llvm_api::Optimization::O2,
            };
            let text_err = zil_llvm_api::compile(ir, &config, &|| false)
                .expect_err("formal llvm-api backend must not link text-reference parser");
            assert_eq!(text_err.kind, zil_llvm_api::ErrorKind::Unavailable);

            let obj = zil_llvm_api::compile_module(&module, &config, &|| false)
                .expect("structured compile_module should emit ELF object");
            assert_eq!(obj.receipt.input_kind, "builder");
            assert_eq!(obj.receipt.parse_ns, 0);
            assert!(obj.receipt.preflight_ns > 0);
            assert!(obj.receipt.builder_ns > 0);
            assert!(!obj.bytes.is_empty());
        }
    }

    #[test]
    fn converts_process_args_globals_shims_and_main_wrapper() {
        let ir = r#"
; ZIL Verified Body IR -> LLVM 23 IR
target triple = "x86_64-unknown-linux-gnu"

@zil_process_argc = linkonce_odr global i32 0
@zil_process_argv = linkonce_odr global ptr null

define internal i64 @zil_process_arg_count() {
entry:
  %argv = load ptr, ptr @zil_process_argv
  %is_null = icmp eq ptr %argv, null
  br i1 %is_null, label %zero, label %check_argc
check_argc:
  %c = load i32, ptr @zil_process_argc
  %neg = icmp slt i32 %c, 0
  br i1 %neg, label %zero, label %ok
ok:
  %ext = sext i32 %c to i64
  ret i64 %ext
zero:
  ret i64 0
}

define internal ptr @zil_process_arg_ptr(i64 %idx) {
entry:
  %argv = load ptr, ptr @zil_process_argv
  %is_null = icmp eq ptr %argv, null
  br i1 %is_null, label %ret_null, label %check_bounds
check_bounds:
  %c = load i32, ptr @zil_process_argc
  %c64 = sext i32 %c to i64
  %neg = icmp slt i64 %idx, 0
  br i1 %neg, label %ret_null, label %check_upper
check_upper:
  %in_range = icmp slt i64 %idx, %c64
  br i1 %in_range, label %load_arg, label %ret_null
load_arg:
  %slot = getelementptr inbounds ptr, ptr %argv, i64 %idx
  %arg = load ptr, ptr %slot
  ret ptr %arg
ret_null:
  ret ptr null
}

define i32 @zil_entry() {
entry:
  %n = call i64 @zil_process_arg_count()
  %p = call ptr @zil_process_arg_ptr(i64 0)
  %ok = icmp ne ptr %p, null
  %r = select i1 %ok, i32 0, i32 1
  ret i32 %r
}

define i32 @main(i32 %argc, ptr %argv) {
entry:
  store i32 %argc, ptr @zil_process_argc
  store ptr %argv, ptr @zil_process_argv
  %r = call i32 @zil_entry()
  ret i32 %r
}
"#;
        let module = build_structured_module(ir, "x86_64-unknown-linux-gnu", "test-args-fp")
            .expect("structured module build should succeed for process args IR");
        zil_llvm_api::validate_module(&module)
            .expect("process args structured module should pass zil-llvm-api preflight");

        #[cfg(feature = "llvm-api")]
        {
            let config = zil_llvm_api::Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: zil_llvm_api::Optimization::O2,
            };
            let obj = zil_llvm_api::compile_module(&module, &config, &|| false)
                .expect("structured compile_module should emit ELF object");
            assert!(!obj.bytes.is_empty());
        }
    }
}
