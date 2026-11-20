//! Hand-built body IR programs and verdict plumbing for the independent
//! safety cases. Nothing here evaluates a semantic rule: expectations are
//! written in the tests from the language contract.
#![forbid(unsafe_code)]

use zil_base::Span;
use zil_ir::body::*;

pub use zil_check::{CheckBudget, CheckError, CheckErrorKind};
pub use zil_solve::{Budget, SolveError};

pub struct Types {
    pub unit: TyId,
    pub bool: TyId,
    pub i32: TyId,
    pub usize: TyId,
    pub empty: ArgsId,
}

pub struct ModuleBuilder {
    pub tys: TyTable,
    pub t: Types,
    pub functions: Vec<Function>,
    pub foreigns: Vec<ForeignDecl>,
}

impl Default for ModuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleBuilder {
    pub fn new() -> Self {
        let mut tys = TyTable::new();
        let t = Types {
            unit: tys.intern(TyKind::Unit),
            bool: tys.intern(TyKind::Bool),
            i32: tys.intern(TyKind::Int(IntTy::I32)),
            usize: tys.intern(TyKind::Int(IntTy::Usize)),
            empty: tys.intern_args(GenericArgs {
                tys: vec![],
                regions: vec![],
            }),
        };
        ModuleBuilder {
            tys,
            t,
            functions: Vec::new(),
            foreigns: Vec::new(),
        }
    }

    pub fn ty(&mut self, kind: TyKind) -> TyId {
        self.tys.intern(kind)
    }

    /// Declares a nominal type with one variant per entry; returns its id
    /// and its non-generic instance type.
    pub fn adt(
        &mut self,
        name: &str,
        kind: AdtKind,
        user_drop: bool,
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
            user_drop,
        });
        let ty = self.tys.intern(TyKind::Adt {
            adt: id,
            args: self.t.empty,
        });
        (id, ty)
    }

    /// A resource payload and its carrier/descriptor types.
    pub fn resource(&mut self, name: &str) -> (AdtId, TyId, TyId) {
        let i32 = self.t.i32;
        let (id, payload) = self.adt(name, AdtKind::Resource, false, &[(name, &[i32])]);
        let managed = self.ty(TyKind::Managed(payload));
        self.ty(TyKind::Desc(payload));
        (id, payload, managed)
    }

    pub fn field(&self, adt: AdtId, variant: u32, index: u32) -> FieldRef {
        FieldRef {
            adt,
            variant: VariantIdx(variant),
            field: FieldIdx(index),
            args: self.t.empty,
        }
    }

    pub fn function(&mut self, f: FnBuilder) -> FnId {
        let id = FnId::from_index(self.functions.len());
        self.functions.push(f.finish());
        id
    }

    pub fn finish(self) -> Module {
        Module {
            tys: self.tys,
            functions: self.functions,
            foreigns: self.foreigns,
        }
    }
}

pub struct FnBuilder {
    name: String,
    params: Vec<TyId>,
    locals: Vec<LocalDecl>,
    result: TyId,
    blocks: Vec<Option<Block>>,
    pending: Vec<Vec<Instruction>>,
    drop_flags: Vec<DropFlag>,
    role: FnRole,
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
            drop_flags: Vec::new(),
            role: FnRole::Normal,
        }
    }
    /// Marks this function as the cleanup entry of `adt`.
    pub fn drop_entry(mut self, adt: AdtId) -> Self {
        self.role = FnRole::Drop(adt);
        self
    }
    pub fn param(&self, i: u32) -> Local {
        Local(i)
    }
    /// Declares `flag` as the drop flag of `place`.
    pub fn drop_flag(&mut self, flag: Local, place: Place) {
        self.drop_flags.push(DropFlag { flag, place });
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
        let blocks = self
            .blocks
            .into_iter()
            .map(|b| b.expect("every block terminated"))
            .collect();
        Function {
            name: self.name,
            sig: FnSig {
                params: self.params.clone(),
                result: self.result,
                type_params: 0,
                region_params: 0,
                role: self.role,
            },
            body: Body {
                param_count: self.params.len() as u32,
                locals: self.locals,
                captures: Vec::new(),
                closure: None,
                result: self.result,
                param_env: ParamEnv::default(),
                drop_flags: self.drop_flags,
                blocks,
            },
        }
    }
}

pub fn copy(place: Place) -> Operand {
    Operand::Copy(place)
}
pub fn mv(place: Place) -> Operand {
    Operand::Move(place)
}
pub fn unit() -> Operand {
    Operand::Const(ConstValue::Unit)
}
pub fn int(v: i32) -> Operand {
    Operand::Const(ConstValue::Int {
        ty: IntTy::I32,
        bits: u128::from(v as u32),
    })
}
pub fn ret_unit() -> Terminator {
    Terminator::Return { value: unit() }
}
pub fn goto(target: BlockId) -> Terminator {
    Terminator::Goto { target }
}
pub fn l(local: Local) -> Place {
    Place::local(local)
}

pub const STEPS: u64 = 1_000_000;

/// Outcome of the full candidate pipeline: solve, then independently check.
#[derive(Debug)]
pub enum Verdict {
    Accepted,
    SolverRejected(Vec<SolveError>),
    CheckerRejected(CheckError),
}

pub fn verdict(module: Module) -> Verdict {
    match zil_solve::solve_module(&module, Budget { steps: STEPS }) {
        Err(errors) => Verdict::SolverRejected(errors),
        Ok(cert) => match zil_check::check_module(module, &cert, CheckBudget { steps: STEPS }) {
            Ok(_) => Verdict::Accepted,
            Err(e) => Verdict::CheckerRejected(e),
        },
    }
}

/// Checks a module against a supplied (possibly forged) certificate.
pub fn check_with(module: Module, cert: &Certificate) -> Result<(), CheckError> {
    zil_check::check_module(module, cert, CheckBudget { steps: STEPS }).map(|_| ())
}

pub fn solve(module: &Module) -> Certificate {
    zil_solve::solve_module(module, Budget { steps: STEPS }).expect("solvable module")
}
