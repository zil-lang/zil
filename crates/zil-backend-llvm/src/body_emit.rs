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

/// Emits LLVM 23 opaque-pointer IR for a verified Body IR module as a single unit.
///
/// When `emit_c_main` is true and a zero-argument `main` function exists, a C
/// `main()` entry wrapper is emitted that invokes it and returns its `i32`
/// result (or 0 for unit).
pub fn emit_verified_body(
    verified: &VerifiedModule,
    emit_c_main: bool,
) -> Result<String, BodyEmitError> {
    let mut units = emit_verified_body_units(verified, emit_c_main, 1)?;
    Ok(units.pop().unwrap_or_default())
}

/// Partitions `verified` into `actual_units = requested_units.clamp(1, fn_count.max(1))`
/// independent LLVM 23 IR modules suitable for parallel per-unit object generation.
///
/// Every unit defines its assigned functions (`fn_idx % actual_units == unit_idx`),
/// emits external `declare` prototypes for functions assigned to other units, and
/// emits `private` static byte constants so units link without symbol collisions.
/// The C `main()` wrapper (when requested) is emitted in unit 0.
pub fn emit_verified_body_units(
    verified: &VerifiedModule,
    emit_c_main: bool,
    requested_units: usize,
) -> Result<Vec<String>, BodyEmitError> {
    emit_verified_body_units_with_entry(verified, emit_c_main, requested_units, None)
}

pub fn emit_verified_body_units_with_entry(
    verified: &VerifiedModule,
    emit_c_main: bool,
    requested_units: usize,
    entry_name: Option<&str>,
) -> Result<Vec<String>, BodyEmitError> {
    let module = verified.module();
    let fn_count = module.functions.len();
    let actual_units = requested_units.clamp(1, fn_count.max(1));

    // Precompute header (target triple, runtime decls, private statics, foreign decls).
    let mut header = String::with_capacity(2048);
    header.push_str("; ZIL Verified Body IR -> LLVM 23 IR\n");
    header.push_str("target triple = \"x86_64-unknown-linux-gnu\"\n\n");
    header.push_str("declare ptr @calloc(i64, i64) nobuiltin\n");
    header.push_str("declare ptr @realloc(ptr, i64) nobuiltin\n");
    header.push_str("declare void @free(ptr) nobuiltin\n");
    header.push_str("declare void @abort() noreturn\n");
    header.push_str("declare i32 @printf(ptr, ...)\n");
    header.push_str("declare i32 @getcontext(ptr)\n");
    header.push_str("declare void @makecontext(ptr, ptr, i32, ...)\n");
    header.push_str("declare i32 @swapcontext(ptr, ptr)\n");
    header.push_str("declare i32 @mprotect(ptr, i64, i32)\n");
    header.push_str("declare i32 @pthread_create(ptr, ptr, ptr, ptr)\n");
    header.push_str("declare i32 @pthread_join(i64, ptr)\n");
    header.push_str("declare i32 @pthread_mutex_init(ptr, ptr)\n");
    header.push_str("declare i32 @pthread_mutex_destroy(ptr)\n");
    header.push_str("declare i32 @pthread_mutex_lock(ptr)\n");
    header.push_str("declare i32 @pthread_mutex_unlock(ptr)\n");
    header.push_str("@__zil_current_task = linkonce_odr thread_local global ptr null\n");
    header.push_str(
        "%__zil_task_hdr = type { i32, ptr, [128 x i64], [128 x i64], [9216 x i64], ptr, ptr }\n",
    );
    header.push_str(
        "define linkonce_odr void @__zil_task_drop_frame(ptr %child) {\n\
entry:\n\
  %is_null = icmp eq ptr %child, null\n\
  br i1 %is_null, label %done, label %check\n\
check:\n\
  %st_p = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 0\n\
  %st = load i32, ptr %st_p\n\
  switch i32 %st, label %close_unstarted [\n\
    i32 1, label %req_close\n\
    i32 2, label %close_ready\n\
    i32 4, label %drive_close\n\
    i32 5, label %do_free\n\
    i32 6, label %done\n\
  ]\n\
req_close:\n\
  store i32 4, ptr %st_p\n\
  br label %drive_close\n\
drive_close:\n\
  %prev = load ptr, ptr @__zil_current_task\n\
  store ptr %child, ptr @__zil_current_task\n\
  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 2\n\
  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 3\n\
  %sc1 = call i32 @swapcontext(ptr %cctx, ptr %wctx)\n\
  %sc1_ok = icmp eq i32 %sc1, 0\n\
  br i1 %sc1_ok, label %after_sc1, label %sys_abort\n\
after_sc1:\n\
  store ptr %prev, ptr @__zil_current_task\n\
  %st2 = load i32, ptr %st_p\n\
  %is_closed = icmp eq i32 %st2, 5\n\
  br i1 %is_closed, label %do_free, label %maybe_yield\n\
maybe_yield:\n\
  %has_parent = icmp ne ptr %prev, null\n\
  br i1 %has_parent, label %yield_parent, label %drive_close\n\
yield_parent:\n\
  %pcctx = getelementptr inbounds %__zil_task_hdr, ptr %prev, i32 0, i32 2\n\
  %pwctx = getelementptr inbounds %__zil_task_hdr, ptr %prev, i32 0, i32 3\n\
  %sc2 = call i32 @swapcontext(ptr %pwctx, ptr %pcctx)\n\
  %sc2_ok = icmp eq i32 %sc2, 0\n\
  br i1 %sc2_ok, label %drive_close, label %sys_abort\n\
close_unstarted:\n\
  %da_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 6\n\
  %da_fn = load ptr, ptr %da_slot\n\
  %has_da = icmp ne ptr %da_fn, null\n\
  br i1 %has_da, label %call_da, label %close_ready\n\
call_da:\n\
  store ptr null, ptr %da_slot\n\
  call void %da_fn(ptr %child)\n\
  br label %close_ready\n\
close_ready:\n\
  store i32 5, ptr %st_p\n\
  br label %do_free\n\
do_free:\n\
  store i32 6, ptr %st_p\n\
  %gp_slot = getelementptr inbounds %__zil_task_hdr, ptr %child, i32 0, i32 5\n\
  %gp = load ptr, ptr %gp_slot\n\
  %has_gp = icmp ne ptr %gp, null\n\
  br i1 %has_gp, label %unprotect, label %free_now\n\
unprotect:\n\
  %up_rc = call i32 @mprotect(ptr %gp, i64 4096, i32 3)\n\
  %up_ok = icmp eq i32 %up_rc, 0\n\
  br i1 %up_ok, label %free_now, label %sys_abort\n\
free_now:\n\
  call void @free(ptr %child)\n\
  br label %done\n\
sys_abort:\n\
  call void @abort()\n\
  unreachable\n\
done:\n\
  ret void\n\
}\n",
    );
    header.push_str(
        "@__zil_fmt_i64 = private unnamed_addr constant [6 x i8] c\"%lld\\0A\\00\", align 1\n\n",
    );

    for (idx, bytes) in module.statics.iter().enumerate() {
        let escaped = escape_llvm_bytes(bytes);
        let _ = writeln!(
            header,
            "@__zil_str_{idx} = private unnamed_addr constant [{} x i8] c\"{escaped}\", align 1",
            bytes.len()
        );
    }
    if !module.statics.is_empty() {
        header.push('\n');
    }

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
            header,
            "declare {ret} @{}({})",
            sanitize_symbol(&f.symbol),
            params.join(", ")
        );
    }
    if !module.foreigns.is_empty() {
        header.push('\n');
    }

    let target_entry = entry_name.unwrap_or("main");
    let custom_entry = target_entry != "main";
    let mut main_fn: Option<(FnId, String, TyId)> = None;
    let mut syms = Vec::with_capacity(fn_count);
    for (idx, func) in module.functions.iter().enumerate() {
        let fn_id = FnId::from_index(idx);
        let sym = fn_symbol(fn_id, func);
        if (func.name == target_entry || func.name.ends_with(&format!(".{target_entry}")))
            && func.sig.params.is_empty()
            && func.sig.role == FnRole::Normal
        {
            main_fn = Some((fn_id, sym.clone(), func.sig.result));
        }
        syms.push(sym);
    }

    let mut units = Vec::with_capacity(actual_units);
    for unit_idx in 0..actual_units {
        let mut out = String::with_capacity(header.len() + 4096);
        out.push_str(&header);

        // Emit declarations for functions owned by other units.
        if actual_units > 1 {
            for (idx, func) in module.functions.iter().enumerate() {
                if idx % actual_units != unit_idx {
                    emit_function_decl(module, func, &syms[idx], &mut out)?;
                }
            }
            out.push('\n');
        }

        // Emit definitions for functions owned by this unit.
        for (idx, func) in module.functions.iter().enumerate() {
            if idx % actual_units == unit_idx {
                emit_function(module, FnId::from_index(idx), func, &syms[idx], &mut out)?;
                out.push('\n');
            }
        }

        // Emit C `main()` wrapper in unit 0.
        if unit_idx == 0
            && emit_c_main
            && let Some((_, ref sym, res_ty)) = main_fn
        {
            let ret = llvm_ty(&module.tys, res_ty)?;
            out.push_str("define i32 @main() {\nentry:\n");
            if let Some(TyKind::ErrorUnion { ok, .. }) = module.tys.get(res_ty) {
                let ok_ty = *ok;
                let ok_llvm = llvm_ty(&module.tys, ok_ty)?;
                let _ = writeln!(out, "  %r = call {ret} @{sym}()");
                let _ = writeln!(out, "  %tag = extractvalue {ret} %r, 0");
                out.push_str("  br i1 %tag, label %ok_bb, label %err_bb\n");
                out.push_str("ok_bb:\n");
                if ok_llvm == "void" {
                    out.push_str("  ret i32 0\n");
                } else if ok_llvm == "i32" {
                    let _ = writeln!(out, "  %v = extractvalue {ret} %r, 1");
                    if custom_entry {
                        out.push_str("  %ext = sext i32 %v to i64\n");
                        out.push_str(
                            "  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %ext)\n",
                        );
                        out.push_str("  ret i32 0\n");
                    } else {
                        out.push_str("  ret i32 %v\n");
                    }
                } else if ok_llvm == "i64" {
                    let _ = writeln!(out, "  %v = extractvalue {ret} %r, 1");
                    if custom_entry {
                        out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %v)\n");
                        out.push_str("  ret i32 0\n");
                    } else {
                        out.push_str("  %t = trunc i64 %v to i32\n  ret i32 %t\n");
                    }
                } else {
                    out.push_str("  ret i32 0\n");
                }
                out.push_str("err_bb:\n  ret i32 1\n");
            } else if ret == "void" {
                let _ = writeln!(out, "  call void @{sym}()");
                out.push_str("  ret i32 0\n");
            } else if ret == "i32" {
                let _ = writeln!(out, "  %r = call i32 @{sym}()");
                if custom_entry {
                    out.push_str("  %ext = sext i32 %r to i64\n");
                    out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %ext)\n");
                    out.push_str("  ret i32 0\n");
                } else {
                    out.push_str("  ret i32 %r\n");
                }
            } else if ret == "i64" {
                let _ = writeln!(out, "  %r = call i64 @{sym}()");
                if custom_entry {
                    out.push_str("  call i32 (ptr, ...) @printf(ptr @__zil_fmt_i64, i64 %r)\n");
                    out.push_str("  ret i32 0\n");
                } else {
                    out.push_str("  %t = trunc i64 %r to i32\n  ret i32 %t\n");
                }
            } else if ret == "i1" {
                let _ = writeln!(out, "  %r = call i1 @{sym}()");
                out.push_str("  %t = zext i1 %r to i32\n  ret i32 %t\n");
            } else {
                let _ = writeln!(out, "  %r = call {ret} @{sym}()");
                out.push_str("  ret i32 0\n");
            }
            out.push_str("}\n");
        }

        units.push(out);
    }

    Ok(units)
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

fn is_inline_managed(tys: &TyTable, inner: TyId) -> bool {
    matches!(
        tys.get(inner),
        Some(
            TyKind::Adt { .. } | TyKind::Opaque(OpaqueTy::ByteBuffer | OpaqueTy::OwnedStore { .. })
        )
    )
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
        TyKind::Managed(inner) => match tys.get(*inner) {
            Some(TyKind::Adt { .. }) => llvm_storage_ty(tys, *inner)?,
            Some(TyKind::Opaque(OpaqueTy::ByteBuffer | OpaqueTy::OwnedStore { .. })) => {
                "{ ptr, i64, i64 }".into()
            }
            _ => "ptr".into(),
        },
        TyKind::Desc(_)
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
    fn_id: FnId,
    body: &'a Body,
    out: &'a mut String,
    extra_defs: String,
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
                    let is_transparent = match self.tys().get(ty) {
                        Some(TyKind::Ref { pointee, .. }) => {
                            matches!(self.tys().get(*pointee), Some(TyKind::Slice(_)))
                        }
                        Some(TyKind::Managed(pointee)) => is_inline_managed(self.tys(), *pointee),
                        _ => false,
                    };
                    if is_transparent {
                        // Inline Managed or slice ref already resides at `ptr`.
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
                let inline = is_inline_managed(self.tys(), inner);
                let handle = if inline {
                    ptr.to_string()
                } else {
                    let h = self.fresh();
                    let _ = writeln!(self.out, "  {h} = load ptr, ptr {ptr}");
                    h
                };
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
                        let _ = writeln!(self.out, "  store i64 0, ptr {idx_slot}");
                        let head = self.fresh();
                        let body_bb = self.fresh();
                        let done = self.fresh();
                        let (hl, bl, dl) = (&head[1..], &body_bb[1..], &done[1..]);
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{hl}:");
                        let cur = self.fresh();
                        let _ = writeln!(self.out, "  {cur} = load i64, ptr {idx_slot}");
                        let cond = self.fresh();
                        let _ = writeln!(self.out, "  {cond} = icmp slt i64 {cur}, {len}");
                        let _ = writeln!(self.out, "  br i1 {cond}, label %{bl}, label %{dl}");
                        let _ = writeln!(self.out, "{bl}:");
                        let elem_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {elem_ptr} = getelementptr inbounds {elem_llvm}, ptr {data}, i64 {cur}"
                        );
                        self.emit_drop_ptr(&elem_ptr, elem)?;
                        let next = self.fresh();
                        let _ = writeln!(self.out, "  {next} = add i64 {cur}, 1");
                        let _ = writeln!(self.out, "  store i64 {next}, ptr {idx_slot}");
                        let _ = writeln!(self.out, "  br label %{hl}");
                        let _ = writeln!(self.out, "{dl}:");
                        let _ = writeln!(self.out, "  call void @free(ptr {data})");
                    }
                    Some(TyKind::Opaque(OpaqueTy::Mutex { payload: elem })) => {
                        let rc = self.fresh();
                        let ok = self.fresh();
                        let tid = self.tmp;
                        self.tmp += 1;
                        let l_ok = format!("md_ok_{tid}");
                        let l_ab = format!("md_ab_{tid}");
                        let _ = writeln!(
                            self.out,
                            "  {rc} = call i32 @pthread_mutex_destroy(ptr {handle})"
                        );
                        let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                        let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                        let _ = writeln!(self.out, "{l_ab}:");
                        let _ = writeln!(self.out, "  call void @abort()");
                        let _ = writeln!(self.out, "  unreachable");
                        let _ = writeln!(self.out, "{l_ok}:");
                        let pay_ptr = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {pay_ptr} = getelementptr inbounds i8, ptr {handle}, i64 64"
                        );
                        self.emit_drop_ptr(&pay_ptr, elem)?;
                    }
                    Some(TyKind::Opaque(OpaqueTy::MutexGuard { .. })) => {
                        let rc = self.fresh();
                        let ok = self.fresh();
                        let tid = self.tmp;
                        self.tmp += 1;
                        let l_ok = format!("mgu_ok_{tid}");
                        let l_ab = format!("mgu_ab_{tid}");
                        let _ = writeln!(
                            self.out,
                            "  {rc} = call i32 @pthread_mutex_unlock(ptr {handle})"
                        );
                        let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                        let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                        let _ = writeln!(self.out, "{l_ab}:");
                        let _ = writeln!(self.out, "  call void @abort()");
                        let _ = writeln!(self.out, "  unreachable");
                        let _ = writeln!(self.out, "{l_ok}:");
                        return Ok(());
                    }
                    Some(TyKind::Opaque(OpaqueTy::Task { .. })) => {
                        let _ =
                            writeln!(self.out, "  call void @__zil_task_drop_frame(ptr {handle})");
                        return Ok(());
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
                            OpaqueTy::SyncCell { .. }
                            | OpaqueTy::Registration { .. }
                            | OpaqueTy::AsyncRegistration { .. }
                            | OpaqueTy::ForeignHandle
                            | OpaqueTy::CWideString
                            | OpaqueTy::ForeignCallback { .. },
                        ),
                    )
                    | None => {}
                }
                if !inline {
                    let _ = writeln!(self.out, "  call void @free(ptr {handle})");
                }
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
                            for (fi, f) in v.fields.iter().enumerate() {
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
                for (i, &cty) in cdef.captures.iter().enumerate() {
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
            TyKind::Opaque(OpaqueTy::MutexGuard { .. }) => {
                let handle = self.fresh();
                let rc = self.fresh();
                let ok = self.fresh();
                let tid = self.tmp;
                self.tmp += 1;
                let l_ok = format!("mgo_ok_{tid}");
                let l_ab = format!("mgo_ab_{tid}");
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {ptr}");
                let _ = writeln!(
                    self.out,
                    "  {rc} = call i32 @pthread_mutex_unlock(ptr {handle})"
                );
                let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                let _ = writeln!(self.out, "{l_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
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
                let (p_ptr, p_ty) = self.emit_place_ptr(place)?;
                let inline = matches!(
                    self.tys().get(p_ty),
                    Some(TyKind::Managed(inner)) if is_inline_managed(self.tys(), *inner)
                );
                if inline {
                    let _ = writeln!(self.out, "  store ptr {p_ptr}, ptr {d_ptr}");
                } else {
                    let handle = self.fresh();
                    let _ = writeln!(self.out, "  {handle} = load ptr, ptr {p_ptr}");
                    let _ = writeln!(self.out, "  store ptr {handle}, ptr {d_ptr}");
                }
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
                cancel,
            } => {
                let (d_ptr, d_ty) = self.emit_place_ptr(dest)?;
                let d_llvm = llvm_ty(self.tys(), d_ty)?;
                let was_closing_slot = if cancel.is_some() {
                    let pre_slot = self.fresh();
                    let pre_ct = self.fresh();
                    let pre_has = self.fresh();
                    let pre_chk = format!("cc_pre_{}", self.tmp);
                    let pre_done = format!("cc_predone_{}", self.tmp);
                    self.tmp += 1;
                    let pre_stp = self.fresh();
                    let pre_st = self.fresh();
                    let pre_is = self.fresh();
                    let _ = writeln!(self.out, "  {pre_slot} = alloca i1");
                    let _ = writeln!(self.out, "  store i1 0, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  {pre_ct} = load ptr, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  {pre_has} = icmp ne ptr {pre_ct}, null");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {pre_has}, label %{pre_chk}, label %{pre_done}"
                    );
                    let _ = writeln!(self.out, "{pre_chk}:");
                    let _ = writeln!(
                        self.out,
                        "  {pre_stp} = getelementptr inbounds %__zil_task_hdr, ptr {pre_ct}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  {pre_st} = load i32, ptr {pre_stp}");
                    let _ = writeln!(self.out, "  {pre_is} = icmp eq i32 {pre_st}, 4");
                    let _ = writeln!(self.out, "  store i1 {pre_is}, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  br label %{pre_done}");
                    let _ = writeln!(self.out, "{pre_done}:");
                    Some(pre_slot)
                } else {
                    None
                };
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
                if let Some(cancel_bb) = cancel {
                    let was_cl = self.fresh();
                    let ct = self.fresh();
                    let has_ct = self.fresh();
                    let not_pre = self.fresh();
                    let can_chk = self.fresh();
                    let chk_lbl = format!("cc_chk_{}", self.tmp);
                    self.tmp += 1;
                    let cst_p = self.fresh();
                    let cst = self.fresh();
                    let is_canc = self.fresh();
                    let pre_slot = was_closing_slot.as_deref().unwrap();
                    let _ = writeln!(self.out, "  {was_cl} = load i1, ptr {pre_slot}");
                    let _ = writeln!(self.out, "  {not_pre} = xor i1 {was_cl}, true");
                    let _ = writeln!(self.out, "  {ct} = load ptr, ptr @__zil_current_task");
                    let _ = writeln!(self.out, "  {has_ct} = icmp ne ptr {ct}, null");
                    let _ = writeln!(self.out, "  {can_chk} = and i1 {not_pre}, {has_ct}");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {can_chk}, label %{chk_lbl}, label %bb{}",
                        target.0
                    );
                    let _ = writeln!(self.out, "{chk_lbl}:");
                    let _ = writeln!(
                        self.out,
                        "  {cst_p} = getelementptr inbounds %__zil_task_hdr, ptr {ct}, i32 0, i32 0"
                    );
                    let _ = writeln!(self.out, "  {cst} = load i32, ptr {cst_p}");
                    let _ = writeln!(self.out, "  {is_canc} = icmp eq i32 {cst}, 4");
                    let _ = writeln!(
                        self.out,
                        "  br i1 {is_canc}, label %bb{}, label %bb{}",
                        cancel_bb.0, target.0
                    );
                } else {
                    let _ = writeln!(self.out, "  br label %bb{}", target.0);
                }
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
                    | TrapKind::Explicit
                    | TrapKind::Cancel,
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
            Terminator::Trap(TrapKind::Cancel) => {
                let ret_llvm = llvm_ty(self.tys(), self.body.result)?;
                if ret_llvm == "void" {
                    self.out.push_str("  ret void\n");
                } else {
                    let _ = writeln!(self.out, "  ret {ret_llvm} zeroinitializer");
                }
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
                // Initialize inline resource payload directly in `d_ptr` with zero heap allocation.
                let Some(TyKind::Managed(inner)) = self.tys().get(d_ty) else {
                    return Err(BodyEmitError::new("ManagedCreate dest is not Managed"));
                };
                let inner_llvm = llvm_storage_ty(self.tys(), *inner)?;
                if args.is_empty() {
                    let _ = writeln!(
                        self.out,
                        "  store {inner_llvm} zeroinitializer, ptr {d_ptr}"
                    );
                } else {
                    for (i, a) in args.iter().enumerate() {
                        let (v, lt, _) = self.emit_operand(a)?;
                        let gep = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {gep} = getelementptr inbounds {inner_llvm}, ptr {d_ptr}, i32 0, i32 {i}"
                        );
                        if lt == "void" {
                            let _ = writeln!(self.out, "  store i8 0, ptr {gep}");
                        } else {
                            let _ = writeln!(self.out, "  store {lt} {v}, ptr {gep}");
                        }
                    }
                }
                Ok(())
            }
            IntrinsicOp::ByteBufferNew | IntrinsicOp::StoreNew => {
                // Inline header `{ ptr data, i64 len, i64 cap }`, zero-initialized without heap allocation.
                let _ = writeln!(
                    self.out,
                    "  store {{ ptr, i64, i64 }} zeroinitializer, ptr {d_ptr}"
                );
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
                let box_ty = format!("{{ [8 x i64], {store_lt} }}");
                let tid = self.tmp;
                self.tmp += 1;
                let sz_ptr = self.fresh();
                let sz = self.fresh();
                let raw = self.fresh();
                let is_null = self.fresh();
                let l_alloc_ok = format!("mc_aok_{tid}");
                let l_init_ok = format!("mc_iok_{tid}");
                let l_init_fail = format!("mc_ifail_{tid}");
                let l_fail = format!("mc_fail_{tid}");
                let l_done = format!("mc_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {box_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(self.out, "  {raw} = call ptr @calloc(i64 1, i64 {sz})");
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {raw}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{l_fail}, label %{l_alloc_ok}"
                );
                let _ = writeln!(self.out, "{l_alloc_ok}:");
                let init_rc = self.fresh();
                let init_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {init_rc} = call i32 @pthread_mutex_init(ptr {raw}, ptr null)"
                );
                let _ = writeln!(self.out, "  {init_ok} = icmp eq i32 {init_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {init_ok}, label %{l_init_ok}, label %{l_init_fail}"
                );
                let _ = writeln!(self.out, "{l_init_fail}:");
                let _ = writeln!(self.out, "  call void @free(ptr {raw})");
                let _ = writeln!(self.out, "  br label %{l_fail}");
                let _ = writeln!(self.out, "{l_fail}:");
                let e_tag = self.fresh();
                let e_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {e_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {e_tag}");
                let _ = writeln!(
                    self.out,
                    "  {e_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {e_slot}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_init_ok}:");
                let pay_p = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {pay_p} = getelementptr inbounds {box_ty}, ptr {raw}, i32 0, i32 1"
                );
                if val_lt == "void" {
                    let _ = writeln!(self.out, "  store i8 0, ptr {pay_p}");
                } else {
                    let _ = writeln!(self.out, "  store {val_lt} {val}, ptr {pay_p}");
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
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::MutexLock => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let rc = self.fresh();
                let ok = self.fresh();
                let l_ok = format!("ml_ok_{tid}");
                let l_ab = format!("ml_ab_{tid}");
                let _ = writeln!(self.out, "  {rc} = call i32 @pthread_mutex_lock(ptr {h})");
                let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                let _ = writeln!(self.out, "{l_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                let _ = writeln!(self.out, "  store ptr {h}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexValue => {
                let (guard_ptr, _, _) = self.emit_operand(&args[0])?;
                let handle = self.fresh();
                let pay = self.fresh();
                let _ = writeln!(self.out, "  {handle} = load ptr, ptr {guard_ptr}");
                let _ = writeln!(
                    self.out,
                    "  {pay} = getelementptr inbounds i8, ptr {handle}, i64 64"
                );
                let _ = writeln!(self.out, "  store ptr {pay}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::MutexUnlock => {
                let (h, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let rc = self.fresh();
                let ok = self.fresh();
                let l_ok = format!("mu_ok_{tid}");
                let l_ab = format!("mu_ab_{tid}");
                let _ = writeln!(self.out, "  {rc} = call i32 @pthread_mutex_unlock(ptr {h})");
                let _ = writeln!(self.out, "  {ok} = icmp eq i32 {rc}, 0");
                let _ = writeln!(self.out, "  br i1 {ok}, label %{l_ok}, label %{l_ab}");
                let _ = writeln!(self.out, "{l_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                Ok(())
            }
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
                // Real OS-thread execution of scoped worker(arg) concurrently with parent().
                let (worker_fn, _, _) = self.emit_operand(&args[0])?;
                let (cell_v, cell_lt, _) = self.emit_operand(&args[1])?;
                let Some(TyKind::Tuple(ret_elems)) = self.tys().get(d_ty).cloned() else {
                    return Err(BodyEmitError::new("ThreadScope dest must be 3-tuple"));
                };
                let r1_ty = llvm_ty(self.tys(), ret_elems[1])?;
                let r1_stor = llvm_storage_ty(self.tys(), ret_elems[1])?;
                let r2_ty = llvm_ty(self.tys(), ret_elems[2])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
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

                let cell_stor = if cell_lt == "void" { "i8" } else { &cell_lt };
                let ctx_ty = format!("{{ ptr, {cell_stor}, {r1_stor} }}");
                let tid = self.tmp;
                self.tmp += 1;
                let entry_sym = format!("__zil_thread_entry_f{}_{tid}", self.fn_id.0);

                // Emit pthread worker trampoline into extra_defs.
                let _ = writeln!(
                    self.extra_defs,
                    "\ndefine internal ptr @{entry_sym}(ptr %arg) {{"
                );
                self.extra_defs.push_str("entry:\n");
                let _ = writeln!(
                    self.extra_defs,
                    "  %fn_p = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 0"
                );
                self.extra_defs.push_str("  %wfn = load ptr, ptr %fn_p\n");
                let call_arg = if cell_lt == "void" {
                    String::new()
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %ap = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 1"
                    );
                    let _ = writeln!(self.extra_defs, "  %av = load {cell_lt}, ptr %ap");
                    format!("{cell_lt} %av")
                };
                if r1_ty == "void" {
                    let _ = writeln!(self.extra_defs, "  call void %wfn({call_arg})");
                } else {
                    let _ = writeln!(self.extra_defs, "  %rv = call {r1_ty} %wfn({call_arg})");
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rp = getelementptr inbounds {ctx_ty}, ptr %arg, i32 0, i32 2"
                    );
                    let _ = writeln!(self.extra_defs, "  store {r1_ty} %rv, ptr %rp");
                }
                self.extra_defs.push_str("  ret ptr null\n}\n");

                let ctx = self.fresh();
                let tid_slot = self.fresh();
                let c_fn_p = self.fresh();
                let pc_rc = self.fresh();
                let pc_ok = self.fresh();
                let l_ok = format!("ts_ok_{tid}");
                let l_fail = format!("ts_fail_{tid}");
                let l_joined = format!("ts_join_{tid}");
                let l_abort = format!("ts_ab_{tid}");
                let l_done = format!("ts_done_{tid}");

                let _ = writeln!(self.out, "  {ctx} = alloca {ctx_ty}");
                let _ = writeln!(self.out, "  {tid_slot} = alloca i64");
                let _ = writeln!(
                    self.out,
                    "  {c_fn_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store ptr {worker_fn}, ptr {c_fn_p}");
                if cell_lt != "void" {
                    let c_arg_p = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {c_arg_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  store {cell_lt} {cell_v}, ptr {c_arg_p}");
                }
                let _ = writeln!(
                    self.out,
                    "  {pc_rc} = call i32 @pthread_create(ptr {tid_slot}, ptr null, ptr @{entry_sym}, ptr {ctx})"
                );
                let _ = writeln!(self.out, "  {pc_ok} = icmp eq i32 {pc_rc}, 0");
                let _ = writeln!(self.out, "  br i1 {pc_ok}, label %{l_ok}, label %{l_fail}");

                // Thread creation failed -> (false, zero, zero)
                let _ = writeln!(self.out, "{l_fail}:");
                let _ = writeln!(self.out, "  store {d_llvm} zeroinitializer, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Thread created -> run parent concurrently, then pthread_join.
                let _ = writeln!(self.out, "{l_ok}:");
                let r2 = if r2_ty == "void" {
                    let _ = writeln!(self.out, "  call void @{sym}(ptr {env_ptr})");
                    "0".to_string()
                } else {
                    let r = self.fresh();
                    let _ = writeln!(self.out, "  {r} = call {r2_ty} @{sym}(ptr {env_ptr})");
                    r
                };
                let tid_val = self.fresh();
                let pj_rc = self.fresh();
                let pj_ok = self.fresh();
                let _ = writeln!(self.out, "  {tid_val} = load i64, ptr {tid_slot}");
                let _ = writeln!(
                    self.out,
                    "  {pj_rc} = call i32 @pthread_join(i64 {tid_val}, ptr null)"
                );
                let _ = writeln!(self.out, "  {pj_ok} = icmp eq i32 {pj_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {pj_ok}, label %{l_joined}, label %{l_abort}"
                );
                let _ = writeln!(self.out, "{l_abort}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");

                let _ = writeln!(self.out, "{l_joined}:");
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
                    let r1_p = self.fresh();
                    let r1 = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {r1_p} = getelementptr inbounds {ctx_ty}, ptr {ctx}, i32 0, i32 2"
                    );
                    let _ = writeln!(self.out, "  {r1} = load {r1_ty}, ptr {r1_p}");
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
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskYield => {
                let f = self.fresh();
                let st = self.fresh();
                let cur_st = self.fresh();
                let is_cl = self.fresh();
                let next_st = self.fresh();
                let cctx = self.fresh();
                let wctx = self.fresh();
                let _ = writeln!(self.out, "  {f} = load ptr, ptr @__zil_current_task");
                let _ = writeln!(
                    self.out,
                    "  {st} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {cur_st} = load i32, ptr {st}");
                let _ = writeln!(self.out, "  {is_cl} = icmp eq i32 {cur_st}, 4");
                let _ = writeln!(self.out, "  {next_st} = select i1 {is_cl}, i32 4, i32 1");
                let _ = writeln!(self.out, "  store i32 {next_st}, ptr {st}");
                let _ = writeln!(
                    self.out,
                    "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 2"
                );
                let _ = writeln!(
                    self.out,
                    "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {f}, i32 0, i32 3"
                );
                let tid = self.tmp;
                self.tmp += 1;
                let sc_rc = self.fresh();
                let sc_ok = self.fresh();
                let l_ok = format!("ty_ok_{tid}");
                let l_ab = format!("ty_ab_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sc_rc} = call i32 @swapcontext(ptr {wctx}, ptr {cctx})"
                );
                let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                let _ = writeln!(self.out, "  br i1 {sc_ok}, label %{l_ok}, label %{l_ab}");
                let _ = writeln!(self.out, "{l_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                Ok(())
            }
            IntrinsicOp::TaskCreate => {
                if args.is_empty() {
                    return Err(BodyEmitError::new("TaskCreate missing worker argument"));
                }
                let (wfn_v, _, _) = self.emit_operand(&args[0])?;
                let mut arg_vals: Vec<(String, String, String, Option<TyId>)> =
                    Vec::with_capacity(args.len() - 1);
                for a in &args[1..] {
                    let (v, lt, aty) = self.emit_operand(a)?;
                    let st = if let Some(tid) = aty {
                        llvm_storage_ty(self.tys(), tid)?
                    } else if lt == "void" {
                        "i8".to_string()
                    } else {
                        lt.clone()
                    };
                    arg_vals.push((v, lt, st, aty));
                }
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let task_res_ty = match self.tys().get(d_ty) {
                    Some(TyKind::ErrorUnion { ok, .. }) => match self.tys().get(*ok) {
                        Some(TyKind::Managed(inner)) => match self.tys().get(*inner) {
                            Some(TyKind::Opaque(OpaqueTy::Task { result, .. })) => *result,
                            _ => {
                                return Err(BodyEmitError::new("TaskCreate ok inner is not Task"));
                            }
                        },
                        _ => return Err(BodyEmitError::new("TaskCreate ok is not Managed")),
                    },
                    _ => return Err(BodyEmitError::new("TaskCreate dest is not ErrorUnion")),
                };
                let res_llvm = llvm_ty(self.tys(), task_res_ty)?;
                let res_stor = llvm_storage_ty(self.tys(), task_res_ty)?;

                let mut frame_fields = vec![
                    "%__zil_task_hdr".to_string(),
                    res_stor.clone(),
                    "ptr".to_string(),
                ];
                for (_, _, st, _) in &arg_vals {
                    frame_fields.push(st.clone());
                }
                let frame_ty = format!("{{ {} }}", frame_fields.join(", "));
                let tid = self.tmp;
                self.tmp += 1;
                let tramp_sym = format!("__zil_task_entry_f{}_{tid}", self.fn_id.0);

                // Emit worker trampoline into extra_defs.
                let _ = writeln!(
                    self.extra_defs,
                    "\ndefine internal void @{tramp_sym}(i32 %lo, i32 %hi) {{"
                );
                self.extra_defs.push_str("entry:\n");
                self.extra_defs.push_str("  %lo64 = zext i32 %lo to i64\n");
                self.extra_defs.push_str("  %hi64 = zext i32 %hi to i64\n");
                self.extra_defs.push_str("  %hi_sh = shl i64 %hi64, 32\n");
                self.extra_defs.push_str("  %addr = or i64 %hi_sh, %lo64\n");
                self.extra_defs
                    .push_str("  %frame = inttoptr i64 %addr to ptr\n");
                let _ = writeln!(
                    self.extra_defs,
                    "  %fn_slot = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 2"
                );
                self.extra_defs
                    .push_str("  %wfn = load ptr, ptr %fn_slot\n");
                let mut call_args = Vec::new();
                for (i, (_, lt, _, _)) in arg_vals.iter().enumerate() {
                    if lt != "void" {
                        let slot_idx = 3 + i;
                        let _ = writeln!(
                            self.extra_defs,
                            "  %ap{i} = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 {slot_idx}"
                        );
                        let _ = writeln!(self.extra_defs, "  %av{i} = load {lt}, ptr %ap{i}");
                        call_args.push(format!("{lt} %av{i}"));
                    }
                }
                if res_llvm == "void" {
                    let _ = writeln!(
                        self.extra_defs,
                        "  call void %wfn({})",
                        call_args.join(", ")
                    );
                } else {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rv = call {res_llvm} %wfn({})",
                        call_args.join(", ")
                    );
                }
                self.extra_defs.push_str(
                    "  %st_slot = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 0\n",
                );
                self.extra_defs
                    .push_str("  %end_st = load i32, ptr %st_slot\n");
                self.extra_defs
                    .push_str("  %was_canc = icmp eq i32 %end_st, 4\n");
                self.extra_defs
                    .push_str("  br i1 %was_canc, label %done_canc, label %done_ok\n");
                self.extra_defs.push_str("done_ok:\n");
                if res_llvm != "void" {
                    let _ = writeln!(
                        self.extra_defs,
                        "  %rslot = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 1"
                    );
                    let _ = writeln!(self.extra_defs, "  store {res_llvm} %rv, ptr %rslot");
                }
                self.extra_defs.push_str("  store i32 2, ptr %st_slot\n");
                self.extra_defs.push_str("  br label %swap_back\n");
                self.extra_defs.push_str("done_canc:\n");
                self.extra_defs.push_str("  store i32 5, ptr %st_slot\n");
                self.extra_defs.push_str("  br label %swap_back\n");
                self.extra_defs.push_str("swap_back:\n");
                self.extra_defs.push_str(
                    "  %cctx = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 2\n",
                );
                self.extra_defs.push_str(
                    "  %wctx = getelementptr inbounds %__zil_task_hdr, ptr %frame, i32 0, i32 3\n",
                );
                self.extra_defs
                    .push_str("  %sc_rc = call i32 @swapcontext(ptr %wctx, ptr %cctx)\n");
                self.extra_defs
                    .push_str("  %sc_ok = icmp eq i32 %sc_rc, 0\n");
                self.extra_defs
                    .push_str("  br i1 %sc_ok, label %swap_ok, label %swap_ab\n");
                self.extra_defs.push_str("swap_ab:\n");
                self.extra_defs.push_str("  call void @abort()\n");
                self.extra_defs.push_str("  unreachable\n");
                self.extra_defs.push_str("swap_ok:\n");
                self.extra_defs.push_str("  ret void\n}\n");

                // Optionally emit drop_args helper for tasks cancelled before start.
                let mut drop_args_body = String::new();
                for (i, (_, lt, _, aty)) in arg_vals.iter().enumerate().rev() {
                    if lt != "void"
                        && let Some(arg_ty) = *aty
                    {
                        let slot_idx = 3 + i;
                        let ap = format!("%da_p{i}");
                        let saved_len = self.out.len();
                        let res = self.emit_drop_ptr(&ap, arg_ty);
                        let one_drop = self.out.split_off(saved_len);
                        res?;
                        if !one_drop.is_empty() {
                            let _ = writeln!(
                                drop_args_body,
                                "  {ap} = getelementptr inbounds {frame_ty}, ptr %frame, i32 0, i32 {slot_idx}"
                            );
                            drop_args_body.push_str(&one_drop);
                        }
                    }
                }
                let drop_args_sym = if !drop_args_body.is_empty() {
                    let sym = format!("__zil_task_drop_args_f{}_{tid}", self.fn_id.0);
                    let _ = writeln!(
                        self.extra_defs,
                        "\ndefine internal void @{sym}(ptr %frame) {{"
                    );
                    self.extra_defs.push_str("entry:\n");
                    self.extra_defs.push_str(&drop_args_body);
                    self.extra_defs.push_str("  ret void\n}\n");
                    Some(sym)
                } else {
                    None
                };

                // Allocate single frame via calloc(1, sizeof(frame_ty)).
                let sz_ptr = self.fresh();
                let sz_i64 = self.fresh();
                let frame = self.fresh();
                let is_null = self.fresh();
                let fail_lbl = format!("tc_fail_{tid}");
                let ok_lbl = format!("tc_ok_{tid}");
                let done_lbl = format!("tc_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sz_ptr} = getelementptr {frame_ty}, ptr null, i32 1"
                );
                let _ = writeln!(self.out, "  {sz_i64} = ptrtoint ptr {sz_ptr} to i64");
                let _ = writeln!(
                    self.out,
                    "  {frame} = call ptr @calloc(i64 1, i64 {sz_i64})"
                );
                let _ = writeln!(self.out, "  {is_null} = icmp eq ptr {frame}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_null}, label %{fail_lbl}, label %{ok_lbl}"
                );

                let _ = writeln!(self.out, "{fail_lbl}:");
                let e_tag = self.fresh();
                let e_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {e_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {e_tag}");
                let _ = writeln!(
                    self.out,
                    "  {e_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {e_slot}");
                let _ = writeln!(self.out, "  br label %{done_lbl}");

                let _ = writeln!(self.out, "{ok_lbl}:");
                let st_p = self.fresh();
                let tr_p = self.fresh();
                let da_p = self.fresh();
                let fn_p = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  {tr_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr @{tramp_sym}, ptr {tr_p}");
                let _ = writeln!(
                    self.out,
                    "  {da_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 6"
                );
                if let Some(sym) = &drop_args_sym {
                    let _ = writeln!(self.out, "  store ptr @{sym}, ptr {da_p}");
                } else {
                    let _ = writeln!(self.out, "  store ptr null, ptr {da_p}");
                }
                let _ = writeln!(
                    self.out,
                    "  {fn_p} = getelementptr inbounds {frame_ty}, ptr {frame}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store ptr {wfn_v}, ptr {fn_p}");
                for (i, (v, lt, _, _)) in arg_vals.iter().enumerate() {
                    if lt != "void" {
                        let slot_idx = 3 + i;
                        let ap = self.fresh();
                        let _ = writeln!(
                            self.out,
                            "  {ap} = getelementptr inbounds {frame_ty}, ptr {frame}, i32 0, i32 {slot_idx}"
                        );
                        let _ = writeln!(self.out, "  store {lt} {v}, ptr {ap}");
                    }
                }
                let ok_tag = self.fresh();
                let ok_slot = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {ok_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {ok_tag}");
                let _ = writeln!(
                    self.out,
                    "  {ok_slot} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store ptr {frame}, ptr {ok_slot}");
                let _ = writeln!(self.out, "  br label %{done_lbl}");

                let _ = writeln!(self.out, "{done_lbl}:");
                Ok(())
            }
            IntrinsicOp::TaskValid => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let frame = self.fresh();
                let ok = self.fresh();
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(self.out, "  {ok} = icmp ne ptr {frame}, null");
                let _ = writeln!(self.out, "  store i1 {ok}, ptr {d_ptr}");
                Ok(())
            }
            IntrinsicOp::TaskPoll => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let d_llvm = llvm_storage_ty(self.tys(), d_ty)?;
                let (poll_ty, poll_res_ty) = match self.tys().get(d_ty) {
                    Some(TyKind::ErrorUnion { ok, .. }) => match self.tys().get(*ok) {
                        Some(TyKind::Adt { adt, args: pargs }) => {
                            let rty = self
                                .tys()
                                .field_ty(
                                    *adt,
                                    zil_ir::body::VariantIdx(1),
                                    zil_ir::body::FieldIdx(0),
                                    *pargs,
                                )
                                .ok_or_else(|| BodyEmitError::new("Poll.Ready field missing"))?;
                            (*ok, rty)
                        }
                        _ => return Err(BodyEmitError::new("TaskPoll ok is not Poll")),
                    },
                    _ => return Err(BodyEmitError::new("TaskPoll dest is not ErrorUnion")),
                };
                let poll_llvm = llvm_storage_ty(self.tys(), poll_ty)?;
                let res_llvm = llvm_ty(self.tys(), poll_res_ty)?;
                let res_stor = llvm_storage_ty(self.tys(), poll_res_ty)?;
                let res_frame_ty = format!("{{ %__zil_task_hdr, {res_stor} }}");

                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let l_start = format!("tp_start_{tid}");
                let l_resume = format!("tp_resume_{tid}");
                let l_comp = format!("tp_comp_{tid}");
                let l_canc = format!("tp_canc_{tid}");
                let l_inv = format!("tp_inv_{tid}");
                let l_pend = format!("tp_pend_{tid}");
                let l_ready = format!("tp_ready_{tid}");
                let l_done = format!("tp_done_{tid}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_inv} [ i32 0, label %{l_start} i32 1, label %{l_resume} i32 2, label %{l_comp} i32 3, label %{l_canc} i32 4, label %{l_canc} ]"
                );

                // Cancelled -> err(0)
                let _ = writeln!(self.out, "{l_canc}:");
                let c_tag = self.fresh();
                let c_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {c_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {c_tag}");
                let _ = writeln!(
                    self.out,
                    "  {c_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {c_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Complete -> err(1)
                let _ = writeln!(self.out, "{l_comp}:");
                let k_tag = self.fresh();
                let k_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {k_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {k_tag}");
                let _ = writeln!(
                    self.out,
                    "  {k_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 1, ptr {k_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Invalid -> err(2)
                let _ = writeln!(self.out, "{l_inv}:");
                let i_tag = self.fresh();
                let i_err = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {i_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 0, ptr {i_tag}");
                let _ = writeln!(
                    self.out,
                    "  {i_err} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 2"
                );
                let _ = writeln!(self.out, "  store i32 2, ptr {i_err}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Start: install 4 KiB PROT_NONE guard page inside the stack buffer,
                // then initialize worker_ctx via getcontext + makecontext.
                let _ = writeln!(self.out, "{l_start}:");
                let wctx0 = self.fresh();
                let link_p = self.fresh();
                let sp_p = self.fresh();
                let flg_p = self.fresh();
                let sz_p = self.fresh();
                let stk_raw = self.fresh();
                let stk_int = self.fresh();
                let stk_add = self.fresh();
                let guard_int = self.fresh();
                let guard_ptr = self.fresh();
                let gp_slot = self.fresh();
                let usable_stk = self.fresh();
                let tr_slot = self.fresh();
                let tr_fn = self.fresh();
                let addr = self.fresh();
                let lo = self.fresh();
                let hi64 = self.fresh();
                let hi = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {wctx0} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                );
                let gc_rc = self.fresh();
                let gc_ok = self.fresh();
                let l_gc_ok = format!("tp_gcok_{tid}");
                let _ = writeln!(self.out, "  {gc_rc} = call i32 @getcontext(ptr {wctx0})");
                let _ = writeln!(self.out, "  {gc_ok} = icmp eq i32 {gc_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {gc_ok}, label %{l_gc_ok}, label %{l_inv}"
                );
                let _ = writeln!(self.out, "{l_gc_ok}:");
                let _ = writeln!(
                    self.out,
                    "  {link_p} = getelementptr inbounds i8, ptr {wctx0}, i64 8"
                );
                let _ = writeln!(self.out, "  store ptr null, ptr {link_p}");
                let _ = writeln!(
                    self.out,
                    "  {stk_raw} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 4"
                );
                let _ = writeln!(self.out, "  {stk_int} = ptrtoint ptr {stk_raw} to i64");
                let _ = writeln!(self.out, "  {stk_add} = add i64 {stk_int}, 4095");
                let _ = writeln!(self.out, "  {guard_int} = and i64 {stk_add}, -4096");
                let _ = writeln!(self.out, "  {guard_ptr} = inttoptr i64 {guard_int} to ptr");
                let mp_rc = self.fresh();
                let mp_ok = self.fresh();
                let l_mp_ok = format!("tp_mpok_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {mp_rc} = call i32 @mprotect(ptr {guard_ptr}, i64 4096, i32 0)"
                );
                let _ = writeln!(self.out, "  {mp_ok} = icmp eq i32 {mp_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {mp_ok}, label %{l_mp_ok}, label %{l_inv}"
                );
                let _ = writeln!(self.out, "{l_mp_ok}:");
                let _ = writeln!(
                    self.out,
                    "  {gp_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 5"
                );
                let _ = writeln!(self.out, "  store ptr {guard_ptr}, ptr {gp_slot}");
                let _ = writeln!(
                    self.out,
                    "  {usable_stk} = getelementptr inbounds i8, ptr {guard_ptr}, i64 4096"
                );
                let _ = writeln!(
                    self.out,
                    "  {sp_p} = getelementptr inbounds i8, ptr {wctx0}, i64 16"
                );
                let _ = writeln!(self.out, "  store ptr {usable_stk}, ptr {sp_p}");
                let _ = writeln!(
                    self.out,
                    "  {flg_p} = getelementptr inbounds i8, ptr {wctx0}, i64 24"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {flg_p}");
                let _ = writeln!(
                    self.out,
                    "  {sz_p} = getelementptr inbounds i8, ptr {wctx0}, i64 32"
                );
                let _ = writeln!(self.out, "  store i64 65536, ptr {sz_p}");
                let _ = writeln!(
                    self.out,
                    "  {tr_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  {tr_fn} = load ptr, ptr {tr_slot}");
                let _ = writeln!(self.out, "  {addr} = ptrtoint ptr {frame} to i64");
                let _ = writeln!(self.out, "  {lo} = trunc i64 {addr} to i32");
                let _ = writeln!(self.out, "  {hi64} = lshr i64 {addr}, 32");
                let _ = writeln!(self.out, "  {hi} = trunc i64 {hi64} to i32");
                let _ = writeln!(
                    self.out,
                    "  call void (ptr, ptr, i32, ...) @makecontext(ptr {wctx0}, ptr {tr_fn}, i32 2, i32 {lo}, i32 {hi})"
                );
                let _ = writeln!(self.out, "  br label %{l_resume}");

                // Resume: swapcontext into worker, then check state.
                let _ = writeln!(self.out, "{l_resume}:");
                let prev = self.fresh();
                let cctx = self.fresh();
                let wctx = self.fresh();
                let sc_rc = self.fresh();
                let sc_ok = self.fresh();
                let l_sc_ok = format!("tp_scok_{tid}");
                let l_sc_ab = format!("tp_scab_{tid}");
                let st2 = self.fresh();
                let is_ready = self.fresh();
                let _ = writeln!(self.out, "  {prev} = load ptr, ptr @__zil_current_task");
                let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                let _ = writeln!(
                    self.out,
                    "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                );
                let _ = writeln!(
                    self.out,
                    "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                );
                let _ = writeln!(
                    self.out,
                    "  {sc_rc} = call i32 @swapcontext(ptr {cctx}, ptr {wctx})"
                );
                let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {sc_ok}, label %{l_sc_ok}, label %{l_sc_ab}"
                );
                let _ = writeln!(self.out, "{l_sc_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_sc_ok}:");
                let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                let _ = writeln!(self.out, "  {st2} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_ready} = icmp eq i32 {st2}, 2");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_ready}, label %{l_ready}, label %{l_pend}"
                );

                // Pending -> ok(Poll.Pending)
                let _ = writeln!(self.out, "{l_pend}:");
                let p_tag = self.fresh();
                let p_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {p_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {p_tag}");
                let _ = writeln!(
                    self.out,
                    "  {p_ok} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 0, ptr {p_ok}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Ready -> ok(Poll.Ready(result))
                let _ = writeln!(self.out, "{l_ready}:");
                let r_tag = self.fresh();
                let r_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {r_tag} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  store i1 1, ptr {r_tag}");
                let _ = writeln!(
                    self.out,
                    "  {r_ok} = getelementptr inbounds {d_llvm}, ptr {d_ptr}, i32 0, i32 1"
                );
                let _ = writeln!(self.out, "  store i32 1, ptr {r_ok}");
                if res_llvm != "void" {
                    let rsrc = self.fresh();
                    let rval = self.fresh();
                    let rdst = self.fresh();
                    let _ = writeln!(
                        self.out,
                        "  {rsrc} = getelementptr inbounds {res_frame_ty}, ptr {frame}, i32 0, i32 1"
                    );
                    let _ = writeln!(self.out, "  {rval} = load {res_llvm}, ptr {rsrc}");
                    let _ = writeln!(
                        self.out,
                        "  {rdst} = getelementptr inbounds {poll_llvm}, ptr {r_ok}, i32 0, i32 2, i32 0"
                    );
                    let _ = writeln!(self.out, "  store {res_llvm} {rval}, ptr {rdst}");
                }
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskRequestCancel => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let l_c0 = format!("trc_c0_{tid}");
                let l_c1 = format!("trc_c1_{tid}");
                let l_done = format!("trc_done_{tid}");
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_done} [ i32 0, label %{l_c0} i32 1, label %{l_c1} ]"
                );
                let _ = writeln!(self.out, "{l_c0}:");
                let _ = writeln!(self.out, "  store i32 3, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_c1}:");
                let _ = writeln!(self.out, "  store i32 4, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_done}");
                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskPollClose => {
                let (ref_v, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let frame = self.fresh();
                let st_p = self.fresh();
                let st = self.fresh();
                let l_unstarted = format!("tpc_un_{tid}");
                let l_call_da = format!("tpc_da_{tid}");
                let l_mark_closed = format!("tpc_mc_{tid}");
                let l_mark_close = format!("tpc_cl_{tid}");
                let l_drive = format!("tpc_drv_{tid}");
                let l_pend = format!("tpc_pnd_{tid}");
                let l_ready = format!("tpc_rdy_{tid}");
                let l_done = format!("tpc_done_{tid}");
                let _ = writeln!(self.out, "  {frame} = load ptr, ptr {ref_v}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  switch i32 {st}, label %{l_ready} [ i32 0, label %{l_unstarted} i32 3, label %{l_unstarted} i32 2, label %{l_mark_closed} i32 1, label %{l_mark_close} i32 4, label %{l_drive} ]"
                );

                // Unstarted (0 or 3): drop owned args once, then mark Closed (5).
                let _ = writeln!(self.out, "{l_unstarted}:");
                let da_slot = self.fresh();
                let da_fn = self.fresh();
                let has_da = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {da_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 6"
                );
                let _ = writeln!(self.out, "  {da_fn} = load ptr, ptr {da_slot}");
                let _ = writeln!(self.out, "  {has_da} = icmp ne ptr {da_fn}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {has_da}, label %{l_call_da}, label %{l_mark_closed}"
                );
                let _ = writeln!(self.out, "{l_call_da}:");
                let _ = writeln!(self.out, "  store ptr null, ptr {da_slot}");
                let _ = writeln!(self.out, "  call void {da_fn}(ptr {frame})");
                let _ = writeln!(self.out, "  br label %{l_mark_closed}");

                let _ = writeln!(self.out, "{l_mark_closed}:");
                let _ = writeln!(self.out, "  store i32 5, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_ready}");

                // Suspended (1): transition to Closing (4) and drive worker unwind.
                let _ = writeln!(self.out, "{l_mark_close}:");
                let _ = writeln!(self.out, "  store i32 4, ptr {st_p}");
                let _ = writeln!(self.out, "  br label %{l_drive}");

                // Closing (4): resume worker on cancel path; return Ready iff Closed (5).
                let _ = writeln!(self.out, "{l_drive}:");
                let prev = self.fresh();
                let cctx = self.fresh();
                let wctx = self.fresh();
                let st2 = self.fresh();
                let is_closed = self.fresh();
                let _ = writeln!(self.out, "  {prev} = load ptr, ptr @__zil_current_task");
                let _ = writeln!(self.out, "  store ptr {frame}, ptr @__zil_current_task");
                let _ = writeln!(
                    self.out,
                    "  {cctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 2"
                );
                let _ = writeln!(
                    self.out,
                    "  {wctx} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 3"
                );
                let sc_rc = self.fresh();
                let sc_ok = self.fresh();
                let l_sc_ok = format!("tpc_scok_{tid}");
                let l_sc_ab = format!("tpc_scab_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {sc_rc} = call i32 @swapcontext(ptr {cctx}, ptr {wctx})"
                );
                let _ = writeln!(self.out, "  {sc_ok} = icmp eq i32 {sc_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {sc_ok}, label %{l_sc_ok}, label %{l_sc_ab}"
                );
                let _ = writeln!(self.out, "{l_sc_ab}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_sc_ok}:");
                let _ = writeln!(self.out, "  store ptr {prev}, ptr @__zil_current_task");
                let _ = writeln!(self.out, "  {st2} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_closed} = icmp eq i32 {st2}, 5");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_closed}, label %{l_ready}, label %{l_pend}"
                );

                // Poll.Pending -> enum tag 0
                let _ = writeln!(self.out, "{l_pend}:");
                let _ = writeln!(self.out, "  store i32 0, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                // Poll.Ready(()) -> enum tag 1
                let _ = writeln!(self.out, "{l_ready}:");
                let _ = writeln!(self.out, "  store i32 1, ptr {d_ptr}");
                let _ = writeln!(self.out, "  br label %{l_done}");

                let _ = writeln!(self.out, "{l_done}:");
                Ok(())
            }
            IntrinsicOp::TaskFinishDestroy => {
                let (frame, _, _) = self.emit_operand(&args[0])?;
                let tid = self.tmp;
                self.tmp += 1;
                let st_p = self.fresh();
                let st = self.fresh();
                let is_closed = self.fresh();
                let gp_slot = self.fresh();
                let gp = self.fresh();
                let has_gp = self.fresh();
                let l_abort = format!("tfd_ab_{tid}");
                let l_ok = format!("tfd_ok_{tid}");
                let l_unprot = format!("tfd_up_{tid}");
                let l_free = format!("tfd_fr_{tid}");
                let _ = writeln!(
                    self.out,
                    "  {st_p} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 0"
                );
                let _ = writeln!(self.out, "  {st} = load i32, ptr {st_p}");
                let _ = writeln!(self.out, "  {is_closed} = icmp eq i32 {st}, 5");
                let _ = writeln!(
                    self.out,
                    "  br i1 {is_closed}, label %{l_ok}, label %{l_abort}"
                );
                let _ = writeln!(self.out, "{l_abort}:");
                let _ = writeln!(self.out, "  call void @abort()");
                let _ = writeln!(self.out, "  unreachable");
                let _ = writeln!(self.out, "{l_ok}:");
                let _ = writeln!(self.out, "  store i32 6, ptr {st_p}");
                let _ = writeln!(
                    self.out,
                    "  {gp_slot} = getelementptr inbounds %__zil_task_hdr, ptr {frame}, i32 0, i32 5"
                );
                let _ = writeln!(self.out, "  {gp} = load ptr, ptr {gp_slot}");
                let _ = writeln!(self.out, "  {has_gp} = icmp ne ptr {gp}, null");
                let _ = writeln!(
                    self.out,
                    "  br i1 {has_gp}, label %{l_unprot}, label %{l_free}"
                );
                let _ = writeln!(self.out, "{l_unprot}:");
                let up_rc = self.fresh();
                let up_ok = self.fresh();
                let _ = writeln!(
                    self.out,
                    "  {up_rc} = call i32 @mprotect(ptr {gp}, i64 4096, i32 3)"
                );
                let _ = writeln!(self.out, "  {up_ok} = icmp eq i32 {up_rc}, 0");
                let _ = writeln!(
                    self.out,
                    "  br i1 {up_ok}, label %{l_free}, label %{l_abort}"
                );
                let _ = writeln!(self.out, "{l_free}:");
                let _ = writeln!(self.out, "  call void @free(ptr {frame})");
                Ok(())
            }
            IntrinsicOp::TaskStart
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

fn emit_function_decl(
    module: &Module,
    func: &Function,
    sym: &str,
    out: &mut String,
) -> Result<(), BodyEmitError> {
    let tys = &module.tys;
    let ret = llvm_ty(tys, func.sig.result)?;
    let mut param_tys = Vec::new();
    if func.body.closure.is_some() {
        param_tys.push("ptr".to_string());
    }
    for &pt in &func.sig.params {
        let lt = llvm_ty(tys, pt)?;
        if lt != "void" {
            param_tys.push(lt);
        }
    }
    let _ = writeln!(out, "declare {ret} @{sym}({})", param_tys.join(", "));
    Ok(())
}

fn emit_function(
    module: &Module,
    fn_id: FnId,
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
        fn_id,
        body: &func.body,
        out,
        extra_defs: String::new(),
        tmp: 0,
    };
    for (bi, block) in func.body.blocks.iter().enumerate() {
        emit_block(&mut emitter, bi, block)?;
    }
    let extra = emitter.extra_defs;
    out.push_str("}\n");
    out.push_str(&extra);
    Ok(())
}

fn emit_block(emitter: &mut FnEmitter<'_>, bi: usize, block: &Block) -> Result<(), BodyEmitError> {
    let _ = writeln!(emitter.out, "bb{bi}:");
    for inst in &block.instructions {
        emitter.emit_instruction(&inst.kind)?;
    }
    emitter.emit_terminator(&block.terminator)
}
