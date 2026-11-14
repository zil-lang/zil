//! Type original operations in their declaration domain. Runtime operands and
//! a particular caller's concrete Type actuals never substitute rigid formals.
use super::*;
use zil_base::NodeId;

mod abstract_calls;
mod buffers;
mod capabilities;
mod captures;
mod constructors;
mod effects;
mod enums;
mod function_values;
mod invocations;
mod loops;
pub(super) mod mixed;
mod mutex;
pub(super) mod nominal;
mod places;
mod protocol;
mod registration;
mod registration_start;
pub(super) mod resources;
mod roles;
pub(super) mod sealed;
pub(super) mod slice_borrows;
mod slices;
mod stores;
mod tasks;
mod trait_members;
mod traits;
mod values;
use constructors::Wrapper;
use mixed::Life;

type Term = usize;
pub(super) fn declaration_scope(
    program: &SemanticModule,
    declaration: LifeDeclarationId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    trait_members::declaration_scope(program, declaration, inventory, b)
}
pub(super) fn definition_scope(
    program: &SemanticModule,
    definition: DynDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    trait_members::definition_scope(program, definition, inventory, b)
}
pub(super) fn member_scope(
    program: &SemanticModule,
    declaration: LifeDeclarationId,
    member: u32,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    trait_members::member_scope(program, declaration, member, inventory, b)
}
#[derive(Clone)]
enum Kind {
    Atom(Type),
    Rigid(SemanticDefinitionId, u32),
    SelfType(SemanticDefinitionId),
    Associated {
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: Vec<Term>,
        member: u32,
    },
    Life(Life),
    LifeVariable(Option<Term>),
    ConstRigid {
        owner: SemanticDefinitionId,
        ordinal: u32,
        carrier: Term,
    },
    ConstValue(Constant),
    Address {
        mutable: bool,
        life: Term,
        inner: Term,
    },
    Slice(Term),
    Task(tasks::TaskType),
    MutexGuard {
        payload: Term,
        life: Term,
    },
    Desc {
        life: Term,
        inner: Term,
    },
    Wrapped(Wrapper, Term),
    Code {
        kind: SemanticCodeKind,
        parameters: Vec<Term>,
        result: Term,
        modes: Vec<PassingMode>,
        result_mode: PassingMode,
        call_kind: CallKind,
    },
    ErrorUnion {
        error: Term,
        result: Term,
    },
    Array {
        element: Term,
        length: Term,
    },
    CallResult {
        caller: SemanticDefinitionId,
        node: NodeId,
        hidden: Term,
    },
    Infer(Option<Term>),
    Tuple(Vec<Term>),
    Nominal {
        definition: SemanticDefinitionId,
        arguments: Vec<Term>,
        fields: Vec<Term>,
    },
    Closure {
        definition: SemanticDefinitionId,
        parameters: Vec<Term>,
        result: Term,
        environment: Vec<Term>,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalAnchor {
    Parameter(u32),
    Binding(NodeId),
    Environment(u32),
}
struct Borrow {
    anchor: LocalAnchor,
    inner: Term,
    slice: Option<slice_borrows::SliceProjection>,
}
struct Local {
    anchor: Option<LocalAnchor>,
    name: String,
    ty: Term,
    mutable: bool,
    used: bool,
    borrowed: bool,
    initialized: bool,
}
struct Frame {
    definition: SemanticDefinitionId,
    formals: Vec<Term>,
    parameters: Vec<Term>,
    result: Term,
    locals: Vec<Local>,
    inherited: usize,
    captured: BTreeMap<usize, Term>,
    alive: bool,
    environment: Vec<Term>,
    expressions: BTreeMap<NodeId, Term>,
    bindings: BTreeMap<NodeId, Term>,
}
#[derive(Clone, Copy)]
pub(super) enum Predicate {
    Copy,
    Value,
    SyncDrop,
    StoreElement,
    Capability(SemanticBuiltinTrait),
}
struct Need {
    term: Term,
    predicate: Predicate,
}
struct CallableNeed {
    term: Term,
    kind: CallableKind,
    parameters: Vec<Term>,
    result: Term,
    call_kind: CallKind,
    parameter_modes: Vec<PassingMode>,
    result_mode: PassingMode,
}
struct Literal {
    term: Term,
    text: String,
    float: bool,
    negative_context: bool,
}
struct Call {
    target: SemanticDefinitionId,
    actuals: Vec<Term>,
    parameters: Vec<Term>,
    closure: bool,
}
struct CodeCall {
    code: Term,
    anchor: LocalAnchor,
}
pub(super) struct Checked {
    arena: Vec<Kind>,
    frames: BTreeMap<SemanticDefinitionId, Frame>,
    calls: BTreeMap<(SemanticDefinitionId, NodeId), Call>,
    intrinsics: BTreeMap<(SemanticDefinitionId, NodeId), sealed::Intrinsic>,
    intrinsic_inputs: BTreeMap<(SemanticDefinitionId, NodeId), Vec<Term>>,
    intrinsic_anchors: BTreeMap<(SemanticDefinitionId, NodeId), LocalAnchor>,
    native_targets: BTreeMap<(SemanticDefinitionId, NodeId), Vec<SemanticDefinitionId>>,
    native_paths: BTreeMap<(SemanticDefinitionId, NodeId), Vec<u32>>,
    abstract_calls: BTreeMap<(SemanticDefinitionId, NodeId), abstract_calls::AbstractCall>,
    trait_calls: BTreeMap<(SemanticDefinitionId, NodeId), traits::TraitCall>,
    task_templates: Vec<tasks::Template>,
    borrows: BTreeMap<(SemanticDefinitionId, NodeId), Borrow>,
    descriptions: BTreeMap<(SemanticDefinitionId, NodeId), Borrow>,
    enum_constructors: BTreeMap<(SemanticDefinitionId, NodeId), enums::Constructor>,
    patterns: BTreeMap<(SemanticDefinitionId, NodeId, Vec<u32>), Term>,
    names_at: BTreeMap<(SemanticDefinitionId, NodeId), Vec<(String, Term)>>,
    environment_reads: BTreeMap<(SemanticDefinitionId, NodeId), u32>,
    capture_origins: BTreeMap<(SemanticDefinitionId, u32), LocalAnchor>,
    function_values: BTreeMap<(SemanticDefinitionId, NodeId), SemanticDefinitionId>,
    code_calls: BTreeMap<(SemanticDefinitionId, NodeId), CodeCall>,
}
struct Solver<'a, 'i> {
    inventory: &'i Inventory<'a>,
    program: &'i SemanticModule,
    indexes: BTreeMap<SemanticDefinitionId, original_index::Index<'a>>,
    state: Checked,
    active: BTreeSet<SemanticDefinitionId>,
    creators: BTreeMap<SemanticDefinitionId, (SemanticDefinitionId, NodeId)>,
    numeric: Vec<Term>,
    scalar_captures: Vec<Term>,
    literals: Vec<Literal>,
    integers: Vec<Term>,
    needs: Vec<Need>,
    callable_needs: Vec<CallableNeed>,
    mutating_closures: BTreeSet<SemanticDefinitionId>,
    role_frames: Vec<roles::RoleFrame>,
    next_role: u32,
    loops: Vec<loops::LoopFrame>,
    trait_state: traits::State,
    capture_state: captures::CaptureState,
}
pub(super) fn resource(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn unknown(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::unsupported(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
impl Solver<'_, '_> {
    fn step(&self, depth: usize, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original operation inference depth exceeded"));
        }
        Ok(())
    }
    fn push<T>(rows: &mut Vec<T>, row: T, b: &mut Budget) -> CompileResult<()> {
        b.spend(rows.len().saturating_add(1), WORK)?;
        rows.try_reserve(1)
            .map_err(|_| resource("original inference inventory allocation failed"))?;
        rows.push(row);
        Ok(())
    }
    fn term(&mut self, kind: Kind, b: &mut Budget) -> CompileResult<Term> {
        let id = self.state.arena.len();
        Self::push(&mut self.state.arena, kind, b)?;
        Ok(id)
    }
    fn var(&mut self, b: &mut Budget) -> CompileResult<Term> {
        self.term(Kind::Infer(None), b)
    }
    fn atom(&mut self, ty: Type, b: &mut Budget) -> CompileResult<Term> {
        if !ty.is_b0_scalar()
            && !matches!(ty, Type::ForeignHandle | Type::ByteBuffer)
            && ty != crate::generic_mutex::error_type()
        {
            return Err(unknown(
                "original complete compound type requires its scoped nominal and lifetime proof",
            ));
        }
        self.term(Kind::Atom(ty), b)
    }
    fn root(&self, mut term: Term, b: &mut Budget) -> CompileResult<Term> {
        let mut hops = 0usize;
        loop {
            b.spend(1, WORK)?;
            let kind = self
                .state
                .arena
                .get(term)
                .ok_or_else(|| fail("original type variable is out of range"))?;
            match kind {
                Kind::Infer(Some(next)) | Kind::LifeVariable(Some(next)) => term = *next,
                _ => return Ok(term),
            }
            hops = hops
                .checked_add(1)
                .ok_or_else(|| fail("original inference cycle counter overflow"))?;
            if hops > self.state.arena.len() {
                return Err(fail("original type variable graph is cyclic"));
            }
        }
    }
    fn type_root(&self, term: Term, b: &mut Budget) -> CompileResult<Term> {
        let mut term = self.root(term, b)?;
        for _ in 0..=b.partial_depth {
            if let Kind::CallResult { hidden, .. } = self.state.arena[term] {
                term = self.root(hidden, b)?;
            } else {
                return Ok(term);
            }
        }
        Err(resource("original result-role type depth exceeded"))
    }
    fn occurs(
        &self,
        needle: Term,
        term: Term,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        if term == needle {
            return Ok(true);
        }
        match &self.state.arena[term] {
            Kind::Tuple(fields) => {
                for field in fields {
                    if self.occurs(needle, *field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            Kind::Nominal {
                arguments, fields, ..
            } => {
                for field in arguments.iter().chain(fields) {
                    if self.occurs(needle, *field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            Kind::Array { element, length } => {
                if self.occurs(needle, *element, depth + 1, b)?
                    || self.occurs(needle, *length, depth + 1, b)?
                {
                    return Ok(true);
                }
            }
            Kind::ErrorUnion { error, result } => {
                if self.occurs(needle, *error, depth + 1, b)?
                    || self.occurs(needle, *result, depth + 1, b)?
                {
                    return Ok(true);
                }
            }
            Kind::Code {
                parameters, result, ..
            } => {
                for term in parameters.iter().chain(std::iter::once(result)) {
                    if self.occurs(needle, *term, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            Kind::Wrapped(_, element) => {
                if self.occurs(needle, *element, depth + 1, b)? {
                    return Ok(true);
                }
            }
            Kind::Desc { life, inner }
            | Kind::MutexGuard {
                life,
                payload: inner,
            } => {
                if self.occurs(needle, *life, depth + 1, b)?
                    || self.occurs(needle, *inner, depth + 1, b)?
                {
                    return Ok(true);
                }
            }
            Kind::Slice(element) => {
                if self.occurs(needle, *element, depth + 1, b)? {
                    return Ok(true);
                }
            }
            Kind::Task(task) => {
                for term in task.terms() {
                    if self.occurs(needle, term, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            Kind::Address { life, inner, .. } => {
                if self.occurs(needle, *life, depth + 1, b)?
                    || self.occurs(needle, *inner, depth + 1, b)?
                {
                    return Ok(true);
                }
            }
            Kind::CallResult { hidden, .. } => {
                if self.occurs(needle, *hidden, depth + 1, b)? {
                    return Ok(true);
                }
            }
            Kind::ConstRigid { carrier, .. } => {
                if self.occurs(needle, *carrier, depth + 1, b)? {
                    return Ok(true);
                }
            }
            Kind::Associated {
                receiver,
                arguments,
                ..
            } => {
                if self.occurs(needle, *receiver, depth + 1, b)? {
                    return Ok(true);
                }
                for argument in arguments {
                    if self.occurs(needle, *argument, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            Kind::Closure {
                parameters,
                result,
                environment,
                ..
            } => {
                for field in parameters
                    .iter()
                    .chain(std::iter::once(result))
                    .chain(environment)
                {
                    if self.occurs(needle, *field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
    fn unify(
        &mut self,
        left: Term,
        right: Term,
        depth: usize,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let left = self.root(left, b)?;
        let right = self.root(right, b)?;
        if left == right {
            return Ok(());
        }
        if matches!(self.state.arena[left], Kind::LifeVariable(None)) {
            if !matches!(
                self.state.arena[right],
                Kind::Life(_) | Kind::LifeVariable(_)
            ) {
                return Err(fail("original lifetime equation changes its declared kind"));
            }
            self.state.arena[left] = Kind::LifeVariable(Some(right));
            return Ok(());
        }
        if matches!(self.state.arena[right], Kind::LifeVariable(None)) {
            return self.unify(right, left, depth + 1, b);
        }
        if matches!(self.state.arena[left], Kind::Infer(None)) {
            if self.occurs(left, right, 0, b)? {
                return Err(fail("original type equation is recursive"));
            }
            self.state.arena[left] = Kind::Infer(Some(right));
            return Ok(());
        }
        if matches!(self.state.arena[right], Kind::Infer(None)) {
            return self.unify(right, left, depth + 1, b);
        }
        if let Kind::CallResult { hidden, .. } = self.state.arena[left] {
            return self.unify(hidden, right, depth + 1, b);
        }
        if let Kind::CallResult { hidden, .. } = self.state.arena[right] {
            return self.unify(left, hidden, depth + 1, b);
        }
        // These clones contain only arena indices or a scalar. Pay all child
        // rows before allocating them; no complete Type clone hides here.
        let width = |kind: &Kind| match kind {
            Kind::Tuple(fields) => fields.len(),
            Kind::Code {
                parameters, modes, ..
            } => parameters.len().saturating_add(modes.len() + 1),
            Kind::Nominal {
                arguments, fields, ..
            } => arguments.len().saturating_add(fields.len()),
            Kind::Associated { arguments, .. } => arguments.len().saturating_add(1),
            Kind::Task(task) => task.work(),
            Kind::Closure {
                parameters,
                environment,
                ..
            } => parameters.len().saturating_add(environment.len() + 1),
            _ => 1,
        };
        b.spend(
            width(&self.state.arena[left]).saturating_add(width(&self.state.arena[right])),
            WORK,
        )?;
        match (
            self.state.arena[left].clone(),
            self.state.arena[right].clone(),
        ) {
            (
                Kind::ErrorUnion {
                    error: a,
                    result: ar,
                },
                Kind::ErrorUnion {
                    error: z,
                    result: zr,
                },
            ) => {
                self.unify(a, z, depth + 1, b)?;
                self.unify(ar, zr, depth + 1, b)
            }
            (
                Kind::Code {
                    kind: ak,
                    parameters: a,
                    result: ar,
                    modes: am,
                    result_mode: arm,
                    call_kind: ac,
                },
                Kind::Code {
                    kind: zk,
                    parameters: z,
                    result: zr,
                    modes: zm,
                    result_mode: zrm,
                    call_kind: zc,
                },
            ) if ak == zk && a.len() == z.len() && am == zm && arm == zrm && ac == zc => {
                for (a, z) in a.iter().zip(z) {
                    self.unify(*a, z, depth + 1, b)?;
                }
                self.unify(ar, zr, depth + 1, b)
            }
            (
                Kind::Array {
                    element: a,
                    length: al,
                },
                Kind::Array {
                    element: z,
                    length: zl,
                },
            ) => {
                self.unify(a, z, depth + 1, b)?;
                self.unify(al, zl, depth + 1, b)
            }
            (Kind::Wrapped(a, ai), Kind::Wrapped(z, zi)) if a == z => {
                self.unify(ai, zi, depth + 1, b)
            }
            (Kind::Desc { life: a, inner: ai }, Kind::Desc { life: z, inner: zi }) => {
                self.unify(a, z, depth + 1, b)?;
                self.unify(ai, zi, depth + 1, b)
            }
            (Kind::Slice(a), Kind::Slice(z)) => self.unify(a, z, depth + 1, b),
            (Kind::Task(a), Kind::Task(z)) => self.task_unify(&a, &z, b, depth + 1),
            (
                Kind::MutexGuard {
                    payload: a,
                    life: al,
                },
                Kind::MutexGuard {
                    payload: z,
                    life: zl,
                },
            ) => {
                self.unify(a, z, depth + 1, b)?;
                self.unify(al, zl, depth + 1, b)
            }
            (Kind::Atom(a), Kind::Atom(z)) if a == z => Ok(()),
            (Kind::Life(a), Kind::Life(z)) if a == z => Ok(()),
            (Kind::ConstValue(a), Kind::ConstValue(z)) if a == z => Ok(()),
            (
                Kind::ConstRigid {
                    owner: a,
                    ordinal: ai,
                    carrier: ac,
                },
                Kind::ConstRigid {
                    owner: z,
                    ordinal: zi,
                    carrier: zc,
                },
            ) if a == z && ai == zi => self.unify(ac, zc, depth + 1, b),
            (
                Kind::Address {
                    mutable: a,
                    life: al,
                    inner: at,
                },
                Kind::Address {
                    mutable: z,
                    life: zl,
                    inner: zt,
                },
            ) if a == z => {
                self.unify(al, zl, depth + 1, b)?;
                self.unify(at, zt, depth + 1, b)
            }
            (Kind::Rigid(a, ai), Kind::Rigid(z, zi)) if a == z && ai == zi => Ok(()),
            (Kind::SelfType(a), Kind::SelfType(z)) if a == z => Ok(()),
            (
                Kind::Associated {
                    receiver: ar,
                    declaration: ad,
                    arguments: aa,
                    member: am,
                },
                Kind::Associated {
                    receiver: zr,
                    declaration: zd,
                    arguments: za,
                    member: zm,
                },
            ) if ad == zd && am == zm && aa.len() == za.len() => {
                self.unify(ar, zr, depth + 1, b)?;
                for (a, z) in aa.into_iter().zip(za) {
                    self.unify(a, z, depth + 1, b)?;
                }
                Ok(())
            }
            (
                Kind::Nominal {
                    definition: a,
                    arguments: aa,
                    ..
                },
                Kind::Nominal {
                    definition: z,
                    arguments: za,
                    ..
                },
            ) if a == z && aa.len() == za.len() => {
                for (a, z) in aa.into_iter().zip(za) {
                    self.unify(a, z, depth + 1, b)?;
                }
                Ok(())
            }
            (Kind::Tuple(a), Kind::Tuple(z)) if a.len() == z.len() => {
                for (a, z) in a.into_iter().zip(z) {
                    self.unify(a, z, depth + 1, b)?;
                }
                Ok(())
            }
            (
                Kind::Closure {
                    definition: a,
                    parameters: ap,
                    result: ar,
                    environment: ae,
                },
                Kind::Closure {
                    definition: z,
                    parameters: zp,
                    result: zr,
                    environment: ze,
                },
            ) if a == z && ap.len() == zp.len() && ae.len() == ze.len() => {
                for (a, z) in ae.into_iter().zip(ze) {
                    self.unify(a, z, depth + 1, b)?;
                }
                for (a, z) in ap.into_iter().zip(zp) {
                    self.unify(a, z, depth + 1, b)?;
                }
                self.unify(ar, zr, depth + 1, b)
            }
            _ => {
                let describe = |kind: &Kind| match kind {
                    Kind::Rigid(owner, ordinal) => format!(
                        "Type formal {}:{} ordinal {}",
                        owner.module.0, owner.index, ordinal
                    ),
                    Kind::SelfType(owner) => format!("Self {}:{}", owner.module.0, owner.index),
                    Kind::Nominal { definition, .. } => {
                        format!("nominal {}:{}", definition.module.0, definition.index)
                    }
                    Kind::Atom(ty) => format!("atom {ty:?}"),
                    Kind::Desc { .. } => "description".into(),
                    Kind::Address { mutable, .. } => format!("address mutable={mutable}"),
                    Kind::Wrapped(wrapper, _) => match wrapper {
                        Wrapper::Managed => "owning manager",
                        Wrapper::Store => "Store payload",
                        Wrapper::Mutex => "Mutex payload",
                        Wrapper::Registration => "Registration payload",
                        Wrapper::AsyncRegistration => "AsyncReg payload",
                    }
                    .into(),
                    Kind::Life(life) => format!("life {life:?}"),
                    _ => "complete compound/role term".into(),
                };
                b.spend(256, WORK)?;
                let mut errors =
                    fail("original operation does not typecheck under its rigid declaration");
                errors[0].notes.push(format!(
                    "left {}; right {}",
                    describe(&self.state.arena[left]),
                    describe(&self.state.arena[right])
                ));
                Err(errors)
            }
        }
    }
    fn local(&self, frame: &Frame, name: &str, b: &mut Budget) -> CompileResult<Option<Term>> {
        for local in frame.locals.iter().rev() {
            b.spend(
                local
                    .name
                    .len()
                    .saturating_add(name.len())
                    .saturating_add(1),
                WORK,
            )?;
            if local.name == name {
                return Ok(Some(local.ty));
            }
        }
        Ok(None)
    }
    fn bind(
        &self,
        frame: &mut Frame,
        name: &str,
        ty: Term,
        mutable: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(name.len(), WORK)?;
        Self::push(
            &mut frame.locals,
            Local {
                anchor: None,
                name: name.to_owned(),
                ty,
                mutable,
                used: false,
                borrowed: false,
                initialized: true,
            },
            b,
        )
    }
    fn original_scope(
        &self,
        mut definition: SemanticDefinitionId,
        scope: &SemanticScopeRef,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        match scope {
            SemanticScopeRef::Ancestor(depth) => {
                if *depth as usize > b.partial_depth {
                    return Err(resource(
                        "original lexical ancestor exceeds the finite scope bound",
                    ));
                }
                for _ in 0..*depth {
                    definition = self
                        .inventory
                        .definition(definition, b)?
                        .parent
                        .ok_or_else(|| fail("original type expression leaves its lexical scope"))?;
                }
                Ok(definition)
            }
        }
    }
    fn recipe(
        &mut self,
        frame: &Frame,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            SemanticTypeRecipe::Atom(ty)
                if ty.is_b0_scalar() || matches!(ty, Type::ForeignHandle | Type::ByteBuffer) =>
            {
                self.atom(ty.clone(), b)
            }
            SemanticTypeRecipe::Formal(reference) => {
                let owner = self.original_scope(frame.definition, &reference.scope, b)?;
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state.frames.get(&owner).ok_or_else(|| {
                        fail("original lexical type formal has no checked parent frame")
                    })?
                };
                if !matches!(
                    self.inventory
                        .definition(owner, b)?
                        .parameters
                        .get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail(
                        "original Type reference changes its mixed formal kind",
                    ));
                }
                context
                    .formals
                    .get(reference.ordinal as usize)
                    .copied()
                    .ok_or_else(|| fail("original type formal ordinal is out of range"))
            }
            SemanticTypeRecipe::Array { element, length } => {
                let element = self.recipe(frame, element, b, depth + 1)?;
                let carrier = self.atom(Type::Usize, b)?;
                let length = self.constant(frame, length, carrier, b, depth + 1)?;
                self.term(Kind::Array { element, length }, b)
            }
            SemanticTypeRecipe::Slice(element) => {
                let element = self.recipe(frame, element, b, depth + 1)?;
                self.term(Kind::Slice(element), b)
            }
            SemanticTypeRecipe::Task { result, retained } => {
                self.task_type(frame, result, retained, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let life = self.life(frame, life, b, depth + 1)?;
                let inner = self.recipe(frame, inner, b, depth + 1)?;
                self.term(
                    Kind::Address {
                        mutable: *mutable,
                        life,
                        inner,
                    },
                    b,
                )
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                let owner = self.inventory.definition(*definition, b)?;
                if owner.id != frame.definition || arguments.len() != owner.parameters.len() {
                    return Err(unknown(
                        "original opaque result needs its exact declaration and complete specialization",
                    ));
                }
                for (ordinal, (formal, recipe)) in
                    owner.parameters.iter().zip(arguments).enumerate()
                {
                    let term = self.actual(frame, formal, recipe, b, depth + 1)?;
                    self.unify(term, frame.formals[ordinal], depth + 1, b)?;
                }
                self.recipe(frame, hidden, b, depth + 1)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => self.nominal(frame, *definition, arguments, b, depth + 1),
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                let owner = self.original_scope(frame.definition, scope, b)?;
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state
                        .frames
                        .get(&owner)
                        .ok_or_else(|| fail("original environment has no checked lexical frame"))?
                };
                context
                    .environment
                    .get(*ordinal as usize)
                    .copied()
                    .ok_or_else(|| fail("original environment type slot is absent"))
            }
            SemanticTypeRecipe::Exact(exact)
                if (exact.ty.is_b0_scalar() || exact.ty == crate::generic_mutex::error_type())
                    && exact.life.binder_count == 0
                    && exact.life.slots.is_empty()
                    && exact.life.retained.is_empty()
                    && exact.life.nominal_arguments.is_empty()
                    && exact.classes.is_empty() =>
            {
                self.atom(exact.ty.clone(), b)
            }
            SemanticTypeRecipe::Tuple(fields) if fields.is_empty() => self.atom(Type::Unit, b),
            SemanticTypeRecipe::Tuple(fields) => {
                let mut terms = Vec::new();
                for field in fields {
                    let field = self.recipe(frame, field, b, depth + 1)?;
                    Self::push(&mut terms, field, b)?;
                }
                self.term(Kind::Tuple(terms), b)
            }
            SemanticTypeRecipe::OriginalType { scope, source } => {
                if !matches!(
                    self.inventory.definition(frame.definition, b)?.body,
                    SemanticDefinitionBody::FunctionWithBody { .. }
                ) {
                    return Err(fail(
                        "original inferred Type requires a real checked function body context",
                    ));
                }
                let owner = self.original_scope(frame.definition, scope, b)?;
                let context = if owner == frame.definition {
                    frame
                } else {
                    self.state.frames.get(&owner).ok_or_else(|| {
                        fail("original inferred type has no checked lexical frame")
                    })?
                };
                match source {
                    SemanticOriginalTypeSource::Expression(id) => context.expressions.get(id).copied(),
                    SemanticOriginalTypeSource::Binding(id) => context.bindings.get(id).copied(),
                    SemanticOriginalTypeSource::BindingProjection { statement, pattern } => {
                        b.spend(pattern.len().saturating_add(self.state.patterns.len()+1),WORK)?;
                        self.state.patterns.get(&(owner,*statement,pattern.clone())).copied()
                    },
                    SemanticOriginalTypeSource::Parameter(index) => context.parameters.get(*index as usize).copied(),
                    SemanticOriginalTypeSource::Result => Some(context.result),
                    SemanticOriginalTypeSource::NameAt { at: SemanticOriginalNode::Expression(at), name } => {
                        let rows = self.state.names_at.get(&(owner,*at)).ok_or_else(|| fail("original lexical Type query does not select a checked creator point"))?;
                        let mut found = None;
                        for (original,term) in rows {
                            b.spend(original.len().saturating_add(name.len()),WORK)?;
                            if original == name { found = Some(*term); }
                        }
                        found
                    }

                    _ => None,
                }.ok_or_else(|| unknown("original type inference needs its real expression, binding or dependent call carrier"))
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.recipe(frame, error, b, depth + 1)?;
                let result = self.recipe(frame, result, b, depth + 1)?;
                self.term(Kind::ErrorUnion { error, result }, b)
            }
            SemanticTypeRecipe::Code { kind, signature } => {
                if signature.bound_lifetimes != 0
                    || signature.invocation_lifetimes != 0
                    || signature.call_kind != CallKind::Direct
                    || *kind == SemanticCodeKind::BorrowedFunctionPointer
                {
                    return Err(unknown(
                        "original code pointer needs its complete per-call quantified relation",
                    ));
                }
                let mut parameters = Vec::new();
                let mut modes = Vec::new();
                for parameter in &signature.parameters {
                    if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                        return Err(unknown(
                            "original code pointer needs its complete resource passing relation",
                        ));
                    }
                    let term = self.recipe(frame, &parameter.ty, b, depth + 1)?;
                    if parameter.mode == PassingMode::Copy {
                        Self::push(
                            &mut self.needs,
                            Need {
                                term,
                                predicate: Predicate::Copy,
                            },
                            b,
                        )?;
                    }
                    Self::push(&mut parameters, term, b)?;
                    Self::push(&mut modes, parameter.mode, b)?;
                }
                if !matches!(
                    signature.result_mode,
                    PassingMode::Infer | PassingMode::Copy
                ) {
                    return Err(unknown(
                        "original code pointer needs its complete resource result relation",
                    ));
                }
                let result = self.recipe(frame, &signature.result, b, depth + 1)?;
                self.term(
                    Kind::Code {
                        kind: *kind,
                        parameters,
                        result,
                        modes,
                        result_mode: signature.result_mode,
                        call_kind: signature.call_kind,
                    },
                    b,
                )
            }
            SemanticTypeRecipe::Managed(inner) => {
                self.wrapper(frame, Wrapper::Managed, inner, b, depth + 1)
            }
            SemanticTypeRecipe::OwnedStore(inner) => {
                self.wrapper(frame, Wrapper::Store, inner, b, depth + 1)
            }
            SemanticTypeRecipe::Mutex(inner) => {
                self.wrapper(frame, Wrapper::Mutex, inner, b, depth + 1)
            }
            SemanticTypeRecipe::Registration(inner) => {
                self.wrapper(frame, Wrapper::Registration, inner, b, depth + 1)
            }
            SemanticTypeRecipe::AsyncRegistration(inner) => {
                self.wrapper(frame, Wrapper::AsyncRegistration, inner, b, depth + 1)
            }
            SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let life = self.life(frame, life, b, depth + 1)?;
                let inner = self.recipe(frame, inner, b, depth + 1)?;
                self.term(Kind::Desc { life, inner }, b)
            }
            SemanticTypeRecipe::SelfType { scope } => self.trait_self(frame, *scope, b, depth + 1),
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } => {
                let receiver = self.recipe(frame, receiver, b, depth + 1)?;
                let arguments = self.trait_arguments(frame, application, b, depth + 1)?;
                self.associated_type(
                    frame,
                    receiver,
                    application.declaration,
                    arguments,
                    *member,
                    b,
                    depth + 1,
                )
            }
            _ => Err(unknown(
                "original type constructor requires its complete scoped domain interpretation",
            )),
        }
    }
}

impl Solver<'_, '_> {
    fn snapshot(&mut self, frame: &Frame, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            frame
                .formals
                .len()
                .saturating_add(frame.parameters.len())
                .saturating_add(frame.expressions.len())
                .saturating_add(frame.bindings.len())
                .saturating_add(frame.environment.len())
                .saturating_add(frame.captured.len())
                .saturating_add(self.state.frames.len()),
            WORK,
        )?;
        let mut locals = Vec::new();
        for local in &frame.locals {
            b.spend(local.name.len(), WORK)?;
            Self::push(
                &mut locals,
                Local {
                    anchor: local.anchor,
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
                },
                b,
            )?;
        }
        self.state.frames.insert(
            frame.definition,
            Frame {
                definition: frame.definition,
                formals: frame.formals.clone(),
                parameters: frame.parameters.clone(),
                result: frame.result,
                locals,
                inherited: frame.inherited,
                captured: frame.captured.clone(),
                alive: frame.alive,
                environment: frame.environment.clone(),
                expressions: frame.expressions.clone(),
                bindings: frame.bindings.clone(),
            },
        );
        Ok(())
    }
    fn scalar_literal(&mut self, text: &str, float: bool, b: &mut Budget) -> CompileResult<Term> {
        b.spend(text.len(), WORK)?;
        let suffix = if float {
            [("f32", Type::F32), ("f64", Type::F64)]
                .into_iter()
                .find(|(suffix, _)| text.ends_with(suffix))
        } else {
            [
                ("usize", Type::Usize),
                ("isize", Type::Isize),
                ("u64", Type::U64),
                ("i64", Type::I64),
                ("u32", Type::U32),
                ("i32", Type::I32),
                ("u16", Type::U16),
                ("i16", Type::I16),
                ("u8", Type::U8),
                ("i8", Type::I8),
            ]
            .into_iter()
            .find(|(suffix, _)| text.ends_with(suffix))
        };
        let ty = if let Some((_, ty)) = suffix {
            self.atom(ty, b)?
        } else {
            self.var(b)?
        };
        Self::push(&mut self.numeric, ty, b)?;
        b.spend(text.len(), WORK)?;
        Self::push(
            &mut self.literals,
            Literal {
                term: ty,
                text: text.to_owned(),
                float,
                negative_context: false,
            },
            b,
        )?;
        Ok(ty)
    }
    fn original_site<'a>(
        &self,
        definition: &'a SemanticDefinition,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticSiteRecipe> {
        b.lookup(self.indexes.len(), WORK)?;
        self.indexes
            .get(&definition.id)
            .ok_or_else(|| fail("original operation definition has no complete node index"))?
            .get_expr(node, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(fail("original call has no complete body inventory"));
        };
        b.spend(site_nodes.len().saturating_add(sites.len()), WORK)?;
        let mut found = None;
        for link in site_nodes {
            if link.node != SemanticOriginalNode::Expression(node) {
                continue;
            }
            let recipe = sites
                .iter()
                .find(|recipe| recipe.id == link.site)
                .ok_or_else(|| fail("original node points to an absent semantic site"))?;
            if matches!(
                recipe.kind,
                SemanticSiteRecipeKind::OriginalCall
                    | SemanticSiteRecipeKind::Call { .. }
                    | SemanticSiteRecipeKind::CallableCall { .. }
                    | SemanticSiteRecipeKind::Closure { .. }
            ) {
                if found.replace(recipe).is_some() {
                    return Err(fail("original operation has ambiguous resolution sites"));
                }
            }
        }
        found.ok_or_else(|| fail("original call or closure has no exact resolution site"))
    }
    fn scheme(
        &mut self,
        caller: &Frame,
        target: SemanticDefinitionId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Vec<Term>, Term)> {
        self.step(depth, b)?;
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
            return Err(fail("original callee has no complete declared scheme"));
        };
        if definition.parent.is_some()
            || !definition.environment.is_empty()
            || signature.bound_lifetimes != 0
        {
            return Err(unknown(
                "original lexical/member call needs its complete declared scope and quantifier instantiation",
            ));
        }
        if !written.is_empty() && written.len() != definition.parameters.len() {
            return Err(fail(
                "original written actual vector has another mixed arity",
            ));
        }
        let mut formals = Vec::new();
        for (index, formal) in definition.parameters.iter().enumerate() {
            let term = match (formal, written.get(index)) {
                (
                    SemanticFormal::Type { requirements },
                    Some(CoreGenericArgument::Type(recipe)),
                ) => {
                    self.premises_well_formed(target, requirements, b)?;
                    self.recipe(caller, recipe, b, depth + 1)?
                }
                (SemanticFormal::Type { requirements }, None) => {
                    self.premises_well_formed(target, requirements, b)?;
                    self.var(b)?
                }
                (SemanticFormal::Lifetime, Some(CoreGenericArgument::Lifetime(name))) => {
                    self.named_life(caller, name, b, depth + 1)?
                }
                (SemanticFormal::Lifetime, None) => self.term(Kind::LifeVariable(None), b)?,
                (SemanticFormal::Const { .. }, _) => {
                    return Err(unknown(
                        "original call constant inference needs its complete carrier and expression proof",
                    ));
                }
                _ => {
                    return Err(fail(
                        "original explicit actual changes its full mixed formal kind",
                    ));
                }
            };
            Self::push(&mut formals, term, b)?;
        }
        self.premises_well_formed(target, &definition.requirements, b)?;
        let mut frame = Frame {
            definition: target,
            formals,
            parameters: Vec::new(),
            result: self.var(b)?,
            locals: Vec::new(),
            inherited: 0,
            captured: BTreeMap::new(),
            alive: true,
            environment: Vec::new(),
            expressions: BTreeMap::new(),
            bindings: BTreeMap::new(),
        };
        // Prove the opaque declaration in its own rigid domain before any
        // caller-specific signature namespace exists. Never execute its body
        // under a first caller's implicit lifetime substitution.
        let opaque = if matches!(signature.result, SemanticTypeRecipe::Opaque { .. }) {
            if !self.state.frames.contains_key(&target) {
                if self.active.contains(&target) {
                    return Err(unknown(
                        "recursive opaque result needs its complete original callable fixed point",
                    ));
                }
                self.function(target, None, b, depth + 1)?;
            }
            let checked = self
                .state
                .frames
                .get(&target)
                .ok_or_else(|| fail("original opaque result has no checked declaration body"))?
                .result;
            Some(self.instantiate(checked, target, &frame.formals, b, depth + 1)?)
        } else {
            None
        };
        let mut implicits = BTreeMap::new();
        for row in &definition.implicit_lifetimes {
            b.spend(row.occurrence_path.len().saturating_add(1), WORK)?;
            let life = self.term(Kind::LifeVariable(None), b)?;
            b.lookup(implicits.len(), WORK)?;
            if implicits.insert(row.ordinal, life).is_some() {
                return Err(fail(
                    "original call repeats an implicit declaration occurrence",
                ));
            }
        }
        let lifetimes = self.invocation_actuals(signature.invocation_lifetimes, b)?;
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: target,
                lifetimes,
                implicits,
            },
            b,
        )?;
        let answer: CompileResult<Term> = (|| {
            for parameter in &signature.parameters {
                let term = self.recipe(&frame, &parameter.ty, b, depth + 1)?;
                Self::push(&mut frame.parameters, term, b)?;
            }
            for formal in &definition.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.premise_needs(&frame, requirements, b)?;
                }
            }
            self.premise_needs(&frame, &definition.requirements, b)?;
            let result = if let Some(opaque) = opaque {
                opaque
            } else {
                self.recipe(&frame, &signature.result, b, depth + 1)?
            };
            self.unify(frame.result, result, depth + 1, b)?;
            Ok(result)
        })();
        self.role_frames
            .pop()
            .ok_or_else(|| fail("original static call lost its local signature frame"))?;
        let result = answer?;
        Ok((frame.formals, frame.parameters, result))
    }
    fn expression(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let result: CompileResult<Term> = (|| {
            self.step(depth, b)?;
            b.lookup(self.indexes.len(), WORK)?;
            self.indexes
                .get(&frame.definition)
                .ok_or_else(|| fail("original expression has no definition index"))?
                .get_expr(value.id, b)?;
            if frame.expressions.contains_key(&value.id) {
                return Err(fail("original expression identity is reused"));
            }
            if let Some(term) = self.function_value(frame, value, b, depth + 1)? {
                b.lookup(frame.expressions.len(), WORK)?;
                frame.expressions.insert(value.id, term);
                return Ok(term);
            }
            let ty = match &value.kind {
                CoreExprKind::Integer(text) => self.scalar_literal(text, false, b)?,
                CoreExprKind::Float(text) => self.scalar_literal(text, true, b)?,
                CoreExprKind::Literal(value) if value.ty().is_b0_scalar() => {
                    self.atom(value.ty(), b)?
                }
                CoreExprKind::Bool(_) => self.atom(Type::Bool, b)?,
                CoreExprKind::Unit => self.atom(Type::Unit, b)?,
                CoreExprKind::Name(name) => {
                    let mut selected = None;
                    for (index, local) in frame.locals.iter().enumerate().rev() {
                        b.spend(local.name.len().saturating_add(name.len()), WORK)?;
                        if local.name == *name {
                            selected = Some(index);
                            break;
                        }
                    }
                    let index = selected.ok_or_else(|| {
                        fail("original name is not in the complete lexical value context")
                    })?;
                    if index < frame.inherited {
                        self.record_environment_read(frame, index, value.id, b)?;
                        b.lookup(frame.captured.len(), WORK)?;
                        frame.captured.insert(index, frame.locals[index].ty);
                    }
                    self.capture_read(frame, index, b)?;
                    let local = &mut frame.locals[index];
                    if frame.alive {
                        if !local.initialized {
                            return Err(fail("original value read is not initialized"));
                        }
                        if local.used || local.borrowed {
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term: local.ty,
                                    predicate: Predicate::Copy,
                                },
                                b,
                            )?;
                        }
                        local.used = true;
                        if index < frame.inherited {
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term: local.ty,
                                    predicate: Predicate::Value,
                                },
                                b,
                            )?;
                        }
                    }
                    local.ty
                }
                CoreExprKind::Array(values) => {
                    let element = self.var(b)?;
                    for value in values {
                        let actual = self.expression(frame, value, b, depth + 1)?;
                        self.unify(actual, element, depth + 1, b)?;
                    }
                    let length = u64::try_from(values.len())
                        .map_err(|_| resource("original array length exceeds its carrier"))?;
                    let length = self.term(Kind::ConstValue(Constant::Usize(length)), b)?;
                    self.term(Kind::Array { element, length }, b)?
                }
                CoreExprKind::Index { object, index } => {
                    self.index_read(frame, object, index, b, depth + 1)?
                }
                CoreExprKind::Tuple(fields) => {
                    if fields.is_empty() {
                        self.atom(Type::Unit, b)?
                    } else {
                        let mut terms = Vec::new();
                        for field in fields {
                            let term = self.expression(frame, field, b, depth + 1)?;
                            Self::push(&mut terms, term, b)?;
                        }
                        self.term(Kind::Tuple(terms), b)?
                    }
                }
                CoreExprKind::Group(value) | CoreExprKind::CompileTime(value) => {
                    self.expression(frame, value, b, depth + 1)?
                }
                CoreExprKind::Try(value) => {
                    let value = self.expression(frame, value, b, depth + 1)?;
                    let Kind::ErrorUnion { error, result } =
                        self.state.arena[self.type_root(value, b)?]
                    else {
                        return Err(fail(
                            "original propagation requires a complete ErrorUnion value",
                        ));
                    };
                    let returned = self.type_root(frame.result, b)?;
                    let expected_error = match self.state.arena[returned] {
                        Kind::ErrorUnion { error, .. } => error,
                        Kind::Infer(None) => {
                            let success = self.var(b)?;
                            let output = self.term(
                                Kind::ErrorUnion {
                                    error,
                                    result: success,
                                },
                                b,
                            )?;
                            self.unify(frame.result, output, depth + 1, b)?;
                            error
                        }
                        _ => {
                            return Err(fail(
                                "original propagation error edge has no ErrorUnion return context",
                            ));
                        }
                    };
                    self.unify(error, expected_error, depth + 1, b)?;
                    if frame.alive {
                        self.capture_exit(frame, b)?;
                        self.cleanup(&frame.locals[frame.inherited..], b)?;
                    }
                    result
                }
                CoreExprKind::NumericConvert {
                    value: input,
                    target,
                } => {
                    let input = self.expression(frame, input, b, depth + 1)?;
                    let output = self.recipe(frame, target, b, depth + 1)?;
                    // Both endpoints must be numeric under the original Γ. A
                    // selected concrete header cannot make an abstract T numeric.
                    // Actual checked conversion and overflow remain kernel facts.
                    Self::push(&mut self.numeric, input, b)?;
                    Self::push(&mut self.numeric, output, b)?;
                    output
                }
                CoreExprKind::IntegerConvert {
                    value: input,
                    target,
                } => {
                    let input = self.expression(frame, input, b, depth + 1)?;
                    let output = self.recipe(frame, target, b, depth + 1)?;
                    Self::push(&mut self.integers, input, b)?;
                    Self::push(&mut self.integers, output, b)?;
                    Self::push(&mut self.numeric, input, b)?;
                    Self::push(&mut self.numeric, output, b)?;
                    output
                }
                CoreExprKind::Ascribed { ty, value } => {
                    let actual = self.expression(frame, value, b, depth + 1)?;
                    let expected = self.recipe(frame, ty, b, depth + 1)?;
                    self.unify(actual, expected, depth + 1, b)?;
                    expected
                }
                CoreExprKind::Binary { op, left, right } => {
                    let left = self.expression(frame, left, b, depth + 1)?;
                    let right = self.expression(frame, right, b, depth + 1)?;
                    self.unify(left, right, depth + 1, b)?;
                    match op {
                        BinaryOp::And | BinaryOp::Or => {
                            let boolean = self.atom(Type::Bool, b)?;
                            self.unify(left, boolean, depth + 1, b)?;
                            boolean
                        }
                        BinaryOp::Equal | BinaryOp::NotEqual => {
                            Self::push(&mut self.scalar_captures, left, b)?;
                            self.atom(Type::Bool, b)?
                        }
                        BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual => {
                            Self::push(&mut self.numeric, left, b)?;
                            self.atom(Type::Bool, b)?
                        }
                        BinaryOp::AddWrapping
                        | BinaryOp::SubtractWrapping
                        | BinaryOp::MultiplyWrapping
                        | BinaryOp::BitAnd
                        | BinaryOp::BitOr
                        | BinaryOp::BitXor
                        | BinaryOp::ShiftLeft
                        | BinaryOp::ShiftRight
                        | BinaryOp::Remainder => {
                            Self::push(&mut self.integers, left, b)?;
                            Self::push(&mut self.numeric, left, b)?;
                            left
                        }
                        _ => {
                            Self::push(&mut self.numeric, left, b)?;
                            left
                        }
                    }
                }
                CoreExprKind::Unary { op, operand }
                    if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut) =>
                {
                    if matches!(&operand.kind, CoreExprKind::Slice { .. }) {
                        let term = self.slice_borrow(
                            frame,
                            value.id,
                            operand,
                            *op == UnaryOp::BorrowMut,
                            b,
                            depth + 1,
                        )?;
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original range borrow node is reused"));
                        }
                        return Ok(term);
                    }
                    let CoreExprKind::Name(name) = &operand.kind else {
                        return Err(unknown(
                            "original borrow needs its exact initialized place projection",
                        ));
                    };
                    let inner = self
                        .local(frame, name, b)?
                        .ok_or_else(|| fail("original borrow selects no lexical local"))?;
                    let mut anchor = None;
                    for local in frame.locals.iter().rev() {
                        b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                        if local.name == *name {
                            if !local.initialized {
                                return Err(fail("original borrowed place is uninitialized"));
                            }
                            if *op == UnaryOp::BorrowMut && !local.mutable {
                                return Err(fail(
                                    "original mutable borrow requires a mutable place",
                                ));
                            }
                            anchor = local.anchor;
                            if local.used {
                                Self::push(
                                    &mut self.needs,
                                    Need {
                                        term: local.ty,
                                        predicate: Predicate::Copy,
                                    },
                                    b,
                                )?;
                            }
                            break;
                        }
                    }
                    let anchor = anchor.ok_or_else(|| {
                        unknown("original borrow needs its exact parameter or binding anchor")
                    })?;
                    b.lookup(self.state.borrows.len(), WORK)?;
                    if self
                        .state
                        .borrows
                        .insert(
                            (frame.definition, value.id),
                            Borrow {
                                anchor,
                                inner,
                                slice: None,
                            },
                        )
                        .is_some()
                    {
                        return Err(fail(
                            "original borrow occurrence has competing source binders",
                        ));
                    }
                    let row = self.borrow_occurrence(frame.definition, value.id, b)?;
                    // Read the original Name's type without moving its occupant.
                    b.spend(frame.expressions.len().saturating_add(1), WORK)?;
                    if frame.expressions.insert(operand.id, inner).is_some() {
                        return Err(fail("original borrowed place node is reused"));
                    }
                    let life = self.term(Kind::Life(Life::Implicit(frame.definition, row)), b)?;
                    self.term(
                        Kind::Address {
                            mutable: *op == UnaryOp::BorrowMut,
                            life,
                            inner,
                        },
                        b,
                    )?
                }
                CoreExprKind::Unary {
                    op: UnaryOp::Give,
                    operand,
                } => {
                    let value = self.expression(frame, operand, b, depth + 1)?;
                    if !matches!(
                        self.state.arena[self.type_root(value, b)?],
                        Kind::Wrapped(Wrapper::Managed, _)
                    ) {
                        return Err(unknown(
                            "original give requires its complete actual owning manager category",
                        ));
                    }
                    value
                }
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand,
                } => {
                    let address = self.expression(frame, operand, b, depth + 1)?;
                    let root = self.root(address, b)?;
                    let Kind::Address { inner, .. } = self.state.arena[root] else {
                        return Err(fail("original dereference does not select an address"));
                    };
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: inner,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                    inner
                }
                CoreExprKind::Unary { op, operand } => {
                    let mut literal = operand.as_ref();
                    while let CoreExprKind::Group(inner) = &literal.kind {
                        self.step(depth, b)?;
                        literal = inner;
                    }
                    let negative =
                        *op == UnaryOp::Negate && matches!(literal.kind, CoreExprKind::Integer(_));
                    let before = self.literals.len();
                    let operand = self.expression(frame, operand, b, depth + 1)?;
                    if negative && self.literals.len() == before.saturating_add(1) {
                        self.literals[before].negative_context = true;
                    }
                    match op {
                        UnaryOp::Not => {
                            let boolean = self.atom(Type::Bool, b)?;
                            self.unify(operand, boolean, depth + 1, b)?;
                            boolean
                        }
                        UnaryOp::BitNot => {
                            Self::push(&mut self.integers, operand, b)?;
                            Self::push(&mut self.numeric, operand, b)?;
                            operand
                        }
                        UnaryOp::Negate => {
                            Self::push(&mut self.numeric, operand, b)?;
                            operand
                        }
                        _ => {
                            return Err(unknown(
                                "original borrow, dereference or owner transfer needs its complete place and lifetime context",
                            ));
                        }
                    }
                }
                CoreExprKind::Call { callee, arguments } => {
                    let definition = self.inventory.definition(frame.definition, b)?;
                    let recipe = self.original_site(definition, value.id, b)?;
                    if !matches!(
                        recipe.kind,
                        SemanticSiteRecipeKind::OriginalCall
                            | SemanticSiteRecipeKind::Call { .. }
                            | SemanticSiteRecipeKind::CallableCall { .. }
                    ) {
                        return Err(fail("original call is linked to another operation kind"));
                    }
                    if let Some(term) =
                        self.slice_call(frame, value.id, callee, arguments, b, depth + 1)?
                    {
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original slice operation node is reused"));
                        }
                        return Ok(term);
                    }
                    if let Some(term) =
                        self.enum_call(frame, value.id, callee, arguments, b, depth + 1)?
                    {
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original constructor node is reused"));
                        }
                        return Ok(term);
                    }
                    if let Some(term) =
                        self.original_trait_call(frame, value.id, callee, arguments, b, depth + 1)?
                    {
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original trait call node is reused"));
                        }
                        return Ok(term);
                    }
                    let mut head = callee.as_ref();
                    while let CoreExprKind::Group(inner) = &head.kind {
                        self.step(depth, b)?;
                        head = inner;
                    }
                    let mut type_written = Vec::new();
                    let (head, written) = match &head.kind {
                        CoreExprKind::GenericApply { callee, arguments } => {
                            (callee.as_ref(), arguments.as_slice())
                        }
                        CoreExprKind::TypeApply {
                            callee,
                            type_arguments,
                        } => {
                            for argument in type_arguments {
                                crate::dyn_metadata::recipe(argument, b)?;
                                Self::push(
                                    &mut type_written,
                                    CoreGenericArgument::Type(argument.clone()),
                                    b,
                                )?;
                            }
                            (callee.as_ref(), type_written.as_slice())
                        }
                        _ => (head, &[][..]),
                    };
                    if matches!(
                        head.kind,
                        CoreExprKind::GenericApply { .. } | CoreExprKind::TypeApply { .. }
                    ) {
                        return Err(unknown(
                            "original nested generic application needs every mixed layer interpreted",
                        ));
                    }
                    let local = if let CoreExprKind::Name(name) = &head.kind {
                        self.local(frame, name, b)?
                    } else {
                        None
                    };
                    let (parameters, result) = if let Some(local) = local {
                        if let CoreExprKind::Name(name) = &head.kind {
                            b.spend(
                                frame
                                    .locals
                                    .iter()
                                    .map(|local| local.name.len().saturating_add(name.len() + 1))
                                    .sum::<usize>(),
                                WORK,
                            )?;
                            if let Some(index) =
                                frame.locals.iter().rposition(|local| local.name == *name)
                                && index < frame.inherited
                            {
                                b.lookup(frame.captured.len(), WORK)?;
                                frame.captured.insert(index, local);
                            }
                        }
                        if !written.is_empty() {
                            return Err(unknown(
                                "original generic callable needs its complete method instantiation",
                            ));
                        }
                        if let Some((parameters, result)) =
                            self.abstract_scheme(frame, head, value.id, local, b, depth + 1)?
                        {
                            (parameters, result)
                        } else if let Some(call) =
                            self.code_call(frame, head, value.id, local, b, depth + 1)?
                        {
                            call
                        } else {
                            let mut local = self.root(local, b)?;
                            for _ in 0..=b.partial_depth {
                                if let Kind::CallResult { hidden, .. } = self.state.arena[local] {
                                    local = self.root(hidden, b)?;
                                } else {
                                    break;
                                }
                            }
                            b.spend(
                                match &self.state.arena[local] {
                                    Kind::Closure {
                                        parameters,
                                        environment,
                                        ..
                                    } => parameters.len().saturating_add(environment.len() + 1),
                                    _ => 1,
                                },
                                WORK,
                            )?;
                            let Kind::Closure {
                                definition: target,
                                parameters,
                                result,
                                ..
                            } = self.state.arena[local].clone()
                            else {
                                return Err(unknown(
                                    "original indirect call needs its full callable premise",
                                ));
                            };
                            self.known_capture_call(frame, head, target, b)?;
                            let declaration = self.inventory.definition(target, b)?;
                            let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                                &declaration.body
                            else {
                                return Err(fail(
                                    "original local callable has no full declared invocation signature",
                                ));
                            };
                            let lives =
                                self.invocation_actuals(signature.invocation_lifetimes, b)?;
                            let mut opened = Vec::new();
                            for parameter in parameters {
                                let parameter =
                                    self.open_invocations(parameter, target, &lives, b, depth + 1)?;
                                Self::push(&mut opened, parameter, b)?;
                            }
                            let parameters = opened;
                            let result =
                                self.open_invocations(result, target, &lives, b, depth + 1)?;
                            b.spend(
                                self.state.calls.len().saturating_add(parameters.len() + 1),
                                WORK,
                            )?;
                            self.state.calls.insert(
                                (frame.definition, value.id),
                                Call {
                                    target,
                                    actuals: Vec::new(),
                                    parameters: parameters.clone(),
                                    closure: true,
                                },
                            );
                            (parameters, result)
                        }
                    } else {
                        let path = sealed::path(head, b, depth + 1)?.ok_or_else(|| {
                            unknown("original expression call needs its full source resolution")
                        })?;
                        // A qualified lexical path is resolved by its complete
                        // Names binding. A value root shadows every static path;
                        // neither a member spelling nor ABI shape selects code.
                        let mut names = BTreeSet::new();
                        for local in &frame.locals {
                            b.spend(local.name.len().saturating_add(names.len() + 1), WORK)?;
                            names.insert(local.name.clone());
                        }
                        let resolution =
                            sealed::resolve(self.inventory, frame.definition, &path, &names, b)?;
                        match resolution {
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::StoreNew
                                | sealed::Intrinsic::StoreReserve
                                | sealed::Intrinsic::StorePush
                                | sealed::Intrinsic::StorePop
                                | sealed::Intrinsic::StoreLen
                                | sealed::Intrinsic::StoreCapacity
                                | sealed::Intrinsic::StoreAt
                                | sealed::Intrinsic::StoreAtMut),
                            ) => {
                                let result = self.store_call(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original Store call node is reused"));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::BytesNew
                                | sealed::Intrinsic::BytesReserve
                                | sealed::Intrinsic::BytesPush
                                | sealed::Intrinsic::BytesTruncate
                                | sealed::Intrinsic::BytesLen
                                | sealed::Intrinsic::BytesCapacity),
                            ) => {
                                let result = self.buffer_call(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original ByteBuffer call node is reused"));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::Register
                                | sealed::Intrinsic::RegisterAsync),
                            ) => {
                                let result = self.registration_start(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail(
                                        "original registration installation node is reused",
                                    ));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::AsyncRequest
                                | sealed::Intrinsic::AsyncPoll
                                | sealed::Intrinsic::AsyncFinish
                                | sealed::Intrinsic::AsyncFinishChecked
                                | sealed::Intrinsic::AsyncValid
                                | sealed::Intrinsic::RegistrationValid
                                | sealed::Intrinsic::RegistrationClose),
                            ) => {
                                let result = self.registration_call(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original registration call node is reused"));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(sealed::Intrinsic::TaskYield) => {
                                if !written.is_empty() || !arguments.is_empty() {
                                    return Err(fail(
                                        "original task_yield has neither generic nor value arguments",
                                    ));
                                }
                                b.lookup(self.state.intrinsics.len(), WORK)?;
                                if self
                                    .state
                                    .intrinsics
                                    .insert(
                                        (frame.definition, value.id),
                                        sealed::Intrinsic::TaskYield,
                                    )
                                    .is_some()
                                {
                                    return Err(fail(
                                        "original core operation has competing scheme resolutions",
                                    ));
                                }
                                (Vec::new(), self.atom(Type::Unit, b)?)
                            }
                            sealed::Resolution::Intrinsic(
                                id @ sealed::Intrinsic::NumericConvert(number),
                            ) => {
                                if !written.is_empty() || arguments.len() != 1 {
                                    return Err(fail(
                                        "original numeric conversion changes its closed source scheme",
                                    ));
                                }
                                let input = self.var(b)?;
                                Self::push(&mut self.numeric, input, b)?;
                                b.lookup(self.state.intrinsics.len(), WORK)?;
                                if self
                                    .state
                                    .intrinsics
                                    .insert((frame.definition, value.id), id)
                                    .is_some()
                                {
                                    return Err(fail(
                                        "original numeric conversion has competing resolutions",
                                    ));
                                }
                                b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
                                self.state
                                    .intrinsic_inputs
                                    .insert((frame.definition, value.id), vec![input]);
                                (vec![input], self.atom(number.ty(), b)?)
                            }
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::MutexCreate
                                | sealed::Intrinsic::MutexLock
                                | sealed::Intrinsic::MutexValue
                                | sealed::Intrinsic::ThreadScope),
                            ) => {
                                let result = self.mutex_call(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail(
                                        "original synchronization call node is reused",
                                    ));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(
                                id @ (sealed::Intrinsic::TaskCreate
                                | sealed::Intrinsic::TaskPoll
                                | sealed::Intrinsic::TaskPollClose
                                | sealed::Intrinsic::TaskCancel
                                | sealed::Intrinsic::TaskFinish),
                            ) => {
                                let result = self.task_call(
                                    frame,
                                    value.id,
                                    id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original Task call node is reused"));
                                }
                                return Ok(result);
                            }
                            sealed::Resolution::Intrinsic(_) => {
                                return Err(unknown(
                                    "original core operation needs its complete parameter, mode and cleanup scheme",
                                ));
                            }
                            sealed::Resolution::Definition(target) => {
                                let (actuals, parameters, result) =
                                    self.scheme(frame, target, written, b, depth + 1)?;
                                b.spend(
                                    self.state
                                        .calls
                                        .len()
                                        .saturating_add(actuals.len())
                                        .saturating_add(parameters.len()),
                                    WORK,
                                )?;
                                self.state.calls.insert(
                                    (frame.definition, value.id),
                                    Call {
                                        target,
                                        actuals,
                                        parameters: parameters.clone(),
                                        closure: false,
                                    },
                                );
                                (parameters, result)
                            }
                            sealed::Resolution::Deferred => {
                                return Err(unknown(
                                    "original call has no complete lexical declaration or core scheme",
                                ));
                            }
                        }
                    };
                    if parameters.len() != arguments.len() {
                        return Err(fail(
                            "original call value arity differs from its declared scheme",
                        ));
                    }
                    for (argument, parameter) in arguments.iter().zip(parameters) {
                        self.call_argument(frame, argument, parameter, b, depth + 1)?;
                    }
                    self.finish_abstract_call(frame.definition, value.id, b)?;
                    b.lookup(self.state.intrinsics.len(), WORK)?;
                    if self
                        .state
                        .intrinsics
                        .contains_key(&(frame.definition, value.id))
                        || self
                            .state
                            .abstract_calls
                            .contains_key(&(frame.definition, value.id))
                        || self
                            .state
                            .code_calls
                            .contains_key(&(frame.definition, value.id))
                    {
                        // Unit has no hidden callee result domain. Its actual opcode
                        // is bound separately at this exact original call site.
                        result
                    } else {
                        self.term(
                            Kind::CallResult {
                                caller: frame.definition,
                                node: value.id,
                                hidden: result,
                            },
                            b,
                        )?
                    }
                }
                CoreExprKind::Record { ty, fields } => {
                    let term = self.recipe(frame, ty, b, depth + 1)?;
                    self.record_fields(frame, term, fields, true, b, depth + 1)?;
                    if self.resource_payload(term, b)? {
                        self.term(Kind::Wrapped(Wrapper::Managed, term), b)?
                    } else {
                        term
                    }
                }
                CoreExprKind::RecordUpdate { ty, base, fields } => {
                    let term = self.expression(frame, base, b, depth + 1)?;
                    if let Some(ty) = ty {
                        let ty = self.recipe(frame, ty, b, depth + 1)?;
                        self.unify(term, ty, depth + 1, b)?;
                    }
                    self.record_fields(frame, term, fields, false, b, depth + 1)?;
                    term
                }
                CoreExprKind::Member {
                    object,
                    member,
                    owner,
                } => {
                    if let Some(term) =
                        self.copy_member(frame, object, member, owner, b, depth + 1)?
                    {
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original Copy-field node is reused"));
                        }
                        return Ok(term);
                    }
                    let term = self.expression(frame, object, b, depth + 1)?;
                    if let Some(owner) = owner {
                        let owner = self.recipe(frame, owner, b, depth + 1)?;
                        self.unify(term, owner, depth + 1, b)?;
                    }
                    self.member(term, member, b, depth + 1)?
                }
                CoreExprKind::Closure {
                    parameters,
                    return_type,
                    body,
                    move_capture,
                } => {
                    let definition = self.inventory.definition(frame.definition, b)?;
                    let recipe = self.original_site(definition, value.id, b)?;
                    let SemanticSiteRecipeKind::Closure {
                        definition: target, ..
                    } = recipe.kind
                    else {
                        return Err(fail(
                            "original closure node is linked to another operation kind",
                        ));
                    };
                    let child = self.inventory.definition(target, b)?;
                    if child.parent != Some(frame.definition)
                        || child.kind != SemanticDefinitionKind::Closure
                    {
                        return Err(fail(
                            "original closure changes its lexical declaration owner",
                        ));
                    }
                    b.spend(self.creators.len().saturating_add(1), WORK)?;
                    if self
                        .creators
                        .insert(target, (frame.definition, value.id))
                        .is_some()
                    {
                        return Err(fail(
                            "one original closure declaration has multiple creation nodes",
                        ));
                    }
                    // Until complete symbolic ownership/class domains are handled,
                    // abstract owning captures cannot become scalar environments.
                    let mut names = Vec::new();
                    for local in &frame.locals {
                        b.spend(local.name.len(), WORK)?;
                        Self::push(&mut names, (local.name.clone(), local.ty), b)?;
                    }
                    b.spend(self.state.names_at.len().saturating_add(1), WORK)?;
                    if self
                        .state
                        .names_at
                        .insert((frame.definition, value.id), names)
                        .is_some()
                    {
                        return Err(fail("original creator lexical point is reused"));
                    }
                    self.snapshot(frame, b)?;
                    let term = self.function(target, Some(&frame.locals), b, depth + 1)?;
                    let captured = &self
                        .state
                        .frames
                        .get(&target)
                        .ok_or_else(|| fail("original child capture footprint is absent"))?
                        .captured;
                    b.spend(captured.len(), WORK)?;
                    let captures = captured
                        .iter()
                        .map(|(index, ty)| (*index, *ty))
                        .collect::<Vec<_>>();
                    for (ordinal, environment) in child.environment.iter().enumerate() {
                        crate::dyn_metadata::recipe(&environment.source, b)?;
                        let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source:
                                SemanticOriginalTypeSource::NameAt {
                                    at: SemanticOriginalNode::Expression(at),
                                    name,
                                },
                        }) = &environment.source
                        else {
                            continue;
                        };
                        if *at != value.id {
                            return Err(fail(
                                "captured lexical value is queried at another original creator",
                            ));
                        }
                        let index=frame.locals.iter().rposition(|local|local.name==*name)
                            .ok_or_else(||fail("captured lexical value has no original name binding at its creator"))?;
                        b.spend(
                            frame
                                .locals
                                .iter()
                                .map(|local| local.name.len() + name.len() + 1)
                                .sum(),
                            WORK,
                        )?;
                        if !captures.iter().any(|(captured, _)| *captured == index) {
                            continue;
                        }
                        let anchor = match frame.locals[index].anchor {
                            Some(anchor) => anchor,
                            None if index < frame.inherited => {
                                self.environment_anchor(frame, name, b)?
                            }
                            None => {
                                return Err(unknown(
                                    "captured lexical binding needs its complete original pattern projection",
                                ));
                            }
                        };
                        let ordinal = u32::try_from(ordinal)
                            .map_err(|_| resource("original capture ordinal overflow"))?;
                        b.lookup(self.state.capture_origins.len(), WORK)?;
                        if self
                            .state
                            .capture_origins
                            .insert((target, ordinal), anchor)
                            .is_some()
                        {
                            return Err(fail(
                                "one original environment value has several creator bindings",
                            ));
                        }
                    }
                    for (index, ty) in captures {
                        if index < frame.inherited {
                            b.lookup(frame.captured.len(), WORK)?;
                            frame.captured.insert(index, ty);
                        }
                        if *move_capture {
                            let local = frame.locals.get(index).ok_or_else(|| {
                                fail("original move capture loses its parent lexical binding")
                            })?;
                            if !local.initialized {
                                return Err(fail(
                                    "original move capture reads an uninitialized parent binding",
                                ));
                            }
                            if local.used || local.borrowed {
                                Self::push(
                                    &mut self.needs,
                                    Need {
                                        term: ty,
                                        predicate: Predicate::Copy,
                                    },
                                    b,
                                )?;
                            }
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term: ty,
                                    predicate: Predicate::Value,
                                },
                                b,
                            )?;
                            if frame.alive {
                                frame.locals[index].used = true;
                            }
                        }
                    }
                    let root = self.root(term, b)?;
                    let Kind::Closure {
                        parameters: expected,
                        result,
                        ..
                    } = &self.state.arena[root]
                    else {
                        return Err(fail("original child declaration has no callable type"));
                    };
                    b.spend(expected.len() + 1, WORK)?;
                    let expected = expected.clone();
                    let result = *result;
                    if expected.len() != parameters.len() {
                        return Err(fail(
                            "original closure literal changes the child parameter arity",
                        ));
                    }
                    let SemanticDefinitionBody::FunctionWithBody {
                        parameters: child_parameters,
                        signature: child_signature,
                        ..
                    } = &child.body
                    else {
                        return Err(fail("original child has no parameter binding inventory"));
                    };
                    for (literal, declared) in parameters.iter().zip(child_parameters) {
                        b.spend(literal.name.len().saturating_add(declared.name.len()), WORK)?;
                        if literal.name != declared.name {
                            return Err(fail(
                                "original closure literal and child have different value binders",
                            ));
                        }
                    }
                    // The literal's Invocation words belong to this closure
                    // signature, while its ordinary lexical names stay in the
                    // creator. Compare by a local alpha map, never a parent Inv0.
                    let mut lives = Vec::new();
                    for ordinal in 0..child_signature.invocation_lifetimes {
                        let life = self.term(Kind::Life(Life::Invocation(target, ordinal)), b)?;
                        Self::push(&mut lives, life, b)?;
                    }
                    Self::push(
                        &mut self.role_frames,
                        roles::RoleFrame {
                            owner: frame.definition,
                            lifetimes: lives,
                            implicits: BTreeMap::new(),
                        },
                        b,
                    )?;
                    let annotations: CompileResult<()> = (|| {
                        for (parameter, expected) in parameters.iter().zip(expected) {
                            if let Some(annotation) = &parameter.ty {
                                let annotation = self.recipe(frame, annotation, b, depth + 1)?;
                                self.unify(annotation, expected, depth + 1, b)?;
                            }
                        }
                        if let Some(annotation) = return_type {
                            let annotation = self.recipe(frame, annotation, b, depth + 1)?;
                            self.unify(annotation, result, depth + 1, b)?;
                        }
                        Ok(())
                    })();
                    self.role_frames.pop().ok_or_else(|| {
                        fail("original closure literal lost its own signature frame")
                    })?;
                    annotations?;
                    self.same_body(body, target, b)?;
                    term
                }
                CoreExprKind::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    let condition = self.expression(frame, condition, b, depth + 1)?;
                    let boolean = self.atom(Type::Bool, b)?;
                    self.unify(condition, boolean, depth + 1, b)?;
                    let left = self.expression(frame, then_value, b, depth + 1)?;
                    let right = self.expression(frame, else_value, b, depth + 1)?;
                    self.unify(left, right, depth + 1, b)?;
                    left
                }
                _ => {
                    return Err(unknown(
                        "original operation needs its complete symbolic type, place, stage or cleanup interpretation",
                    ));
                }
            };
            b.spend(frame.expressions.len().saturating_add(1), WORK)?;
            frame.expressions.insert(value.id, ty);
            Ok(ty)
        })();
        result.map_err(|mut diagnostics| {
            if let Err(limit) = b.spend(
                diagnostics.len().saturating_mul(64),
                "original operation diagnostic context",
            ) {
                return limit;
            }
            for diagnostic in &mut diagnostics {
                diagnostic.notes.push(format!(
                    "original definition module {} index {}, expression {}",
                    frame.definition.module.0, frame.definition.index, value.id.0
                ));
            }
            diagnostics
        })
    }
    fn block(
        &mut self,
        frame: &mut Frame,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let scope = frame.locals.len();
        for statement in &body.statements {
            self.step(depth + 1, b)?;
            b.lookup(self.indexes.len(), WORK)?;
            self.indexes
                .get(&frame.definition)
                .ok_or_else(|| fail("original statement has no definition index"))?
                .get_stmt(statement.id, b)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let local = self.var(b)?;
                    b.spend(frame.bindings.len().saturating_add(1), WORK)?;
                    if frame.bindings.insert(statement.id, local).is_some() {
                        return Err(fail("original binding node identity is reused"));
                    }
                    let mut family = None;
                    if let Some(annotation) = ty {
                        let expected = self.recipe(frame, annotation, b, depth + 1)?;
                        if self.raw_resource_family(expected, b)? {
                            family = Some(expected);
                        } else {
                            self.unify(local, expected, depth + 1, b)?;
                        }
                    }
                    if let Some(value) = initializer {
                        let value = self.expression(frame, value, b, depth + 1)?;
                        if let Some(family) = family {
                            let payload = match self.state.arena[self.type_root(value, b)?] {
                                Kind::Wrapped(Wrapper::Managed, payload)
                                | Kind::Desc { inner: payload, .. } => payload,
                                _ => {
                                    return Err(fail(
                                        "bare resource binding annotation needs its initializer's real owning or description category",
                                    ));
                                }
                            };
                            self.unify(family, payload, depth + 1, b)?;
                        }
                        self.unify(local, value, depth + 1, b)?;
                    } else if family.is_some() {
                        return Err(unknown(
                            "uninitialized raw resource family needs its later whole category constraint",
                        ));
                    }
                    self.bind(frame, name, local, *mutable, b)?;
                    frame
                        .locals
                        .last_mut()
                        .ok_or_else(|| fail("original binding lost its local"))?
                        .anchor = Some(LocalAnchor::Binding(statement.id));
                    if initializer.is_none() {
                        if let Some(local) = frame.locals.last_mut() {
                            local.initialized = false;
                        }
                    }
                }
                CoreStatementKind::TupleBinding {
                    pattern,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let subject = self.expression(frame, initializer, b, depth + 1)?;
                    if let Some(annotation) = ty {
                        let expected = self.recipe(frame, annotation, b, depth + 1)?;
                        self.unify(subject, expected, depth + 1, b)?;
                    }
                    b.lookup(frame.bindings.len(), WORK)?;
                    if frame.bindings.insert(statement.id, subject).is_some() {
                        return Err(fail(
                            "original destructuring binding reuses its source node",
                        ));
                    }
                    self.tuple_binding(
                        frame,
                        statement.id,
                        pattern,
                        subject,
                        *mutable,
                        b,
                        depth + 1,
                    )?;
                }
                CoreStatementKind::Expression(value) => {
                    let discarded = self.expression(frame, value, b, depth + 1)?;
                    if frame.alive {
                        Self::push(
                            &mut self.needs,
                            Need {
                                term: discarded,
                                predicate: Predicate::SyncDrop,
                            },
                            b,
                        )?;
                    }
                }
                CoreStatementKind::Return(value) => {
                    let value = match value {
                        Some(value) => self.expression(frame, value, b, depth + 1)?,
                        None => self.atom(Type::Unit, b)?,
                    };
                    let expected = self.type_root(frame.result, b)?;
                    let actual = self.type_root(value, b)?;
                    if let Kind::ErrorUnion { result, .. } = self.state.arena[expected]
                        && !matches!(self.state.arena[actual], Kind::ErrorUnion { .. })
                    {
                        // Ordinary return values are the success arm of the
                        // declaration's real ErrorUnion, never a new error type
                        // inferred from a selected concrete caller.
                        self.unify(value, result, depth + 1, b)?;
                    } else {
                        self.unify(value, frame.result, depth + 1, b)?;
                    }
                    if frame.alive {
                        self.capture_exit(frame, b)?;
                        self.cleanup(&frame.locals[frame.inherited..], b)?;
                    }
                    frame.alive = false;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.expression(frame, condition, b, depth + 1)?;
                    let boolean = self.atom(Type::Bool, b)?;
                    self.unify(condition, boolean, depth + 1, b)?;
                    let alive = frame.alive;
                    let start = self.copy_locals(&frame.locals, b)?;
                    self.block(frame, then_body, b, depth + 1)?;
                    let then_alive = frame.alive;
                    let then_locals = std::mem::replace(&mut frame.locals, start);
                    frame.alive = alive;
                    if let Some(body) = else_body {
                        self.block(frame, body, b, depth + 1)?;
                    }
                    let else_alive = frame.alive;
                    if then_locals.len() != frame.locals.len() {
                        return Err(fail(
                            "original branch changes the enclosing lexical binding inventory",
                        ));
                    }
                    for (then, other) in then_locals.iter().zip(&mut frame.locals) {
                        b.spend(1, WORK)?;
                        other.used = match (then_alive, else_alive) {
                            (true, true) => then.used || other.used,
                            (true, false) => then.used,
                            _ => other.used,
                        };
                        other.initialized = match (then_alive, else_alive) {
                            (true, true) => then.initialized && other.initialized,
                            (true, false) => then.initialized,
                            _ => other.initialized,
                        };
                    }
                    frame.alive = then_alive || else_alive;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.match_statement(frame, statement.id, value, arms, b, depth + 1)?;
                }
                CoreStatementKind::While { condition, body } => {
                    self.loop_statement(frame, Some(condition), body, b, depth + 1)?;
                }
                CoreStatementKind::Loop(body) => {
                    self.loop_statement(frame, None, body, b, depth + 1)?;
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    self.loop_jump(
                        frame,
                        matches!(&statement.kind, CoreStatementKind::Break),
                        b,
                    )?;
                }
                CoreStatementKind::CompoundAssign { target, op, value } => {
                    if let CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        operand,
                    } = &target.kind
                    {
                        let address = self.borrowed_argument(frame, operand, b, depth + 1)?;
                        let Kind::Address {
                            mutable: true,
                            inner: expected,
                            ..
                        } = self.state.arena[self.type_root(address, b)?]
                        else {
                            return Err(fail(
                                "original compound write requires its actual exclusive pointee",
                            ));
                        };
                        Self::push(
                            &mut self.needs,
                            Need {
                                term: expected,
                                predicate: Predicate::Copy,
                            },
                            b,
                        )?;
                        let actual = self.expression(frame, value, b, depth + 1)?;
                        self.unify(actual, expected, depth + 1, b)?;
                        Self::push(&mut self.numeric, expected, b)?;
                        if matches!(
                            op,
                            BinaryOp::Remainder
                                | BinaryOp::BitAnd
                                | BinaryOp::BitOr
                                | BinaryOp::BitXor
                                | BinaryOp::ShiftLeft
                                | BinaryOp::ShiftRight
                        ) {
                            Self::push(&mut self.integers, expected, b)?;
                        }
                        if matches!(
                            op,
                            BinaryOp::And
                                | BinaryOp::Or
                                | BinaryOp::Equal
                                | BinaryOp::NotEqual
                                | BinaryOp::Less
                                | BinaryOp::LessEqual
                                | BinaryOp::Greater
                                | BinaryOp::GreaterEqual
                        ) {
                            return Err(fail(
                                "original compound write uses a non-assignment operator",
                            ));
                        }
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(target.id, expected).is_some() {
                            return Err(fail("original compound pointee node is reused"));
                        }
                        continue;
                    }
                    if matches!(target.kind, CoreExprKind::Index { .. }) {
                        self.index_compound(frame, target, *op, value, b, depth + 1)?;
                        continue;
                    }
                    let CoreExprKind::Name(name) = &target.kind else {
                        return Err(unknown(
                            "original compound assignment needs its exact initialized place projection",
                        ));
                    };
                    b.spend(
                        frame
                            .locals
                            .iter()
                            .map(|local| local.name.len().saturating_add(name.len() + 1))
                            .sum::<usize>(),
                        WORK,
                    )?;
                    let index = frame
                        .locals
                        .iter()
                        .rposition(|local| local.name == *name)
                        .ok_or_else(|| fail("original compound assignment has no lexical local"))?;
                    let local = &frame.locals[index];
                    if !local.mutable || !local.initialized {
                        return Err(fail(
                            "original compound assignment needs an initialized mutable binding",
                        ));
                    }
                    let expected = local.ty;
                    if index < frame.inherited {
                        b.lookup(frame.captured.len(), WORK)?;
                        frame.captured.insert(index, expected);
                    }
                    if local.used {
                        Self::push(
                            &mut self.needs,
                            Need {
                                term: expected,
                                predicate: Predicate::Copy,
                            },
                            b,
                        )?;
                    }
                    let actual = self.expression(frame, value, b, depth + 1)?;
                    self.unify(actual, expected, depth + 1, b)?;
                    Self::push(&mut self.numeric, expected, b)?;
                    if matches!(
                        op,
                        BinaryOp::AddWrapping
                            | BinaryOp::SubtractWrapping
                            | BinaryOp::MultiplyWrapping
                            | BinaryOp::Remainder
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                            | BinaryOp::ShiftLeft
                            | BinaryOp::ShiftRight
                    ) {
                        Self::push(&mut self.integers, expected, b)?;
                    }
                    if matches!(
                        op,
                        BinaryOp::And
                            | BinaryOp::Or
                            | BinaryOp::Equal
                            | BinaryOp::NotEqual
                            | BinaryOp::Less
                            | BinaryOp::LessEqual
                            | BinaryOp::Greater
                            | BinaryOp::GreaterEqual
                    ) {
                        return Err(fail(
                            "original compound assignment uses a non-assignment operator",
                        ));
                    }
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(target.id, expected).is_some() {
                        return Err(fail("original compound target node is reused"));
                    }
                    if index < frame.inherited {
                        b.lookup(self.mutating_closures.len(), WORK)?;
                        self.mutating_closures.insert(frame.definition);
                    }
                    if frame.alive {
                        frame.locals[index].used = false;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    let CoreExprKind::Name(name) = &target.kind else {
                        return Err(unknown(
                            "original projected assignment needs its complete place interpretation",
                        ));
                    };
                    b.spend(
                        frame
                            .locals
                            .iter()
                            .map(|local| local.name.len().saturating_add(name.len()))
                            .sum::<usize>(),
                        WORK,
                    )?;
                    let index = frame
                        .locals
                        .iter()
                        .rposition(|local| local.name == *name)
                        .ok_or_else(|| fail("original assignment has no lexical local"))?;
                    if !frame.locals[index].mutable {
                        return Err(fail("original assignment targets an immutable binding"));
                    }
                    if index < frame.inherited {
                        b.lookup(self.mutating_closures.len(), WORK)?;
                        self.mutating_closures.insert(frame.definition);
                    }
                    let expected = frame.locals[index].ty;
                    let value = self.expression(frame, value, b, depth + 1)?;
                    self.unify(value, expected, depth + 1, b)?;
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(target.id, expected).is_some() {
                        return Err(fail("original assignment destination node is reused"));
                    }
                    if frame.alive {
                        let old = &frame.locals[index];
                        if old.initialized && !old.used {
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term: old.ty,
                                    predicate: Predicate::SyncDrop,
                                },
                                b,
                            )?;
                        }
                        frame.locals[index].used = false;
                        frame.locals[index].initialized = true;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(frame, body, b, depth + 1)?
                }
                _ => {
                    return Err(unknown(
                        "original statement needs complete scoped pattern, cleanup, loop or stage proof",
                    ));
                }
            }
        }
        if frame.alive {
            self.cleanup(&frame.locals[scope..], b)?;
        }
        frame.locals.truncate(scope);
        Ok(())
    }
    fn function(
        &mut self,
        id: SemanticDefinitionId,
        inherited: Option<&[Local]>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if !self.active.contains(&id) && self.state.frames.contains_key(&id) {
            let frame = self
                .state
                .frames
                .get(&id)
                .ok_or_else(|| fail("original completed frame disappeared"))?;
            b.spend(
                frame
                    .parameters
                    .len()
                    .saturating_add(frame.environment.len()),
                WORK,
            )?;
            let environment = frame.environment.clone();
            let parameters = frame.parameters.clone();
            let result = frame.result;
            return self.term(
                Kind::Closure {
                    definition: id,
                    parameters,
                    result,
                    environment,
                },
                b,
            );
        }
        if !self.active.insert(id) {
            return Err(fail("original lexical body graph is cyclic"));
        }
        let definition = self.inventory.definition(id, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            foreign,
            body,
            ..
        } = &definition.body
        else {
            return Err(fail("original function omits its complete body"));
        };
        if !matches!(
            definition.kind,
            SemanticDefinitionKind::Function
                | SemanticDefinitionKind::Closure
                | SemanticDefinitionKind::Method
        ) || signature.bound_lifetimes != 0
        {
            return Err(unknown(
                "original method, task or quantified declaration needs its complete rigid context",
            ));
        }
        if signature.parameters.len() != parameters.len() {
            return Err(fail(
                "original value parameter identities differ from the declared signature",
            ));
        }
        self.premises_well_formed(id, &definition.requirements, b)?;
        let mut frame = Frame {
            definition: id,
            formals: Vec::new(),
            parameters: Vec::new(),
            result: self.var(b)?,
            locals: Vec::new(),
            inherited: 0,
            captured: BTreeMap::new(),
            alive: true,
            environment: Vec::new(),
            expressions: BTreeMap::new(),
            bindings: BTreeMap::new(),
        };
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            let term = self.rigid_formal(id, ordinal, formal, &frame.formals, b)?;
            Self::push(&mut frame.formals, term, b)?;
        }
        self.original_trait_facts(&frame, b)?;
        if let Some(inherited) = inherited {
            for local in inherited {
                self.bind(&mut frame, &local.name, local.ty, local.mutable, b)?;
            }
            frame.inherited = frame.locals.len();
            let owned = self.owns_captures(id, b)?;
            for local in &mut frame.locals {
                local.borrowed = !owned;
            }
        }
        if !definition.environment.is_empty() {
            let parent = definition
                .parent
                .ok_or_else(|| fail("original environment lacks its actual lexical parent"))?;
            // Borrowing the immutable frame separately avoids moving any
            // source value or admitting an actual caller into this domain.
            let parent = self.state.frames.remove(&parent).ok_or_else(|| {
                fail("original environment parent is not closed over at its creator")
            })?;
            for environment in &definition.environment {
                let (SemanticFormal::Type { requirements }, SemanticActualRecipe::Type(source)) =
                    (&environment.kind, &environment.source)
                else {
                    return Err(unknown(
                        "original constant or lifetime environment needs its sealed parent-domain proof",
                    ));
                };
                if !requirements.is_empty() {
                    return Err(unknown(
                        "original environment kind has unproved parent requirements",
                    ));
                }
                let term = self.recipe(&parent, source, b, depth + 1)?;
                Self::push(&mut frame.environment, term, b)?;
            }
            b.spend(self.state.frames.len().saturating_add(1), WORK)?;
            self.state.frames.insert(parent.definition, parent);
        }
        for formal in &definition.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                self.role_premises_wf(&frame, requirements, b)?;
            }
        }
        self.role_premises_wf(&frame, &definition.requirements, b)?;
        for _ in parameters {
            let term = self.var(b)?;
            Self::push(&mut frame.parameters, term, b)?;
        }
        for (index, parameter) in parameters.iter().enumerate() {
            if parameters[..index]
                .iter()
                .any(|old| old.name == parameter.name)
            {
                return Err(fail("original value parameter name is duplicated"));
            }
            let declared = self.recipe(&frame, &signature.parameters[index].ty, b, depth + 1)?;
            self.unify(frame.parameters[index], declared, depth + 1, b)?;
            if matches!(self.state.arena[self.root(declared, b)?], Kind::Slice(_)) {
                return Err(fail(
                    "original unsized Slice parameter needs an actual reference carrier",
                ));
            }
            if let Some(annotation) = &parameter.ty {
                let annotation = self.recipe(&frame, annotation, b, depth + 1)?;
                self.unify(frame.parameters[index], annotation, depth + 1, b)?;
            }
            let term = frame.parameters[index];
            self.bind(&mut frame, &parameter.name, term, false, b)?;
            frame
                .locals
                .last_mut()
                .ok_or_else(|| fail("original parameter lost its local"))?
                .anchor = Some(LocalAnchor::Parameter(index as u32));
        }
        if matches!(signature.result, SemanticTypeRecipe::ErrorUnion { .. }) {
            let expected = self.recipe(&frame, &signature.result, b, depth + 1)?;
            self.unify(frame.result, expected, depth + 1, b)?;
        }
        match body {
            SemanticOriginalFunctionBody::Defined(body) => {
                self.block(&mut frame, body, b, depth + 1)?
            }
            SemanticOriginalFunctionBody::External => {
                if signature.call_kind != CallKind::Direct {
                    return Err(fail(
                        "original foreign declaration cannot claim a Suspend execution ABI",
                    ));
                }
                let foreign = foreign.as_ref().ok_or_else(|| {
                    fail("external original declaration omits its full ABI promises")
                })?;
                for ordinal in &foreign.noescape_parameters {
                    let parameter = *frame
                        .parameters
                        .get(*ordinal as usize)
                        .ok_or_else(|| fail("original external noescape parameter is absent"))?;
                    if !matches!(
                        self.state.arena[self.type_root(parameter, b)?],
                        Kind::Address { .. }
                    ) {
                        return Err(fail(
                            "original external noescape promise requires a borrowed value parameter",
                        ));
                    }
                }
                for instance in self
                    .inventory
                    .instances
                    .values()
                    .filter(|instance| instance.definition == id)
                {
                    let SemanticInstanceTarget::Function(target) = &instance.target else {
                        return Err(fail(
                            "foreign original declaration has a nonfunction actual target",
                        ));
                    };
                    let actual_foreign = if let Some(source) = self.inventory.source {
                        let SemanticFunctionTarget::Core { module, function } = target else {
                            return Err(fail(
                                "frozen external original has another stage's target",
                            ));
                        };
                        super::frozen::core_function(source, *module, *function, b)?
                            .foreign
                            .as_ref()
                    } else {
                        self.inventory.function(target, b)?.foreign.as_ref()
                    };
                    if actual_foreign != Some(foreign) {
                        return Err(fail(
                            "actual external ABI or provider promises differ from the original declaration",
                        ));
                    }
                }
            }
            SemanticOriginalFunctionBody::Declaration => {
                let parent = self.inventory.definition(
                    definition.parent.ok_or_else(|| {
                        fail("abstract original member has no original trait parent")
                    })?,
                    b,
                )?;
                if definition.kind != SemanticDefinitionKind::Method
                    || !matches!(
                        parent.body,
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
                    )
                    || foreign.is_some()
                {
                    return Err(fail(
                        "only an exact original abstract trait member may omit its body",
                    ));
                }
            }
        }
        if !frame.captured.is_empty() {
            let (parent, creator) = self
                .creators
                .get(&id)
                .copied()
                .ok_or_else(|| fail("original captured use has no actual lexical creator"))?;
            if definition.parent != Some(parent) {
                return Err(fail("original captured use has another lexical parent"));
            }
            for (index, term) in &frame.captured {
                let name = &frame
                    .locals
                    .get(*index)
                    .ok_or_else(|| fail("original captured binder is absent"))?
                    .name;
                let mut selected = None;
                for (ordinal, environment) in definition.environment.iter().enumerate() {
                    b.spend(name.len().saturating_add(1), WORK)?;
                    if let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source:
                            SemanticOriginalTypeSource::NameAt {
                                at: SemanticOriginalNode::Expression(node),
                                name: actual,
                            },
                    }) = &environment.source
                    {
                        b.spend(actual.len(), WORK)?;
                        if *node == creator && actual == name {
                            if selected.replace(ordinal).is_some() {
                                return Err(fail(
                                    "original captured binder repeats its exact creator environment source",
                                ));
                            }
                        }
                    }
                }
                let ordinal = selected.ok_or_else(|| {
                    fail("original captured name is absent from its complete creator environment")
                })?;
                let expected = *frame.environment.get(ordinal).ok_or_else(|| {
                    fail("original captured source lacks its complete parent-domain type")
                })?;
                self.unify(*term, expected, depth + 1, b)?;
            }
        }
        if frame.alive && matches!(body, SemanticOriginalFunctionBody::Defined(_)) {
            self.capture_exit(&frame, b)?;
            self.cleanup(&frame.locals[frame.inherited..], b)?;
            let unit = self.atom(Type::Unit, b)?;
            if let Kind::ErrorUnion { result, .. } =
                self.state.arena[self.type_root(frame.result, b)?]
            {
                self.unify(result, unit, depth + 1, b)?;
            } else {
                self.unify(frame.result, unit, depth + 1, b)?;
            }
        }
        let result = self.recipe(&frame, &signature.result, b, depth + 1)?;
        self.unify(frame.result, result, depth + 1, b)
            .map_err(|mut errors| {
                for error in &mut errors {
                    error.notes.push(format!(
                        "original definition module {} index {}, final result",
                        id.module.0, id.index
                    ));
                }
                errors
            })?;
        if matches!(
            self.state.arena[self.root(frame.result, b)?],
            Kind::Slice(_)
        ) {
            return Err(fail(
                "original unsized Slice result needs an actual reference carrier",
            ));
        }
        for requirement in &definition.requirements {
            if let SemanticRequirementRecipe::Callable {
                ty:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                kind,
                signature,
            } = requirement
            {
                self.callable_need(&frame, frame.result, *kind, signature, b, depth + 1)?;
            }
        }
        let mut parameter_terms = Vec::new();
        for parameter in &frame.parameters {
            Self::push(&mut parameter_terms, *parameter, b)?;
        }
        b.spend(frame.environment.len(), WORK)?;
        let environment = frame.environment.clone();
        let term = self.term(
            Kind::Closure {
                definition: id,
                parameters: parameter_terms,
                result: frame.result,
                environment,
            },
            b,
        )?;
        self.snapshot(&frame, b)?;
        self.active.remove(&id);
        Ok(term)
    }
}

impl Solver<'_, '_> {
    fn same_body(
        &self,
        body: &SemanticOriginalBlock,
        target: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(other),
            ..
        } = &definition.body
        else {
            return Err(fail("original closure definition omits its literal body"));
        };
        crate::dyn_metadata::recipe(body, b)?;
        crate::dyn_metadata::recipe(other, b)?;
        let mut body = body.clone();
        let mut other = other.clone();
        normalize_block(&mut body, b, 0)?;
        normalize_block(&mut other, b, 0)?;
        if body != other {
            return Err(fail(
                "original closure literal and child declaration have different operations",
            ));
        }
        Ok(())
    }
}
fn normalize_block(
    body: &mut SemanticOriginalBlock,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource("original body comparison depth exceeded"));
    }
    body.span = zil_base::Span::default();
    for statement in &mut body.statements {
        b.spend(1, WORK)?;
        statement.id = NodeId(0);
        statement.span = zil_base::Span::default();
        match &mut statement.kind {
            CoreStatementKind::Binding { initializer, .. } => {
                if let Some(value) = initializer {
                    normalize_expr(value, b, depth + 1)?;
                }
            }
            CoreStatementKind::Expression(value) => normalize_expr(value, b, depth + 1)?,
            CoreStatementKind::Return(value) => {
                if let Some(value) = value {
                    normalize_expr(value, b, depth + 1)?;
                }
            }
            CoreStatementKind::Assign { target, value }
            | CoreStatementKind::CompoundAssign { target, value, .. } => {
                normalize_expr(target, b, depth + 1)?;
                normalize_expr(value, b, depth + 1)?;
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                normalize_expr(condition, b, depth + 1)?;
                normalize_block(then_body, b, depth + 1)?;
                if let Some(body) = else_body {
                    normalize_block(body, b, depth + 1)?;
                }
            }
            CoreStatementKind::Match { value, arms } => {
                normalize_expr(value, b, depth + 1)?;
                for arm in arms {
                    arm.span = zil_base::Span::default();
                    normalize_pattern(&mut arm.pattern, b, depth + 1)?;
                    normalize_block(&mut arm.body, b, depth + 1)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                normalize_expr(condition, b, depth + 1)?;
                normalize_block(body, b, depth + 1)?;
            }
            CoreStatementKind::Loop(body) => normalize_block(body, b, depth + 1)?,
            CoreStatementKind::Break | CoreStatementKind::Continue => {}
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                normalize_block(body, b, depth + 1)?
            }
            _ => {
                return Err(unknown(
                    "original child-body comparison requires its complete operation and annotation rebasing",
                ));
            }
        }
    }
    Ok(())
}
fn normalize_pattern(
    pattern: &mut SemanticOriginalPattern,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource("original pattern comparison depth exceeded"));
    }
    pattern.span = zil_base::Span::default();
    match &mut pattern.kind {
        CorePatternKind::Tuple(fields)
        | CorePatternKind::Variant { fields, .. }
        | CorePatternKind::Constructor { fields, .. } => {
            for field in fields {
                normalize_pattern(field, b, depth + 1)?;
            }
        }
        CorePatternKind::Record { fields, .. }
        | CorePatternKind::RecordConstructor { fields, .. } => {
            for (_, field) in fields {
                normalize_pattern(field, b, depth + 1)?;
            }
        }
        CorePatternKind::Reference { pattern, .. } | CorePatternKind::ErrorArm { pattern, .. } => {
            normalize_pattern(pattern, b, depth + 1)?
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn normalize_expr(
    value: &mut SemanticOriginalExpr,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource("original expression comparison depth exceeded"));
    }
    value.id = NodeId(0);
    value.span = zil_base::Span::default();
    match &mut value.kind {
        CoreExprKind::Group(value)
        | CoreExprKind::CompileTime(value)
        | CoreExprKind::Try(value)
        | CoreExprKind::Unary { operand: value, .. }
        | CoreExprKind::Ascribed { value, .. } => normalize_expr(value, b, depth + 1)?,
        CoreExprKind::Binary { left, right, .. } => {
            normalize_expr(left, b, depth + 1)?;
            normalize_expr(right, b, depth + 1)?;
        }
        CoreExprKind::Call { callee, arguments } => {
            normalize_expr(callee, b, depth + 1)?;
            for argument in arguments {
                normalize_expr(argument, b, depth + 1)?;
            }
        }
        CoreExprKind::GenericApply { callee, arguments } => {
            normalize_expr(callee, b, depth + 1)?;
            for argument in arguments {
                if let CoreGenericArgument::Const(value) = argument {
                    normalize_expr(value, b, depth + 1)?;
                }
            }
        }
        CoreExprKind::Member { object, .. } => normalize_expr(object, b, depth + 1)?,
        CoreExprKind::Record { fields, .. } | CoreExprKind::RecordConstructor { fields, .. } => {
            for (_, field) in fields {
                normalize_expr(field, b, depth + 1)?;
            }
        }
        CoreExprKind::RecordUpdate { base, fields, .. } => {
            normalize_expr(base, b, depth + 1)?;
            for (_, field) in fields {
                normalize_expr(field, b, depth + 1)?;
            }
        }
        CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
            for field in fields {
                normalize_expr(field, b, depth + 1)?;
            }
        }
        CoreExprKind::Index { object, index } => {
            normalize_expr(object, b, depth + 1)?;
            normalize_expr(index, b, depth + 1)?;
        }
        CoreExprKind::Slice { object, start, end } => {
            normalize_expr(object, b, depth + 1)?;
            for bound in [start, end].into_iter().flatten() {
                normalize_expr(bound, b, depth + 1)?;
            }
        }
        CoreExprKind::NumericConvert { value, .. } | CoreExprKind::IntegerConvert { value, .. } => {
            normalize_expr(value, b, depth + 1)?
        }
        CoreExprKind::Closure {
            parameters, body, ..
        } => {
            for parameter in parameters {
                parameter.span = zil_base::Span::default();
            }
            normalize_block(body, b, depth + 1)?;
        }
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            normalize_expr(condition, b, depth + 1)?;
            normalize_expr(then_value, b, depth + 1)?;
            normalize_expr(else_value, b, depth + 1)?;
        }
        CoreExprKind::Name(_)
        | CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::Unit
        | CoreExprKind::Literal(_) => {}
        _ => {
            return Err(unknown(
                "original child expression requires its complete scope-preserving normalization",
            ));
        }
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<Checked> {
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            foreign,
            body,
            signature,
            parameters,
            ..
        } = &definition.body
        {
            b.spend(protocol_contracts.len(), WORK)?;
            if foreign.is_some() != matches!(body, SemanticOriginalFunctionBody::External) {
                return Err(fail(
                    "original external ABI header and original body kind disagree",
                ));
            }
            if let Some(foreign) = foreign {
                b.spend(
                    foreign.abi.len().saturating_add(
                        foreign.symbol.len() + foreign.noescape_parameters.len() + 1,
                    ),
                    WORK,
                )?;
                if signature.call_kind != CallKind::Direct
                    || foreign.abi != "C"
                    || foreign.symbol.is_empty()
                    || foreign.symbol.contains('\0')
                {
                    return Err(fail(
                        "original external declaration has an invalid fixed ABI or symbol",
                    ));
                }
                let mut previous = None;
                for ordinal in &foreign.noescape_parameters {
                    b.spend(1, WORK)?;
                    if *ordinal as usize >= parameters.len()
                        || previous.is_some_and(|old| old >= *ordinal)
                    {
                        return Err(fail(
                            "original external noescape ordinals are not unique ordered value parameters",
                        ));
                    }
                    previous = Some(*ordinal);
                }
            }
        }
    }
    let effects = effects::collect(program, inventory, b)?;
    let mut indexes = BTreeMap::new();
    for definition in inventory.definitions.values() {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) {
            let index = original_index::index(definition, b)?;
            b.lookup(indexes.len(), WORK)?;
            indexes.insert(definition.id, index);
        }
    }
    let mut solver = Solver {
        inventory,
        program,
        indexes,
        state: Checked {
            arena: Vec::new(),
            frames: BTreeMap::new(),
            calls: BTreeMap::new(),
            intrinsics: BTreeMap::new(),
            intrinsic_inputs: BTreeMap::new(),
            intrinsic_anchors: BTreeMap::new(),
            native_targets: BTreeMap::new(),
            native_paths: BTreeMap::new(),
            abstract_calls: BTreeMap::new(),
            trait_calls: BTreeMap::new(),
            task_templates: Vec::new(),
            borrows: BTreeMap::new(),
            descriptions: BTreeMap::new(),
            enum_constructors: BTreeMap::new(),
            patterns: BTreeMap::new(),
            names_at: BTreeMap::new(),
            environment_reads: BTreeMap::new(),
            capture_origins: BTreeMap::new(),
            function_values: BTreeMap::new(),
            code_calls: BTreeMap::new(),
        },
        active: BTreeSet::new(),
        creators: BTreeMap::new(),
        numeric: Vec::new(),
        scalar_captures: Vec::new(),
        literals: Vec::new(),
        integers: Vec::new(),
        needs: Vec::new(),
        callable_needs: Vec::new(),
        mutating_closures: BTreeSet::new(),
        role_frames: Vec::new(),
        next_role: 0,
        loops: Vec::new(),
        trait_state: traits::State::default(),
        capture_state: captures::CaptureState::default(),
    };
    solver.trait_catalog(b)?;
    for definition in &program.definitions {
        if definition.kind == SemanticDefinitionKind::Method
            && definition.parent.is_some_and(|parent| {
                inventory
                    .definitions
                    .get(&parent)
                    .is_some_and(|owner| matches!(owner.body, SemanticDefinitionBody::Scope(_)))
            })
        {
            solver.function(definition.id, None, b, 0)?;
        }
    }
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if matches!(definition.body, SemanticDefinitionBody::Nominal { .. }) {
            let mut arguments = Vec::new();
            for (ordinal, formal) in definition.parameters.iter().enumerate() {
                let term = solver.rigid_formal(definition.id, ordinal, formal, &arguments, b)?;
                Solver::push(&mut arguments, term, b)?;
            }
            solver.application(definition.id, arguments, b, 0)?;
        }
    }
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if definition.parent.is_none()
            && matches!(
                definition.body,
                SemanticDefinitionBody::FunctionWithBody { .. }
            )
        {
            solver.function(definition.id, None, b, 0)?;
        }
    }
    if let Some(wrapper) = inventory.frozen_wrapper {
        let definition = inventory.definition(wrapper, b)?;
        if definition.parent.is_none()
            || !matches!(
                definition.body,
                SemanticDefinitionBody::FunctionWithBody { .. }
            )
        {
            return Err(fail(
                "frozen query wrapper is not a real compiler child function",
            ));
        }
        solver.function(wrapper, None, b, 0)?;
    }
    for definition in &program.definitions {
        b.spend(solver.state.frames.len().saturating_add(1), WORK)?;
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) && !solver.state.frames.contains_key(&definition.id)
        {
            return Err(unknown(
                "original nested function or member has no complete checked lexical creator context",
            ));
        }
    }
    solver
        .original_provider_conformance(b)
        .map_err(|mut errors| {
            for error in &mut errors {
                error
                    .notes
                    .push("original provider method/associated conformance".into());
            }
            errors
        })?;
    solver.check_callables(b).map_err(|mut errors| {
        for error in &mut errors {
            error
                .notes
                .push("original complete Callable role obligations".into());
        }
        errors
    })?;
    for literal in &solver.literals {
        let root = solver.type_root(literal.term, b)?;
        if matches!(solver.state.arena[root], Kind::Infer(None)) {
            solver.state.arena[root] =
                Kind::Atom(if literal.float { Type::F64 } else { Type::I32 });
        }
    }
    // Numeric constraints arise from original operators, not actual headers.
    for term in &solver.numeric {
        let root = solver.type_root(*term, b)?;
        if matches!(solver.state.arena[root], Kind::Infer(None)) {
            solver.state.arena[root] = Kind::Atom(Type::I32);
        }
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if ty.is_b0_scalar() && *ty != Type::Bool && *ty != Type::Unit)
        {
            return Err(fail(
                "original numeric operation has no universally valid numeric type",
            ));
        }
    }
    for literal in &solver.literals {
        let root = solver.type_root(literal.term, b)?;
        let Kind::Atom(ty) = &solver.state.arena[root] else {
            return Err(fail("original literal has no concrete numeric carrier"));
        };
        validate_literal(
            &literal.text,
            ty,
            literal.float,
            literal.negative_context,
            b,
        )?;
    }
    for term in &solver.integers {
        let root = solver.type_root(*term, b)?;
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if integer_width(ty).is_some()) {
            return Err(fail(
                "original bit or remainder operation has a noninteger type",
            ));
        }
    }
    for term in &solver.scalar_captures {
        let root = solver.type_root(*term, b)?;
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if ty.is_b0_scalar()) {
            return Err(unknown(
                "original capture or assignment needs its complete abstract value and cleanup proof",
            ));
        }
    }
    for need in &solver.needs {
        if !solver.satisfies(need.term, need.predicate, b, 0)? {
            return Err(fail(match need.predicate {
                Predicate::Copy => "original repeated use requires Copy in its declaration context",
                Predicate::SyncDrop => {
                    "original live cleanup edge requires SyncDrop in its declaration context"
                }
                Predicate::Value => "original actual does not establish Value",
                Predicate::StoreElement => "original actual does not establish StoreElement",
                Predicate::Capability(_) => {
                    "original actual does not establish its declared sealed capability"
                }
            }));
        }
    }
    for frame in solver.state.frames.values() {
        for value in frame
            .parameters
            .iter()
            .chain(std::iter::once(&frame.result))
        {
            solver.sized_original(*value, b, 0)?;
        }
    }
    effects.finish(&solver.state, inventory, b)?;
    protocol::check(&solver.state, inventory, b)?;
    solver.check_resume_capabilities(b)?;
    solver.check_scoped_threads(b)?;
    b.spend(module.functions.len(), WORK)?;
    Ok(solver.state)
}

fn integer_width(ty: &Type) -> Option<(bool, u32)> {
    Some(match ty {
        Type::I8 => (true, 8),
        Type::U8 => (false, 8),
        Type::I16 => (true, 16),
        Type::U16 => (false, 16),
        Type::I32 => (true, 32),
        Type::U32 => (false, 32),
        Type::I64 | Type::Isize => (true, 64),
        Type::U64 | Type::Usize => (false, 64),
        _ => return None,
    })
}
fn validate_literal(
    text: &str,
    ty: &Type,
    float: bool,
    negative_context: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(
        if float {
            zil_base::numeric::decimal_work(text.len())
        } else {
            text.len().saturating_mul(3)
        },
        WORK,
    )?;
    let clean = text.replace('_', "");
    if float {
        let (digits, format) = match ty {
            Type::F32 => (
                clean.strip_suffix("f32").unwrap_or(&clean),
                zil_base::numeric::FloatFormat::F32,
            ),
            Type::F64 => (
                clean.strip_suffix("f64").unwrap_or(&clean),
                zil_base::numeric::FloatFormat::F64,
            ),
            _ => return Err(fail("original floating literal has a nonfloating carrier")),
        };
        zil_base::numeric::FloatValue::from_decimal(digits, format)
            .map_err(|_| fail("original floating literal spelling or range is invalid"))?;
        return Ok(());
    }
    let (signed, width) = integer_width(ty)
        .ok_or_else(|| fail("original integer literal has a noninteger carrier"))?;
    let mut digits = clean.as_str();
    for suffix in [
        "usize", "isize", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        if let Some(rest) = digits.strip_suffix(suffix) {
            digits = rest;
            break;
        }
    }
    let written_negative = digits.starts_with('-');
    if written_negative {
        digits = &digits[1..];
    }
    let negative = written_negative != negative_context;
    let (radix, digits) = if let Some(digits) = digits.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = digits.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = digits.strip_prefix("0o") {
        (8, digits)
    } else {
        (10, digits)
    };
    let value = u128::from_str_radix(digits, radix)
        .map_err(|_| fail("original integer literal spelling or range is invalid"))?;
    let bound = if signed {
        if negative {
            1u128 << (width - 1)
        } else {
            (1u128 << (width - 1)) - 1
        }
    } else {
        if negative {
            return Err(fail("negative original literal has an unsigned carrier"));
        }
        (1u128 << width) - 1
    };
    if value > bound {
        return Err(fail(
            "original integer literal is outside its declared carrier",
        ));
    }
    Ok(())
}

impl Solver<'_, '_> {
    fn copy_locals(&self, locals: &[Local], b: &mut Budget) -> CompileResult<Vec<Local>> {
        let mut output = Vec::new();
        for local in locals {
            b.spend(local.name.len(), WORK)?;
            Self::push(
                &mut output,
                Local {
                    anchor: local.anchor,
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
                },
                b,
            )?;
        }
        Ok(output)
    }
    fn sized_original(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.step(depth, b)?;
        match &self.state.arena[self.root(term, b)?] {
            Kind::Atom(Type::ByteBuffer)
            | Kind::Wrapped(
                Wrapper::Store
                | Wrapper::Mutex
                | Wrapper::Registration
                | Wrapper::AsyncRegistration,
                _,
            ) => Err(fail(
                "original sealed resource payload requires its real Managed or Desc carrier",
            )),
            Kind::Wrapped(Wrapper::Managed, _) | Kind::Desc { .. } => Ok(()),
            Kind::Slice(_) => Err(fail(
                "original unsized Slice needs a real reference carrier",
            )),
            Kind::Task(_) => Err(fail(
                "original Task payload requires its real Managed or Desc carrier",
            )),
            Kind::Array { element, .. } => self.sized_original(*element, b, depth + 1),
            Kind::ErrorUnion { error, result } => {
                self.sized_original(*error, b, depth + 1)?;
                self.sized_original(*result, b, depth + 1)
            }
            Kind::Code {
                parameters, result, ..
            } => {
                for parameter in parameters {
                    self.sized_original(*parameter, b, depth + 1)?;
                }
                self.sized_original(*result, b, depth + 1)
            }
            Kind::Nominal { definition, .. }
                if matches!(
                    self.inventory.definition(*definition, b)?.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Resource,
                        ..
                    }
                ) =>
            {
                Err(fail(
                    "original resource payload requires its real Managed or Desc carrier",
                ))
            }
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                for field in fields {
                    self.sized_original(*field, b, depth + 1)?;
                }
                Ok(())
            }
            Kind::CallResult { hidden, .. } => self.sized_original(*hidden, b, depth + 1),
            Kind::Associated { .. } if self.satisfies(term, Predicate::Value, b, depth + 1)? => {
                Ok(())
            }
            Kind::Associated { .. } | Kind::SelfType(_) => Err(unknown(
                "original associated layout needs its complete declared Value or concrete member theorem",
            )),
            Kind::Infer(_) => Err(unknown(
                "original value layout is not resolved in its declaration context",
            )),
            Kind::Life(_)
            | Kind::LifeVariable(_)
            | Kind::ConstRigid { .. }
            | Kind::ConstValue(_) => Err(fail("original value layout has a non-Type subject")),
            _ => Ok(()),
        }
    }
    fn cleanup(&mut self, locals: &[Local], b: &mut Budget) -> CompileResult<()> {
        for local in locals {
            b.spend(1, WORK)?;
            if local.initialized && !local.used && !local.borrowed {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: local.ty,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
            }
        }
        Ok(())
    }
    fn requirement<'a>(
        &self,
        requirement: &'a SemanticRequirementRecipe,
    ) -> CompileResult<(&'a SemanticTypeRecipe, Predicate)> {
        let (ty, predicate) = match requirement {
            SemanticRequirementRecipe::Structural { ty, bound } => (
                ty,
                match bound {
                    DynStructuralBound::Copy => Predicate::Copy,
                    DynStructuralBound::Value => Predicate::Value,
                    DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                    DynStructuralBound::StoreElement => Predicate::StoreElement,
                },
            ),
            SemanticRequirementRecipe::Builtin { ty, trait_kind } => (
                ty,
                match trait_kind {
                    SemanticBuiltinTrait::Copy => Predicate::Copy,
                    SemanticBuiltinTrait::Value => Predicate::Value,
                    SemanticBuiltinTrait::SyncDrop => Predicate::SyncDrop,
                    SemanticBuiltinTrait::StoreElement => Predicate::StoreElement,
                    kind => Predicate::Capability(*kind),
                },
            ),
            _ => {
                return Err(unknown(
                    "original nominal, callable or equality premise needs its complete scoped interpretation",
                ));
            }
        };
        Ok((ty, predicate))
    }
    fn premises_well_formed(
        &self,
        context: SemanticDefinitionId,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        for requirement in requirements {
            b.spend(1, WORK)?;
            if let SemanticRequirementRecipe::Trait { application, .. } = requirement {
                let raw = self.inventory.original_dyn(application.declaration, b)?;
                if raw.kind != DynDeclarationKind::Trait
                    || raw.parameters.len() != application.arguments.len()
                {
                    return Err(fail(
                        "original trait premise changes its declaration kind or full mixed arity",
                    ));
                }
                continue;
            }
            if let SemanticRequirementRecipe::Callable { ty, signature, .. } = requirement {
                if signature.bound_lifetimes != 0 {
                    return Err(unknown(
                        "original callable premise needs its complete quantified relation",
                    ));
                }
                if !matches!(
                    ty,
                    SemanticTypeRecipe::Formal(_)
                        | SemanticTypeRecipe::OriginalType {
                            source: SemanticOriginalTypeSource::Result,
                            ..
                        }
                ) {
                    return Err(unknown(
                        "original callable premise needs its exact original subject",
                    ));
                }
                continue;
            }
            let (ty, _) = self.requirement(requirement)?;
            match ty {
                SemanticTypeRecipe::Formal(reference) => {
                    let owner = self.original_scope(context, &reference.scope, b)?;
                    if !matches!(
                        self.inventory
                            .definition(owner, b)?
                            .parameters
                            .get(reference.ordinal as usize),
                        Some(SemanticFormal::Type { .. })
                    ) {
                        return Err(fail("original bound subject changes its formal kind"));
                    }
                }
                SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => {}
                SemanticTypeRecipe::AssociatedType { .. } | SemanticTypeRecipe::SelfType { .. } => {
                }
                _ => {
                    return Err(unknown(
                        "original structural premise subject needs its complete symbolic type",
                    ));
                }
            }
        }
        Ok(())
    }
    fn premise_needs(
        &mut self,
        frame: &Frame,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        for requirement in requirements {
            if matches!(requirement, SemanticRequirementRecipe::Trait { .. }) {
                self.original_trait_need(frame, requirement, b)?;
                continue;
            }
            if let SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } = requirement
            {
                let term = self.recipe(frame, ty, b, 0)?;
                if matches!(self.state.arena[self.root(term, b)?], Kind::Rigid(owner, _) if owner == frame.definition)
                {
                    // This is the current original domain's declared premise;
                    // actual callers still prove it at the concrete boundary.
                    self.role_premises_wf(frame, std::slice::from_ref(requirement), b)?;
                    continue;
                }
                self.callable_need(frame, term, *kind, signature, b, 0)?;
                continue;
            }
            let (recipe, predicate) = self.requirement(requirement)?;
            let term = self.recipe(frame, recipe, b, 0)?;
            Self::push(&mut self.needs, Need { term, predicate }, b)?;
        }
        Ok(())
    }
    fn assumption(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        requirements: &[SemanticRequirementRecipe],
        wanted: Predicate,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        for requirement in requirements {
            b.spend(1, WORK)?;
            if matches!(
                requirement,
                SemanticRequirementRecipe::Callable { .. }
                    | SemanticRequirementRecipe::Trait { .. }
            ) {
                continue;
            }
            let (ty, available) = self.requirement(requirement)?;
            let SemanticTypeRecipe::Formal(reference) = ty else {
                continue;
            };
            let target = self.original_scope(owner, &reference.scope, b)?;
            if target == owner && reference.ordinal == ordinal && entails(available, wanted) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn satisfies(
        &self,
        term: Term,
        predicate: Predicate,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        if let Predicate::Capability(kind) = predicate {
            return self.capability(term, kind, b, depth + 1);
        }
        match &self.state.arena[term] {
            Kind::Atom(ty) => Ok((ty.is_b0_scalar()
                || *ty == Type::ForeignHandle
                || *ty == crate::generic_mutex::error_type())
                && (!matches!(predicate, Predicate::StoreElement) || *ty != Type::Unit)),
            Kind::CallResult { hidden, .. } => self.satisfies(*hidden, predicate, b, depth + 1),
            Kind::Address { mutable, .. } => Ok(!matches!(predicate, Predicate::StoreElement)
                && (!matches!(predicate, Predicate::Copy) || !*mutable)),
            Kind::Life(_)
            | Kind::LifeVariable(_)
            | Kind::ConstRigid { .. }
            | Kind::ConstValue(_) => {
                Err(fail("original structural predicate has a non-Type subject"))
            }
            Kind::Rigid(owner, ordinal) => {
                let definition = self.inventory.definition(*owner, b)?;
                let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(*ordinal as usize)
                else {
                    return Err(fail(
                        "rigid original Type changes its declared parameter kind",
                    ));
                };
                Ok(
                    self.assumption(*owner, *ordinal, requirements, predicate, b)?
                        || self.assumption(
                            *owner,
                            *ordinal,
                            &definition.requirements,
                            predicate,
                            b,
                        )?,
                )
            }
            Kind::Associated { .. } => self.associated_satisfies(term, predicate, b, depth + 1),
            Kind::SelfType(_) => self.associated_satisfies(term, predicate, b, depth + 1),
            Kind::Array { element, .. } => {
                if matches!(predicate, Predicate::StoreElement) {
                    return Err(unknown(
                        "original array Store qualification requires its full layout and cleanup proof",
                    ));
                }
                self.satisfies(*element, predicate, b, depth + 1)
            }
            Kind::Desc { .. } => Ok(!matches!(predicate, Predicate::StoreElement)),
            Kind::MutexGuard { .. } => {
                Ok(matches!(predicate, Predicate::Value | Predicate::SyncDrop))
            }
            Kind::Wrapped(Wrapper::Managed, payload) => match predicate {
                Predicate::Copy | Predicate::StoreElement => Ok(false),
                Predicate::Value => Ok(true),
                Predicate::SyncDrop => self.owned_sync(*payload, b, depth + 1),
                Predicate::Capability(_) => {
                    Err(fail("sealed capability dispatch lost its original kind"))
                }
            },
            Kind::Wrapped(_, _) => Ok(false),
            Kind::Code { .. } => Ok(!matches!(predicate, Predicate::StoreElement)),
            Kind::ErrorUnion { error, result } => {
                Ok(self.satisfies(*error, predicate, b, depth + 1)?
                    && self.satisfies(*result, predicate, b, depth + 1)?)
            }
            Kind::Slice(_) => Ok(false),
            Kind::Task(_) => Ok(false),
            Kind::Tuple(fields) => {
                if matches!(predicate, Predicate::StoreElement) {
                    return Err(unknown(
                        "original Store element product needs complete layout and initialized-prefix cleanup proof",
                    ));
                }
                for field in fields {
                    if !self.satisfies(*field, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Nominal {
                definition, fields, ..
            } => {
                let decl = self.inventory.definition(*definition, b)?;
                let SemanticDefinitionBody::Nominal { kind, .. } = &decl.body else {
                    return Err(fail(
                        "original nominal predicate selects a nonnominal definition",
                    ));
                };
                if *kind == LifeNominalKind::Resource
                    || (*kind == LifeNominalKind::Package && matches!(predicate, Predicate::Copy))
                {
                    return Ok(false);
                }
                if matches!(predicate, Predicate::StoreElement) {
                    resources::ordinary_drop_absent(*definition, self.inventory, b)?;
                    for field in fields {
                        if !self.satisfies(*field, Predicate::StoreElement, b, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
                for field in fields {
                    if !self.satisfies(*field, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Closure { environment, .. } => match predicate {
                Predicate::Copy | Predicate::StoreElement => Ok(false),
                Predicate::Value => Ok(true),
                Predicate::SyncDrop => {
                    for captured in environment {
                        match &self.state.arena[self.type_root(*captured, b)?] {
                            Kind::Life(_)
                            | Kind::LifeVariable(_)
                            | Kind::ConstRigid { .. }
                            | Kind::ConstValue(_) => {}
                            _ => {
                                if !self.satisfies(*captured, Predicate::SyncDrop, b, depth + 1)? {
                                    return Ok(false);
                                }
                            }
                        }
                    }
                    Ok(true)
                }
                Predicate::Capability(_) => {
                    Err(fail("sealed capability dispatch lost its original kind"))
                }
            },
            Kind::Infer(_) => Err(unknown(
                "original structural predicate has an unresolved type variable",
            )),
        }
    }
}
fn entails(available: Predicate, wanted: Predicate) -> bool {
    if let (Predicate::Capability(a), Predicate::Capability(z)) = (available, wanted) {
        return a == z;
    }
    matches!(
        (available, wanted),
        (
            Predicate::Copy,
            Predicate::Copy | Predicate::Value | Predicate::SyncDrop
        ) | (Predicate::Value, Predicate::Value)
            | (Predicate::SyncDrop, Predicate::SyncDrop)
            | (
                Predicate::StoreElement,
                Predicate::StoreElement | Predicate::Value | Predicate::SyncDrop
            )
    )
}

impl Checked {
    fn root(&self, mut term: Term, b: &mut Budget) -> CompileResult<Term> {
        for _ in 0..=self.arena.len() {
            b.spend(1, WORK)?;
            match self
                .arena
                .get(term)
                .ok_or_else(|| fail("checked original type index is absent"))?
            {
                Kind::Infer(Some(next)) | Kind::LifeVariable(Some(next)) => term = *next,
                _ => return Ok(term),
            }
        }
        Err(fail("checked original type graph is cyclic"))
    }
    fn materialize(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original actual materialization exceeds its complete type depth",
            ));
        }
        match &self.arena[self.root(term, b)?] {
            Kind::Atom(ty) => Ok(ty.clone()),
            Kind::CallResult { .. } => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .ty),
            Kind::Array { .. }
            | Kind::Task(_)
            | Kind::MutexGuard { .. }
            | Kind::Code { .. }
            | Kind::ErrorUnion { .. }
            | Kind::Desc { .. }
            | Kind::Wrapped(_, _) => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .ty),
            Kind::Slice(element) => Ok(Type::Slice {
                element: Box::new(self.materialize(
                    *element,
                    context,
                    program,
                    inventory,
                    b,
                    depth + 1,
                )?),
            }),
            Kind::Address { mutable, inner, .. } => Ok(Type::Address {
                mutable: *mutable,
                inner: Box::new(self.materialize(
                    *inner,
                    context,
                    program,
                    inventory,
                    b,
                    depth + 1,
                )?),
            }),
            Kind::Life(_)
            | Kind::LifeVariable(_)
            | Kind::ConstRigid { .. }
            | Kind::ConstValue(_) => Err(fail("original Type materialization has a non-Type term")),
            Kind::Rigid(owner, ordinal) => {
                let mut scope = context;
                while scope.definition != *owner {
                    b.spend(1, WORK)?;
                    scope = inventory.instance(
                        scope.parent.ok_or_else(|| {
                            fail("actual original Type argument is outside its declaration domain")
                        })?,
                        b,
                    )?;
                }
                let Some(SemanticActual::Type(SemanticTypeValue::Resolved { ty, .. })) =
                    scope.actuals.get(*ordinal as usize)
                else {
                    return Err(fail(
                        "original Type argument has no complete executable actual",
                    ));
                };
                crate::dyn_metadata::ty(ty, b)?;
                Ok(ty.clone())
            }
            Kind::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    let field =
                        self.materialize(*field, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut result, field, b)?;
                }
                Ok(Type::Tuple(result))
            }
            Kind::Nominal { .. } => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .ty),
            Kind::Closure { definition, .. } => {
                let mut selected = None;
                for instance in &program.instances {
                    b.spend(1, WORK)?;
                    if instance.definition != *definition {
                        continue;
                    }
                    let Some(environment) = &instance.environment else {
                        continue;
                    };
                    let SemanticClosureTarget::Typed(id) = environment.closure else {
                        return Err(fail("original closure type retains a Core target"));
                    };
                    if selected.is_some_and(|previous| previous != id) {
                        return Err(fail(
                            "original closure Type has multiple actual entry identities",
                        ));
                    }
                    selected = Some(id);
                }
                selected.map(Type::Closure).ok_or_else(|| {
                    fail("original closure Type has no exact actual creator instance")
                })
            }
            Kind::Infer(_) => Err(unknown(
                "original Type remains unresolved independently of actual caller headers",
            )),
            Kind::Associated { .. } | Kind::SelfType(_) => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .ty),
        }
    }
    pub(super) fn source_type(
        &self,
        context: &SemanticInstance,
        scope: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Type> {
        Ok(self
            .source_value(context, scope, source, program, inventory, b)?
            .ty)
    }
    pub(super) fn source_value(
        &self,
        context: &SemanticInstance,
        scope: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<crate::semantic::value::Value> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        if depth as usize > b.partial_depth {
            return Err(resource(
                "original Type ancestor exceeds its finite lexical bound",
            ));
        }
        let mut context = context;
        for _ in 0..depth {
            context = inventory.instance(
                context
                    .parent
                    .ok_or_else(|| fail("original Type source has no lexical parent instance"))?,
                b,
            )?;
        }
        b.spend(self.frames.len().saturating_add(1), WORK)?;
        let frame = self
            .frames
            .get(&context.definition)
            .ok_or_else(|| fail("original Type source has no checked declaration frame"))?;
        let term = match source {
            SemanticOriginalTypeSource::Expression(id) => { b.spend(frame.expressions.len().saturating_add(1),WORK)?; frame.expressions.get(id).copied() },
            SemanticOriginalTypeSource::Binding(id) => { b.spend(frame.bindings.len().saturating_add(1),WORK)?; frame.bindings.get(id).copied() },
            SemanticOriginalTypeSource::BindingProjection {statement,pattern} => {b.spend(pattern.len().saturating_add(self.patterns.len()+1),WORK)?;self.patterns.get(&(context.definition,*statement,pattern.clone())).copied()},
            SemanticOriginalTypeSource::Parameter(index) => frame.parameters.get(*index as usize).copied(),
            SemanticOriginalTypeSource::Result => Some(frame.result),
            SemanticOriginalTypeSource::NameAt { at:SemanticOriginalNode::Expression(at),name } => {
                b.spend(self.names_at.len().saturating_add(1),WORK)?;
                let rows = self.names_at.get(&(context.definition,*at)).ok_or_else(|| fail("original name Type is not at its checked creator"))?;
                let mut selected = None;
                for (key,term) in rows { b.spend(key.len().saturating_add(name.len()),WORK)?; if key == name { selected = Some(*term); } }
                selected
            }
            _ => return Err(unknown("original projected binder or constant carrier needs its complete inference evidence")),
        }.ok_or_else(|| fail("original Type query selects no actual original node or binding"))?;
        self.value(term, context, program, inventory, b, 0)
    }
    pub(super) fn call_inputs(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<(SemanticDefinitionId, Vec<crate::semantic::value::Value>)> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("actual call inputs omit their complete original body"));
        };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(id) = link.node else {
                    return Err(fail("actual call input selects an original statement"));
                };
                if node.replace(id).is_some() {
                    return Err(fail("actual call inputs have competing original nodes"));
                }
            }
        }
        b.lookup(self.calls.len(), WORK)?;
        let call = self
            .calls
            .get(&(
                context.definition,
                node.ok_or_else(|| fail("actual call inputs have no original node"))?,
            ))
            .ok_or_else(|| {
                fail("actual call inputs have no original declaration-domain inference")
            })?;
        let mut values = Vec::new();
        for parameter in &call.parameters {
            let value = self.value(*parameter, context, program, inventory, b, 0)?;
            Solver::push(&mut values, value, b)?;
        }
        Ok((call.target, values))
    }
    pub(super) fn borrowed_source(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<
        Option<(
            LocalAnchor,
            crate::semantic::value::Value,
            Option<slice_borrows::SliceProjection>,
        )>,
    > {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("actual borrow has no complete original body"));
        };
        let mut found = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    continue;
                };
                b.lookup(self.borrows.len(), WORK)?;
                if let Some(source) = self.borrows.get(&(context.definition, node)) {
                    if found.is_some() {
                        return Err(fail("actual borrow has competing original source anchors"));
                    }
                    found = Some((
                        source.anchor,
                        self.value(source.inner, context, program, inventory, b, 0)?,
                        source.slice,
                    ));
                }
            }
        }
        Ok(found)
    }
    pub(super) fn intrinsic(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<sealed::Intrinsic>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("actual core operation has no original body"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual core operation selects a statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail("actual core operation has competing original anchors"));
                }
            }
        }
        let node = selected.ok_or_else(|| fail("actual core operation has no original anchor"))?;
        b.lookup(self.intrinsics.len(), WORK)?;
        Ok(self.intrinsics.get(&(context.definition, node)).copied())
    }
    pub(super) fn intrinsic_anchor(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<LocalAnchor> {
        let declaration = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail("actual native head lacks its original declaration"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual native head selects an original statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail("actual native head has competing source anchors"));
                }
            }
        }
        let node =
            selected.ok_or_else(|| fail("actual native head omits its source occurrence"))?;
        b.lookup(self.intrinsic_anchors.len(), WORK)?;
        self.intrinsic_anchors
            .get(&(context.definition, node))
            .copied()
            .ok_or_else(|| {
                fail("actual native head has no checked original parameter or binding anchor")
            })
    }
    pub(super) fn intrinsic_output(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<crate::semantic::value::Value> {
        let declaration = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail(
                "actual native result has no complete original declaration",
            ));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual native result selects an original statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail(
                        "actual native result has competing original occurrences",
                    ));
                }
            }
        }
        let node =
            selected.ok_or_else(|| fail("actual native result omits its original occurrence"))?;
        let frame = self
            .frames
            .get(&context.definition)
            .ok_or_else(|| fail("actual native result has no original checked frame"))?;
        b.lookup(frame.expressions.len(), WORK)?;
        let term = *frame.expressions.get(&node).ok_or_else(|| {
            fail("actual native result lacks its independently checked original result")
        })?;
        self.value(term, context, program, inventory, b, 0)
    }
    pub(super) fn intrinsic_inputs(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Vec<crate::semantic::value::Value>> {
        let declaration = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail(
                "actual native inputs have no complete original function",
            ));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual native call selects an original statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail("actual native inputs have competing original anchors"));
                }
            }
        }
        let node = selected.ok_or_else(|| fail("actual native inputs omit their original node"))?;
        b.lookup(self.intrinsic_inputs.len(), WORK)?;
        let terms = self
            .intrinsic_inputs
            .get(&(context.definition, node))
            .ok_or_else(|| fail("actual native inputs have no checked original scheme"))?;
        let mut values = Vec::new();
        for term in terms {
            let value = self.value(*term, context, program, inventory, b, 0)?;
            Solver::push(&mut values, value, b)?;
        }
        Ok(values)
    }
    pub(super) fn call(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<(
        SemanticDefinitionId,
        bool,
        Vec<crate::semantic::value::Argument>,
    )> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "actual BodyCall has no complete original function body",
            ));
        };
        b.spend(site_nodes.len(), WORK)?;
        let mut selected = None;
        for node in site_nodes {
            if node.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(id) = node.node else {
                return Err(fail("actual BodyCall selects an original statement"));
            };
            if selected.replace(id).is_some() {
                return Err(fail("actual BodyCall has multiple original nodes"));
            }
        }
        let node = selected.ok_or_else(|| fail("actual BodyCall has no original node"))?;
        b.spend(self.calls.len().saturating_add(1), WORK)?;
        let call = self
            .calls
            .get(&(context.definition, node))
            .ok_or_else(|| fail("actual BodyCall has no independently resolved original callee"))?;
        let mut actuals = Vec::new();
        for actual in &call.actuals {
            let actual = self.argument(*actual, context, program, inventory, b, 0)?;
            Solver::push(&mut actuals, actual, b)?;
        }
        Ok((call.target, call.closure, actuals))
    }
}

impl Checked {
    pub(super) fn native_targets(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Vec<SemanticDefinitionId>> {
        let declaration = inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &declaration.body else {
            return Err(fail(
                "actual installation lacks its full original function body",
            ));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(node) = link.node else {
                    return Err(fail("actual installation selects an original statement"));
                };
                if selected.replace(node).is_some() {
                    return Err(fail(
                        "actual installation has competing original call nodes",
                    ));
                }
            }
        }
        let node =
            selected.ok_or_else(|| fail("actual installation loses its original call anchor"))?;
        b.lookup(self.native_targets.len(), WORK)?;
        let targets = self
            .native_targets
            .get(&(instance.definition, node))
            .ok_or_else(|| {
                fail("actual installation has no independently checked original providers")
            })?;
        b.spend(targets.len(), WORK)?;
        Ok(targets.clone())
    }
}
