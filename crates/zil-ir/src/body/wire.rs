//! Canonical binary wire codec for Body IR [`Module`] and [`Certificate`].
//!
//! Every enum tag is encoded and decoded exhaustively (no wildcard arms).
//! Decoding enforces strict size limits and rejects truncated, malformed, or
//! trailing-byte inputs without panicking (`SafeReject`).

use super::cert::{
    BlockInvariant, Certificate, FlagClaim, FunctionCertificate, InitClaim, LoanClaim, LoanOrigin,
    PlaceInit, TagClaim, VariantClaim,
};
use super::ids::{
    AdtId, ArgsId, BlockId, ClosureId, ConstId, DynId, FieldIdx, FnId, ForeignId, Local, RegionVid,
    ScopeId, StaticId, TyId, VariantIdx,
};
use super::inst::{
    AggregateKind, BinOp, Block, Body, Callee, ClosureCallMode, ConstValue, DropFlag, FnRole,
    FnSig, ForeignAbi, ForeignDecl, Function, Instruction, InstructionKind, IntrinsicOp, LocalDecl,
    Module, Operand, Rvalue, ScopeExit, Terminator, TrapKind, UnOp,
};
use super::place::{FieldRef, Place, PlaceRoot, Projection};
use super::source_map::OpLocation;
use super::ty::{
    AdtDef, AdtKind, ClosureDef, ClosureKind, DynDef, DynSlotSig, FieldDef, FloatTy, GenericArgs,
    IntTy, Mutability, OpaqueTy, ParamEnv, Region, SyncCellKind, TyKind, TyTable, VariantDef,
};
use zil_base::{FileId, Span};

const MODULE_MAGIC: &[u8; 4] = b"ZIRB";
const CERT_MAGIC: &[u8; 4] = b"ZIRC";
const WIRE_VERSION: u32 = 1;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SEQ_LEN: usize = 65_536;
const MAX_STRING_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    InputTooLarge,
    BadMagic,
    UnsupportedVersion(u32),
    UnexpectedEof,
    InvalidTag(&'static str, u8),
    InvalidBool(u8),
    InvalidUtf8,
    SequenceTooLong(usize),
    TrailingBytes(usize),
}

pub fn encode_module(module: &Module) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(MODULE_MAGIC);
    w.u32(WIRE_VERSION);
    let parts = module.tys.raw_parts();
    w.vec(parts.kinds, encode_ty_kind);
    w.vec(parts.args, encode_generic_args);
    w.vec(parts.adts, encode_adt_def);
    w.vec(parts.closures, encode_closure_def);
    w.vec(parts.dyns, encode_dyn_def);
    w.vec(&module.functions, encode_function);
    w.vec(&module.foreigns, encode_foreign_decl);
    w.vec(&module.statics, |w, b| w.byte_vec(b));
    w.out
}

pub fn decode_module(bytes: &[u8]) -> Result<Module, WireError> {
    let mut r = Reader::new(bytes)?;
    r.expect_magic(MODULE_MAGIC)?;
    r.expect_version()?;
    let kinds = r.vec(decode_ty_kind)?;
    let args = r.vec(decode_generic_args)?;
    let adts = r.vec(decode_adt_def)?;
    let closures = r.vec(decode_closure_def)?;
    let dyns = r.vec(decode_dyn_def)?;
    let functions = r.vec(decode_function)?;
    let foreigns = r.vec(decode_foreign_decl)?;
    let statics = r.vec(|r| r.byte_vec())?;
    r.finish()?;
    Ok(Module {
        tys: TyTable::from_raw_parts(kinds, args, adts, closures, dyns),
        functions,
        foreigns,
        statics,
    })
}

pub fn encode_certificate(cert: &Certificate) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(CERT_MAGIC);
    w.u32(WIRE_VERSION);
    w.vec(&cert.functions, encode_fn_cert);
    w.out
}

pub fn decode_certificate(bytes: &[u8]) -> Result<Certificate, WireError> {
    let mut r = Reader::new(bytes)?;
    r.expect_magic(CERT_MAGIC)?;
    r.expect_version()?;
    let functions = r.vec(decode_fn_cert)?;
    r.finish()?;
    Ok(Certificate { functions })
}

// ---- primitive writer / reader ---------------------------------------------

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u128(&mut self, v: u128) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.byte_vec(s.as_bytes());
    }
    fn byte_vec(&mut self, b: &[u8]) {
        self.u32(u32::try_from(b.len()).expect("wire byte_vec len"));
        self.bytes(b);
    }
    fn vec<T>(&mut self, items: &[T], f: impl Fn(&mut Self, &T)) {
        self.u32(u32::try_from(items.len()).expect("wire seq len"));
        for item in items {
            f(self, item);
        }
    }
    fn opt<T>(&mut self, item: Option<&T>, f: impl Fn(&mut Self, &T)) {
        match item {
            None => self.u8(0),
            Some(v) => {
                self.u8(1);
                f(self, v);
            }
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Result<Self, WireError> {
        if data.len() > MAX_INPUT_BYTES {
            return Err(WireError::InputTooLarge);
        }
        Ok(Self { data, pos: 0 })
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).ok_or(WireError::UnexpectedEof)?;
        if end > self.data.len() {
            return Err(WireError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
    fn expect_magic(&mut self, magic: &[u8; 4]) -> Result<(), WireError> {
        let got = self.take(4)?;
        if got != magic {
            return Err(WireError::BadMagic);
        }
        Ok(())
    }
    fn expect_version(&mut self) -> Result<(), WireError> {
        let v = self.u32()?;
        if v != WIRE_VERSION {
            return Err(WireError::UnsupportedVersion(v));
        }
        Ok(())
    }
    fn finish(&self) -> Result<(), WireError> {
        let rem = self.data.len().saturating_sub(self.pos);
        if rem != 0 {
            return Err(WireError::TrailingBytes(rem));
        }
        Ok(())
    }
    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, WireError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(WireError::InvalidBool(b)),
        }
    }
    fn u32(&mut self) -> Result<u32, WireError> {
        let b: [u8; 4] = self.take(4)?.try_into().expect("4 bytes");
        Ok(u32::from_le_bytes(b))
    }
    fn u64(&mut self) -> Result<u64, WireError> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("8 bytes");
        Ok(u64::from_le_bytes(b))
    }
    fn u128(&mut self) -> Result<u128, WireError> {
        let b: [u8; 16] = self.take(16)?.try_into().expect("16 bytes");
        Ok(u128::from_le_bytes(b))
    }
    fn byte_vec(&mut self) -> Result<Vec<u8>, WireError> {
        let len = self.u32()? as usize;
        if len > MAX_STRING_BYTES {
            return Err(WireError::SequenceTooLong(len));
        }
        Ok(self.take(len)?.to_vec())
    }
    fn string(&mut self) -> Result<String, WireError> {
        let bytes = self.byte_vec()?;
        String::from_utf8(bytes).map_err(|_| WireError::InvalidUtf8)
    }
    fn vec<T>(
        &mut self,
        f: impl Fn(&mut Self) -> Result<T, WireError>,
    ) -> Result<Vec<T>, WireError> {
        let len = self.u32()? as usize;
        if len > MAX_SEQ_LEN {
            return Err(WireError::SequenceTooLong(len));
        }
        let mut out = Vec::with_capacity(len.min(256));
        for _ in 0..len {
            out.push(f(self)?);
        }
        Ok(out)
    }
    fn opt<T>(
        &mut self,
        f: impl Fn(&mut Self) -> Result<T, WireError>,
    ) -> Result<Option<T>, WireError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(f(self)?)),
            b => Err(WireError::InvalidTag("Option", b)),
        }
    }
}

// ---- types -----------------------------------------------------------------

fn encode_int_ty(w: &mut Writer, t: IntTy) {
    w.u8(match t {
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
    });
}
fn decode_int_ty(r: &mut Reader<'_>) -> Result<IntTy, WireError> {
    match r.u8()? {
        0 => Ok(IntTy::I8),
        1 => Ok(IntTy::I16),
        2 => Ok(IntTy::I32),
        3 => Ok(IntTy::I64),
        4 => Ok(IntTy::Isize),
        5 => Ok(IntTy::U8),
        6 => Ok(IntTy::U16),
        7 => Ok(IntTy::U32),
        8 => Ok(IntTy::U64),
        9 => Ok(IntTy::Usize),
        t => Err(WireError::InvalidTag("IntTy", t)),
    }
}

fn encode_float_ty(w: &mut Writer, t: FloatTy) {
    w.u8(match t {
        FloatTy::F32 => 0,
        FloatTy::F64 => 1,
    });
}
fn decode_float_ty(r: &mut Reader<'_>) -> Result<FloatTy, WireError> {
    match r.u8()? {
        0 => Ok(FloatTy::F32),
        1 => Ok(FloatTy::F64),
        t => Err(WireError::InvalidTag("FloatTy", t)),
    }
}

fn encode_mutability(w: &mut Writer, m: Mutability) {
    w.u8(match m {
        Mutability::Shared => 0,
        Mutability::Exclusive => 1,
    });
}
fn decode_mutability(r: &mut Reader<'_>) -> Result<Mutability, WireError> {
    match r.u8()? {
        0 => Ok(Mutability::Shared),
        1 => Ok(Mutability::Exclusive),
        t => Err(WireError::InvalidTag("Mutability", t)),
    }
}

fn encode_region(w: &mut Writer, reg: &Region) {
    match reg {
        Region::Static => w.u8(0),
        Region::Param(i) => {
            w.u8(1);
            w.u32(*i);
        }
        Region::Var(v) => {
            w.u8(2);
            w.u32(v.0);
        }
    }
}
fn decode_region(r: &mut Reader<'_>) -> Result<Region, WireError> {
    match r.u8()? {
        0 => Ok(Region::Static),
        1 => Ok(Region::Param(r.u32()?)),
        2 => Ok(Region::Var(RegionVid(r.u32()?))),
        t => Err(WireError::InvalidTag("Region", t)),
    }
}

fn encode_sync_cell(w: &mut Writer, k: SyncCellKind) {
    w.u8(match k {
        SyncCellKind::I32 => 0,
        SyncCellKind::I64 => 1,
        SyncCellKind::Usize => 2,
    });
}
fn decode_sync_cell(r: &mut Reader<'_>) -> Result<SyncCellKind, WireError> {
    match r.u8()? {
        0 => Ok(SyncCellKind::I32),
        1 => Ok(SyncCellKind::I64),
        2 => Ok(SyncCellKind::Usize),
        t => Err(WireError::InvalidTag("SyncCellKind", t)),
    }
}

fn encode_opaque(w: &mut Writer, o: &OpaqueTy) {
    match o {
        OpaqueTy::ByteBuffer => w.u8(0),
        OpaqueTy::OwnedStore { element } => {
            w.u8(1);
            w.u32(element.0);
        }
        OpaqueTy::Mutex { payload } => {
            w.u8(2);
            w.u32(payload.0);
        }
        OpaqueTy::MutexGuard { payload, region } => {
            w.u8(3);
            w.u32(payload.0);
            encode_region(w, region);
        }
        OpaqueTy::Registration { environment } => {
            w.u8(4);
            w.u32(environment.0);
        }
        OpaqueTy::AsyncRegistration { environment } => {
            w.u8(5);
            w.u32(environment.0);
        }
        OpaqueTy::Task { result } => {
            w.u8(6);
            w.u32(result.0);
        }
        OpaqueTy::ForeignHandle => w.u8(7),
        OpaqueTy::CWideString => w.u8(8),
        OpaqueTy::ForeignCallback { params, result } => {
            w.u8(9);
            w.vec(params, |w, t| w.u32(t.0));
            w.u32(result.0);
        }
        OpaqueTy::SyncCell(k) => {
            w.u8(10);
            encode_sync_cell(w, *k);
        }
    }
}
fn decode_opaque(r: &mut Reader<'_>) -> Result<OpaqueTy, WireError> {
    match r.u8()? {
        0 => Ok(OpaqueTy::ByteBuffer),
        1 => Ok(OpaqueTy::OwnedStore {
            element: TyId(r.u32()?),
        }),
        2 => Ok(OpaqueTy::Mutex {
            payload: TyId(r.u32()?),
        }),
        3 => Ok(OpaqueTy::MutexGuard {
            payload: TyId(r.u32()?),
            region: decode_region(r)?,
        }),
        4 => Ok(OpaqueTy::Registration {
            environment: TyId(r.u32()?),
        }),
        5 => Ok(OpaqueTy::AsyncRegistration {
            environment: TyId(r.u32()?),
        }),
        6 => Ok(OpaqueTy::Task {
            result: TyId(r.u32()?),
        }),
        7 => Ok(OpaqueTy::ForeignHandle),
        8 => Ok(OpaqueTy::CWideString),
        9 => Ok(OpaqueTy::ForeignCallback {
            params: r.vec(|r| Ok(TyId(r.u32()?)))?,
            result: TyId(r.u32()?),
        }),
        10 => Ok(OpaqueTy::SyncCell(decode_sync_cell(r)?)),
        t => Err(WireError::InvalidTag("OpaqueTy", t)),
    }
}

fn encode_ty_kind(w: &mut Writer, k: &TyKind) {
    match k {
        TyKind::Unit => w.u8(0),
        TyKind::Bool => w.u8(1),
        TyKind::Never => w.u8(2),
        TyKind::Int(i) => {
            w.u8(3);
            encode_int_ty(w, *i);
        }
        TyKind::Float(f) => {
            w.u8(4);
            encode_float_ty(w, *f);
        }
        TyKind::Adt { adt, args } => {
            w.u8(5);
            w.u32(adt.0);
            w.u32(args.0);
        }
        TyKind::Tuple(items) => {
            w.u8(6);
            w.vec(items, |w, t| w.u32(t.0));
        }
        TyKind::Array { element, length } => {
            w.u8(7);
            w.u32(element.0);
            w.u32(*length);
        }
        TyKind::Slice(e) => {
            w.u8(8);
            w.u32(e.0);
        }
        TyKind::Ref {
            region,
            mutability,
            pointee,
        } => {
            w.u8(9);
            encode_region(w, region);
            encode_mutability(w, *mutability);
            w.u32(pointee.0);
        }
        TyKind::Managed(t) => {
            w.u8(10);
            w.u32(t.0);
        }
        TyKind::Desc(t) => {
            w.u8(11);
            w.u32(t.0);
        }
        TyKind::ErrorUnion { ok, err } => {
            w.u8(12);
            w.u32(ok.0);
            w.u32(err.0);
        }
        TyKind::FnPtr { params, result } => {
            w.u8(13);
            w.vec(params, |w, t| w.u32(t.0));
            w.u32(result.0);
        }
        TyKind::Closure { def, args } => {
            w.u8(14);
            w.u32(def.0);
            w.u32(args.0);
        }
        TyKind::Dyn(d) => {
            w.u8(15);
            w.u32(d.0);
        }
        TyKind::OwnedDyn(d) => {
            w.u8(16);
            w.u32(d.0);
        }
        TyKind::Param(p) => {
            w.u8(17);
            w.u32(*p);
        }
        TyKind::Opaque(o) => {
            w.u8(18);
            encode_opaque(w, o);
        }
    }
}
fn decode_ty_kind(r: &mut Reader<'_>) -> Result<TyKind, WireError> {
    match r.u8()? {
        0 => Ok(TyKind::Unit),
        1 => Ok(TyKind::Bool),
        2 => Ok(TyKind::Never),
        3 => Ok(TyKind::Int(decode_int_ty(r)?)),
        4 => Ok(TyKind::Float(decode_float_ty(r)?)),
        5 => Ok(TyKind::Adt {
            adt: AdtId(r.u32()?),
            args: ArgsId(r.u32()?),
        }),
        6 => Ok(TyKind::Tuple(r.vec(|r| Ok(TyId(r.u32()?)))?)),
        7 => Ok(TyKind::Array {
            element: TyId(r.u32()?),
            length: r.u32()?,
        }),
        8 => Ok(TyKind::Slice(TyId(r.u32()?))),
        9 => Ok(TyKind::Ref {
            region: decode_region(r)?,
            mutability: decode_mutability(r)?,
            pointee: TyId(r.u32()?),
        }),
        10 => Ok(TyKind::Managed(TyId(r.u32()?))),
        11 => Ok(TyKind::Desc(TyId(r.u32()?))),
        12 => Ok(TyKind::ErrorUnion {
            ok: TyId(r.u32()?),
            err: TyId(r.u32()?),
        }),
        13 => Ok(TyKind::FnPtr {
            params: r.vec(|r| Ok(TyId(r.u32()?)))?,
            result: TyId(r.u32()?),
        }),
        14 => Ok(TyKind::Closure {
            def: ClosureId(r.u32()?),
            args: ArgsId(r.u32()?),
        }),
        15 => Ok(TyKind::Dyn(DynId(r.u32()?))),
        16 => Ok(TyKind::OwnedDyn(DynId(r.u32()?))),
        17 => Ok(TyKind::Param(r.u32()?)),
        18 => Ok(TyKind::Opaque(decode_opaque(r)?)),
        t => Err(WireError::InvalidTag("TyKind", t)),
    }
}

fn encode_generic_args(w: &mut Writer, a: &GenericArgs) {
    w.vec(&a.tys, |w, t| w.u32(t.0));
    w.vec(&a.regions, encode_region);
}
fn decode_generic_args(r: &mut Reader<'_>) -> Result<GenericArgs, WireError> {
    Ok(GenericArgs {
        tys: r.vec(|r| Ok(TyId(r.u32()?)))?,
        regions: r.vec(decode_region)?,
    })
}

fn encode_adt_def(w: &mut Writer, d: &AdtDef) {
    w.str(&d.name);
    w.u8(match d.kind {
        AdtKind::Record => 0,
        AdtKind::Package => 1,
        AdtKind::Resource => 2,
        AdtKind::Enum => 3,
    });
    w.u32(d.type_params);
    w.u32(d.region_params);
    w.vec(&d.variants, |w, v| {
        w.str(&v.name);
        w.vec(&v.fields, |w, f| {
            w.str(&f.name);
            w.u32(f.ty.0);
        });
    });
    w.bool(d.user_drop);
}
fn decode_adt_def(r: &mut Reader<'_>) -> Result<AdtDef, WireError> {
    let name = r.string()?;
    let kind = match r.u8()? {
        0 => AdtKind::Record,
        1 => AdtKind::Package,
        2 => AdtKind::Resource,
        3 => AdtKind::Enum,
        t => return Err(WireError::InvalidTag("AdtKind", t)),
    };
    let type_params = r.u32()?;
    let region_params = r.u32()?;
    let variants = r.vec(|r| {
        let name = r.string()?;
        let fields = r.vec(|r| {
            Ok(FieldDef {
                name: r.string()?,
                ty: TyId(r.u32()?),
            })
        })?;
        Ok(VariantDef { name, fields })
    })?;
    let user_drop = r.bool()?;
    Ok(AdtDef {
        name,
        kind,
        type_params,
        region_params,
        variants,
        user_drop,
    })
}

fn encode_closure_def(w: &mut Writer, d: &ClosureDef) {
    w.u8(match d.kind {
        ClosureKind::Fn => 0,
        ClosureKind::FnMut => 1,
        ClosureKind::FnOnce => 2,
    });
    w.u32(d.type_params);
    w.vec(&d.captures, |w, t| w.u32(t.0));
    w.vec(&d.params, |w, t| w.u32(t.0));
    w.u32(d.result.0);
}
fn decode_closure_def(r: &mut Reader<'_>) -> Result<ClosureDef, WireError> {
    let kind = match r.u8()? {
        0 => ClosureKind::Fn,
        1 => ClosureKind::FnMut,
        2 => ClosureKind::FnOnce,
        t => return Err(WireError::InvalidTag("ClosureKind", t)),
    };
    Ok(ClosureDef {
        kind,
        type_params: r.u32()?,
        captures: r.vec(|r| Ok(TyId(r.u32()?)))?,
        params: r.vec(|r| Ok(TyId(r.u32()?)))?,
        result: TyId(r.u32()?),
    })
}

fn encode_dyn_def(w: &mut Writer, d: &DynDef) {
    w.str(&d.name);
    w.vec(&d.slots, |w, s| {
        w.str(&s.name);
        w.opt(s.receiver.as_ref(), |w, m| encode_mutability(w, *m));
        w.vec(&s.params, |w, t| w.u32(t.0));
        w.u32(s.result.0);
    });
}
fn decode_dyn_def(r: &mut Reader<'_>) -> Result<DynDef, WireError> {
    let name = r.string()?;
    let slots = r.vec(|r| {
        Ok(DynSlotSig {
            name: r.string()?,
            receiver: r.opt(decode_mutability)?,
            params: r.vec(|r| Ok(TyId(r.u32()?)))?,
            result: TyId(r.u32()?),
        })
    })?;
    Ok(DynDef { name, slots })
}

// ---- places, instructions, bodies ------------------------------------------

fn encode_span(w: &mut Writer, s: Span) {
    w.u32(s.file.0);
    w.u32(u32::try_from(s.start).unwrap_or(u32::MAX));
    w.u32(u32::try_from(s.end).unwrap_or(u32::MAX));
}
fn decode_span(r: &mut Reader<'_>) -> Result<Span, WireError> {
    Ok(Span {
        file: FileId(r.u32()?),
        start: r.u32()? as usize,
        end: r.u32()? as usize,
    })
}

fn encode_place(w: &mut Writer, p: &Place) {
    match p.root {
        PlaceRoot::Local(l) => {
            w.u8(0);
            w.u32(l.0);
        }
        PlaceRoot::Capture(c) => {
            w.u8(1);
            w.u32(c);
        }
    }
    w.vec(&p.projections, |w, step| match step {
        Projection::Field(f) => {
            w.u8(0);
            w.u32(f.adt.0);
            w.u32(f.variant.0);
            w.u32(f.field.0);
            w.u32(f.args.0);
        }
        Projection::TupleField(i) => {
            w.u8(1);
            w.u32(*i);
        }
        Projection::ErrorArm(ok) => {
            w.u8(2);
            w.bool(*ok);
        }
        Projection::Index(l) => {
            w.u8(3);
            w.u32(l.0);
        }
        Projection::ConstIndex(i) => {
            w.u8(4);
            w.u32(*i);
        }
        Projection::Deref => w.u8(5),
    });
}
fn decode_place(r: &mut Reader<'_>) -> Result<Place, WireError> {
    let root = match r.u8()? {
        0 => PlaceRoot::Local(Local(r.u32()?)),
        1 => PlaceRoot::Capture(r.u32()?),
        t => return Err(WireError::InvalidTag("PlaceRoot", t)),
    };
    let projections = r.vec(|r| match r.u8()? {
        0 => Ok(Projection::Field(FieldRef {
            adt: AdtId(r.u32()?),
            variant: VariantIdx(r.u32()?),
            field: FieldIdx(r.u32()?),
            args: ArgsId(r.u32()?),
        })),
        1 => Ok(Projection::TupleField(r.u32()?)),
        2 => Ok(Projection::ErrorArm(r.bool()?)),
        3 => Ok(Projection::Index(Local(r.u32()?))),
        4 => Ok(Projection::ConstIndex(r.u32()?)),
        5 => Ok(Projection::Deref),
        t => Err(WireError::InvalidTag("Projection", t)),
    })?;
    Ok(Place { root, projections })
}

fn encode_const(w: &mut Writer, c: &ConstValue) {
    match c {
        ConstValue::Unit => w.u8(0),
        ConstValue::Bool(b) => {
            w.u8(1);
            w.bool(*b);
        }
        ConstValue::Int { ty, bits } => {
            w.u8(2);
            encode_int_ty(w, *ty);
            w.u128(*bits);
        }
        ConstValue::Float { ty, bits } => {
            w.u8(3);
            encode_float_ty(w, *ty);
            w.u64(*bits);
        }
    }
}
fn decode_const(r: &mut Reader<'_>) -> Result<ConstValue, WireError> {
    match r.u8()? {
        0 => Ok(ConstValue::Unit),
        1 => Ok(ConstValue::Bool(r.bool()?)),
        2 => Ok(ConstValue::Int {
            ty: decode_int_ty(r)?,
            bits: r.u128()?,
        }),
        3 => Ok(ConstValue::Float {
            ty: decode_float_ty(r)?,
            bits: r.u64()?,
        }),
        t => Err(WireError::InvalidTag("ConstValue", t)),
    }
}

fn encode_operand(w: &mut Writer, o: &Operand) {
    match o {
        Operand::Copy(p) => {
            w.u8(0);
            encode_place(w, p);
        }
        Operand::Move(p) => {
            w.u8(1);
            encode_place(w, p);
        }
        Operand::Const(c) => {
            w.u8(2);
            encode_const(w, c);
        }
        Operand::Pooled(id) => {
            w.u8(3);
            w.u32(id.0);
        }
    }
}
fn decode_operand(r: &mut Reader<'_>) -> Result<Operand, WireError> {
    match r.u8()? {
        0 => Ok(Operand::Copy(decode_place(r)?)),
        1 => Ok(Operand::Move(decode_place(r)?)),
        2 => Ok(Operand::Const(decode_const(r)?)),
        3 => Ok(Operand::Pooled(ConstId(r.u32()?))),
        t => Err(WireError::InvalidTag("Operand", t)),
    }
}

fn encode_rvalue(w: &mut Writer, rv: &Rvalue) {
    match rv {
        Rvalue::Use(o) => {
            w.u8(0);
            encode_operand(w, o);
        }
        Rvalue::Ref {
            region,
            mutability,
            place,
        } => {
            w.u8(1);
            encode_region(w, region);
            encode_mutability(w, *mutability);
            encode_place(w, place);
        }
        Rvalue::SliceRef {
            region,
            mutability,
            place,
            start,
            end,
        } => {
            w.u8(2);
            encode_region(w, region);
            encode_mutability(w, *mutability);
            encode_place(w, place);
            w.opt(start.as_ref(), encode_operand);
            w.opt(end.as_ref(), encode_operand);
        }
        Rvalue::Describe { place } => {
            w.u8(3);
            encode_place(w, place);
        }
        Rvalue::Aggregate { kind, operands } => {
            w.u8(4);
            match kind {
                AggregateKind::Tuple => w.u8(0),
                AggregateKind::Adt { adt, variant, args } => {
                    w.u8(1);
                    w.u32(adt.0);
                    w.u32(variant.0);
                    w.u32(args.0);
                }
                AggregateKind::Array { element } => {
                    w.u8(2);
                    w.u32(element.0);
                }
                AggregateKind::Closure { def, args } => {
                    w.u8(3);
                    w.u32(def.0);
                    w.u32(args.0);
                }
            }
            w.vec(operands, encode_operand);
        }
        Rvalue::ErrorUnion { ok, value, ty } => {
            w.u8(5);
            w.bool(*ok);
            encode_operand(w, value);
            w.u32(ty.0);
        }
        Rvalue::Unary { op, operand } => {
            w.u8(6);
            w.u8(match op {
                UnOp::Not => 0,
                UnOp::Neg => 1,
                UnOp::WrappingNeg => 2,
            });
            encode_operand(w, operand);
        }
        Rvalue::Binary { op, lhs, rhs } => {
            w.u8(7);
            w.u8(match op {
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
            });
            encode_operand(w, lhs);
            encode_operand(w, rhs);
        }
        Rvalue::Convert { operand, to } => {
            w.u8(8);
            encode_operand(w, operand);
            w.u32(to.0);
        }
        Rvalue::Discriminant(p) => {
            w.u8(9);
            encode_place(w, p);
        }
        Rvalue::ErrorArm(p) => {
            w.u8(10);
            encode_place(w, p);
        }
        Rvalue::Len(p) => {
            w.u8(11);
            encode_place(w, p);
        }
        Rvalue::FnRef(id) => {
            w.u8(12);
            w.u32(id.0);
        }
        Rvalue::ForeignFnRef(id) => {
            w.u8(13);
            w.u32(id.0);
        }
        Rvalue::StaticUtf(id) => {
            w.u8(14);
            w.u32(id.0);
        }
    }
}
fn decode_rvalue(r: &mut Reader<'_>) -> Result<Rvalue, WireError> {
    match r.u8()? {
        0 => Ok(Rvalue::Use(decode_operand(r)?)),
        1 => Ok(Rvalue::Ref {
            region: decode_region(r)?,
            mutability: decode_mutability(r)?,
            place: decode_place(r)?,
        }),
        2 => Ok(Rvalue::SliceRef {
            region: decode_region(r)?,
            mutability: decode_mutability(r)?,
            place: decode_place(r)?,
            start: r.opt(decode_operand)?,
            end: r.opt(decode_operand)?,
        }),
        3 => Ok(Rvalue::Describe {
            place: decode_place(r)?,
        }),
        4 => {
            let kind = match r.u8()? {
                0 => AggregateKind::Tuple,
                1 => AggregateKind::Adt {
                    adt: AdtId(r.u32()?),
                    variant: VariantIdx(r.u32()?),
                    args: ArgsId(r.u32()?),
                },
                2 => AggregateKind::Array {
                    element: TyId(r.u32()?),
                },
                3 => AggregateKind::Closure {
                    def: ClosureId(r.u32()?),
                    args: ArgsId(r.u32()?),
                },
                t => return Err(WireError::InvalidTag("AggregateKind", t)),
            };
            let operands = r.vec(decode_operand)?;
            Ok(Rvalue::Aggregate { kind, operands })
        }
        5 => Ok(Rvalue::ErrorUnion {
            ok: r.bool()?,
            value: decode_operand(r)?,
            ty: TyId(r.u32()?),
        }),
        6 => {
            let op = match r.u8()? {
                0 => UnOp::Not,
                1 => UnOp::Neg,
                2 => UnOp::WrappingNeg,
                t => return Err(WireError::InvalidTag("UnOp", t)),
            };
            Ok(Rvalue::Unary {
                op,
                operand: decode_operand(r)?,
            })
        }
        7 => {
            let op = match r.u8()? {
                0 => BinOp::Add,
                1 => BinOp::Sub,
                2 => BinOp::Mul,
                3 => BinOp::Div,
                4 => BinOp::Rem,
                5 => BinOp::WrappingAdd,
                6 => BinOp::WrappingSub,
                7 => BinOp::WrappingMul,
                8 => BinOp::BitAnd,
                9 => BinOp::BitOr,
                10 => BinOp::BitXor,
                11 => BinOp::Shl,
                12 => BinOp::Shr,
                13 => BinOp::Eq,
                14 => BinOp::Ne,
                15 => BinOp::Lt,
                16 => BinOp::Le,
                17 => BinOp::Gt,
                18 => BinOp::Ge,
                t => return Err(WireError::InvalidTag("BinOp", t)),
            };
            Ok(Rvalue::Binary {
                op,
                lhs: decode_operand(r)?,
                rhs: decode_operand(r)?,
            })
        }
        8 => Ok(Rvalue::Convert {
            operand: decode_operand(r)?,
            to: TyId(r.u32()?),
        }),
        9 => Ok(Rvalue::Discriminant(decode_place(r)?)),
        10 => Ok(Rvalue::ErrorArm(decode_place(r)?)),
        11 => Ok(Rvalue::Len(decode_place(r)?)),
        12 => Ok(Rvalue::FnRef(FnId(r.u32()?))),
        13 => Ok(Rvalue::ForeignFnRef(ForeignId(r.u32()?))),
        14 => Ok(Rvalue::StaticUtf(StaticId(r.u32()?))),
        t => Err(WireError::InvalidTag("Rvalue", t)),
    }
}

fn encode_inst(w: &mut Writer, i: &Instruction) {
    match &i.kind {
        InstructionKind::Assign { dest, value } => {
            w.u8(0);
            encode_place(w, dest);
            encode_rvalue(w, value);
        }
        InstructionKind::Drop { place } => {
            w.u8(1);
            encode_place(w, place);
        }
        InstructionKind::Replace { place, value, old } => {
            w.u8(2);
            encode_place(w, place);
            encode_operand(w, value);
            w.u32(old.0);
        }
        InstructionKind::StorageLive(l) => {
            w.u8(3);
            w.u32(l.0);
        }
        InstructionKind::StorageDead(l) => {
            w.u8(4);
            w.u32(l.0);
        }
        InstructionKind::ScopeEnter(s) => {
            w.u8(5);
            w.u32(s.0);
        }
        InstructionKind::ScopeExit { scope, exit } => {
            w.u8(6);
            w.u32(scope.0);
            w.u8(match exit {
                ScopeExit::Fallthrough => 0,
                ScopeExit::Return => 1,
                ScopeExit::Error => 2,
                ScopeExit::Break => 3,
                ScopeExit::Continue => 4,
            });
        }
        InstructionKind::RegisterDefer { scope, closure } => {
            w.u8(7);
            w.u32(scope.0);
            encode_operand(w, closure);
        }
    }
    encode_span(w, i.span);
}
fn decode_inst(r: &mut Reader<'_>) -> Result<Instruction, WireError> {
    let kind = match r.u8()? {
        0 => InstructionKind::Assign {
            dest: decode_place(r)?,
            value: decode_rvalue(r)?,
        },
        1 => InstructionKind::Drop {
            place: decode_place(r)?,
        },
        2 => InstructionKind::Replace {
            place: decode_place(r)?,
            value: decode_operand(r)?,
            old: Local(r.u32()?),
        },
        3 => InstructionKind::StorageLive(Local(r.u32()?)),
        4 => InstructionKind::StorageDead(Local(r.u32()?)),
        5 => InstructionKind::ScopeEnter(ScopeId(r.u32()?)),
        6 => {
            let scope = ScopeId(r.u32()?);
            let exit = match r.u8()? {
                0 => ScopeExit::Fallthrough,
                1 => ScopeExit::Return,
                2 => ScopeExit::Error,
                3 => ScopeExit::Break,
                4 => ScopeExit::Continue,
                t => return Err(WireError::InvalidTag("ScopeExit", t)),
            };
            InstructionKind::ScopeExit { scope, exit }
        }
        7 => InstructionKind::RegisterDefer {
            scope: ScopeId(r.u32()?),
            closure: decode_operand(r)?,
        },
        t => return Err(WireError::InvalidTag("InstructionKind", t)),
    };
    let span = decode_span(r)?;
    Ok(Instruction { kind, span })
}

fn encode_intrinsic(w: &mut Writer, op: IntrinsicOp) {
    match op {
        IntrinsicOp::StoreNew => w.u8(0),
        IntrinsicOp::StoreReserve => w.u8(1),
        IntrinsicOp::StorePush => w.u8(2),
        IntrinsicOp::StorePop => w.u8(3),
        IntrinsicOp::StoreLen => w.u8(4),
        IntrinsicOp::StoreCapacity => w.u8(5),
        IntrinsicOp::StoreBorrow { mutability } => {
            w.u8(6);
            encode_mutability(w, mutability);
        }
        IntrinsicOp::ByteBufferNew => w.u8(7),
        IntrinsicOp::ByteBufferReserve => w.u8(8),
        IntrinsicOp::ByteBufferPush => w.u8(9),
        IntrinsicOp::ByteBufferTruncate => w.u8(10),
        IntrinsicOp::ByteBufferLen => w.u8(11),
        IntrinsicOp::ByteBufferCapacity => w.u8(12),
        IntrinsicOp::ByteBufferBorrow => w.u8(13),
        IntrinsicOp::MutexCreate => w.u8(14),
        IntrinsicOp::MutexLock => w.u8(15),
        IntrinsicOp::MutexValue => w.u8(16),
        IntrinsicOp::MutexUnlock => w.u8(17),
        IntrinsicOp::SyncLoad => w.u8(18),
        IntrinsicOp::SyncStore => w.u8(19),
        IntrinsicOp::SyncFetchAdd => w.u8(20),
        IntrinsicOp::SyncCompareExchange => w.u8(21),
        IntrinsicOp::TaskStart => w.u8(22),
        IntrinsicOp::TaskCreate => w.u8(23),
        IntrinsicOp::TaskYield => w.u8(24),
        IntrinsicOp::TaskValid => w.u8(25),
        IntrinsicOp::TaskPoll => w.u8(26),
        IntrinsicOp::TaskRequestCancel => w.u8(27),
        IntrinsicOp::TaskPollClose => w.u8(28),
        IntrinsicOp::TaskFinishDestroy => w.u8(29),
        IntrinsicOp::RegistrationStart => w.u8(30),
        IntrinsicOp::RegistrationValid => w.u8(31),
        IntrinsicOp::RegistrationClose => w.u8(32),
        IntrinsicOp::AsyncRegistrationStart => w.u8(33),
        IntrinsicOp::AsyncRegistrationValid => w.u8(34),
        IntrinsicOp::AsyncRegistrationRequestClose => w.u8(35),
        IntrinsicOp::AsyncRegistrationPollClose => w.u8(36),
        IntrinsicOp::AsyncRegistrationFinishDestroy => w.u8(37),
        IntrinsicOp::ManagedCreate => w.u8(38),
        IntrinsicOp::OwnedDynCreate => w.u8(39),
        IntrinsicOp::ThreadScope => w.u8(40),
    }
}
fn decode_intrinsic(r: &mut Reader<'_>) -> Result<IntrinsicOp, WireError> {
    match r.u8()? {
        0 => Ok(IntrinsicOp::StoreNew),
        1 => Ok(IntrinsicOp::StoreReserve),
        2 => Ok(IntrinsicOp::StorePush),
        3 => Ok(IntrinsicOp::StorePop),
        4 => Ok(IntrinsicOp::StoreLen),
        5 => Ok(IntrinsicOp::StoreCapacity),
        6 => Ok(IntrinsicOp::StoreBorrow {
            mutability: decode_mutability(r)?,
        }),
        7 => Ok(IntrinsicOp::ByteBufferNew),
        8 => Ok(IntrinsicOp::ByteBufferReserve),
        9 => Ok(IntrinsicOp::ByteBufferPush),
        10 => Ok(IntrinsicOp::ByteBufferTruncate),
        11 => Ok(IntrinsicOp::ByteBufferLen),
        12 => Ok(IntrinsicOp::ByteBufferCapacity),
        13 => Ok(IntrinsicOp::ByteBufferBorrow),
        14 => Ok(IntrinsicOp::MutexCreate),
        15 => Ok(IntrinsicOp::MutexLock),
        16 => Ok(IntrinsicOp::MutexValue),
        17 => Ok(IntrinsicOp::MutexUnlock),
        18 => Ok(IntrinsicOp::SyncLoad),
        19 => Ok(IntrinsicOp::SyncStore),
        20 => Ok(IntrinsicOp::SyncFetchAdd),
        21 => Ok(IntrinsicOp::SyncCompareExchange),
        22 => Ok(IntrinsicOp::TaskStart),
        23 => Ok(IntrinsicOp::TaskCreate),
        24 => Ok(IntrinsicOp::TaskYield),
        25 => Ok(IntrinsicOp::TaskValid),
        26 => Ok(IntrinsicOp::TaskPoll),
        27 => Ok(IntrinsicOp::TaskRequestCancel),
        28 => Ok(IntrinsicOp::TaskPollClose),
        29 => Ok(IntrinsicOp::TaskFinishDestroy),
        30 => Ok(IntrinsicOp::RegistrationStart),
        31 => Ok(IntrinsicOp::RegistrationValid),
        32 => Ok(IntrinsicOp::RegistrationClose),
        33 => Ok(IntrinsicOp::AsyncRegistrationStart),
        34 => Ok(IntrinsicOp::AsyncRegistrationValid),
        35 => Ok(IntrinsicOp::AsyncRegistrationRequestClose),
        36 => Ok(IntrinsicOp::AsyncRegistrationPollClose),
        37 => Ok(IntrinsicOp::AsyncRegistrationFinishDestroy),
        38 => Ok(IntrinsicOp::ManagedCreate),
        39 => Ok(IntrinsicOp::OwnedDynCreate),
        40 => Ok(IntrinsicOp::ThreadScope),
        t => Err(WireError::InvalidTag("IntrinsicOp", t)),
    }
}

fn encode_trap(w: &mut Writer, t: TrapKind) {
    w.u8(match t {
        TrapKind::Bounds => 0,
        TrapKind::Overflow => 1,
        TrapKind::DivideByZero => 2,
        TrapKind::Conversion => 3,
        TrapKind::Explicit => 4,
    });
}
fn decode_trap(r: &mut Reader<'_>) -> Result<TrapKind, WireError> {
    match r.u8()? {
        0 => Ok(TrapKind::Bounds),
        1 => Ok(TrapKind::Overflow),
        2 => Ok(TrapKind::DivideByZero),
        3 => Ok(TrapKind::Conversion),
        4 => Ok(TrapKind::Explicit),
        t => Err(WireError::InvalidTag("TrapKind", t)),
    }
}

fn encode_terminator(w: &mut Writer, t: &Terminator) {
    match t {
        Terminator::Goto { target } => {
            w.u8(0);
            w.u32(target.0);
        }
        Terminator::Switch {
            discr,
            arms,
            otherwise,
        } => {
            w.u8(1);
            encode_operand(w, discr);
            w.vec(arms, |w, (v, b)| {
                w.u128(*v);
                w.u32(b.0);
            });
            w.u32(otherwise.0);
        }
        Terminator::Return { value } => {
            w.u8(2);
            encode_operand(w, value);
        }
        Terminator::Call {
            callee,
            args,
            dest,
            target,
            cancel,
        } => {
            w.u8(3);
            match callee {
                Callee::Direct(id) => {
                    w.u8(0);
                    w.u32(id.0);
                }
                Callee::Indirect(o) => {
                    w.u8(1);
                    encode_operand(w, o);
                }
                Callee::Closure { closure, mode } => {
                    w.u8(2);
                    encode_place(w, closure);
                    w.u8(match mode {
                        ClosureCallMode::Shared => 0,
                        ClosureCallMode::Mut => 1,
                        ClosureCallMode::Once => 2,
                    });
                }
                Callee::DynSlot { object, slot } => {
                    w.u8(3);
                    encode_operand(w, object);
                    w.u32(*slot);
                }
                Callee::Foreign(id) => {
                    w.u8(4);
                    w.u32(id.0);
                }
                Callee::Intrinsic(op) => {
                    w.u8(5);
                    encode_intrinsic(w, *op);
                }
            }
            w.vec(args, encode_operand);
            encode_place(w, dest);
            w.u32(target.0);
            w.opt(cancel.as_ref(), |w, b| w.u32(b.0));
        }
        Terminator::Assert {
            cond,
            expected,
            target,
            trap,
        } => {
            w.u8(4);
            encode_operand(w, cond);
            w.bool(*expected);
            w.u32(target.0);
            encode_trap(w, *trap);
        }
        Terminator::Unreachable => w.u8(5),
        Terminator::Trap(k) => {
            w.u8(6);
            encode_trap(w, *k);
        }
    }
}
fn decode_terminator(r: &mut Reader<'_>) -> Result<Terminator, WireError> {
    match r.u8()? {
        0 => Ok(Terminator::Goto {
            target: BlockId(r.u32()?),
        }),
        1 => Ok(Terminator::Switch {
            discr: decode_operand(r)?,
            arms: r.vec(|r| Ok((r.u128()?, BlockId(r.u32()?))))?,
            otherwise: BlockId(r.u32()?),
        }),
        2 => Ok(Terminator::Return {
            value: decode_operand(r)?,
        }),
        3 => {
            let callee = match r.u8()? {
                0 => Callee::Direct(FnId(r.u32()?)),
                1 => Callee::Indirect(decode_operand(r)?),
                2 => {
                    let closure = decode_place(r)?;
                    let mode = match r.u8()? {
                        0 => ClosureCallMode::Shared,
                        1 => ClosureCallMode::Mut,
                        2 => ClosureCallMode::Once,
                        t => return Err(WireError::InvalidTag("ClosureCallMode", t)),
                    };
                    Callee::Closure { closure, mode }
                }
                3 => Callee::DynSlot {
                    object: decode_operand(r)?,
                    slot: r.u32()?,
                },
                4 => Callee::Foreign(ForeignId(r.u32()?)),
                5 => Callee::Intrinsic(decode_intrinsic(r)?),
                t => return Err(WireError::InvalidTag("Callee", t)),
            };
            Ok(Terminator::Call {
                callee,
                args: r.vec(decode_operand)?,
                dest: decode_place(r)?,
                target: BlockId(r.u32()?),
                cancel: r.opt(|r| Ok(BlockId(r.u32()?)))?,
            })
        }
        4 => Ok(Terminator::Assert {
            cond: decode_operand(r)?,
            expected: r.bool()?,
            target: BlockId(r.u32()?),
            trap: decode_trap(r)?,
        }),
        5 => Ok(Terminator::Unreachable),
        6 => Ok(Terminator::Trap(decode_trap(r)?)),
        t => Err(WireError::InvalidTag("Terminator", t)),
    }
}

fn encode_function(w: &mut Writer, f: &Function) {
    w.str(&f.name);
    w.vec(&f.sig.params, |w, t| w.u32(t.0));
    w.u32(f.sig.result.0);
    w.u32(f.sig.type_params);
    w.u32(f.sig.region_params);
    match f.sig.role {
        FnRole::Normal => w.u8(0),
        FnRole::Drop(adt) => {
            w.u8(1);
            w.u32(adt.0);
        }
    }
    w.u32(f.body.param_count);
    w.vec(&f.body.locals, |w, l| {
        w.u32(l.ty.0);
        w.opt(l.name.as_ref(), |w, s| w.str(s));
    });
    w.vec(&f.body.captures, |w, t| w.u32(t.0));
    w.opt(f.body.closure.as_ref(), |w, c| w.u32(c.0));
    w.u32(f.body.result.0);
    w.vec(&f.body.param_env.copy_params, |w, b| w.bool(*b));
    w.vec(&f.body.drop_flags, |w, df| {
        w.u32(df.flag.0);
        encode_place(w, &df.place);
    });
    w.vec(&f.body.blocks, |w, b| {
        w.vec(&b.instructions, encode_inst);
        encode_terminator(w, &b.terminator);
        encode_span(w, b.terminator_span);
    });
}
fn decode_function(r: &mut Reader<'_>) -> Result<Function, WireError> {
    let name = r.string()?;
    let params = r.vec(|r| Ok(TyId(r.u32()?)))?;
    let sig_result = TyId(r.u32()?);
    let type_params = r.u32()?;
    let region_params = r.u32()?;
    let role = match r.u8()? {
        0 => FnRole::Normal,
        1 => FnRole::Drop(AdtId(r.u32()?)),
        t => return Err(WireError::InvalidTag("FnRole", t)),
    };
    let param_count = r.u32()?;
    let locals = r.vec(|r| {
        Ok(LocalDecl {
            ty: TyId(r.u32()?),
            name: r.opt(Reader::string)?,
        })
    })?;
    let captures = r.vec(|r| Ok(TyId(r.u32()?)))?;
    let closure = r.opt(|r| Ok(ClosureId(r.u32()?)))?;
    let result = TyId(r.u32()?);
    let copy_params = r.vec(Reader::bool)?;
    let drop_flags = r.vec(|r| {
        Ok(DropFlag {
            flag: Local(r.u32()?),
            place: decode_place(r)?,
        })
    })?;
    let blocks = r.vec(|r| {
        Ok(Block {
            instructions: r.vec(decode_inst)?,
            terminator: decode_terminator(r)?,
            terminator_span: decode_span(r)?,
        })
    })?;
    Ok(Function {
        name,
        sig: FnSig {
            params,
            result: sig_result,
            type_params,
            region_params,
            role,
        },
        body: Body {
            param_count,
            locals,
            captures,
            closure,
            result,
            param_env: ParamEnv { copy_params },
            drop_flags,
            blocks,
        },
    })
}

fn encode_foreign_decl(w: &mut Writer, f: &ForeignDecl) {
    w.str(&f.symbol);
    w.u8(match f.abi {
        ForeignAbi::C => 0,
    });
    w.vec(&f.params, |w, t| w.u32(t.0));
    w.u32(f.result.0);
    w.vec(&f.noescape_params, |w, i| w.u32(*i));
}
fn decode_foreign_decl(r: &mut Reader<'_>) -> Result<ForeignDecl, WireError> {
    let symbol = r.string()?;
    let abi = match r.u8()? {
        0 => ForeignAbi::C,
        t => return Err(WireError::InvalidTag("ForeignAbi", t)),
    };
    Ok(ForeignDecl {
        symbol,
        abi,
        params: r.vec(|r| Ok(TyId(r.u32()?)))?,
        result: TyId(r.u32()?),
        noescape_params: r.vec(Reader::u32)?,
    })
}

// ---- certificate -----------------------------------------------------------

fn encode_fn_cert(w: &mut Writer, c: &FunctionCertificate) {
    w.u32(c.function.0);
    w.vec(&c.invariants, |w, inv| {
        w.u32(inv.block.0);
        w.vec(&inv.init, |w, p| {
            encode_place(w, &p.place);
            w.u8(match p.state {
                InitClaim::Init => 0,
                InitClaim::Uninit => 1,
                InitClaim::Maybe => 2,
            });
        });
        w.vec(&inv.variants, |w, v| {
            encode_place(w, &v.place);
            w.u32(v.variant);
        });
        w.vec(&inv.tags, |w, t| {
            w.u32(t.local.0);
            encode_place(w, &t.place);
        });
        w.vec(&inv.flags, |w, f| {
            w.u32(f.flag.0);
            encode_place(w, &f.place);
        });
        w.vec(&inv.loans, |w, l| {
            w.u32(l.holder.0);
            match l.origin {
                LoanOrigin::External => w.u8(0),
                LoanOrigin::Region(reg) => {
                    w.u8(1);
                    w.u32(reg);
                }
                LoanOrigin::At(loc) => {
                    w.u8(2);
                    w.u32(loc.block.0);
                    w.opt(loc.instruction.as_ref(), |w, i| w.u32(*i));
                }
            }
        });
    });
}
fn decode_fn_cert(r: &mut Reader<'_>) -> Result<FunctionCertificate, WireError> {
    let function = FnId(r.u32()?);
    let invariants = r.vec(|r| {
        let block = BlockId(r.u32()?);
        let init = r.vec(|r| {
            let place = decode_place(r)?;
            let state = match r.u8()? {
                0 => InitClaim::Init,
                1 => InitClaim::Uninit,
                2 => InitClaim::Maybe,
                t => return Err(WireError::InvalidTag("InitClaim", t)),
            };
            Ok(PlaceInit { place, state })
        })?;
        let variants = r.vec(|r| {
            Ok(VariantClaim {
                place: decode_place(r)?,
                variant: r.u32()?,
            })
        })?;
        let tags = r.vec(|r| {
            Ok(TagClaim {
                local: Local(r.u32()?),
                place: decode_place(r)?,
            })
        })?;
        let flags = r.vec(|r| {
            Ok(FlagClaim {
                flag: Local(r.u32()?),
                place: decode_place(r)?,
            })
        })?;
        let loans = r.vec(|r| {
            let holder = Local(r.u32()?);
            let origin = match r.u8()? {
                0 => LoanOrigin::External,
                1 => LoanOrigin::Region(r.u32()?),
                2 => LoanOrigin::At(OpLocation {
                    block: BlockId(r.u32()?),
                    instruction: r.opt(Reader::u32)?,
                }),
                t => return Err(WireError::InvalidTag("LoanOrigin", t)),
            };
            Ok(LoanClaim { holder, origin })
        })?;
        Ok(BlockInvariant {
            block,
            init,
            variants,
            tags,
            flags,
            loans,
        })
    })?;
    Ok(FunctionCertificate {
        function,
        invariants,
    })
}
