//! Hand-built body IR programs admitted through the real pipeline
//! (`zil_solve::solve_module` then `zil_check::check_module`). Nothing here
//! computes an expectation: tests write expected values from the language
//! rules.
#![allow(dead_code)]

use zil_base::Span;
use zil_check::{CheckBudget, VerifiedModule};
use zil_interp::{EvalBudget, EvalError, Value, evaluate};
use zil_ir::body::*;

pub const BUDGET: EvalBudget = EvalBudget {
    steps: 1_000_000,
    call_depth: 256,
};

pub struct ModuleBuilder {
    pub tys: TyTable,
    pub functions: Vec<Function>,
    pub empty: ArgsId,
}

impl Default for ModuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleBuilder {
    pub fn new() -> Self {
        let mut tys = TyTable::new();
        let empty = tys.intern_args(GenericArgs {
            tys: vec![],
            regions: vec![],
        });
        ModuleBuilder {
            tys,
            functions: Vec::new(),
            empty,
        }
    }
    pub fn ty(&mut self, kind: TyKind) -> TyId {
        self.tys.intern(kind)
    }
    pub fn int(&mut self, i: IntTy) -> TyId {
        self.ty(TyKind::Int(i))
    }
    pub fn adt(
        &mut self,
        name: &str,
        kind: AdtKind,
        variants: &[(&str, &[TyId])],
    ) -> (AdtId, TyId) {
        let id = self.tys.add_adt(AdtDef {
            name: name.into(),
            kind,
            type_params: 0,
            region_params: 0,
            variants: variants
                .iter()
                .map(|(n, fields)| VariantDef {
                    name: (*n).into(),
                    fields: fields
                        .iter()
                        .enumerate()
                        .map(|(i, t)| FieldDef {
                            name: format!("f{i}"),
                            ty: *t,
                        })
                        .collect(),
                })
                .collect(),
            user_drop: false,
        });
        let ty = self.tys.intern(TyKind::Adt {
            adt: id,
            args: self.empty,
        });
        (id, ty)
    }
    pub fn field(&self, adt: AdtId, variant: u32, index: u32) -> FieldRef {
        FieldRef {
            adt,
            variant: VariantIdx(variant),
            field: FieldIdx(index),
            args: self.empty,
        }
    }
    pub fn function(&mut self, f: FnBuilder) -> FnId {
        let id = FnId::from_index(self.functions.len());
        self.functions.push(f.finish());
        id
    }
    /// Admits the module through solver and checker.
    pub fn verify(self) -> VerifiedModule {
        let module = Module {
            tys: self.tys,
            functions: self.functions,
            foreigns: Vec::new(),
            statics: Vec::new(),
        };
        let cert = zil_solve::solve_module(&module, zil_solve::Budget { steps: 1_000_000 })
            .unwrap_or_else(|e| panic!("solver rejected the test program: {e:?}"));
        zil_check::check_module(module, &cert, CheckBudget { steps: 1_000_000 })
            .unwrap_or_else(|e| panic!("checker rejected the test program: {e:?}"))
    }
}

pub struct FnBuilder {
    name: String,
    params: Vec<TyId>,
    locals: Vec<LocalDecl>,
    result: TyId,
    blocks: Vec<Option<Block>>,
    pending: Vec<Vec<Instruction>>,
}

impl FnBuilder {
    pub fn new(name: &str, params: &[TyId], result: TyId) -> Self {
        FnBuilder {
            name: name.into(),
            params: params.to_vec(),
            locals: params
                .iter()
                .map(|t| LocalDecl { ty: *t, name: None })
                .collect(),
            result,
            blocks: Vec::new(),
            pending: Vec::new(),
        }
    }
    pub fn local(&mut self, ty: TyId) -> Local {
        self.locals.push(LocalDecl { ty, name: None });
        Local::from_index(self.locals.len() - 1)
    }
    pub fn block(&mut self) -> BlockId {
        self.blocks.push(None);
        self.pending.push(Vec::new());
        BlockId::from_index(self.blocks.len() - 1)
    }
    pub fn push(&mut self, block: BlockId, kind: InstructionKind) {
        self.pending[block.index()].push(Instruction {
            kind,
            span: Span::default(),
        });
    }
    pub fn assign(&mut self, block: BlockId, dest: Place, value: Rvalue) {
        self.push(block, InstructionKind::Assign { dest, value });
    }
    pub fn terminate(&mut self, block: BlockId, terminator: Terminator) {
        let instructions = std::mem::take(&mut self.pending[block.index()]);
        self.blocks[block.index()] = Some(Block {
            instructions,
            terminator,
            terminator_span: Span::default(),
        });
    }
    fn finish(self) -> Function {
        Function {
            name: self.name,
            sig: FnSig {
                params: self.params.clone(),
                result: self.result,
                type_params: 0,
                region_params: 0,
                role: FnRole::Normal,
            },
            body: Body {
                param_count: self.params.len() as u32,
                locals: self.locals,
                captures: Vec::new(),
                closure: None,
                result: self.result,
                param_env: ParamEnv::default(),
                drop_flags: Vec::new(),
                blocks: self
                    .blocks
                    .into_iter()
                    .map(|b| b.expect("every block terminated"))
                    .collect(),
            },
        }
    }
}

pub fn l(local: Local) -> Place {
    Place::local(local)
}
pub fn cp(local: Local) -> Operand {
    Operand::Copy(Place::local(local))
}
pub fn mv(local: Local) -> Operand {
    Operand::Move(Place::local(local))
}
pub fn ret(local: Local) -> Terminator {
    Terminator::Return { value: cp(local) }
}
pub fn goto(target: BlockId) -> Terminator {
    Terminator::Goto { target }
}

/// Hand encoding of a mathematical integer in `ty` (64-bit pointer target).
pub fn width(ty: IntTy) -> u32 {
    match ty {
        IntTy::I8 | IntTy::U8 => 8,
        IntTy::I16 | IntTy::U16 => 16,
        IntTy::I32 | IntTy::U32 => 32,
        IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => 64,
    }
}
pub fn bits(ty: IntTy, v: i128) -> u128 {
    (v as u128) & ((1u128 << width(ty)) - 1)
}
pub fn iv(ty: IntTy, v: i128) -> Value {
    Value::Int {
        ty,
        bits: bits(ty, v),
    }
}
pub fn ic(ty: IntTy, v: i128) -> Operand {
    Operand::Const(ConstValue::Int {
        ty,
        bits: bits(ty, v),
    })
}
pub fn f64v(x: f64) -> Value {
    Value::Float {
        ty: FloatTy::F64,
        bits: x.to_bits(),
    }
}
pub fn f32v(x: f32) -> Value {
    Value::Float {
        ty: FloatTy::F32,
        bits: u64::from(x.to_bits()),
    }
}

pub fn eval(m: &VerifiedModule, f: FnId, args: Vec<Value>) -> Result<Value, EvalError> {
    evaluate(m, f, args, BUDGET)
}

pub fn is_trap(r: &Result<Value, EvalError>, expected: TrapKind) -> bool {
    matches!(r, Err(EvalError::Trap { kind, .. }) if *kind == expected)
}

/// `fn f(a: T, b: T) -> R { let r = a op b; r }`
pub fn binary_fn(operand: TyKind, op: BinOp) -> (VerifiedModule, FnId) {
    let mut m = ModuleBuilder::new();
    let t = m.ty(operand);
    let r = if op.is_comparison() {
        m.ty(TyKind::Bool)
    } else {
        t
    };
    let mut f = FnBuilder::new("f", &[t, t], r);
    let out = f.local(r);
    let b = f.block();
    f.assign(
        b,
        l(out),
        Rvalue::Binary {
            op,
            lhs: cp(Local(0)),
            rhs: cp(Local(1)),
        },
    );
    f.terminate(b, ret(out));
    let id = m.function(f);
    (m.verify(), id)
}

/// `fn f(a: T) -> T { let r = op a; r }`
pub fn unary_fn(operand: TyKind, op: UnOp) -> (VerifiedModule, FnId) {
    let mut m = ModuleBuilder::new();
    let t = m.ty(operand);
    let mut f = FnBuilder::new("f", &[t], t);
    let out = f.local(t);
    let b = f.block();
    f.assign(
        b,
        l(out),
        Rvalue::Unary {
            op,
            operand: cp(Local(0)),
        },
    );
    f.terminate(b, ret(out));
    let id = m.function(f);
    (m.verify(), id)
}

/// `fn f(a: From) -> To { let r = To(a); r }`
pub fn convert_fn(from: TyKind, to: TyKind) -> (VerifiedModule, FnId) {
    let mut m = ModuleBuilder::new();
    let a = m.ty(from);
    let t = m.ty(to);
    let mut f = FnBuilder::new("f", &[a], t);
    let out = f.local(t);
    let b = f.block();
    f.assign(
        b,
        l(out),
        Rvalue::Convert {
            operand: cp(Local(0)),
            to: t,
        },
    );
    f.terminate(b, ret(out));
    let id = m.function(f);
    (m.verify(), id)
}
