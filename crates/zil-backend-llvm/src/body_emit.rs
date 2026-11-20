//! LLVM 23 opaque-pointer IR emitter directly consuming `zil_check::VerifiedModule`.
//!
//! Only a `VerifiedModule` constructed by `zil_check::check_module` can enter
//! this emitter. Every `InstructionKind`, `Rvalue`, `Terminator`, and `TyKind`
//! is matched exhaustively without wildcard arms.

#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt::Write as _;
use zil_check::VerifiedModule;
use zil_ir::body::{
    AdtKind, AggregateKind, BinOp, Block, Body, Callee, ClosureCallMode, ConstValue, FieldRef,
    FloatTy, FnId, FnRole, Function, InstructionKind, IntTy, IntrinsicOp, Local, Module, OpaqueTy,
    Operand, Place, PlaceRoot, Projection, Rvalue, ScopeExit, Terminator, TrapKind, TyId, TyKind,
    TyTable, UnOp,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyEmitError {
    pub message: String,
}

impl BodyEmitError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// Emits LLVM 23 opaque-pointer IR for a verified Body IR module.
///
/// When `emit_c_main` is true and a zero-argument `main` function exists, a C
/// `main()` entry wrapper is emitted that invokes it and returns its `i32`
/// result (or 0 for unit).
pub fn emit_verified_body(
    verified: &VerifiedModule,
    emit_c_main: bool,
) -> Result<String, BodyEmitError> {
    let module = verified.module();
    let mut out = String::with_capacity(8192);
    out.push_str("; ZIL Verified Body IR -> LLVM 23 IR\n");
    out.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
    out.push_str("declare ptr @calloc(i64, i64)\n");
    out.push_str("declare ptr @realloc(ptr, i64)\n");
    out.push_str("declare void @free(ptr)\n");
    out.push_str("declare void @abort() noreturn\n\n");

    // Emit static UTF-8 byte globals.
    for (idx, bytes) in module.statics.iter().enumerate() {
        let escaped = escape_llvm_bytes(bytes);
        let _ = writeln!(
            out,
            "@__zil_str_{idx} = private unnamed_addr constant [{} x i8] c\"{escaped}\", align 1",
            bytes.len()
        );
    }
    if !module.statics.is_empty() {
        out.push('\n');
    }

    // Emit foreign declarations.
    for f in &module.foreigns {
        let ret = llvm_ty(&module.tys, f.result)?;
        let mut params = Vec::with_capacity(f.params.len());
        for &p in &f.params {
            let t = llvm_ty(&module.tys, p)?;
            if t != "void" {
                params.push(t);
            }
        }
        let _ = writeln!(
            out,
            "declare {ret} @{}({})",
            sanitize_symbol(&f.symbol),
            params.join(", ")
        );
    }
    if !module.foreigns.is_empty() {
        out.push('\n');
    }

    // Emit all functions.
    let mut main_fn: Option<(FnId, String, TyId)> = None;
    for (idx, func) in module.functions.iter().enumerate() {
        let fn_id = FnId::from_index(idx);
        let sym = fn_symbol(fn_id, func);
        if (func.name == "main" || func.name.ends_with(".main"))
            && func.sig.params.is_empty()
            && func.sig.role == FnRole::Normal
        {
            main_fn = Some((fn_id, sym.clone(), func.sig.result));
        }
        emit_function(module, fn_id, func, &sym, &mut out)?;
        out.push('\n');
    }

    if emit_c_main && let Some((_, sym, res_ty)) = main_fn {
        let ret = llvm_ty(&module.tys, res_ty)?;
        out.push_str("define i32 @main() {\nentry:\n");
        if ret == "void" {
            let _ = writeln!(out, "  call void @{sym}()");
            out.push_str("  ret i32 0\n");
        } else if ret == "i32" {
            let _ = writeln!(out, "  %r = call i32 @{sym}()");
            out.push_str("  ret i32 %r\n");
        } else if ret == "i64" {
            let _ = writeln!(out, "  %r = call i64 @{sym}()");
            out.push_str("  %t = trunc i64 %r to i32\n  ret i32 %t\n");
        } else if ret == "i1" {
            let _ = writeln!(out, "  %r = call i1 @{sym}()");
            out.push_str("  %t = zext i1 %r to i32\n  ret i32 %t\n");
        } else {
            let _ = writeln!(out, "  %r = call {ret} @{sym}()");
            out.push_str("  ret i32 0\n");
        }
        out.push_str("}\n");
    }

    Ok(out)
}

fn fn_symbol(id: FnId, f: &Function) -> String {
    format!("__zil_fn_{}_{}", id.0, sanitize_symbol(&f.name))
}

fn sanitize_symbol(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn escape_llvm_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        let _ = write!(out, "\\{b:02X}");
    }
    out
}

fn llvm_int_ty(i: IntTy) -> &'static str {
    match i {
        IntTy::I8 | IntTy::U8 => "i8",
        IntTy::I16 | IntTy::U16 => "i16",
        IntTy::I32 | IntTy::U32 => "i32",
        IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => "i64",
    }
}

fn llvm_ty(tys: &TyTable, id: TyId) -> Result<String, BodyEmitError> {
    let kind = tys
        .get(id)
        .ok_or_else(|| BodyEmitError::new(format!("missing type {id:?}")))?;
    Ok(match kind {
        TyKind::Unit | TyKind::Never => "void".into(),
        TyKind::Bool => "i1".into(),
        TyKind::Int(i) => llvm_int_ty(*i).into(),
        TyKind::Float(FloatTy::F32) => "float".into(),
        TyKind::Float(FloatTy::F64) => "double".into(),
        TyKind::Ref { pointee, .. } => {
            if matches!(tys.get(*pointee), Some(TyKind::Slice(_))) {
                "{ ptr, i64 }".into()
            } else {
                "ptr".into()
            }
        }
        TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::FnPtr { .. }
        | TyKind::Dyn(_)
        | TyKind::OwnedDyn(_)
        | TyKind::Opaque(_) => "ptr".into(),
        TyKind::Slice(_) => "{ ptr, i64 }".into(),
        TyKind::Array { element, length } => {
            let elem = llvm_storage_ty(tys, *element)?;
            format!("[{length} x {elem}]")
        }
        TyKind::Tuple(items) => {
            if items.is_empty() {
                "void".into()
            } else {
                let mut fields = Vec::with_capacity(items.len());
                for &item in items {
                    fields.push(llvm_storage_ty(tys, item)?);
                }
                format!("{{ {} }}", fields.join(", "))
            }
        }
        TyKind::ErrorUnion { ok, err } => {
            let ok_t = llvm_storage_ty(tys, *ok)?;
            let err_t = llvm_storage_ty(tys, *err)?;
            format!("{{ i1, {ok_t}, {err_t} }}")
        }
        TyKind::Adt { adt, args } => {
            let def = tys
                .get_adt(*adt)
                .ok_or_else(|| BodyEmitError::new(format!("missing ADT {adt:?}")))?;
            match def.kind {
                AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                    let fields = def
                        .variants
                        .first()
                        .map(|v| &v.fields)
                        .ok_or_else(|| BodyEmitError::new("ADT has no variant"))?;
                    if fields.is_empty() {
                        "{ i8 }".into()
                    } else {
                        let mut parts = Vec::with_capacity(fields.len());
                        for f in fields {
                            let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                            parts.push(llvm_storage_ty(tys, fty)?);
                        }
                        format!("{{ {} }}", parts.join(", "))
                    }
                }
                AdtKind::Enum => {
                    // Tagged layout: `{ i32, <v0>, <v1>, ... }` so every variant
                    // payload has a statically typed GEP slot without bitcast UB.
                    let mut parts = vec!["i32".to_string()];
                    for v in &def.variants {
                        if v.fields.is_empty() {
                            parts.push("i8".into());
                        } else {
                            let mut vfields = Vec::with_capacity(v.fields.len());
                            for f in &v.fields {
                                let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                                vfields.push(llvm_storage_ty(tys, fty)?);
                            }
                            parts.push(format!("{{ {} }}", vfields.join(", ")));
                        }
                    }
                    format!("{{ {} }}", parts.join(", "))
                }
            }
        }
        TyKind::Closure { def, args } => {
            let cdef = tys
                .closure(*def)
                .ok_or_else(|| BodyEmitError::new(format!("missing closure {def:?}")))?;
            if cdef.captures.is_empty() {
                "{ i8 }".into()
            } else {
                let mut parts = Vec::with_capacity(cdef.captures.len());
                for &cap in &cdef.captures {
                    let cty = tys.subst_lookup(cap, *args).unwrap_or(cap);
                    parts.push(llvm_storage_ty(tys, cty)?);
                }
                format!("{{ {} }}", parts.join(", "))
            }
        }
        TyKind::Param(_) => {
            return Err(BodyEmitError::new(
                "unmonomorphized generic type parameter in body IR",
            ));
        }
    })
}

fn llvm_storage_ty(tys: &TyTable, id: TyId) -> Result<String, BodyEmitError> {
    let t = llvm_ty(tys, id)?;
    if t == "void" { Ok("i8".into()) } else { Ok(t) }
}

struct FnEmitter<'a> {
    module: &'a Module,
    body: &'a Body,
    out: &'a mut String,
    tmp: u32,
}

impl<'a> FnEmitter<'a> {
    fn fresh(&mut self) -> String {
        let id = self.tmp;
        self.tmp += 1;
        format!("%t{id}")
    }

    fn tys(&self) -> &'a TyTable {
        &self.module.tys
    }

    fn local_ty(&self, local: Local) -> Result<TyId, BodyEmitError> {
        self.body
            .locals
            .get(local.index())
            .map(|l| l.ty)
            .ok_or_else(|| BodyEmitError::new(format!("unknown local {local:?}")))
    }

    fn place_ty(&self, place: &Place) -> Result<TyId, BodyEmitError> {
        let mut ty = match place.root {
            PlaceRoot::Local(l) => self.local_ty(l)?,
            PlaceRoot::Capture(i) => *self
                .body
                .captures
                .get(i as usize)
                .ok_or_else(|| BodyEmitError::new(format!("unknown capture {i}")))?,
        };
        for step in &place.projections {
            ty = self.step_ty(ty, *step)?;
        }
        Ok(ty)
    }

    fn step_ty(&self, base: TyId, step: Projection) -> Result<TyId, BodyEmitError> {
        let tys = self.tys();
        match step {
            Projection::Field(FieldRef {
                adt,
                variant,
                field,
                args,
            }) => tys
                .field_ty(adt, variant, field, args)
                .ok_or_else(|| BodyEmitError::new("missing field type")),
            Projection::TupleField(i) => match tys.get(base) {
                Some(TyKind::Tuple(items)) => items
                    .get(i as usize)
                    .copied()
                    .ok_or_else(|| BodyEmitError::new("tuple field out of range")),
                _ => Err(BodyEmitError::new("tuple field on non-tuple")),
            },
            Projection::ErrorArm(ok) => match tys.get(base) {
                Some(TyKind::ErrorUnion { ok: o, err: e }) => Ok(if ok { *o } else { *e }),
                _ => Err(BodyEmitError::new("error arm on non-error-union")),
            },
            Projection::Index(_) | Projection::ConstIndex(_) => match tys.get(base) {
                Some(TyKind::Array { element, .. } | TyKind::Slice(element)) => Ok(*element),
                _ => Err(BodyEmitError::new("index on non-array/slice")),
            },
            Projection::Deref => match tys.get(base) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => Ok(*pointee),
                _ => Err(BodyEmitError::new("deref on non-pointer")),
            },
        }
    }

    /// Emits a pointer to the storage of `place` and returns `(ptr_ssa, place_ty_id)`.
    fn emit_place_ptr(&mut self, place: &Place) -> Result<(String, TyId), BodyEmitError> {
        let mut ty = match place.root {
            PlaceRoot::Local(l) => self.local_ty(l)?,
            PlaceRoot::Capture(i) => *self
                .body
                .captures
                .get(i as usize)
                .ok_or_else(|| BodyEmitError::new(format!("unknown capture {i}")))?,
        };
        let mut ptr = match place.root {
            PlaceRoot::Local(l) => format!("%l{}", l.0),
            PlaceRoot::Capture(i) => {
                let Some(cid) = self.body.closure else {
                    return Err(BodyEmitError::new("capture root outside closure"));
                };
                let cdef = self
                    .tys()
                    .closure(cid)
                    .ok_or_else(|| BodyEmitError::new("missing closure def"))?;
                let mut fields = Vec::with_capacity(cdef.captures.len());
                for &ct in &cdef.captures {
                    fields.push(llvm_storage_ty(self.tys(), ct)?);
                }
                let env_ty = if fields.is_empty() {
                    "{ i8 }".to_string()
                } else {
                    format!("{{ {} }}", fields.join(", "))
                };
                let gep = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {gep} = getelementptr inbounds {env_ty}, ptr %__env, i32 0, i32 {i}"
                );
                gep
            }
        };

        for step in &place.projections {
            let base_llvm = llvm_storage_ty(self.tys(), ty)?;
            match *step {
                Projection::Deref => {
                    let is_slice_ref = matches!(
                        self.tys().get(ty),
                        Some(TyKind::Ref { pointee, .. }) if matches!(self.tys().get(*pointee), Some(TyKind::Slice(_)))
                    );
                    if is_slice_ref {
                        // A slice ref is `{ ptr, i64 }` stored at `ptr`; deref stays at the fat ptr slot.
                        ty = self.step_ty(ty, *step)?;
                    } else {
                        let loaded = self.fresh();
                        let _ = writeln!(self.out, "  {loaded} = load ptr, ptr {ptr}");
                        ptr = loaded;
                        ty = self.step_ty(ty, *step)?;
                    }
                }
                Projection::Field(FieldRef {
                    adt,
                    variant,
                    field,
                    ..
                }) => {
                    let def = self
                        .tys()
                        .get_adt(adt)
                        .ok_or_else(|| BodyEmitError::new("missing ADT"))?;
                    match def.kind {
                        AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {}",
                                field.0
                            );
                            ptr = gep;
                        }
                        AdtKind::Enum => {
                            let v_slot = variant.0 + 1;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {v_slot}, i32 {}",
                                field.0
                            );
                            ptr = gep;
                        }
                    }
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::TupleField(i) => {
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    ptr = gep;
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::ErrorArm(ok) => {
                    let slot = if ok { 1 } else { 2 };
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {slot}"
                    );
                    ptr = gep;
                    ty = self.step_ty(ty, *step)?;
                }
                Projection::ConstIndex(idx) => {
                    let elem_ty = self.step_ty(ty, *step)?;
                    let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                    if matches!(self.tys().get(ty), Some(TyKind::Slice(_))) {
                        let data_ptr_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_ptr_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {ptr}, i32 0, i32 0"
                        );
                        let data_ptr = self.fresh();
                        let _ = writeln!(self.out, "  {data_ptr} = load ptr, ptr {data_ptr_slot}");
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data_ptr}, i64 {idx}"
                        );
                        ptr = gep;
                    } else {
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i64 0, i64 {idx}"
                        );
                        ptr = gep;
                    }
                    ty = elem_ty;
                }
                Projection::Index(idx_local) => {
                    let elem_ty = self.step_ty(ty, *step)?;
                    let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                    let idx_val = self.fresh();
                    let _ = writeln!(self.out, "  {idx_val} = load i64, ptr %l{}", idx_local.0);
                    if matches!(self.tys().get(ty), Some(TyKind::Slice(_))) {
                        let data_ptr_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_ptr_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {ptr}, i32 0, i32 0"
                        );
                        let data_ptr = self.fresh();
                        let _ = writeln!(self.out, "  {data_ptr} = load ptr, ptr {data_ptr_slot}");
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data_ptr}, i64 {idx_val}"
                        );
                        ptr = gep;
                    } else {
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i64 0, i64 {idx_val}"
                        );
                        ptr = gep;
                    }
                    ty = elem_ty;
                }
            }
        }
        Ok((ptr, ty))
    }

    fn emit_operand(
        &mut self,
        op: &Operand,
    ) -> Result<(String, String, Option<TyId>), BodyEmitError> {
        match op {
            Operand::Copy(p) | Operand::Move(p) => {
                let (ptr, ty) = self.emit_place_ptr(p)?;
                let lt = llvm_ty(self.tys(), ty)?;
                if lt == "void" {
                    Ok(("0".into(), "void".into(), Some(ty)))
                } else {
                    let val = self.fresh();
                    let _ = writeln!(self.out, "  {val} = load {lt}, ptr {ptr}");
                    Ok((val, lt, Some(ty)))
                }
            }
            Operand::Const(c) => match c {
                ConstValue::Unit => Ok(("0".into(), "void".into(), None)),
                ConstValue::Bool(b) => Ok((if *b { "1" } else { "0" }.into(), "i1".into(), None)),
                ConstValue::Int { ty, bits } => {
                    let lt = llvm_int_ty(*ty);
                    let val = match ty {
                        IntTy::I8 => (*bits as i8).to_string(),
                        IntTy::I16 => (*bits as i16).to_string(),
                        IntTy::I32 => (*bits as i32).to_string(),
                        IntTy::I64 | IntTy::Isize => (*bits as i64).to_string(),
                        IntTy::U8 => (*bits as u8).to_string(),
                        IntTy::U16 => (*bits as u16).to_string(),
                        IntTy::U32 => (*bits as u32).to_string(),
                        IntTy::U64 | IntTy::Usize => (*bits as u64).to_string(),
                    };
                    Ok((val, lt.into(), None))
                }
                ConstValue::Float { ty, bits } => match ty {
                    FloatTy::F32 => {
                        let f = f32::from_bits(*bits as u32) as f64;
                        let hex = f.to_bits();
                        Ok((format!("0x{hex:016X}"), "float".into(), None))
                    }
                    FloatTy::F64 => Ok((format!("0x{bits:016X}"), "double".into(), None)),
                },
            },
            Operand::Pooled(_) => Err(BodyEmitError::new("pooled constants not yet emitted")),
        }
    }

    fn emit_instruction(&mut self, kind: &InstructionKind) -> Result<(), BodyEmitError> {
        match kind {
            InstructionKind::Assign { dest, value } => self.emit_assign(dest, value),
            InstructionKind::Drop { place } => {
                let (ptr, ty) = self.emit_place_ptr(place)?;
                self.emit_drop_ptr(&ptr, ty)
            }
            InstructionKind::Replace { place, value, old } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let lt = llvm_ty(self.tys(), p_ty)?;
                let (new_val, _, _) = self.emit_operand(value)?;
                if lt != "void" {
                    let old_val = self.fresh();
                    let _ = writeln!(self.out, "  {old_val} = load {lt}, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store {lt} {old_val}, ptr %l{}", old.0);
                    let _ = writeln!(self.out, "  store {lt} {new_val}, ptr {p_ptr}");
                }
                Ok(())
            }
            InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter(_)
            | InstructionKind::ScopeExit {
                scope: _,
                exit:
                    ScopeExit::Fallthrough
                    | ScopeExit::Return
                    | ScopeExit::Error
                    | ScopeExit::Break
                    | ScopeExit::Continue,
            }
            | InstructionKind::RegisterDefer { .. } => Ok(()),
        }
    }

    fn emit_drop_ptr(&mut self, ptr: &str, ty: TyId) -> Result<(), BodyEmitError> {
        let Some(kind) = self.tys().get(ty).cloned() else {
            return Ok(());
        };
        match kind {
            TyKind::Managed(inner) => {
                let handle = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {ptr}");
                match self.tys().get(inner).cloned() {
                    Some(TyKind::Adt { adt, .. }) => {
                        if let Some(drop_fn_id) = self.module.drop_fn(adt) {
                            let sym =
                                fn_symbol(drop_fn_id, &self.module.functions[drop_fn_id.index()]);
                            let _ = writeln!(self.out, "  call void @{sym}(ptr {handle})");
                        }
                    }
                    Some(TyKind::Opaque(OpaqueTy::ByteBuffer)) => {
                        let data_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 0"
                        );
                        let data = self.fresh();
                        let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                        let _ = writeln!(self.out, "  call void @free(ptr {data})");
                    }
                    Some(TyKind::Opaque(OpaqueTy::OwnedStore { element: elem })) => {
                        let data_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 0"
                        );
                        let data = self.fresh();
                        let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                        let len_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {handle}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                        let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                        let idx_slot = self.fresh();
                        let _ = writeln!(self.out, "  {idx_slot} = alloca i64");
                        let _ = writeln!(self.out, "  store i64 {len}, ptr {idx_slot}");
                        let head = self.fresh();
                        let body_bb = self.fresh();
                        let done = self.fresh();
                        let (hl, bl, dl) = (&head[1..], &body_bb[1..], &done[1..]);
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{hl}:");
                        let cur = self.fresh();
                        let _ = writeln!(self.out, "  {cur} = load i64, ptr {idx_slot}");
                        let cond = self.fresh();
                        let _ = writeln!(self.out, "  {cond} = icmp sgt i64 {cur}, 0");
                        let _ = writeln!(self.out, "  br i1 {cond}, label %{bl}, label %{dl}");
                        let _ = writeln!(self.out, "{bl}:");
                        let next = self.fresh();
                        let _ = writeln!(self.out, "  {next} = sub i64 {cur}, 1");
                        let _ = writeln!(self.out, "  store i64 {next}, ptr {idx_slot}");
                        let elem_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {elem_ptr} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {next}"
                        );
                        self.emit_drop_ptr(&elem_ptr, elem)?;
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{dl}:");
                        let _ = writeln!(self.out, "  call void @free(ptr {data})");
                    }
                    Some(TyKind::Opaque(OpaqueTy::Mutex { payload: elem })) => {
                        self.emit_drop_ptr(&handle, elem)?;
                    }
                    Some(
                        TyKind::Unit
                        | TyKind::Bool
                        | TyKind::Never
                        | TyKind::Int(_)
                        | TyKind::Float(_)
                        | TyKind::Tuple(_)
                        | TyKind::Array { .. }
                        | TyKind::Slice(_)
                        | TyKind::Ref { .. }
                        | TyKind::Managed(_)
                        | TyKind::Desc(_)
                        | TyKind::ErrorUnion { .. }
                        | TyKind::FnPtr { .. }
                        | TyKind::Closure { .. }
                        | TyKind::Dyn(_)
                        | TyKind::OwnedDyn(_)
                        | TyKind::Param(_)
                        | TyKind::Opaque(
                            OpaqueTy::MutexGuard { .. }
                            | OpaqueTy::SyncCell { .. }
                            | OpaqueTy::Task { .. }
                            | OpaqueTy::Registration { .. }
                            | OpaqueTy::AsyncRegistration { .. }
                            | OpaqueTy::ForeignHandle
                            | OpaqueTy::CWideString
                            | OpaqueTy::ForeignCallback { .. },
                        ),
                    )
                    | None => {}
                }
                let _ = writeln!(self.out, "  call void @free(ptr {handle})");
                Ok(())
            }
            TyKind::Adt { adt, args } => {
                let Some(def) = self.tys().get_adt(adt) else {
                    return Ok(());
                };
                match def.kind {
                    AdtKind::Package => {
                        let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                        if let Some(v) = def.variants.first() {
                            for (fi, f) in v.fields.iter().enumerate().rev() {
                                let fty = self.tys().subst_lookup(f.ty, args).unwrap_or(f.ty);
                                let gep = self.fresh();
                                let _ = writeln!(
                                    self.out,
                                    "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {fi}"
                                );
                                self.emit_drop_ptr(&gep, fty)?;
                            }
                        }
                        Ok(())
                    }
                    AdtKind::Record | AdtKind::Resource | AdtKind::Enum => Ok(()),
                }
            }
            TyKind::Tuple(elems) => {
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                for (i, ety) in elems.into_iter().enumerate().rev() {
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    self.emit_drop_ptr(&gep, ety)?;
                }
                Ok(())
            }
            TyKind::Closure { def, args } => {
                let Some(cdef) = self.tys().closure(def).cloned() else {
                    return Ok(());
                };
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                for (i, &cty) in cdef.captures.iter().enumerate().rev() {
                    let fty = self.tys().subst_lookup(cty, args).unwrap_or(cty);
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 {i}"
                    );
                    self.emit_drop_ptr(&gep, fty)?;
                }
                Ok(())
            }
            TyKind::ErrorUnion { ok, err } => {
                let base_llvm = llvm_storage_ty(self.tys(), ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i1, ptr {tag_ptr}");
                let ok_bb = self.fresh();
                let err_bb = self.fresh();
                let done_bb = self.fresh();
                let (ol, el, dl) = (&ok_bb[1..], &err_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {tag}, label %{ol}, label %{el}");
                let _ = writeln!(self.out, "{ol}:");
                let ok_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 1"
                );
                self.emit_drop_ptr(&ok_ptr, ok)?;
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{el}:");
                let err_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_ptr} = getelementptr inbounds {base_llvm}, ptr {ptr}, i32 0, i32 2"
                );
                self.emit_drop_ptr(&err_ptr, err)?;
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Desc(_)
            | TyKind::FnPtr { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn(_)
            | TyKind::Param(_)
            | TyKind::Opaque(_) => Ok(()),
        }
    }

    fn emit_assign(&mut self, dest: &Place, value: &Rvalue) -> Result<(), BodyEmitError> {
        let (d_ptr, d_ty) = self.emit_place_ptr(dest)?;
        let d_llvm = llvm_ty(self.tys(), d_ty)?;
        match value {
            Rvalue::Use(op) => {
                let (v, lt, _) = self.emit_operand(op)?;
                if lt != "void" && d_llvm != "void" {
                    let _ = writeln!(self.out, "  store {d_llvm} {v}, ptr {d_ptr}");
                }
                Ok(())
            }
            Rvalue::Ref { place, .. } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                if matches!(self.tys().get(p_ty), Some(TyKind::Slice(_))) {
                    let fat = self.fresh();
                    let _ = writeln!(self.out, "  {fat} = load {{ ptr, i64 }}, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store {{ ptr, i64 }} {fat}, ptr {d_ptr}");
                } else {
                    let _ = writeln!(self.out, "  store ptr {p_ptr}, ptr {d_ptr}");
                }
                Ok(())
            }
            Rvalue::SliceRef {
                place, start, end, ..
            } => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let (base_data, base_len, elem_ty) = match self.tys().get(p_ty) {
                    Some(TyKind::Array { element, length }) => {
                        (p_ptr, (*length as i64).to_string(), *element)
                    }
                    Some(TyKind::Slice(element)) => {
                        let dp_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {dp_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 0"
                        );
                        let dp = self.fresh();
                        let _ = writeln!(self.out, "  {dp} = load ptr, ptr {dp_slot}");
                        let len_slot = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_slot} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                        (dp, len, *element)
                    }
                    _ => return Err(BodyEmitError::new("SliceRef on non-array/slice")),
                };
                let start_v = match start {
                    Some(op) => self.emit_operand(op)?.0,
                    None => "0".into(),
                };
                let end_v = match end {
                    Some(op) => self.emit_operand(op)?.0,
                    None => base_len,
                };
                let elem_llvm = llvm_storage_ty(self.tys(), elem_ty)?;
                let new_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_ptr} = getelementptr inbounds {elem_llvm}, ptr {base_data}, i64 {start_v}"
                );
                let new_len = self.fresh();
                let _ = writeln!(self.out, "  {new_len} = sub i64 {end_v}, {start_v}");
                let s0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s0} = insertvalue {{ ptr, i64 }} undef, ptr {new_ptr}, 0"
                );
                let s1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s1} = insertvalue {{ ptr, i64 }} {s0}, i64 {new_len}, 1"
                );
                let _ = writeln!(self.out, "  store {{ ptr, i64 }} {s1}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Describe { place } => {
                let (p_ptr, _) = self.emit_place_ptr(place)?;
                let handle = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {p_ptr}");
                let _ = writeln!(self.out, "  store ptr {handle}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Aggregate { kind, operands } => {
                if d_llvm == "void" {
                    return Ok(());
                }
                match kind {
                    AggregateKind::Tuple | AggregateKind::Closure { .. } => {
                        for (i, op) in operands.iter().enumerate() {
                            let (v, lt, _) = self.emit_operand(op)?;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                            );
                            if lt == "void" {
                                let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                            } else {
                                let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                            }
                        }
                        Ok(())
                    }
                    AggregateKind::Array { .. } => {
                        for (i, op) in operands.iter().enumerate() {
                            let (v, lt, _) = self.emit_operand(op)?;
                            let gep = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i64 0, i64 {i}"
                            );
                            if lt == "void" {
                                let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                            } else {
                                let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                            }
                        }
                        Ok(())
                    }
                    AggregateKind::Adt { adt, variant, .. } => {
                        let def = self
                            .tys()
                            .get_adt(*adt)
                            .ok_or_else(|| BodyEmitError::new("missing ADT"))?;
                        match def.kind {
                            AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                                for (i, op) in operands.iter().enumerate() {
                                    let (v, lt, _) = self.emit_operand(op)?;
                                    let gep = self.fresh();
                                    let _ = writeln!(
                                        self.out,
                                        "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                                    );
                                    if lt == "void" {
                                        let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                                    } else {
                                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                                    }
                                }
                            }
                            AdtKind::Enum => {
                                let tag_ptr = self.fresh();
                                let _ = writeln!(
                                    self.out,
                                    "  {tag_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                                );
                                let _ =
                                    writeln!(self.out, "  store i32 {}, ptr {tag_ptr}", variant.0);
                                let v_slot = variant.0 + 1;
                                for (i, op) in operands.iter().enumerate() {
                                    let (v, lt, _) = self.emit_operand(op)?;
                                    let gep = self.fresh();
                                    let _ = writeln!(
                                        self.out,
                                        "  {gep} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {v_slot}, i32 {i}"
                                    );
                                    if lt == "void" {
                                        let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                                    } else {
                                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                                    }
                                }
                            }
                        }
                        Ok(())
                    }
                }
            }
            Rvalue::ErrorUnion { ok, value, .. } => {
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(
                    self.out,
                    "  store i1 {}, ptr {tag_ptr}",
                    if *ok { 1 } else { 0 }
                );
                let (v, lt, _) = self.emit_operand(value)?;
                let slot = if *ok { 1 } else { 2 };
                let val_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {val_ptr} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 {slot}"
                );
                if lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {val_ptr}");
                } else {
                    let _ = writeln!(self.out, "  store {lt} {v}, ptr {val_ptr}");
                }
                Ok(())
            }
            Rvalue::Unary { op, operand } => {
                let (v, lt, _) = self.emit_operand(operand)?;
                let res = self.fresh();
                match op {
                    UnOp::Not => {
                        let _ = writeln!(self.out, "  {res} = xor {lt} {v}, -1");
                    }
                    UnOp::Neg | UnOp::WrappingNeg => {
                        if lt == "float" || lt == "double" {
                            let _ = writeln!(self.out, "  {res} = fneg {lt} {v}");
                        } else {
                            let _ = writeln!(self.out, "  {res} = sub {lt} 0, {v}");
                        }
                    }
                }
                let _ = writeln!(self.out, "  store {d_llvm} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Binary { op, lhs, rhs } => {
                let (lv, lt, l_ty) = self.emit_operand(lhs)?;
                let (rv, _, _) = self.emit_operand(rhs)?;
                let signed = l_ty
                    .and_then(|t| self.tys().get(t))
                    .is_some_and(|k| matches!(k, TyKind::Int(i) if i.signed()));
                let is_float = lt == "float" || lt == "double";
                let res = self.fresh();
                let instr = match op {
                    BinOp::Add | BinOp::WrappingAdd => {
                        if is_float {
                            "fadd"
                        } else {
                            "add"
                        }
                    }
                    BinOp::Sub | BinOp::WrappingSub => {
                        if is_float {
                            "fsub"
                        } else {
                            "sub"
                        }
                    }
                    BinOp::Mul | BinOp::WrappingMul => {
                        if is_float {
                            "fmul"
                        } else {
                            "mul"
                        }
                    }
                    BinOp::Div => {
                        if is_float {
                            "fdiv"
                        } else if signed {
                            "sdiv"
                        } else {
                            "udiv"
                        }
                    }
                    BinOp::Rem => {
                        if is_float {
                            "frem"
                        } else if signed {
                            "srem"
                        } else {
                            "urem"
                        }
                    }
                    BinOp::BitAnd => "and",
                    BinOp::BitOr => "or",
                    BinOp::BitXor => "xor",
                    BinOp::Shl => "shl",
                    BinOp::Shr => {
                        if signed {
                            "ashr"
                        } else {
                            "lshr"
                        }
                    }
                    BinOp::Eq => {
                        if is_float {
                            "fcmp oeq"
                        } else {
                            "icmp eq"
                        }
                    }
                    BinOp::Ne => {
                        if is_float {
                            "fcmp one"
                        } else {
                            "icmp ne"
                        }
                    }
                    BinOp::Lt => {
                        if is_float {
                            "fcmp olt"
                        } else if signed {
                            "icmp slt"
                        } else {
                            "icmp ult"
                        }
                    }
                    BinOp::Le => {
                        if is_float {
                            "fcmp ole"
                        } else if signed {
                            "icmp sle"
                        } else {
                            "icmp ule"
                        }
                    }
                    BinOp::Gt => {
                        if is_float {
                            "fcmp ogt"
                        } else if signed {
                            "icmp sgt"
                        } else {
                            "icmp ugt"
                        }
                    }
                    BinOp::Ge => {
                        if is_float {
                            "fcmp oge"
                        } else if signed {
                            "icmp sge"
                        } else {
                            "icmp uge"
                        }
                    }
                };
                let _ = writeln!(self.out, "  {res} = {instr} {lt} {lv}, {rv}");
                let _ = writeln!(self.out, "  store {d_llvm} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Convert { operand, to } => {
                let (v, from_lt, from_ty) = self.emit_operand(operand)?;
                let to_lt = llvm_ty(self.tys(), *to)?;
                if from_lt == to_lt {
                    let _ = writeln!(self.out, "  store {to_lt} {v}, ptr {d_ptr}");
                    return Ok(());
                }
                let res = self.fresh();
                let from_int = match operand {
                    Operand::Const(ConstValue::Int { ty, .. }) => Some(*ty),
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => from_ty.and_then(|t| match self.tys().get(t) {
                        Some(TyKind::Int(i)) => Some(*i),
                        _ => None,
                    }),
                };
                let from_kind = from_ty.and_then(|t| self.tys().get(t));
                let to_kind = self.tys().get(*to);
                let cast = match (from_int, from_kind, to_kind) {
                    (Some(a), _, Some(TyKind::Int(b))) => {
                        let ab = a.bits(64);
                        let bb = b.bits(64);
                        if ab < bb {
                            if a.signed() { "sext" } else { "zext" }
                        } else {
                            "trunc"
                        }
                    }
                    (Some(a), _, Some(TyKind::Float(_))) => {
                        if a.signed() {
                            "sitofp"
                        } else {
                            "uitofp"
                        }
                    }
                    (_, Some(TyKind::Float(_)), Some(TyKind::Int(b))) => {
                        if b.signed() {
                            "fptosi"
                        } else {
                            "fptoui"
                        }
                    }
                    (_, Some(TyKind::Float(FloatTy::F32)), Some(TyKind::Float(FloatTy::F64))) => {
                        "fpext"
                    }
                    (_, Some(TyKind::Float(FloatTy::F64)), Some(TyKind::Float(FloatTy::F32))) => {
                        "fptrunc"
                    }
                    _ => "bitcast",
                };
                let _ = writeln!(self.out, "  {res} = {cast} {from_lt} {v} to {to_lt}");
                let _ = writeln!(self.out, "  store {to_lt} {res}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Discriminant(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let p_llvm = llvm_storage_ty(self.tys(), p_ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {p_llvm}, ptr {p_ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i32, ptr {tag_ptr}");
                let _ = writeln!(self.out, "  store i32 {tag}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::ErrorArm(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let p_llvm = llvm_storage_ty(self.tys(), p_ty)?;
                let tag_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {tag_ptr} = getelementptr inbounds {p_llvm}, ptr {p_ptr}, i32 0, i32 0"
                );
                let tag = self.fresh();
                let _ = writeln!(self.out, "  {tag} = load i1, ptr {tag_ptr}");
                let _ = writeln!(self.out, "  store i1 {tag}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::Len(place) => {
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                match self.tys().get(p_ty) {
                    Some(TyKind::Array { length, .. }) => {
                        let _ = writeln!(self.out, "  store i64 {length}, ptr {d_ptr}");
                    }
                    Some(TyKind::Slice(_)) => {
                        let len_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {len_ptr} = getelementptr inbounds {{ ptr, i64 }}, ptr {p_ptr}, i32 0, i32 1"
                        );
                        let len = self.fresh();
                        let _ = writeln!(self.out, "  {len} = load i64, ptr {len_ptr}");
                        let _ = writeln!(self.out, "  store i64 {len}, ptr {d_ptr}");
                    }
                    _ => return Err(BodyEmitError::new("Len on non-array/slice")),
                }
                Ok(())
            }
            Rvalue::FnRef(fid) => {
                let sym = fn_symbol(*fid, &self.module.functions[fid.index()]);
                let _ = writeln!(self.out, "  store ptr @{sym}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::ForeignFnRef(fid) => {
                let sym = sanitize_symbol(&self.module.foreigns[fid.index()].symbol);
                let _ = writeln!(self.out, "  store ptr @{sym}, ptr {d_ptr}");
                Ok(())
            }
            Rvalue::StaticUtf(sid) => {
                let len = self.module.statics[sid.index()].len();
                let s0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s0} = insertvalue {{ ptr, i64 }} undef, ptr @__zil_str_{}, 0",
                    sid.0
                );
                let s1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {s1} = insertvalue {{ ptr, i64 }} {s0}, i64 {len}, 1"
                );
                let _ = writeln!(self.out, "  store {{ ptr, i64 }} {s1}, ptr {d_ptr}");
                Ok(())
            }
        }
    }

    fn emit_terminator(&mut self, term: &Terminator) -> Result<(), BodyEmitError> {
        match term {
            Terminator::Goto { target } => {
                let _ = writeln!(self.out, "  br label %bb{}", target.0);
                Ok(())
            }
            Terminator::Switch {
                discr,
                arms,
                otherwise,
            } => {
                let (v, lt, _) = self.emit_operand(discr)?;
                let _ = writeln!(self.out, "  switch {lt} {v}, label %bb{} [", otherwise.0);
                for (val, target) in arms {
                    let _ = writeln!(self.out, "    {lt} {val}, label %bb{}", target.0);
                }
                self.out.push_str("  ]\n");
                Ok(())
            }
            Terminator::Return { value } => {
                let (v, lt, _) = self.emit_operand(value)?;
                if lt == "void" {
                    self.out.push_str("  ret void\n");
                } else {
                    let _ = writeln!(self.out, "  ret {lt} {v}");
                }
                Ok(())
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                cancel: _,
            } => {
                let (d_ptr, d_ty) = self.emit_place_ptr(dest)?;
                let d_llvm = llvm_ty(self.tys(), d_ty)?;
                match callee {
                    Callee::Direct(fid) => {
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        let sym = fn_symbol(*fid, &self.module.functions[fid.index()]);
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Foreign(fid) => {
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        let sym = sanitize_symbol(&self.module.foreigns[fid.index()].symbol);
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Indirect(op) => {
                        let (fptr, _, _) = self.emit_operand(op)?;
                        let mut arg_strs = Vec::new();
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void {fptr}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} {fptr}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Closure {
                        closure,
                        mode: ClosureCallMode::Shared | ClosureCallMode::Mut | ClosureCallMode::Once,
                    } => {
                        let (env_ptr, c_ty) = self.emit_place_ptr(closure)?;
                        let Some(TyKind::Closure { def, .. }) = self.tys().get(c_ty) else {
                            return Err(BodyEmitError::new("closure call on non-closure"));
                        };
                        let c_fn_id = self
                            .module
                            .functions
                            .iter()
                            .position(|f| f.body.closure == Some(*def))
                            .map(FnId::from_index)
                            .ok_or_else(|| BodyEmitError::new("closure body function missing"))?;
                        let sym = fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]);
                        let mut arg_strs = vec![format!("ptr {env_ptr}")];
                        for a in args {
                            let (v, lt, _) = self.emit_operand(a)?;
                            if lt != "void" {
                                arg_strs.push(format!("{lt} {v}"));
                            }
                        }
                        if d_llvm == "void" {
                            let _ =
                                writeln!(self.out, "  call void @{sym}({})", arg_strs.join(", "));
                        } else {
                            let r = self.fresh();
                            let _ = writeln!(
                                self.out,
                                "  {r} = call {d_llvm} @{sym}({})",
                                arg_strs.join(", ")
                            );
                            let _ = writeln!(self.out, "  store {d_llvm} {r}, ptr {d_ptr}");
                        }
                    }
                    Callee::Intrinsic(op) => self.emit_intrinsic(*op, args, &d_ptr, d_ty)?,
                    Callee::DynSlot { .. } => {
                        return Err(BodyEmitError::new(
                            "Callee::DynSlot native emission not yet implemented",
                        ));
                    }
                }
                let _ = writeln!(self.out, "  br label %bb{}", target.0);
                Ok(())
            }
            Terminator::Assert {
                cond,
                expected,
                target,
                trap:
                    TrapKind::Bounds
                    | TrapKind::Overflow
                    | TrapKind::DivideByZero
                    | TrapKind::Conversion
                    | TrapKind::Explicit,
            } => {
                let (cv, _, _) = self.emit_operand(cond)?;
                let fail_bb = self.fresh();
                let fail_label = &fail_bb[1..];
                if *expected {
                    let _ = writeln!(
                        self.out,
                        "  br i1 {cv}, label %bb{}, label %{fail_label}",
                        target.0
                    );
                } else {
                    let _ = writeln!(
                        self.out,
                        "  br i1 {cv}, label %{fail_label}, label %bb{}",
                        target.0
                    );
                }
                let _ = writeln!(
                    self.out,
                    "{fail_label}:\n  call void @abort()\n  unreachable"
                );
                Ok(())
            }
            Terminator::Unreachable => {
                self.out.push_str("  unreachable\n");
                Ok(())
            }
            Terminator::Trap(
                TrapKind::Bounds
                | TrapKind::Overflow
                | TrapKind::DivideByZero
                | TrapKind::Conversion
                | TrapKind::Explicit,
            ) => {
                self.out.push_str("  call void @abort()\n  unreachable\n");
                Ok(())
            }
        }
    }

    fn emit_intrinsic(
        &mut self,
        op: IntrinsicOp,
        args: &[Operand],
        d_ptr: &str,
        d_ty: TyId,
    ) -> Result<(), BodyEmitError> {
        match op {
            IntrinsicOp::ManagedCreate => {
                // Allocate resource payload on heap via calloc(1, sizeof(payload)).
                let Some(TyKind::Managed(inner)) = self.tys().get(d_ty) else {
                    return Err(BodyEmitError::new("ManagedCreate dest is not Managed"));
                };
                let inner_llvm = llvm_storage_ty(self.tys(), *inner)?;
                let sz_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {inner_llvm}, ptr null, i32 1"
                );
                let sz = self.fresh();
                let _ = writeln!(self.out, "  {sz} = ptrtoint ptr {sz_ptr} to i64");
                let raw = self.fresh();
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz})");
                for (i, a) in args.iter().enumerate() {
                    let (v, lt, _) = self.emit_operand(a)?;
                    let gep = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {gep} = getelementptr inbounds {inner_llvm}, ptr {raw}, i32 0, i32 {i}"
                    );
                    if lt == "void" {
                        let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                    } else {
                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                    }
                }
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferNew | IntrinsicOp::StoreNew => {
                // Header layout: { ptr data, i64 len, i64 cap } = 24 bytes, zeroed.
                let raw = self.fresh();
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 24)");
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferLen | IntrinsicOp::StoreLen => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let val = self.fresh();
                let _ = writeln!(self.out, "  {val} = load i64, ptr {slot}");
                let _ = writeln!(self.out, "  store i64 {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferCapacity | IntrinsicOp::StoreCapacity => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let val = self.fresh();
                let _ = writeln!(self.out, "  {val} = load i64, ptr {slot}");
                let _ = writeln!(self.out, "  store i64 {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferTruncate => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let (new_len, _, _) = self.emit_operand(&args[1])?;
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cur_len = self.fresh();
                let _ = writeln!(self.out, "  {cur_len} = load i64, ptr {len_slot}");
                let non_neg = self.fresh();
                let _ = writeln!(self.out, "  {non_neg} = icmp sge i64 {new_len}, 0");
                let le_cur = self.fresh();
                let _ = writeln!(self.out, "  {le_cur} = icmp sle i64 {new_len}, {cur_len}");
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = and i1 {non_neg}, {le_cur}");
                let set_bb = self.fresh();
                let done_bb = self.fresh();
                let (sl, dl) = (&set_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {ok}, label %{sl}, label %{dl}");
                let _ = writeln!(self.out, "{sl}:");
                let _ = writeln!(self.out, "  store i64 {new_len}, ptr {len_slot}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ByteBufferBorrow => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let out_p0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {out_p0} = getelementptr inbounds {{ ptr, i64 }}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store ptr {data}, ptr {out_p0}");
                let out_p1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {out_p1} = getelementptr inbounds {{ ptr, i64 }}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i64 {len}, ptr {out_p1}");
                Ok(())
            }
            IntrinsicOp::ByteBufferReserve | IntrinsicOp::StoreReserve => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (need, _, _) = self.emit_operand(&args[1])?;
                let elem_llvm = if op == IntrinsicOp::ByteBufferReserve {
                    "i8".to_string()
                } else {
                    let elem = self.store_elem_ty(h_ty)?;
                    llvm_storage_ty(self.tys(), elem)?
                };
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let cur_cap = self.fresh();
                let _ = writeln!(self.out, "  {cur_cap} = load i64, ptr {cap_slot}");
                let need_grow = self.fresh();
                let _ = writeln!(self.out, "  {need_grow} = icmp sgt i64 {need}, {cur_cap}");
                let grow_bb = self.fresh();
                let ok_bb = self.fresh();
                let done_bb = self.fresh();
                let (gl, ol, dl) = (&grow_bb[1..], &ok_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {need_grow}, label %{gl}, label %{ol}");
                let _ = writeln!(self.out, "{gl}:");
                let sz_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {elem_llvm}, ptr null, i32 1"
                );
                let elem_sz = self.fresh();
                let _ = writeln!(self.out, "  {elem_sz} = ptrtoint ptr {sz_ptr} to i64");
                let total_bytes = self.fresh();
                let _ = writeln!(self.out, "  {total_bytes} = mul i64 {need}, {elem_sz}");
                let old_data = self.fresh();
                let _ = writeln!(self.out, "  {old_data} = load ptr, ptr {data_slot}");
                let new_data = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_data} = call ptr @realloc(ptr {old_data}, i64 {total_bytes})"
                );
                let not_null = self.fresh();
                let _ = writeln!(self.out, "  {not_null} = icmp ne ptr {new_data}, null");
                let commit_bb = self.fresh();
                let fail_bb = self.fresh();
                let (cl, fl) = (&commit_bb[1..], &fail_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {not_null}, label %{cl}, label %{fl}");
                let _ = writeln!(self.out, "{cl}:");
                let _ = writeln!(self.out, "  store ptr {new_data}, ptr {data_slot}");
                let _ = writeln!(self.out, "  store i64 {need}, ptr {cap_slot}");
                let _ = writeln!(self.out, "  br label %{ol}");
                let _ = writeln!(self.out, "{ol}:");
                let _ = writeln!(self.out, "  store i1 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let _ = writeln!(self.out, "  store i1 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::ByteBufferPush => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let (byte_v, _, _) = self.emit_operand(&args[1])?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let cap = self.fresh();
                let _ = writeln!(self.out, "  {cap} = load i64, ptr {cap_slot}");
                let full = self.fresh();
                let _ = writeln!(self.out, "  {full} = icmp eq i64 {len}, {cap}");
                let grow_bb = self.fresh();
                let write_bb = self.fresh();
                let fail_bb = self.fresh();
                let done_bb = self.fresh();
                let (gl, wl, fl, dl) =
                    (&grow_bb[1..], &write_bb[1..], &fail_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {full}, label %{gl}, label %{wl}");
                let _ = writeln!(self.out, "{gl}:");
                let is_zero = self.fresh();
                let _ = writeln!(self.out, "  {is_zero} = icmp eq i64 {cap}, 0");
                let doubled = self.fresh();
                let _ = writeln!(self.out, "  {doubled} = shl i64 {cap}, 1");
                let new_cap = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_cap} = select i1 {is_zero}, i64 8, i64 {doubled}"
                );
                let old_data = self.fresh();
                let _ = writeln!(self.out, "  {old_data} = load ptr, ptr {data_slot}");
                let new_data = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_data} = call ptr @realloc(ptr {old_data}, i64 {new_cap})"
                );
                let ok_alloc = self.fresh();
                let _ = writeln!(self.out, "  {ok_alloc} = icmp ne ptr {new_data}, null");
                let commit_bb = self.fresh();
                let cl = &commit_bb[1..];
                let _ = writeln!(self.out, "  br i1 {ok_alloc}, label %{cl}, label %{fl}");
                let _ = writeln!(self.out, "{cl}:");
                let _ = writeln!(self.out, "  store ptr {new_data}, ptr {data_slot}");
                let _ = writeln!(self.out, "  store i64 {new_cap}, ptr {cap_slot}");
                let _ = writeln!(self.out, "  br label %{wl}");
                let _ = writeln!(self.out, "{wl}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let dst = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {dst} = getelementptr inbounds i8, ptr {data}, i64 {len}"
                );
                let _ = writeln!(self.out, "  store i8 {byte_v}, ptr {dst}");
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = add i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let _ = writeln!(self.out, "  store i1 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let _ = writeln!(self.out, "  store i1 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StorePush => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (val, val_lt, _) = self.emit_operand(&args[1])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let cap_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {cap_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 2"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let cap = self.fresh();
                let _ = writeln!(self.out, "  {cap} = load i64, ptr {cap_slot}");
                let full = self.fresh();
                let _ = writeln!(self.out, "  {full} = icmp eq i64 {len}, {cap}");
                let grow_bb = self.fresh();
                let write_bb = self.fresh();
                let fail_bb = self.fresh();
                let done_bb = self.fresh();
                let (gl, wl, fl, dl) =
                    (&grow_bb[1..], &write_bb[1..], &fail_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {full}, label %{gl}, label %{wl}");
                let _ = writeln!(self.out, "{gl}:");
                let is_zero = self.fresh();
                let _ = writeln!(self.out, "  {is_zero} = icmp eq i64 {cap}, 0");
                let doubled = self.fresh();
                let _ = writeln!(self.out, "  {doubled} = shl i64 {cap}, 1");
                let new_cap = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_cap} = select i1 {is_zero}, i64 8, i64 {doubled}"
                );
                let sz_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {elem_llvm}, ptr null, i32 1"
                );
                let elem_sz = self.fresh();
                let _ = writeln!(self.out, "  {elem_sz} = ptrtoint ptr {sz_ptr} to i64");
                let total_bytes = self.fresh();
                let _ = writeln!(self.out, "  {total_bytes} = mul i64 {new_cap}, {elem_sz}");
                let old_data = self.fresh();
                let _ = writeln!(self.out, "  {old_data} = load ptr, ptr {data_slot}");
                let new_data = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {new_data} = call ptr @realloc(ptr {old_data}, i64 {total_bytes})"
                );
                let ok_alloc = self.fresh();
                let _ = writeln!(self.out, "  {ok_alloc} = icmp ne ptr {new_data}, null");
                let commit_bb = self.fresh();
                let cl = &commit_bb[1..];
                let _ = writeln!(self.out, "  br i1 {ok_alloc}, label %{cl}, label %{fl}");
                let _ = writeln!(self.out, "{cl}:");
                let _ = writeln!(self.out, "  store ptr {new_data}, ptr {data_slot}");
                let _ = writeln!(self.out, "  store i64 {new_cap}, ptr {cap_slot}");
                let _ = writeln!(self.out, "  br label %{wl}");
                let _ = writeln!(self.out, "{wl}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let dst = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {dst} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {len}"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {dst}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {dst}");
                }
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = add i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{fl}:");
                let err_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {err_tag}");
                let err_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {err_val}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {err_val}");
                }
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StorePop => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let has = self.fresh();
                let _ = writeln!(self.out, "  {has} = icmp sgt i64 {len}, 0");
                let ok_bb = self.fresh();
                let empty_bb = self.fresh();
                let done_bb = self.fresh();
                let (ol, el, dl) = (&ok_bb[1..], &empty_bb[1..], &done_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {has}, label %{ol}, label %{el}");
                let _ = writeln!(self.out, "{ol}:");
                let next_len = self.fresh();
                let _ = writeln!(self.out, "  {next_len} = sub i64 {len}, 1");
                let _ = writeln!(self.out, "  store i64 {next_len}, ptr {len_slot}");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let src = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {src} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {next_len}"
                );
                let loaded = self.fresh();
                let _ = writeln!(self.out, "  {loaded} = load {elem_llvm}, ptr {src}");
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let ok_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store {elem_llvm} {loaded}, ptr {ok_val}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{el}:");
                let err_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {err_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {err_tag}");
                let _ = writeln!(self.out, "  br label %{dl}");
                let _ = writeln!(self.out, "{dl}:");
                Ok(())
            }
            IntrinsicOp::StoreBorrow { .. } => {
                let (h, _, h_ty) = self.emit_operand(&args[0])?;
                let (idx, _, _) = self.emit_operand(&args[1])?;
                let elem = self.store_elem_ty(h_ty)?;
                let elem_llvm = llvm_storage_ty(self.tys(), elem)?;
                let data_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {data_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 0"
                );
                let len_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {len_slot} = getelementptr inbounds {{ ptr, i64, i64 }}, ptr {h}, i32 0, i32 1"
                );
                let len = self.fresh();
                let _ = writeln!(self.out, "  {len} = load i64, ptr {len_slot}");
                let non_neg = self.fresh();
                let _ = writeln!(self.out, "  {non_neg} = icmp sge i64 {idx}, 0");
                let in_len = self.fresh();
                let _ = writeln!(self.out, "  {in_len} = icmp slt i64 {idx}, {len}");
                let in_bounds = self.fresh();
                let _ = writeln!(self.out, "  {in_bounds} = and i1 {non_neg}, {in_len}");
                let ok_bb = self.fresh();
                let oob_bb = self.fresh();
                let (ol, fl) = (&ok_bb[1..], &oob_bb[1..]);
                let _ = writeln!(self.out, "  br i1 {in_bounds}, label %{ol}, label %{fl}");
                let _ = writeln!(self.out, "{fl}:\n  call void @abort()\n  unreachable");
                let _ = writeln!(self.out, "{ol}:");
                let data = self.fresh();
                let _ = writeln!(self.out, "  {data} = load ptr, ptr {data_slot}");
                let gep = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {gep} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {idx}"
                );
                let _ = writeln!(self.out, "  store ptr {gep}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexCreate => {
                let (val, val_lt, _) = self.emit_operand(&args[0])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let store_lt = if val_lt == "void" { "i8" } else { &val_lt };
                let sz_ptr = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {store_lt}, ptr null, i32 1"
                );
                let sz = self.fresh();
                let _ = writeln!(self.out, "  {sz} = ptrtoint ptr {sz_ptr} to i64");
                let raw = self.fresh();
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz})");
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {raw}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {raw}");
                }
                let ok_tag = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let ok_val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_val} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr {raw}, ptr {ok_val}");
                Ok(())
            }
            IntrinsicOp::MutexLock => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let _ = writeln!(self.out, "  store ptr {h}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexValue => {
                let (guard_ptr, _, _) = self.emit_operand(&args[0])?;
                let handle = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {guard_ptr}");
                let _ = writeln!(self.out, "  store ptr {handle}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexUnlock => Ok(()),
            IntrinsicOp::SyncLoad => {
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let lt = llvm_ty(self.tys(), d_ty)?;
                let val = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {val} = load atomic {lt}, ptr {cell_ptr} seq_cst, align 8"
                );
                let _ = writeln!(self.out, "  store {lt} {val}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::SyncStore => {
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (val, lt, _) = self.emit_operand(&args[1])?;
                let _ = writeln!(
                    self.out,
                    "  store atomic {lt} {val}, ptr {cell_ptr} seq_cst, align 8"
                );
                Ok(())
            }
            IntrinsicOp::SyncFetchAdd => {
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (delta, lt, _) = self.emit_operand(&args[1])?;
                let old = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {old} = atomicrmw add ptr {cell_ptr}, {lt} {delta} seq_cst"
                );
                let _ = writeln!(self.out, "  store {lt} {old}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::SyncCompareExchange => {
                let (cell_ptr, _, _) = self.emit_operand(&args[0])?;
                let (expected, lt, _) = self.emit_operand(&args[1])?;
                let (desired, _, _) = self.emit_operand(&args[2])?;
                let pair = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {pair} = cmpxchg ptr {cell_ptr}, {lt} {expected}, {lt} {desired} seq_cst seq_cst"
                );
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {ok} = extractvalue {{ {lt}, i1 }} {pair}, 1");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::ThreadScope => {
                // Deterministic native execution of scoped worker(cell) + parent().
                let (worker_fn, _, _) = self.emit_operand(&args[0])?;
                let (cell_v, cell_lt, _) = self.emit_operand(&args[1])?;
                let Some(TyKind::Tuple(ret_elems)) = self.tys().get(d_ty).cloned() else {
                    return Err(BodyEmitError::new("ThreadScope dest must be 3-tuple"));
                };
                let r1_ty = llvm_ty(self.tys(), ret_elems[1])?;
                let r2_ty = llvm_ty(self.tys(), ret_elems[2])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let r1 = if r1_ty == "void" {
                    let _ = writeln!(self.out, "  call void {worker_fn}({cell_lt} {cell_v})");
                    "0".to_string()
                } else {
                    let r = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {r} = call {r1_ty} {worker_fn}({cell_lt} {cell_v})"
                    );
                    r
                };
                let (env_ptr, env_ty) = match &args[2] {
                    Operand::Copy(p) | Operand::Move(p) => self.emit_place_ptr(p)?,
                    Operand::Const(_) | Operand::Pooled(_) => {
                        return Err(BodyEmitError::new("ThreadScope parent must be a closure"));
                    }
                };
                let Some(TyKind::Closure { def, .. }) = self.tys().get(env_ty) else {
                    return Err(BodyEmitError::new("ThreadScope parent is not a closure"));
                };
                let c_fn_id = self
                    .module
                    .functions
                    .iter()
                    .position(|f| f.body.closure == Some(*def))
                    .map(FnId::from_index)
                    .ok_or_else(|| BodyEmitError::new("ThreadScope closure body missing"))?;
                let sym = fn_symbol(c_fn_id, &self.module.functions[c_fn_id.index()]);
                let r2 = if r2_ty == "void" {
                    let _ = writeln!(self.out, "  call void @{sym}(ptr {env_ptr})");
                    "0".to_string()
                } else {
                    let r = self.fresh();
                    let _ = writeln!(self.out, "  {r} = call {r2_ty} @{sym}(ptr {env_ptr})");
                    r
                };
                let f0 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {f0} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {f0}");
                let f1 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {f1} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                if r1_ty == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {f1}");
                } else {
                    let _ = writeln!(self.out, "  store {r1_ty} {r1}, ptr {f1}");
                }
                let f2 = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {f2} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                if r2_ty == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {f2}");
                } else {
                    let _ = writeln!(self.out, "  store {r2_ty} {r2}, ptr {f2}");
                }
                Ok(())
            }
            IntrinsicOp::TaskYield => Ok(()),
            IntrinsicOp::TaskStart
            | IntrinsicOp::TaskCreate
            | IntrinsicOp::TaskValid
            | IntrinsicOp::TaskPoll
            | IntrinsicOp::TaskRequestCancel
            | IntrinsicOp::TaskPollClose
            | IntrinsicOp::TaskFinishDestroy
            | IntrinsicOp::RegistrationStart
            | IntrinsicOp::RegistrationValid
            | IntrinsicOp::RegistrationClose
            | IntrinsicOp::AsyncRegistrationStart
            | IntrinsicOp::AsyncRegistrationValid
            | IntrinsicOp::AsyncRegistrationRequestClose
            | IntrinsicOp::AsyncRegistrationPollClose
            | IntrinsicOp::AsyncRegistrationFinishDestroy
            | IntrinsicOp::OwnedDynCreate => Err(BodyEmitError::new(format!(
                "intrinsic {op:?} native emission not yet implemented"
            ))),
        }
    }

    fn store_elem_ty(&self, handle_ty: Option<TyId>) -> Result<TyId, BodyEmitError> {
        let ty = handle_ty.ok_or_else(|| BodyEmitError::new("Store handle missing type"))?;
        let Some(TyKind::Desc(inner) | TyKind::Managed(inner)) = self.tys().get(ty) else {
            return Err(BodyEmitError::new("Store handle is not Desc/Managed"));
        };
        if let Some(TyKind::Opaque(OpaqueTy::OwnedStore { element: elem })) = self.tys().get(*inner)
        {
            Ok(*elem)
        } else {
            Err(BodyEmitError::new("Store handle inner is not OwnedStore"))
        }
    }
}

fn emit_function(
    module: &Module,
    _id: FnId,
    func: &Function,
    sym: &str,
    out: &mut String,
) -> Result<(), BodyEmitError> {
    let tys = &module.tys;
    let ret = llvm_ty(tys, func.sig.result)?;
    let mut param_sigs = Vec::new();
    if func.body.closure.is_some() {
        param_sigs.push("ptr %__env".to_string());
    }
    for (i, &pt) in func.sig.params.iter().enumerate() {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            param_sigs.push(format!("{lt} %p{i}"));
        }
    }
    let _ = writeln!(out, "define {ret} @{sym}({}) {{", param_sigs.join(", "));
    out.push_str("entry:\n");
    for (i, local) in func.body.locals.iter().enumerate() {
        let st = llvm_storage_ty(tys, local.ty)?;
        let _ = writeln!(out, "  %l{i} = alloca {st}");
    }
    for (i, &pt) in func.sig.params.iter().enumerate() {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            let _ = writeln!(out, "  store {lt} %p{i}, ptr %l{i}");
        }
    }
    out.push_str("  br label %bb0\n");

    let mut emitter = FnEmitter {
        module,
        body: &func.body,
        out,
        tmp: 0,
    };
    for (bi, block) in func.body.blocks.iter().enumerate() {
        emit_block(&mut emitter, bi, block)?;
    }
    out.push_str("}\n");
    Ok(())
}

fn emit_block(emitter: &mut FnEmitter<'_>, bi: usize, block: &Block) -> Result<(), BodyEmitError> {
    let _ = writeln!(emitter.out, "bb{bi}:");
    for inst in &block.instructions {
        emitter.emit_instruction(&inst.kind)?;
    }
    emitter.emit_terminator(&block.terminator)
}
