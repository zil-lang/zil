//! Canonical structural Body-IR CGU fingerprinter for direct object-cache keys.
//!
//! Computes a deterministic SHA-256 fingerprint for one Codegen Unit directly
//! from `VerifiedModule`'s Body IR slice BEFORE any LLVM IR text formatting or
//! `zil_llvm_api::model::Module` construction. Cache-hit CGUs therefore pay zero
//! IR emission cost.
//!
//! ## Soundness & Stability Invariants
//! 1. Every `TyKind`, `OpaqueTy`, `InstructionKind`, `Rvalue`, `Terminator`,
//!    `Callee`, `Operand`, `Projection`, `ConstValue`, and `AggregateKind` variant
//!    is matched exhaustively (`#![deny(clippy::wildcard_enum_match_arm)]`).
//! 2. `FnId`, `TyId`, `AdtId`, `ClosureId`, `DynId`, `StaticId`, and `ForeignId`
//!    are hashed structurally (by canonical symbol, structural definition, or
//!    raw bytes), NOT by raw numeric table indices — so adding/removing functions
//!    or modules elsewhere never perturbs untouched CGUs' cache keys.
//! 3. Spans are ignored because `body_emit` never emits span data into LLVM IR.

#![deny(clippy::wildcard_enum_match_arm)]

use crate::body_emit::PreparedBodyUnits;
use std::collections::HashSet;
use zil_base::{Fingerprint, FingerprintBuilder};
use zil_ir::body::{
    AdtId, AdtKind, AggregateKind, ArgsId, BinOp, Block, Body, Callee, ClosureCallMode, ClosureId,
    ClosureKind, ConstValue, DynId, FieldRef, FloatTy, FnId, FnRole, FnSig, ForeignId,
    InstructionKind, IntTy, IntrinsicOp, MemoryOrder, Mutability, OpaqueTy, Operand, Place,
    PlaceRoot, Projection, Region, ResumeMode, Rvalue, ScopeExit, StaticId, SyncCellKind,
    Terminator, TrapKind, TyId, TyKind, UnOp,
};

impl<'a> PreparedBodyUnits<'a> {
    /// Computes the canonical structural Body-IR SHA-256 fingerprint of CGU `unit_idx`.
    pub fn unit_body_fingerprint(&self, unit_idx: usize) -> Fingerprint {
        let mut h = UnitBodyHasher {
            plan: self,
            unit_idx,
            h: FingerprintBuilder::new(),
            seen_adts: HashSet::new(),
            seen_closures: HashSet::new(),
            seen_dyns: HashSet::new(),
        };
        h.hash_unit();
        h.h.digest()
    }
}

struct UnitBodyHasher<'p, 'a> {
    plan: &'p PreparedBodyUnits<'a>,
    unit_idx: usize,
    h: FingerprintBuilder,
    seen_adts: HashSet<AdtId>,
    seen_closures: HashSet<ClosureId>,
    seen_dyns: HashSet<DynId>,
}

impl<'p, 'a> UnitBodyHasher<'p, 'a> {
    fn tag(&mut self, b: &[u8]) {
        self.h.update(b);
    }

    fn u32(&mut self, v: u32) {
        self.h.update(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.h.update(&v.to_le_bytes());
    }

    fn u128(&mut self, v: u128) {
        self.h.update(&v.to_le_bytes());
    }

    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.h.update(s.as_bytes());
    }

    fn hash_unit(&mut self) {
        let is_unit0 = self.unit_idx == 0;
        self.tag(b"zil-cgu-body-fp-v1\0");
        self.tag(&[
            u8::from(is_unit0),
            u8::from(self.plan.is_windows),
            u8::from(self.plan.single_exe),
            u8::from(self.plan.actual_units > 1),
            u8::from(self.plan.reach.uses_tasks),
            u8::from(self.plan.reach.uses_regs),
            u8::from(self.plan.reach.uses_aregs),
            u8::from(self.plan.reach.uses_bytes_grow),
        ]);
        self.h.update(&self.plan.header_fp.0);
        if is_unit0 {
            self.h.update(&self.plan.runtime_defs_fp.0);
        } else {
            self.h.update(&self.plan.runtime_decls_fp.0);
        }
        self.h.update(&self.plan.foreign_header_fp.0);

        let unit_fns = &self.plan.unit_fn_lists[self.unit_idx];
        self.u64(unit_fns.len() as u64);
        for &fn_idx in unit_fns {
            self.hash_defined_function(fn_idx);
        }

        if is_unit0 {
            // Reachable dyn Trait vtables, drop shims, and slot thunks live in unit 0.
            self.tag(b"DYN_VTABLES");
            for (idx, dimpl) in self.plan.module.dyn_impls.iter().enumerate() {
                if !self.plan.reach.dyn_impls.get(idx).copied().unwrap_or(true) {
                    continue;
                }
                self.tag(b"VT");
                self.u64(idx as u64);
                self.hash_dyn_def(dimpl.dyn_id);
                self.hash_ty(dimpl.concrete_ty);
                self.u64(dimpl.methods.len() as u64);
                for &m_id in &dimpl.methods {
                    self.hash_fn_ref(m_id);
                }
            }

            // C `main()` wrapper lives in unit 0 when requested.
            self.tag(b"C_MAIN");
            self.tag(&[
                u8::from(self.plan.emit_c_main),
                u8::from(self.plan.custom_entry),
            ]);
            if self.plan.emit_c_main
                && let Some((main_id, ref sym, res_ty)) = self.plan.main_fn
            {
                self.tag(b"1");
                self.str(sym);
                self.tag(&[u8::from(
                    self.plan.unit_of_fn[main_id.index()] != self.unit_idx,
                )]);
                self.hash_ty(res_ty);
            } else {
                self.tag(b"0");
            }
        }
    }

    fn hash_defined_function(&mut self, fn_idx: usize) {
        let func = &self.plan.module.functions[fn_idx];
        self.tag(b"DEF_FN");
        self.str(&self.plan.syms[fn_idx]);
        self.hash_fn_sig(&func.sig);
        self.hash_body(&func.body);
    }

    fn hash_fn_ref(&mut self, fid: FnId) {
        let idx = fid.index();
        self.tag(b"FN_REF");
        if let Some(sym) = self.plan.syms.get(idx) {
            self.str(sym);
            let cross_unit =
                self.plan.actual_units > 1 && self.plan.unit_of_fn[idx] != self.unit_idx;
            self.tag(&[u8::from(cross_unit)]);
            let func = &self.plan.module.functions[idx];
            self.hash_fn_sig(&func.sig);
            self.tag(&[u8::from(func.body.closure.is_some())]);
        } else {
            self.u32(fid.0);
        }
    }

    fn hash_foreign_ref(&mut self, fid: ForeignId) {
        self.tag(b"FOREIGN");
        if let Some(f) = self.plan.module.foreigns.get(fid.index()) {
            self.str(&f.symbol);
            self.u64(f.params.len() as u64);
            for &p in &f.params {
                self.hash_ty(p);
            }
            self.hash_ty(f.result);
        } else {
            self.u32(fid.0);
        }
    }

    fn hash_static_ref(&mut self, sid: StaticId) {
        self.tag(b"STATIC");
        if let Some(sym) = self.plan.static_syms.get(sid.index()) {
            self.str(sym);
        }
        if let Some(bytes) = self.plan.module.statics.get(sid.index()) {
            self.u64(bytes.len() as u64);
            self.h.update(bytes);
        }
    }

    fn hash_fn_sig(&mut self, sig: &FnSig) {
        self.tag(b"SIG");
        self.u64(sig.params.len() as u64);
        for &p in &sig.params {
            self.hash_ty(p);
        }
        self.hash_ty(sig.result);
        self.u32(sig.type_params);
        self.u32(sig.region_params);
        match sig.role {
            FnRole::Normal => self.tag(b"RN"),
            FnRole::Drop(adt) => {
                self.tag(b"RD");
                self.hash_adt_def(adt);
            }
            FnRole::DynSlotImpl {
                dyn_id,
                concrete_ty,
                slot,
            } => {
                self.tag(b"RS");
                self.hash_dyn_def(dyn_id);
                self.hash_ty(concrete_ty);
                self.u32(slot);
            }
        }
        match sig.resume_mode {
            ResumeMode::Local => self.tag(b"ML"),
            ResumeMode::Transfer => self.tag(b"MT"),
        }
    }

    fn hash_region(&mut self, r: Region) {
        match r {
            Region::Static => self.tag(b"RS"),
            Region::Param(p) => {
                self.tag(b"RP");
                self.u32(p);
            }
            Region::Var(v) => {
                self.tag(b"RV");
                self.u32(v.0);
            }
        }
    }

    fn hash_mutability(&mut self, m: Mutability) {
        match m {
            Mutability::Shared => self.tag(b"SH"),
            Mutability::Exclusive => self.tag(b"EX"),
        }
    }

    fn hash_args(&mut self, args: ArgsId) {
        let ga = self.plan.module.tys.args(args);
        self.u64(ga.tys.len() as u64);
        for &t in &ga.tys {
            self.hash_ty(t);
        }
        self.u64(ga.regions.len() as u64);
        for &r in &ga.regions {
            self.hash_region(r);
        }
    }

    fn hash_adt_def(&mut self, adt: AdtId) {
        let Some(def) = self.plan.module.tys.get_adt(adt) else {
            self.tag(b"ADT_NONE");
            self.u32(adt.0);
            return;
        };
        self.tag(b"ADT");
        self.str(&def.name);
        if !self.seen_adts.insert(adt) {
            self.tag(b"REC");
            return;
        }
        let kind_b = match def.kind {
            AdtKind::Record => 0,
            AdtKind::Package => 1,
            AdtKind::Resource => 2,
            AdtKind::Enum => 3,
        };
        self.tag(&[kind_b, u8::from(def.user_drop)]);
        self.u32(def.type_params);
        self.u32(def.region_params);
        self.u64(def.variants.len() as u64);
        for v in &def.variants {
            self.str(&v.name);
            self.u64(v.fields.len() as u64);
            for f in &v.fields {
                self.str(&f.name);
                self.hash_ty(f.ty);
            }
        }
        if let Some(&drop_idx) = self.plan.drop_fn_by_adt.get(&adt) {
            self.tag(b"DROP1");
            self.hash_fn_ref(FnId::from_index(drop_idx));
        } else {
            self.tag(b"DROP0");
        }
        self.seen_adts.remove(&adt);
    }

    fn hash_closure_def(&mut self, cid: ClosureId) {
        self.tag(b"CLS");
        if !self.seen_closures.insert(cid) {
            self.tag(b"REC");
            return;
        }
        if let Some(def) = self.plan.module.tys.closure(cid) {
            let kb = match def.kind {
                ClosureKind::Fn => 0,
                ClosureKind::FnMut => 1,
                ClosureKind::FnOnce => 2,
            };
            self.tag(&[kb]);
            self.u32(def.type_params);
            self.u64(def.captures.len() as u64);
            for &c in &def.captures {
                self.hash_ty(c);
            }
            self.u64(def.params.len() as u64);
            for &p in &def.params {
                self.hash_ty(p);
            }
            self.hash_ty(def.result);
        }
        if let Some(&cf_idx) = self.plan.closure_fn_by_def.get(&cid) {
            self.tag(b"CF1");
            self.hash_fn_ref(FnId::from_index(cf_idx));
        } else {
            self.tag(b"CF0");
        }
        self.seen_closures.remove(&cid);
    }

    fn hash_dyn_def(&mut self, did: DynId) {
        self.tag(b"DYN");
        if !self.seen_dyns.insert(did) {
            self.tag(b"REC");
            return;
        }
        if let Some(def) = self.plan.module.tys.get_dyn(did) {
            self.str(&def.name);
            self.u64(def.slots.len() as u64);
            for slot in &def.slots {
                self.str(&slot.name);
                match slot.receiver {
                    None => self.tag(b"N"),
                    Some(m) => {
                        self.tag(b"R");
                        self.hash_mutability(m);
                    }
                }
                self.u64(slot.params.len() as u64);
                for &p in &slot.params {
                    self.hash_ty(p);
                }
                self.hash_ty(slot.result);
            }
        }
        self.seen_dyns.remove(&did);
    }

    fn hash_int_ty(&mut self, i: IntTy) {
        let b = match i {
            IntTy::I8 => 0,
            IntTy::I16 => 1,
            IntTy::I32 => 2,
            IntTy::I64 => 3,
            IntTy::Isize => 4,
            IntTy::U8 => 5,
            IntTy::U16 => 6,
            IntTy::U32 => 7,
            IntTy::U64 => 8,
            IntTy::Usize => 9,
        };
        self.tag(&[b]);
    }

    fn hash_float_ty(&mut self, f: FloatTy) {
        let b = match f {
            FloatTy::F32 => 0,
            FloatTy::F64 => 1,
        };
        self.tag(&[b]);
    }

    fn hash_ty(&mut self, ty: TyId) {
        let Some(kind) = self.plan.module.tys.get(ty) else {
            self.tag(b"T_NONE");
            self.u32(ty.0);
            return;
        };
        match kind {
            TyKind::Unit => self.tag(b"TU"),
            TyKind::Bool => self.tag(b"TB"),
            TyKind::Never => self.tag(b"TN"),
            TyKind::Int(i) => {
                self.tag(b"TI");
                self.hash_int_ty(*i);
            }
            TyKind::Float(f) => {
                self.tag(b"TF");
                self.hash_float_ty(*f);
            }
            TyKind::Adt { adt, args } => {
                self.tag(b"TA");
                self.hash_adt_def(*adt);
                self.hash_args(*args);
            }
            TyKind::Tuple(ts) => {
                self.tag(b"TT");
                self.u64(ts.len() as u64);
                for &t in ts {
                    self.hash_ty(t);
                }
            }
            TyKind::Array { element, length } => {
                self.tag(b"TAR");
                self.u32(*length);
                self.hash_ty(*element);
            }
            TyKind::Slice(e) => {
                self.tag(b"TS");
                self.hash_ty(*e);
            }
            TyKind::Ref {
                region,
                mutability,
                pointee,
            } => {
                self.tag(b"TR");
                self.hash_region(*region);
                self.hash_mutability(*mutability);
                self.hash_ty(*pointee);
            }
            TyKind::Managed(inner) => {
                self.tag(b"TM");
                self.hash_ty(*inner);
            }
            TyKind::Desc(inner) => {
                self.tag(b"TD");
                self.hash_ty(*inner);
            }
            TyKind::ErrorUnion { ok, err } => {
                self.tag(b"TE");
                self.hash_ty(*ok);
                self.hash_ty(*err);
            }
            TyKind::FnPtr { params, result } => {
                self.tag(b"TFP");
                self.u64(params.len() as u64);
                for &p in params {
                    self.hash_ty(p);
                }
                self.hash_ty(*result);
            }
            TyKind::Closure { def, args } => {
                self.tag(b"TC");
                self.hash_closure_def(*def);
                self.hash_args(*args);
            }
            TyKind::Dyn { def, regions } => {
                self.tag(b"TDY");
                self.hash_dyn_def(*def);
                self.u64(regions.len() as u64);
                for &r in regions {
                    self.hash_region(r);
                }
            }
            TyKind::OwnedDyn {
                def,
                regions,
                context,
            } => {
                self.tag(b"TOD");
                self.hash_dyn_def(*def);
                self.u64(regions.len() as u64);
                for &r in regions {
                    self.hash_region(r);
                }
                match context {
                    Some(c) => {
                        self.tag(b"1");
                        self.hash_ty(*c);
                    }
                    None => self.tag(b"0"),
                }
            }
            TyKind::Param(p) => {
                self.tag(b"TP");
                self.u32(*p);
            }
            TyKind::Opaque(op) => {
                self.tag(b"TO");
                match op {
                    OpaqueTy::ByteBuffer => self.tag(b"BB"),
                    OpaqueTy::OwnedStore { element } => {
                        self.tag(b"OS");
                        self.hash_ty(*element);
                    }
                    OpaqueTy::Mutex { payload } => {
                        self.tag(b"MX");
                        self.hash_ty(*payload);
                    }
                    OpaqueTy::MutexGuard { payload, region } => {
                        self.tag(b"MG");
                        self.hash_ty(*payload);
                        self.hash_region(*region);
                    }
                    OpaqueTy::Registration { environment } => {
                        self.tag(b"RG");
                        self.hash_ty(*environment);
                    }
                    OpaqueTy::AsyncRegistration { environment } => {
                        self.tag(b"AR");
                        self.hash_ty(*environment);
                    }
                    OpaqueTy::Task { result, context } => {
                        self.tag(b"TK");
                        self.hash_ty(*result);
                        match context {
                            Some(c) => {
                                self.tag(b"1");
                                self.hash_ty(*c);
                            }
                            None => self.tag(b"0"),
                        }
                    }
                    OpaqueTy::ForeignHandle => self.tag(b"FH"),
                    OpaqueTy::CWideString => self.tag(b"CW"),
                    OpaqueTy::ForeignCallback { params, result } => {
                        self.tag(b"FC");
                        self.u64(params.len() as u64);
                        for &p in params {
                            self.hash_ty(p);
                        }
                        self.hash_ty(*result);
                    }
                    OpaqueTy::SyncCell(k) => {
                        self.tag(b"SC");
                        let kb = match k {
                            SyncCellKind::I32 => 0,
                            SyncCellKind::I64 => 1,
                            SyncCellKind::Usize => 2,
                        };
                        self.tag(&[kb]);
                    }
                }
            }
        }
    }

    fn hash_field_ref(&mut self, fr: FieldRef) {
        self.hash_adt_def(fr.adt);
        self.u32(fr.variant.0);
        self.u32(fr.field.0);
        self.hash_args(fr.args);
    }

    fn hash_place(&mut self, p: &Place) {
        match p.root {
            PlaceRoot::Local(l) => {
                self.tag(b"PL");
                self.u32(l.0);
            }
            PlaceRoot::Capture(c) => {
                self.tag(b"PC");
                self.u32(c);
            }
        }
        self.u64(p.projections.len() as u64);
        for proj in &p.projections {
            match *proj {
                Projection::Field(fr) => {
                    self.tag(b"PF");
                    self.hash_field_ref(fr);
                }
                Projection::TupleField(i) => {
                    self.tag(b"PT");
                    self.u32(i);
                }
                Projection::ErrorArm(ok) => {
                    self.tag(b"PE");
                    self.tag(&[u8::from(ok)]);
                }
                Projection::Index(l) => {
                    self.tag(b"PI");
                    self.u32(l.0);
                }
                Projection::ConstIndex(i) => {
                    self.tag(b"PX");
                    self.u32(i);
                }
                Projection::Deref => self.tag(b"PD"),
            }
        }
    }

    fn hash_operand(&mut self, op: &Operand) {
        match op {
            Operand::Copy(p) => {
                self.tag(b"OC");
                self.hash_place(p);
            }
            Operand::Move(p) => {
                self.tag(b"OM");
                self.hash_place(p);
            }
            Operand::Const(c) => {
                self.tag(b"OK");
                match c {
                    ConstValue::Unit => self.tag(b"U"),
                    ConstValue::Bool(b) => self.tag(&[1, u8::from(*b)]),
                    ConstValue::Int { ty, bits } => {
                        self.tag(b"I");
                        self.hash_int_ty(*ty);
                        self.u128(*bits);
                    }
                    ConstValue::Float { ty, bits } => {
                        self.tag(b"F");
                        self.hash_float_ty(*ty);
                        self.u64(*bits);
                    }
                }
            }
            Operand::Pooled(cid) => {
                self.tag(b"OP");
                self.u32(cid.0);
            }
        }
    }

    fn hash_rvalue(&mut self, rv: &Rvalue) {
        match rv {
            Rvalue::Use(op) => {
                self.tag(b"VU");
                self.hash_operand(op);
            }
            Rvalue::Ref {
                region,
                mutability,
                place,
            } => {
                self.tag(b"VR");
                self.hash_region(*region);
                self.hash_mutability(*mutability);
                self.hash_place(place);
            }
            Rvalue::SliceRef {
                region,
                mutability,
                place,
                start,
                end,
            } => {
                self.tag(b"VS");
                self.hash_region(*region);
                self.hash_mutability(*mutability);
                self.hash_place(place);
                match start {
                    Some(s) => {
                        self.tag(b"1");
                        self.hash_operand(s);
                    }
                    None => self.tag(b"0"),
                }
                match end {
                    Some(e) => {
                        self.tag(b"1");
                        self.hash_operand(e);
                    }
                    None => self.tag(b"0"),
                }
            }
            Rvalue::Describe { place } => {
                self.tag(b"VD");
                self.hash_place(place);
            }
            Rvalue::Aggregate { kind, operands } => {
                self.tag(b"VA");
                match kind {
                    AggregateKind::Tuple => self.tag(b"T"),
                    AggregateKind::Adt { adt, variant, args } => {
                        self.tag(b"A");
                        self.hash_adt_def(*adt);
                        self.u32(variant.0);
                        self.hash_args(*args);
                    }
                    AggregateKind::Array { element } => {
                        self.tag(b"R");
                        self.hash_ty(*element);
                    }
                    AggregateKind::Closure { def, args } => {
                        self.tag(b"C");
                        self.hash_closure_def(*def);
                        self.hash_args(*args);
                    }
                }
                self.u64(operands.len() as u64);
                for o in operands {
                    self.hash_operand(o);
                }
            }
            Rvalue::ErrorUnion { ok, value, ty } => {
                self.tag(b"VE");
                self.tag(&[u8::from(*ok)]);
                self.hash_operand(value);
                self.hash_ty(*ty);
            }
            Rvalue::Unary { op, operand } => {
                self.tag(b"VN");
                let b = match op {
                    UnOp::Not => 0,
                    UnOp::Neg => 1,
                    UnOp::WrappingNeg => 2,
                };
                self.tag(&[b]);
                self.hash_operand(operand);
            }
            Rvalue::Binary { op, lhs, rhs } => {
                self.tag(b"VB");
                let b = match op {
                    BinOp::Add => 0,
                    BinOp::Sub => 1,
                    BinOp::Mul => 2,
                    BinOp::Div => 3,
                    BinOp::Rem => 4,
                    BinOp::WrappingAdd => 5,
                    BinOp::WrappingSub => 6,
                    BinOp::WrappingMul => 7,
                    BinOp::BitAnd => 8,
                    BinOp::BitOr => 9,
                    BinOp::BitXor => 10,
                    BinOp::Shl => 11,
                    BinOp::Shr => 12,
                    BinOp::Eq => 13,
                    BinOp::Ne => 14,
                    BinOp::Lt => 15,
                    BinOp::Le => 16,
                    BinOp::Gt => 17,
                    BinOp::Ge => 18,
                };
                self.tag(&[b]);
                self.hash_operand(lhs);
                self.hash_operand(rhs);
            }
            Rvalue::Convert { operand, to } => {
                self.tag(b"VC");
                self.hash_operand(operand);
                self.hash_ty(*to);
            }
            Rvalue::Discriminant(p) => {
                self.tag(b"VDI");
                self.hash_place(p);
            }
            Rvalue::ErrorArm(p) => {
                self.tag(b"VEA");
                self.hash_place(p);
            }
            Rvalue::Len(p) => {
                self.tag(b"VL");
                self.hash_place(p);
            }
            Rvalue::FnRef(fid) => {
                self.tag(b"VFR");
                self.hash_fn_ref(*fid);
            }
            Rvalue::ForeignFnRef(fid) => {
                self.tag(b"VFF");
                self.hash_foreign_ref(*fid);
            }
            Rvalue::StaticUtf(sid) => {
                self.tag(b"VST");
                self.hash_static_ref(*sid);
            }
        }
    }

    fn hash_body(&mut self, body: &Body) {
        self.tag(b"BODY");
        self.u32(body.param_count);
        self.u64(body.locals.len() as u64);
        for l in &body.locals {
            self.hash_ty(l.ty);
        }
        self.u64(body.captures.len() as u64);
        for &c in &body.captures {
            self.hash_ty(c);
        }
        match body.closure {
            Some(cid) => {
                self.tag(b"1");
                self.hash_closure_def(cid);
            }
            None => self.tag(b"0"),
        }
        self.hash_ty(body.result);
        self.u64(body.drop_flags.len() as u64);
        for df in &body.drop_flags {
            self.u32(df.flag.0);
            self.hash_place(&df.place);
        }
        self.u64(body.blocks.len() as u64);
        for blk in &body.blocks {
            self.hash_block(body, blk);
        }
    }

    fn hash_block(&mut self, body: &Body, blk: &Block) {
        self.tag(b"BLK");
        self.u64(blk.instructions.len() as u64);
        for inst in &blk.instructions {
            match &inst.kind {
                InstructionKind::Assign { dest, value } => {
                    self.tag(b"IA");
                    self.hash_place(dest);
                    self.hash_rvalue(value);
                }
                InstructionKind::Drop { place } => {
                    self.tag(b"ID");
                    self.hash_place(place);
                }
                InstructionKind::Replace { place, value, old } => {
                    self.tag(b"IR");
                    self.hash_place(place);
                    self.hash_operand(value);
                    self.u32(old.0);
                }
                InstructionKind::StorageLive(l) => {
                    self.tag(b"SL");
                    self.u32(l.0);
                }
                InstructionKind::StorageDead(l) => {
                    self.tag(b"SD");
                    self.u32(l.0);
                }
                InstructionKind::ScopeEnter(s) => {
                    self.tag(b"SE");
                    self.u32(s.0);
                }
                InstructionKind::ScopeExit { scope, exit } => {
                    self.tag(b"SX");
                    self.u32(scope.0);
                    let eb = match exit {
                        ScopeExit::Fallthrough => 0,
                        ScopeExit::Return => 1,
                        ScopeExit::Error => 2,
                        ScopeExit::Break => 3,
                        ScopeExit::Continue => 4,
                    };
                    self.tag(&[eb]);
                }
                InstructionKind::RegisterDefer { scope, closure } => {
                    self.tag(b"RD");
                    self.u32(scope.0);
                    self.hash_operand(closure);
                }
            }
        }
        match &blk.terminator {
            Terminator::Goto { target } => {
                self.tag(b"TG");
                self.u32(target.0);
            }
            Terminator::Switch {
                discr,
                arms,
                otherwise,
            } => {
                self.tag(b"TS");
                self.hash_operand(discr);
                self.u64(arms.len() as u64);
                for &(val, tgt) in arms {
                    self.u128(val);
                    self.u32(tgt.0);
                }
                self.u32(otherwise.0);
            }
            Terminator::Return { value } => {
                self.tag(b"TR");
                self.hash_operand(value);
            }
            Terminator::Call {
                callee,
                args,
                dest,
                target,
                cancel,
            } => {
                self.tag(b"TC");
                match callee {
                    Callee::Direct(fid) => {
                        self.tag(b"D");
                        self.hash_fn_ref(*fid);
                    }
                    Callee::Indirect(op) => {
                        self.tag(b"I");
                        self.hash_operand(op);
                    }
                    Callee::Closure { closure, mode } => {
                        self.tag(b"C");
                        self.hash_place(closure);
                        let mb = match mode {
                            ClosureCallMode::Shared => 0,
                            ClosureCallMode::Mut => 1,
                            ClosureCallMode::Once => 2,
                        };
                        self.tag(&[mb]);
                    }
                    Callee::DynSlot { object, slot } => {
                        self.tag(b"Y");
                        self.hash_operand(object);
                        self.u32(*slot);
                    }
                    Callee::Foreign(fid) => {
                        self.tag(b"F");
                        self.hash_foreign_ref(*fid);
                    }
                    Callee::Intrinsic(op) => {
                        self.tag(b"N");
                        self.hash_intrinsic_op(*op);
                        // OwnedDynCreate embeds `@__zil_vtable_{vt_idx}` by numeric index into
                        // `module.dyn_impls`; bind that exact vtable index & method count!
                        if matches!(op, IntrinsicOp::OwnedDynCreate) {
                            self.hash_owned_dyn_create_vtable(body, dest, args);
                        }
                    }
                }
                self.u64(args.len() as u64);
                for a in args {
                    self.hash_operand(a);
                }
                self.hash_place(dest);
                self.u32(target.0);
                match cancel {
                    Some(c) => {
                        self.tag(b"1");
                        self.u32(c.0);
                    }
                    None => self.tag(b"0"),
                }
            }
            Terminator::Assert {
                cond,
                expected,
                target,
                trap,
            } => {
                self.tag(b"TA");
                self.hash_operand(cond);
                self.tag(&[u8::from(*expected)]);
                self.u32(target.0);
                self.hash_trap(*trap);
            }
            Terminator::Unreachable => self.tag(b"TU"),
            Terminator::Trap(t) => {
                self.tag(b"TT");
                self.hash_trap(*t);
            }
        }
    }

    fn hash_memory_order(&mut self, ord: MemoryOrder) {
        let b = match ord {
            MemoryOrder::Relaxed => 0,
            MemoryOrder::Acquire => 1,
            MemoryOrder::Release => 2,
            MemoryOrder::AcqRel => 3,
            MemoryOrder::SeqCst => 4,
        };
        self.tag(&[b]);
    }

    fn hash_intrinsic_op(&mut self, op: IntrinsicOp) {
        match op {
            IntrinsicOp::StoreNew => self.tag(&[0]),
            IntrinsicOp::StoreReserve => self.tag(&[1]),
            IntrinsicOp::StorePush => self.tag(&[2]),
            IntrinsicOp::StorePop => self.tag(&[3]),
            IntrinsicOp::StoreLen => self.tag(&[4]),
            IntrinsicOp::StoreCapacity => self.tag(&[5]),
            IntrinsicOp::StoreBorrow { mutability } => {
                self.tag(&[6]);
                self.hash_mutability(mutability);
            }
            IntrinsicOp::ByteBufferNew => self.tag(&[7]),
            IntrinsicOp::ByteBufferReserve => self.tag(&[8]),
            IntrinsicOp::ByteBufferPush => self.tag(&[9]),
            IntrinsicOp::ByteBufferTruncate => self.tag(&[10]),
            IntrinsicOp::ByteBufferLen => self.tag(&[11]),
            IntrinsicOp::ByteBufferCapacity => self.tag(&[12]),
            IntrinsicOp::ByteBufferBorrow => self.tag(&[13]),
            IntrinsicOp::MutexCreate => self.tag(&[14]),
            IntrinsicOp::MutexLock => self.tag(&[15]),
            IntrinsicOp::MutexValue => self.tag(&[16]),
            IntrinsicOp::MutexUnlock => self.tag(&[17]),
            IntrinsicOp::SyncLoad => self.tag(&[18]),
            IntrinsicOp::SyncStore => self.tag(&[19]),
            IntrinsicOp::SyncFetchAdd => self.tag(&[20]),
            IntrinsicOp::SyncCompareExchange => self.tag(&[21]),
            IntrinsicOp::TaskStart => self.tag(&[22]),
            IntrinsicOp::TaskCreate => self.tag(&[23]),
            IntrinsicOp::TaskYield => self.tag(&[24]),
            IntrinsicOp::TaskValid => self.tag(&[25]),
            IntrinsicOp::TaskPoll => self.tag(&[26]),
            IntrinsicOp::TaskRequestCancel => self.tag(&[27]),
            IntrinsicOp::TaskPollClose => self.tag(&[28]),
            IntrinsicOp::TaskFinishDestroy => self.tag(&[29]),
            IntrinsicOp::RegistrationStart => self.tag(&[30]),
            IntrinsicOp::RegistrationValid => self.tag(&[31]),
            IntrinsicOp::RegistrationClose => self.tag(&[32]),
            IntrinsicOp::AsyncRegistrationStart => self.tag(&[33]),
            IntrinsicOp::AsyncRegistrationValid => self.tag(&[34]),
            IntrinsicOp::AsyncRegistrationRequestClose => self.tag(&[35]),
            IntrinsicOp::AsyncRegistrationPollClose => self.tag(&[36]),
            IntrinsicOp::AsyncRegistrationFinishDestroy => self.tag(&[37]),
            IntrinsicOp::ManagedCreate => self.tag(&[38]),
            IntrinsicOp::OwnedDynCreate => self.tag(&[39]),
            IntrinsicOp::ThreadScope => self.tag(&[40]),
            IntrinsicOp::SyncInit => self.tag(&[41]),
            IntrinsicOp::ScopedCallback => self.tag(&[42]),
            IntrinsicOp::SyncLoadOrdered(ord) => {
                self.tag(&[43]);
                self.hash_memory_order(ord);
            }
            IntrinsicOp::SyncStoreOrdered(ord) => {
                self.tag(&[44]);
                self.hash_memory_order(ord);
            }
            IntrinsicOp::SyncFetchAddOrdered(ord) => {
                self.tag(&[45]);
                self.hash_memory_order(ord);
            }
            IntrinsicOp::SyncExchangeOrdered(ord) => {
                self.tag(&[46]);
                self.hash_memory_order(ord);
            }
            IntrinsicOp::SyncCompareExchangeOrdered { success, failure } => {
                self.tag(&[47]);
                self.hash_memory_order(success);
                self.hash_memory_order(failure);
            }
            IntrinsicOp::CNull => self.tag(&[48]),
            IntrinsicOp::CCallback => self.tag(&[49]),
        }
    }

    fn hash_trap(&mut self, trap: TrapKind) {
        let b = match trap {
            TrapKind::Bounds => 0,
            TrapKind::Overflow => 1,
            TrapKind::DivideByZero => 2,
            TrapKind::Conversion => 3,
            TrapKind::Explicit => 4,
            TrapKind::Cancel => 5,
        };
        self.tag(&[b]);
    }

    fn hash_owned_dyn_create_vtable(&mut self, body: &Body, dest: &Place, args: &[Operand]) {
        let dest_ty = if dest.projections.is_empty() {
            match dest.root {
                PlaceRoot::Local(l) => body.locals.get(l.index()).map(|d| d.ty),
                PlaceRoot::Capture(i) => body.captures.get(i as usize).copied(),
            }
        } else {
            None
        };
        let arg0_ty = args.first().and_then(|a| match a {
            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => match p.root {
                PlaceRoot::Local(l) => body.locals.get(l.index()).map(|d| d.ty),
                PlaceRoot::Capture(i) => body.captures.get(i as usize).copied(),
            },
            Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => None,
        });
        if let (Some(dt), Some(at)) = (dest_ty, arg0_ty)
            && let Some(TyKind::ErrorUnion { ok, .. }) = self.plan.module.tys.get(dt)
            && let Some(TyKind::Managed(inner)) = self.plan.module.tys.get(*ok)
            && let Some(TyKind::OwnedDyn { def, .. }) = self.plan.module.tys.get(*inner)
            && let Some(vt_idx) = self
                .plan
                .module
                .dyn_impls
                .iter()
                .position(|d| d.dyn_id == *def && d.concrete_ty == at)
        {
            self.tag(b"ODVT");
            self.u64(vt_idx as u64);
            self.u64(self.plan.module.dyn_impls[vt_idx].methods.len() as u64);
            return;
        }
        // Conservative fallback for projected places: hash the full dyn_impl table order.
        self.tag(b"ODVT_ALL");
        self.u64(self.plan.module.dyn_impls.len() as u64);
        for (i, d) in self.plan.module.dyn_impls.iter().enumerate() {
            self.u64(i as u64);
            self.hash_dyn_def(d.dyn_id);
            self.hash_ty(d.concrete_ty);
            self.u64(d.methods.len() as u64);
        }
    }
}
