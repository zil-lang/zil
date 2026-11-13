//! Pay for every machine node and byte before recursive Hash traverses it.
//! This is separate from instruction-count heuristics used for CGU placement.
use super::{bad, spend};
use zil_base::CompileResult;
use zil_llvm_api::model as m;

fn text(s: &str, left: &mut usize) -> CompileResult<()> {
    // One traversal here and one Hash traversal; length is checked before bytes.
    spend(left, s.len().saturating_mul(2).saturating_add(2))
}
fn attrs(a: &[m::Attr], left: &mut usize) -> CompileResult<()> {
    spend(left, a.len().saturating_mul(2).saturating_add(2))
}
fn ty(t: &m::Ty, left: &mut usize, depth: usize) -> CompileResult<()> {
    spend(left, 16)?;
    if depth > 128 {
        return Err(bad(
            "task codegen machine type depth exceeds supported bound",
        ));
    }
    match t {
        m::Ty::Void | m::Ty::Int(_) | m::Ty::F32 | m::Ty::F64 | m::Ty::Ptr => Ok(()),
        m::Ty::Named(s) => text(s, left),
        m::Ty::Array(_, t) => ty(t, left, depth + 1),
        m::Ty::Struct(ts) => {
            for t in ts {
                ty(t, left, depth + 1)?;
            }
            Ok(())
        }
    }
}
fn value(v: &m::Val, left: &mut usize, depth: usize) -> CompileResult<()> {
    spend(left, 16)?;
    if depth > 128 {
        return Err(bad(
            "task codegen machine value depth exceeds supported bound",
        ));
    }
    match v {
        m::Val::Local(s) | m::Val::Global(s) | m::Val::Invalid(s) => text(s, left),
        m::Val::Bytes(b) => spend(left, b.len().saturating_mul(2)),
        m::Val::Int(_)
        | m::Val::F32(_)
        | m::Val::F64(_)
        | m::Val::Bool(_)
        | m::Val::Null
        | m::Val::Zero
        | m::Val::Undef
        | m::Val::Poison => Ok(()),
        m::Val::Array(args) => {
            for a in args {
                arg(a, left, depth + 1)?;
            }
            Ok(())
        }
        m::Val::Gep {
            ty: t,
            ptr,
            indices,
        } => {
            ty(t, left, depth + 1)?;
            value(ptr, left, depth + 1)?;
            for a in indices {
                arg(a, left, depth + 1)?;
            }
            Ok(())
        }
    }
}
fn arg(a: &m::Arg, left: &mut usize, depth: usize) -> CompileResult<()> {
    let m::Arg {
        ty: t,
        value: v,
        attrs: a,
    } = a;
    spend(left, 8)?;
    ty(t, left, depth + 1)?;
    value(v, left, depth + 1)?;
    attrs(a, left)
}
fn op(o: &m::Op, left: &mut usize) -> CompileResult<()> {
    spend(left, 32)?; // discriminant plus all fixed scalar fields and their hash
    match o {
        m::Op::Alloca { ty: t, align: _ } => ty(t, left, 0),
        m::Op::Load {
            ty: t,
            ptr,
            align: _,
        }
        | m::Op::AtomicLoad {
            ty: t,
            ptr,
            align: _,
            ordering: _,
        } => {
            ty(t, left, 0)?;
            value(ptr, left, 0)
        }
        m::Op::Store {
            arg: a,
            ptr,
            align: _,
        }
        | m::Op::AtomicStore {
            arg: a,
            ptr,
            align: _,
            ordering: _,
        }
        | m::Op::AtomicRmw {
            operation: _,
            arg: a,
            ptr,
            align: _,
            ordering: _,
        } => {
            arg(a, left, 0)?;
            value(ptr, left, 0)
        }
        m::Op::AtomicCompareExchange {
            result,
            expected,
            desired,
            ptr,
            align: _,
            success: _,
            failure: _,
        } => {
            ty(result, left, 0)?;
            arg(expected, left, 0)?;
            value(desired, left, 0)?;
            value(ptr, left, 0)
        }
        m::Op::Binary {
            opcode: s,
            ty: t,
            left: l,
            right: r,
        }
        | m::Op::Compare {
            predicate: s,
            ty: t,
            left: l,
            right: r,
        } => {
            text(s, left)?;
            ty(t, left, 0)?;
            value(l, left, 0)?;
            value(r, left, 0)
        }
        m::Op::Cast { opcode, arg: a, to } => {
            text(opcode, left)?;
            arg(a, left, 0)?;
            ty(to, left, 0)
        }
        m::Op::Gep {
            ty: t,
            ptr,
            indices,
        } => {
            ty(t, left, 0)?;
            value(ptr, left, 0)?;
            for a in indices {
                arg(a, left, 0)?;
            }
            Ok(())
        }
        m::Op::Extract { arg: a, index: _ } | m::Op::Freeze(a) => arg(a, left, 0),
        m::Op::Insert {
            arg: a,
            field,
            index: _,
        } => {
            arg(a, left, 0)?;
            arg(field, left, 0)
        }
        m::Op::Call {
            result,
            result_attrs,
            callee,
            args,
            vararg,
        } => {
            ty(result, left, 0)?;
            attrs(result_attrs, left)?;
            value(callee, left, 0)?;
            for a in args {
                arg(a, left, 0)?;
            }
            spend(left, 2)?;
            for t in vararg.iter().flatten() {
                ty(t, left, 0)?;
            }
            Ok(())
        }
        m::Op::Select { ty: t, condition, yes, no } => {
            ty(t,left,0)?; value(condition,left,0)?; value(yes,left,0)?; value(no,left,0)
        }
        m::Op::Phi { ty: t, incoming } => {
            ty(t, left, 0)?;
            for (v, s) in incoming {
                value(v, left, 0)?;
                text(s, left)?;
            }
            Ok(())
        }
        m::Op::Branch(s) => text(s, left),
        m::Op::CondBranch { condition, yes, no } => {
            value(condition, left, 0)?;
            text(yes, left)?;
            text(no, left)
        }
        m::Op::Return(a) => {
            if let Some(a) = a {
                arg(a, left, 0)?;
            }
            Ok(())
        }
        m::Op::Unreachable => Ok(()),
    }
}
pub(super) fn prepay(unit: &m::Module, left: &mut usize) -> CompileResult<()> {
    let m::Module {
        triple,
        fingerprint,
        items,
    } = unit;
    spend(left, 16)?;
    text(triple, left)?;
    text(fingerprint, left)?;
    for item in items {
        spend(left, 16)?;
        match item {
            m::Item::NamedType(s, ts) => {
                text(s, left)?;
                for t in ts {
                    ty(t, left, 0)?;
                }
            }
            m::Item::Global(g) => {
                let m::Global {
                    name,
                    linkage: _,
                    ty: t,
                    initializer,
                    constant: _,
                    unnamed_addr: _,
                    alignment: _,
                } = g;
                text(name, left)?;
                ty(t, left, 0)?;
                if let Some(v) = initializer {
                    value(v, left, 0)?;
                }
            }
            m::Item::Function(f) => {
                let m::Function {
                    name,
                    linkage: _,
                    signature,
                    attrs: a,
                    blocks,
                } = f;
                text(name, left)?;
                attrs(a, left)?;
                let m::Signature {
                    result,
                    result_attrs,
                    params,
                    variadic: _,
                } = signature;
                ty(result, left, 0)?;
                attrs(result_attrs, left)?;
                for p in params {
                    let m::Param {
                        ty: t,
                        name,
                        attrs: a,
                    } = p;
                    spend(left, 8)?;
                    ty(t, left, 0)?;
                    attrs(a, left)?;
                    if let Some(s) = name {
                        text(s, left)?;
                    }
                }
                for b in blocks.iter().flatten() {
                    let m::Block { name, instructions } = b;
                    spend(left, 8)?;
                    text(name, left)?;
                    for i in instructions {
                        let m::Inst { result, op: o } = i;
                        spend(left, 8)?;
                        if let Some(s) = result {
                            text(s, left)?;
                        }
                        op(o, left)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Complete machine reservation reused by owning continuation cloning. The
/// caller supplies its actual remaining ledger and chooses the finite number
/// of subsequent traversals; no default allowance is renewed here.
pub(super) fn prepay_function(f: &m::Function, left: &mut usize) -> CompileResult<()> {
    let m::Function {
        name,
        linkage: _,
        signature,
        attrs: a,
        blocks,
    } = f;
    text(name, left)?;
    attrs(a, left)?;
    let m::Signature {
        result,
        result_attrs,
        params,
        variadic: _,
    } = signature;
    ty(result, left, 0)?;
    attrs(result_attrs, left)?;
    for p in params {
        let m::Param {
            ty: t,
            name,
            attrs: a,
        } = p;
        spend(left, 8)?;
        ty(t, left, 0)?;
        attrs(a, left)?;
        if let Some(s) = name {
            text(s, left)?;
        }
    }
    for b in blocks.iter().flatten() {
        let m::Block { name, instructions } = b;
        spend(left, 8)?;
        text(name, left)?;
        for i in instructions {
            let m::Inst { result, op: o } = i;
            spend(left, 8)?;
            if let Some(s) = result {
                text(s, left)?;
            }
            op(o, left)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;
    use zil_base::{FingerprintBuilder, ResultCategory};
    #[test]
    fn actual_machine_fields_have_paid_exact_identity() {
        let observed = m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(1)]);
        let make = |name: String| m::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "cas".into(),
            items: (vec![m::Item::Function(m::Function {
                name: "cas_identity".into(),
                linkage: m::Linkage::External,
                signature: m::Signature::new(
                    m::Ty::Int(32),
                    vec![
                        m::Param::new(m::Ty::Ptr, "cell"),
                        m::Param::new(m::Ty::Ptr, "next"),
                        m::Param::new(m::Ty::Int(32), name.clone()),
                    ],
                ),
                attrs: vec![],
                blocks: Some(
                    (vec![m::Block {
                        name: "entry".into(),
                        instructions: vec![
                            m::Inst::new(
                                Some("answer".into()),
                                m::Op::AtomicCompareExchange {
                                    result: observed.clone(),
                                    expected: m::Arg::new(m::Ty::Int(32), m::Val::Local(name)),
                                    desired: m::Val::Int(9),
                                    ptr: m::Val::Local("cell".into()),
                                    align: 4,
                                    success: m::AtomicOrdering::SeqCst,
                                    failure: m::AtomicOrdering::SeqCst,
                                },
                            ),
                            m::Inst::new(
                                Some("old".into()),
                                m::Op::Extract {
                                    arg: m::Arg::new(
                                        observed.clone(),
                                        m::Val::Local("answer".into()),
                                    ),
                                    index: 0,
                                },
                            ),
                            m::Inst::new(
                                None,
                                m::Op::Return(Some(m::Arg::new(
                                    m::Ty::Int(32),
                                    m::Val::Local("old".into()),
                                ))),
                            ),
                        ],
                    }])
                    .into(),
                ),
            })])
            .into(),
        };
        let paid = |module: &m::Module, n| {
            let mut left = n;
            prepay(module, &mut left)?;
            let mut hash = FingerprintBuilder::new();
            module.hash(&mut hash);
            Ok::<_, Vec<zil_base::Diagnostic>>((hash.digest(), left))
        };
        let mut growth = vec![];
        for size in [8, 4096] {
            let source = make("e".repeat(size));
            zil_llvm_api::validate_module(&source).unwrap();
            let mut left = usize::MAX;
            prepay(&source, &mut left).unwrap();
            let n = usize::MAX - left;
            let (identity, rest) = paid(&source, n).unwrap();
            assert_eq!(rest, 0);
            assert_eq!(paid(&source, n).unwrap().0, identity);
            assert_eq!(
                paid(&source, n - 1).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
            for field in 0..4 {
                let mut changed = source.clone();
                let m::Item::Function(f) = &mut changed.items[0] else {
                    unreachable!()
                };
                let m::Op::AtomicCompareExchange {
                    expected,
                    desired,
                    ptr,
                    success,
                    failure,
                    ..
                } = &mut f.blocks.as_mut().unwrap()[0].instructions[0].op
                else {
                    unreachable!()
                };
                match field {
                    0 => *failure = m::AtomicOrdering::Acquire,
                    1 => {
                        *success = m::AtomicOrdering::AcqRel;
                        *failure = m::AtomicOrdering::Acquire;
                    }
                    2 => *desired = m::Val::Int(10),
                    3 => {
                        *ptr = m::Val::Local("next".into());
                        expected.value = m::Val::Int(0);
                    }
                    _ => unreachable!(),
                }
                zil_llvm_api::validate_module(&changed).unwrap();
                assert_ne!(paid(&changed, n).unwrap().0, identity);
            }
            growth.push(n);
            eprintln!("cas machine identity operand_bytes={size} Codegen N={n}");
        }
        assert!(growth[1] > growth[0] + 4096);
    }
    fn unit(bytes: usize, nested: usize, symbols: usize) -> m::Module {
        let mut t = m::Ty::Int(8);
        for _ in 0..nested {
            t = m::Ty::Array(2, Box::new(t));
        }
        m::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "input".into(),
            items: (vec![
                m::Item::NamedType("long_type".repeat(symbols), vec![t.clone()]),
                m::Item::Global(m::Global {
                    name: "long_global".repeat(symbols),
                    linkage: m::Linkage::Private,
                    ty: m::Ty::array(bytes as u64, m::Ty::Int(8)),
                    initializer: Some(m::Val::Bytes(vec![42; bytes])),
                    constant: true,
                    unnamed_addr: true,
                    alignment: 1,
                }),
                m::Item::Function(m::Function {
                    name: "long_function".repeat(symbols),
                    linkage: m::Linkage::External,
                    signature: m::Signature::new(
                        t.clone(),
                        vec![m::Param::new(t.clone(), "long_parameter".repeat(symbols))],
                    ),
                    attrs: vec![],
                    blocks: Some(
                        (vec![m::Block {
                            name: "entry".into(),
                            instructions: vec![m::Inst::new(
                                None,
                                m::Op::Return(Some(m::Arg::new(
                                    t,
                                    m::Val::Local("long_parameter".repeat(symbols)),
                                ))),
                            )],
                        }])
                        .into(),
                    ),
                }),
            ])
            .into(),
        }
    }
    #[test]
    fn machine_identity_pays_bytes_types_and_long_symbols_before_hash() {
        let small = unit(1, 0, 1);
        let large = unit(65536, 24, 1024);
        let cost = |m: &m::Module| {
            let mut n = usize::MAX;
            prepay(m, &mut n).unwrap();
            usize::MAX - n
        };
        let n = cost(&large);
        assert!(cost(&unit(65536, 0, 1)) >= cost(&small) + 2 * 65535);
        assert!(cost(&unit(1, 24, 1)) >= cost(&small) + 24 * 16 * 4);
        assert!(cost(&unit(1, 0, 1024)) >= cost(&small) + 1023 * 32);
        assert!(n > cost(&small) + 131072 + 24 * 16 + 1024 * 32);
        let hash = |budget| {
            let mut left = budget;
            prepay(&large, &mut left)?;
            let mut h = FingerprintBuilder::new();
            large.hash(&mut h);
            Ok::<_, Vec<zil_base::Diagnostic>>((h.digest(), left))
        };
        let (fingerprint, left) = hash(n).unwrap();
        assert_eq!(left, 0);
        assert_eq!(
            hash(n - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        assert_eq!(hash(n).unwrap().0, fingerprint);
        // Floating constants are fixed-size raw-bit atoms, including NaN payloads.
        // Their charge is bounded, but their complete bits still change identity.
        let mut floating = small.clone();
        for (ty, value) in [
            (m::Ty::F32, m::Val::F32(0x7fc0_0001)),
            (m::Ty::F64, m::Val::F64(0x7ff8_0000_0000_0001)),
        ] {
            floating.items.push(m::Item::Global(m::Global {
                name: "float_constant".into(),
                linkage: m::Linkage::Private,
                ty,
                initializer: Some(value),
                constant: true,
                unnamed_addr: true,
                alignment: 8,
            }));
        }
        let float_n = cost(&floating);
        let paid_hash = |module: &m::Module, budget| {
            let mut left = budget;
            prepay(module, &mut left)?;
            let mut h = FingerprintBuilder::new();
            module.hash(&mut h);
            Ok::<_, Vec<zil_base::Diagnostic>>((h.digest(), left))
        };
        let before = paid_hash(&floating, float_n).unwrap();
        assert_eq!(before.1, 0);
        assert_eq!(paid_hash(&floating, float_n).unwrap(), before);
        assert_eq!(
            paid_hash(&floating, float_n - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        let Some(m::Item::Global(global)) = floating.items.last_mut() else {
            panic!("actual floating initializer")
        };
        global.initializer = Some(m::Val::F64(0x7ff8_0000_0000_0002));
        assert_eq!(cost(&floating), float_n);
        assert_ne!(paid_hash(&floating, float_n).unwrap().0, before.0);
        eprintln!("actual machine identity N={n}");
    }
}
