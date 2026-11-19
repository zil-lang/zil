//! Type original operations in their declaration domain. Runtime operands and
//! a particular caller's concrete Type actuals never substitute rigid formals.
use super::*;
use zil_base::NodeId;

mod abstract_calls;
mod body_equivalence;
mod borrow_roles;
pub(super) mod borrow_places;
mod buffers;
mod capabilities;
mod cleanup;
pub(super) use cleanup::Source as CleanupSource;
pub(super) use cleanup::Event as CleanupEvent;
mod call_regions;
pub(super) use call_regions::is_region_occurrence;
mod captures;
mod code_inputs;
mod constructors;
mod descriptions;
mod dyn_owned;
mod dyn_types;
mod dyn_values;
mod effects;
mod enums;
mod function_values;
mod frozen_calls;
mod intrinsic_actuals;
mod invocations;
mod loops;
pub(super) mod local_reference_roles;
pub(super) mod mixed;
mod mutex;
pub(super) mod nominal;
pub(super) mod pattern_places;
mod place_writes;
mod places;
mod protocol;
mod registration;
mod registration_start;
pub(super) mod resources;
mod roles;
pub(super) use roles::receiver_result;
pub(super) mod sealed;
pub(super) mod slice_borrows;
pub(super) mod slices;
mod stores;
pub(super) mod strings;
mod ffi_literals;
mod tasks;
mod trait_members;
mod traits;
mod values;
use constructors::Wrapper;
use mixed::Life;

type Term = usize;
pub(super) fn finish_cleanups(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    cleanup::finish(module, b)
}
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
    Dyn(dyn_types::DynType),
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
    Pattern(u32),
}
#[derive(Clone)]
struct ViewSelection {
    local: Option<LocalAnchor>,
    occurrence: Term,
    upper: Option<Term>,
    selected: Term,
}
#[derive(Clone)]
struct Borrow {
    role: Option<ViewSelection>,
    anchor: Option<LocalAnchor>,
    expression: Option<NodeId>,
    index: Option<NodeId>,
    root: Term,
    path: Vec<u32>,
    indirect: bool,
    inner: Term,
    slice: Option<slice_borrows::SliceProjection>,
}
struct Local {
    anchor: Option<LocalAnchor>,
    prepared_at: Option<NodeId>,
    name: String,
    ty: Term,
    mutable: bool,
    used: bool,
    borrowed: bool,
    initialized: bool,
    // Some incoming live path still owns an occupant; `used` separately
    // records whether a repeated read needs Copy on any incoming path.
    live: bool,
    // Definite and possible holes are separate at control-flow joins: only
    // a definitely empty field can omit its old-value cleanup obligation.
    vacant: BTreeSet<Vec<u32>>,
    maybe_vacant: BTreeSet<Vec<u32>>,
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
    declaration: Option<SemanticDefinitionId>,
    term: Term,
    kind: CallableKind,
    parameters: Vec<Term>,
    result: Term,
    call_kind: CallKind,
    parameter_modes: Vec<PassingMode>,
    result_mode: PassingMode,
    receiver: Option<Term>,
}
struct Literal {
    owner: SemanticDefinitionId,
    term: Term,
    text: String,
    float: bool,
    negative_context: bool,
}
struct Call {
    // Complete original consuming-operation upper, independent of the
    // concrete closure entry body ABI. None denotes an ordinary static call.
    operation: Option<CallKind>,
    target: SemanticDefinitionId,
    actuals: Vec<Term>,
    parameters: Vec<Term>,
    receiver: Option<Borrow>,
}
pub(super) enum CodeProjection {
    Field(u32),
    Index(NodeId),
}
pub(super) enum CodeCallAnchor {
    Local(LocalAnchor),
    CallResult { node: NodeId, indirect: bool },
    Projected {
        local: LocalAnchor,
        projections: Vec<CodeProjection>,
    },
}
struct CodeCall {
    code: Term,
    anchor: CodeCallAnchor,
    invocation: Option<(Vec<Term>, Term)>,
}
pub(super) struct CodeCallBoundary<'a> {
    pub anchor: &'a CodeCallAnchor,
    pub code: crate::semantic::value::Value,
    pub invocation: Option<(Vec<(NodeId, crate::semantic::value::Value)>, crate::semantic::value::Value)>,
}
struct ReceivingPlace {
    parameter: u32,
    path: Vec<u32>,
    root: Term,
    source: Term,
    target: Term,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ClosedNominalFormation {
    definition: SemanticDefinitionId,
    depth: usize,
    depth_limit: usize,
    node_limit: usize,
    actuals: Vec<usize>,
    fields: Vec<usize>,
}
// Rows are indexed by immutable term. Only original worlds actually queried
// for that term allocate a sparse substitution table.
type WorldValues = BTreeMap<(usize, usize, usize, usize), value::Value>;
pub(super) struct Checked {
    arena: Vec<Kind>,
    closed_formations: std::cell::RefCell<BTreeMap<ClosedNominalFormation, value::Value>>,
    // Only complete immutable original worlds enter this directory. Each
    // result belongs to its exact term and bound on recursive depth.
    closed_values: Option<std::cell::RefCell<Vec<Option<Box<WorldValues>>>>>,
    frames: BTreeMap<SemanticDefinitionId, Frame>,
    calls: BTreeMap<(SemanticDefinitionId, NodeId), Call>,
    intrinsics: BTreeMap<(SemanticDefinitionId, NodeId), sealed::Intrinsic>,
    intrinsic_inputs: BTreeMap<(SemanticDefinitionId, NodeId), Vec<Term>>,
    intrinsic_anchors: BTreeMap<(SemanticDefinitionId, NodeId), LocalAnchor>,
    store_view_inputs: BTreeMap<(SemanticDefinitionId, NodeId), u32>,
    native_targets: BTreeMap<(SemanticDefinitionId, NodeId), Vec<SemanticDefinitionId>>,
    registration_factories: BTreeMap<(SemanticDefinitionId, NodeId), NodeId>,
    scoped_foreign_operands: BTreeMap<(SemanticDefinitionId, NodeId), [NodeId; 2]>,
    task_worker_actuals: BTreeMap<(SemanticDefinitionId, NodeId), Vec<Term>>,
    native_paths: BTreeMap<(SemanticDefinitionId, NodeId), Vec<u32>>,
    abstract_calls: BTreeMap<(SemanticDefinitionId, NodeId), abstract_calls::AbstractCall>,
    trait_calls: BTreeMap<(SemanticDefinitionId, NodeId), traits::TraitCall>,
    task_templates: Vec<tasks::Template>,
    task_address_heads: BTreeMap<Term, Term>,
    dyn_templates: Vec<dyn_types::Template>,
    borrows: BTreeMap<(SemanticDefinitionId, NodeId), Borrow>,
    descriptions: BTreeMap<(SemanticDefinitionId, NodeId), descriptions::Description>,
    temporary_descriptions: BTreeMap<(SemanticDefinitionId, NodeId, usize), descriptions::Temporary>,
    owner_transfers: BTreeMap<(SemanticDefinitionId, NodeId), places::Place>,
    inferred_owner_captures: BTreeMap<SemanticDefinitionId, BTreeSet<String>>,
    captured_descriptions: BTreeMap<(SemanticDefinitionId, u32), Term>,
    enum_constructors: BTreeMap<(SemanticDefinitionId, NodeId), enums::Constructor>,
    patterns: BTreeMap<(SemanticDefinitionId, NodeId, Vec<u32>), Term>,
    pattern_origins: Vec<pattern_places::PatternOrigin>,
    indirect_pattern_borrows: BTreeMap<(SemanticDefinitionId, zil_base::Span), u32>,
    range_bindings: BTreeMap<u32, loops::RangeSource>,
    names_at: BTreeMap<(SemanticDefinitionId, NodeId), Vec<(String, Term)>>,
    environment_reads: BTreeMap<(SemanticDefinitionId, NodeId), u32>,
    local_reads: BTreeMap<(SemanticDefinitionId, NodeId), LocalAnchor>,
    prepared_reads: BTreeMap<(SemanticDefinitionId, NodeId), NodeId>,
    prepared_address_inputs: BTreeMap<(SemanticDefinitionId, NodeId), LocalAnchor>,
    capture_origins: BTreeMap<(SemanticDefinitionId, u32), LocalAnchor>,
    function_values: BTreeMap<(SemanticDefinitionId, NodeId), SemanticDefinitionId>,
    code_calls: BTreeMap<(SemanticDefinitionId, NodeId), CodeCall>,
    string_literals: BTreeMap<(SemanticDefinitionId, NodeId), Vec<u8>>,
    ffi_literals: BTreeMap<(SemanticDefinitionId, SemanticSiteId), NodeId>,
    return_injections: BTreeMap<(SemanticDefinitionId, NodeId), (Term, bool)>,
    dyn_creations: BTreeMap<(SemanticDefinitionId, NodeId), dyn_owned::Creation>,
    receiving_places: BTreeMap<(SemanticDefinitionId, NodeId), ReceivingPlace>,
    pub(super) received_result_roles: BTreeSet<(SemanticDefinitionId,u32,u32)>,
    view_uppers: BTreeMap<(SemanticDefinitionId, u32), Vec<Term>>,
    reference_roles: BTreeMap<(SemanticDefinitionId, NodeId), local_reference_roles::Role>,
    call_regions: BTreeMap<(SemanticDefinitionId, u32), call_regions::Region>,
    call_uses: BTreeMap<(SemanticDefinitionId, NodeId), Vec<call_regions::Use>>,
    pending_cleanups: Vec<cleanup::Pending>,
    cleanup_writes: BTreeMap<(SemanticDefinitionId, NodeId), cleanup::Write>,
}
struct Solver<'a, 'i> {
    inventory: &'i Inventory<'a>,
    program: &'i SemanticModule,
    indexes: BTreeMap<SemanticDefinitionId, std::sync::Arc<original_index::Index<'a>>>,
    state: Checked,
    active: BTreeSet<SemanticDefinitionId>,
    creators: BTreeMap<SemanticDefinitionId, (SemanticDefinitionId, NodeId)>,
    numeric: Vec<Term>,
    equality_operands: Vec<Term>,
    literals: Vec<Literal>,
    integers: Vec<Term>,
    needs: Vec<Need>,
    cleanup_terms: Vec<cleanup::Occurrence>,
    preparations: Vec<cleanup::Preparation>,
    callable_needs: Vec<CallableNeed>,
    mutating_closures: BTreeSet<SemanticDefinitionId>,
    cleanup_actions: BTreeSet<SemanticDefinitionId>,
    role_frames: Vec<roles::RoleFrame>,
    bound_frames: Vec<roles::BoundFrame>,
    next_role: u32,
    loops: Vec<loops::LoopFrame>,
    trait_state: traits::State,
    capture_state: captures::CaptureState,
    code_inputs: code_inputs::CodeInputs,
    receiving: Vec<(SemanticDefinitionId, NodeId, Term)>,
    local_return_bounds: BTreeSet<(SemanticDefinitionId, Term, Term)>,
    field_needs: Vec<(Term, SemanticDefinitionId, u32)>,
    nominal_declaration: Option<SemanticDefinitionId>,
    call_variables: BTreeMap<Term, call_regions::Role>,
    call_parameters: BTreeSet<Term>,
    constructor_templates: BTreeMap<(SemanticDefinitionId, NodeId), Term>,
    constructor_constants: BTreeMap<Term, Term>,
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
    #[track_caller]
    fn push<T>(rows: &mut Vec<T>, row: T, b: &mut Budget) -> CompileResult<()> {
        // Spare capacity appends one fresh node. Only a real growth can move
        // existing rows; charging their full length on every append is quadratic.
        let relocation = if rows.len() == rows.capacity() {
            rows.len()
        } else {
            0
        };
        b.spend(relocation.saturating_add(1), WORK)?;
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
            && !matches!(
                ty,
                Type::ForeignHandle | Type::CWideString | Type::ByteBuffer | Type::SyncCell(_)
            )
            && ty != crate::generic_mutex::error_type()
            && !crate::dyn_dispatch::is_create_error(&ty)
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
            Kind::Dyn(ty) => {
                for term in ty.terms() {
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
                // The same unresolved type may already be wrapped in its
                // checked invocation result. Equating its hidden type with
                // itself needs no arena edge or result provenance change.
                if self.type_root(right, b)? == left {
                    return Ok(());
                }
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
            Kind::Dyn(ty) => ty.work(),
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
                for (a, z) in a.into_iter().zip(z).chain(std::iter::once((ar, zr))) {
                    if ak == SemanticCodeKind::BorrowedFunctionPointer {
                        match (self.code_outer(a, b)?, self.code_outer(z, b)?) {
                            (Some((am, ac, inner)), Some((zm, zc, other))) if am == zm && ac == zc => {
                                self.unify(inner, other, depth + 1, b)?;
                                continue;
                            }
                            (None, None) => {}
                            _ => return Err(fail("borrowed code types differ in their complete invocation relation")),
                        }
                    }
                    self.unify(a, z, depth + 1, b)?;
                }
                Ok(())
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
            (Kind::Dyn(a), Kind::Dyn(z)) => self.dyn_unify(&a, &z, b, depth + 1),
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
                prepared_at: None,
                name: name.to_owned(),
                ty,
                mutable,
                used: false,
                borrowed: false,
                initialized: true,
                live: true,
                vacant: BTreeSet::new(),
                maybe_vacant: BTreeSet::new(),
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
            SemanticTypeRecipe::Dyn { owned, interface } => {
                self.dyn_type(frame, *owned, interface, b, depth + 1)
            }
            SemanticTypeRecipe::Atom(ty)
                if ty.is_b0_scalar()
                    || matches!(ty, Type::ForeignHandle | Type::CWideString | Type::ByteBuffer | Type::SyncCell(_)) =>
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
                mutable: false,
                lifetime: SemanticOuterLifeRecipe::Static,
                inner,
            } => {
                let inner = self.recipe(frame, inner, b, depth + 1)?;
                if !matches!(self.state.arena[self.type_root(inner,b)?],Kind::Slice(element) if matches!(self.state.arena[self.type_root(element,b)?],Kind::Atom(Type::U8)))
                {
                    return Err(unknown(
                        "original static reference needs its complete inner type and occurrence inventory",
                    ));
                }
                self.term(Kind::Atom(strings::byte_type()), b)
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let life = self.life(frame, life, b, depth + 1)?;
                let child = self.recipe(frame, inner, b, depth + 1)?;
                let inner = if let SemanticTypeRecipe::SelfType { scope } = inner.as_ref()
                    && self.resource_payload(child, b)?
                    && let owner = self.original_scope(frame.definition, scope, b)?
                    && matches!(self.inventory.definition(owner, b)?.body,
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. }))
                {
                    self.term(Kind::Wrapped(Wrapper::Managed, child), b)?
                } else {
                    child
                };
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
                if (exact.ty.is_b0_scalar()
                    || exact.ty == crate::generic_mutex::error_type()
                    || crate::dyn_dispatch::is_create_error(&exact.ty))
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
                if matches!(kind, SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer)
                    && signature.invocation_lifetimes != 0
                {
                    return self.borrowed_code_recipe(frame, signature, b, depth + 1);
                }
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
                let code = self.term(
                    Kind::Code {
                        kind: *kind,
                        parameters,
                        result,
                        modes,
                        result_mode: signature.result_mode,
                        call_kind: signature.call_kind,
                    },
                    b,
                )?;
                self.authored_code(code, b)
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
                    prepared_at: local.prepared_at,
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
                    live: local.live,
                    vacant: self.copy_vacancies(&local.vacant, b)?,
                    maybe_vacant: self.copy_vacancies(&local.maybe_vacant, b)?,
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
    fn scalar_literal(&mut self, owner: SemanticDefinitionId, text: &str, float: bool, b: &mut Budget) -> CompileResult<Term> {
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
                owner,
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
        let expression = self.indexes
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
            ) || matches!(expression.kind, CoreExprKind::Call { .. })
                && matches!(recipe.kind, SemanticSiteRecipeKind::Construct { .. })
            {
                if found.replace(recipe).is_some() {
                    return Err(fail("original operation has ambiguous resolution sites"));
                }
            }
        }
        found.ok_or_else(|| fail("original call or closure has no exact resolution site"))
    }
    fn lexical_input_hint(
        &mut self,
        frame: &Frame,
        expression: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        match &expression.kind {
            CoreExprKind::Name(name) => self.local(frame, name, b),
            CoreExprKind::Group(inner) => self.lexical_input_hint(frame, inner, b, depth + 1),
            CoreExprKind::Ascribed { ty, .. } | CoreExprKind::Record { ty, .. } => {
                let term = self.recipe(frame, ty, b, depth + 1)?;
                if matches!(expression.kind, CoreExprKind::Record { .. })
                    && self.resource_payload(term, b)?
                {
                    Ok(Some(self.term(Kind::Wrapped(Wrapper::Managed, term), b)?))
                } else {
                    Ok(Some(term))
                }
            }
            CoreExprKind::RecordConstructor { definition, fields } => {
                self.record_constructor_hint(frame, expression.id, *definition, fields, b, depth + 1)
            }
            CoreExprKind::RecordUpdate { base, .. } => {
                self.lexical_input_hint(frame, base, b, depth + 1)
            }
            CoreExprKind::Member { object, member, owner } => {
                let Some(object) = self.lexical_input_hint(frame, object, b, depth + 1)? else {
                    return Ok(None);
                };
                if let Some(owner) = owner {
                    let expected = self.recipe(frame, owner, b, depth + 1)?;
                    self.input_hint(expected, object, b, depth + 1)?;
                }
                Ok(Some(self.field(object, member, b, depth + 1)?.1))
            }
            CoreExprKind::Unary { op: UnaryOp::Dereference, operand } => {
                let Some(address) = self.lexical_input_hint(frame, operand, b, depth + 1)? else {
                    return Ok(None);
                };
                Ok(match self.state.arena[self.type_root(address, b)?] {
                    Kind::Address { inner, .. } => Some(inner),
                    _ => None,
                })
            }
            CoreExprKind::Unary { op, operand }
                if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut) =>
            {
                // This reads the original place's Type only. No expression,
                // occurrence, loan, read or consuming event is formed here.
                // The temporary outer life is only an inference variable;
                // the genuine source borrow is checked once with its exact
                // receiving role and must independently solve it.
                let Some(inner) = self.lexical_input_hint(frame, operand, b, depth + 1)? else {
                    return Ok(None);
                };
                let life = self.term(Kind::LifeVariable(None), b)?;
                Ok(Some(self.term(Kind::Address {
                    mutable: *op == UnaryOp::BorrowMut, life, inner,
                }, b)?))
            }
            _ => Ok(None),
        }
    }

    fn input_hint(
        &mut self,
        expected: Term,
        actual: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let expected = self.type_root(expected, b)?;
        let actual = self.type_root(actual, b)?;
        for term in [expected, actual] {
            let work = match &self.state.arena[term] {
                Kind::Nominal {
                    arguments, fields, ..
                } => arguments.len().saturating_add(fields.len()),
                Kind::Closure {
                    parameters,
                    environment,
                    ..
                } => parameters.len().saturating_add(environment.len()),
                Kind::Code {
                    parameters, modes, ..
                } => parameters.len().saturating_add(modes.len()),
                Kind::Tuple(fields) => fields.len(),
                Kind::Task(task) => task.work(),
                Kind::Dyn(ty) => ty.work(),
                _ => 1,
            };
            b.spend(work, WORK)?;
        }
        match (
            self.state.arena[expected].clone(),
            self.state.arena[actual].clone(),
        ) {
            (Kind::Infer(None), _) => self.unify(expected, actual, depth + 1, b),
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
                    self.input_hint(a, z, b, depth + 1)?;
                }
                Ok(())
            }
            (Kind::Wrapped(a, ai), Kind::Wrapped(z, zi)) if a == z => {
                self.input_hint(ai, zi, b, depth + 1)
            }
            (Kind::Address { inner: a, .. }, Kind::Address { inner: z, .. })
            | (Kind::Desc { inner: a, .. }, Kind::Desc { inner: z, .. }) => {
                self.input_hint(a, z, b, depth + 1)
            }
            // Contextual ownership and outer invocation equations belong to
            // call_argument, after these ordinary Type variables are solved.
            _ => Ok(()),
        }
    }

    // Opaque results are selected only after their independent declaration
    // has completed. Its private captured literals need their declaration's
    // callable constraints before defaulting, rather than a caller's actual
    // Type image or the eventual end of the whole program.
    fn settle_opaque_declaration(
        &mut self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut scopes = BTreeSet::new();
        scopes.insert(owner);
        loop {
            let before = scopes.len();
            for definition in &self.program.definitions {
                b.spend(1, WORK)?;
                if definition.parent.is_some_and(|parent| scopes.contains(&parent)) {
                    b.lookup(scopes.len(), WORK)?;
                    scopes.insert(definition.id);
                }
            }
            if scopes.len() == before {
                break;
            }
        }
        for scope in &scopes {
            b.lookup(self.active.len(), WORK)?;
            if self.active.contains(scope) {
                return Err(unknown("opaque result inference still has an active original lexical body"));
            }
        }
        let pending = std::mem::take(&mut self.callable_needs);
        let mut callers = Vec::new();
        for need in pending {
            b.lookup(scopes.len(), WORK)?;
            if need.declaration.is_some_and(|owner| scopes.contains(&owner)) {
                Self::push(&mut self.callable_needs, need, b)?;
            } else {
                Self::push(&mut callers, need, b)?;
            }
        }
        let checked = self.check_callables(b);
        // Keep caller obligations independent, including on a failed proof.
        for need in callers {
            Self::push(&mut self.callable_needs, need, b)?;
        }
        checked?;
        for literal in &self.literals {
            b.lookup(scopes.len(), WORK)?;
            if !scopes.contains(&literal.owner) {
                continue;
            }
            let root = self.type_root(literal.term, b)?;
            if matches!(self.state.arena[root], Kind::Infer(None)) {
                self.state.arena[root] = Kind::Atom(if literal.float { Type::F64 } else { Type::I32 });
            }
        }
        Ok(())
    }

    fn scheme(
        &mut self,
        caller: &Frame,
        call_node: Option<NodeId>,
        target: SemanticDefinitionId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        input_hints: Option<&[Option<Term>]>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Vec<Term>, Term)> {
        self.step(depth, b)?;
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature, receiver_parameter, ..
        } = &definition.body else {
            return Err(fail("original callee has no complete declared scheme"));
        };
        let needs_parent_actuals = if let Some(parent) = definition.parent {
            let scope = self.inventory.definition(parent, b)?;
            // The qualified name already selected this exact authored
            // method. A concrete inherent Self supplies no parent actuals;
            // trait/receiver calls still use their complete call scheme.
            definition.kind != SemanticDefinitionKind::Method
                || receiver_parameter.is_some()
                || !matches!(
                    scope.body,
                    SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl { .. })
                )
                || !scope.parameters.is_empty()
                || !scope.requirements.is_empty()
                || !scope.environment.is_empty()
                || !scope.implicit_lifetimes.is_empty()
        } else {
            false
        };
        if needs_parent_actuals
            || !definition.environment.is_empty()
            || signature.bound_lifetimes != 0
        {
            return Err(unknown(
                "original lexical/member call needs its complete declared scope and quantifier instantiation",
            ));
        }
        let written = mixed::written_call_actuals(&definition.parameters, written, b)?;
        let mut formals = Vec::new();
        for (index, formal) in definition.parameters.iter().enumerate() {
            let term = match (formal, written[index]) {
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
                (SemanticFormal::Lifetime, None) => self.call_variable(call_regions::Role::Life(index as u32), b)?,
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
                self.function(target, None, None, b, depth + 1)?;
            }
            self.settle_opaque_declaration(target, b)?;
            let checked = self
                .state
                .frames
                .get(&target)
                .ok_or_else(|| fail("original opaque result has no checked declaration body"))?
                .result;
            Some(checked)
        } else {
            None
        };
        let mut implicits = BTreeMap::new();
        for row in &definition.implicit_lifetimes {
            b.spend(row.occurrence_path.len().saturating_add(1), WORK)?;
            let life = if let Some(node) = call_node
                && self.reference_result_role(target, row.ordinal, b)? {
                self.term(Kind::Life(Life::CallResult(caller.definition, node, target, row.ordinal)), b)?
            } else { self.call_variable(call_regions::Role::Implicit(row.ordinal), b)? };
            b.lookup(implicits.len(), WORK)?;
            if implicits.insert(row.ordinal, life).is_some() {
                return Err(fail(
                    "original call repeats an implicit declaration occurrence",
                ));
            }
        }
        let lifetimes = self.invocation_actuals(signature.invocation_lifetimes, b)?;
        for (index, life) in lifetimes.iter().enumerate() {
            self.mark_call_variable(*life, call_regions::Role::Invocation(index as u32), b)?;
        }
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: target,
                lifetimes,
                implicits,
                receiver: None,
            },
            b,
        )?;
        let answer: CompileResult<Term> = (|| {
            // Constrain exposed Type variables from the true receiving
            // recipe before lexical hints. This does not form a dependent
            // nominal result before its input directory has been solved.
            if let Some(node) = call_node
                && let Some(wanted) = self.receiving_for(caller, node, b)? {
                self.input_recipe_hint(&frame, &signature.result, wanted, b, depth + 1)?;
            }
            for (ordinal, parameter) in signature.parameters.iter().enumerate() {
                b.spend(1, WORK)?;
                if let Some(Some(actual)) = input_hints.and_then(|hints| hints.get(ordinal)) {
                    self.input_recipe_hint(&frame, &parameter.ty, *actual, b, depth + 1)?;
                }
            }
            for (ordinal, parameter) in signature.parameters.iter().enumerate() {
                let term = self.recipe(&frame, &parameter.ty, b, depth + 1)?;
                if let Some(Some(actual)) = input_hints.and_then(|hints| hints.get(ordinal)) {
                    self.input_hint(term, *actual, b, depth + 1)?;
                }
                self.mark_call_parameter(term, &parameter.ty, b)?;
                Self::push(&mut frame.parameters, term, b)?;
            }
            for formal in &definition.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.premise_needs(&frame, requirements, b)?;
                }
            }
            self.premise_needs(&frame, &definition.requirements, b)?;
            let result = if let Some(opaque) = opaque {
                // Open this result in the same exact call-local implicit
                // frame as its parameters. The declaration remains rigid;
                // each invocation gets its own source-to-use substitution.
                self.instantiate(opaque, target, &frame.formals, b, depth + 1)?
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
    fn prepared_shared_address(
        &mut self,
        frame: &mut Frame,
        bindings: &[(String, SemanticOriginalExpr)],
        invocation: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        let [(temporary, input)] = bindings else { return Ok(None) };
        let CoreExprKind::Call { callee, arguments } = &invocation.kind else { return Ok(None) };
        let [argument] = arguments.as_slice() else { return Ok(None) };
        let CoreExprKind::Name(input_name) = &input.kind else { return Ok(None) };
        b.spend(temporary.len().saturating_add(input_name.len()).saturating_add(1), WORK)?;
        if !matches!(&argument.kind, CoreExprKind::Name(name) if name == temporary)
            || self.local(frame, input_name, b)?.is_none()
        {
            return Ok(None);
        }
        let Some(path) = sealed::path(callee, b, depth + 1)? else { return Ok(None) };
        if self.local(frame, path[0], b)?.is_some() {
            return Ok(None);
        }
        let sealed::Resolution::Definition(target) = sealed::resolve(
            self.inventory, frame.definition, &path, &BTreeSet::new(), b,
        )? else { return Ok(None) };
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
        else { return Ok(None) };
        let [parameter] = signature.parameters.as_slice() else { return Ok(None) };
        if parameter.mode != PassingMode::Borrow
            || !matches!(parameter.ty, SemanticTypeRecipe::Address { mutable: false, .. })
        {
            return Ok(None);
        }
        let place = self.place(frame, input, false, b, depth + 1)?
            .ok_or_else(|| fail("original prepared shared Address has no input place"))?;
        let original = self.borrowed_argument(frame, input, b, depth + 1)?;
        let Kind::Address { life, inner, .. } = self.state.arena[self.type_root(original, b)?]
        else { return Err(fail("original prepared shared Address requires an address input")) };
        let anchor = place.anchor.or_else(|| self.state.environment_reads.get(&(frame.definition, input.id))
            .copied().map(LocalAnchor::Environment))
            .ok_or_else(|| unknown("prepared shared Address requires its exact original source binder"))?;
        b.lookup(self.state.prepared_address_inputs.len(), WORK)?;
        if self.state.prepared_address_inputs.insert((frame.definition, argument.id), anchor).is_some() {
            return Err(fail("original prepared shared Address repeats its actual input"));
        }
        // Keep the original expression's source and each child's invariant
        // Type. Invocation still checks the actual formal and its class images.
        Ok(Some(self.term(Kind::Address { mutable: false, life, inner }, b)?))
    }

    pub(super) fn expression_expected(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        expected: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.expression_expected_with_projection(frame, value, expected, None, b, depth)
    }
    fn expression_expected_with_projection(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        expected: Term,
        projection: Option<&mut Option<Term>>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let shared = matches!(self.state.arena[self.type_root(expected, b)?],
            Kind::Address { mutable: false, .. });
        Self::push(&mut self.receiving, (frame.definition, value.id, expected), b)?;
        let result = if shared {
            self.borrowed_argument(frame, value, b, depth)
        } else { self.expression_with_projection(frame, value, projection, b, depth) };
        let result = match result {
            Ok(actual) => self.shared_receiving_value(frame, value, actual, expected, b, depth + 1)
                .map(|view| view.unwrap_or(actual)),
            Err(errors) => Err(errors),
        };
        self.receiving
            .pop()
            .ok_or_else(|| fail("original receiving context lost its lexical stack"))?;
        match result {
            Ok(actual) if self.static_byte_role(actual, expected, b, depth + 1)? => Ok(expected),
            other => other,
        }
    }
    fn expression(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.expression_with_projection(frame, value, None, b, depth)
    }
    fn expression_with_projection(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        projection: Option<&mut Option<Term>>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let preparation_depth = self.preparations.len();
        let preparing = matches!(value.kind, CoreExprKind::Call { .. }
            | CoreExprKind::Tuple(_) | CoreExprKind::Array(_)
            | CoreExprKind::Record { .. } | CoreExprKind::RecordConstructor { .. }
            | CoreExprKind::RecordUpdate { .. });
        if preparing {
            Self::push(&mut self.preparations, cleanup::Preparation {
                definition: frame.definition, call: value.id, values: Vec::new(),
            }, b)?;
        }
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
                CoreExprKind::Integer(text) => self.scalar_literal(frame.definition, text, false, b)?,
                CoreExprKind::Float(text) => self.scalar_literal(frame.definition, text, true, b)?,
                CoreExprKind::Literal(
                    literal @ (Constant::NullHandle
                    | Constant::NullWideString
                    | Constant::WideString(_)),
                ) => {
                    self.ffi_literal(frame.definition, value.id, literal, b)?
                }
                CoreExprKind::Literal(value) if value.ty().is_b0_scalar() => {
                    self.atom(value.ty(), b)?
                }
                CoreExprKind::Bool(_) => self.atom(Type::Bool, b)?,
                CoreExprKind::String(text) => self.string_literal(frame, value.id, text, b)?,
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
                    self.record_environment_read(frame, index, value.id, b)?;
                    if index < frame.inherited {
                        b.lookup(frame.captured.len(), WORK)?;
                        frame.captured.insert(index, frame.locals[index].ty);
                    }
                    if frame.alive { self.local_available(&frame.locals[index], &[], b)?; }
                    // Ordinary managed names describe their occupants. A
                    // prepared slot carries the result of an already checked
                    // initialization and transfers it only to an owning input.
                    let stored = frame.locals[index].ty;
                    match self.state.arena[self.type_root(stored, b)?] {
                        Kind::Wrapped(Wrapper::Managed, _) => {
                            let place = self.place(frame, value, true, b, depth + 1)?
                                .ok_or_else(|| fail("named managed observation has no initialized place"))?;
                            if frame.locals[index].prepared_at.is_some()
                                && let Some(wanted) = self.receiving_for(frame, value.id, b)?
                                && matches!(self.state.arena[self.type_root(wanted, b)?],
                                    Kind::Wrapped(Wrapper::Managed, _))
                            {
                                if frame.alive {
                                    self.capture_read(frame, index, b)?;
                                    frame.locals[index].used = true;
                                    frame.locals[index].live = false;
                                }
                                b.lookup(frame.expressions.len(), WORK)?;
                                frame.expressions.insert(value.id, stored);
                                return Ok(stored);
                            }
                            return self.describe_argument(frame, value, &place, stored, b, depth + 1);
                        }
                        Kind::Desc { .. } => {
                            if !frame.locals[index].initialized {
                                return Err(fail("description read has no initialized occupant"));
                            }
                            b.lookup(frame.expressions.len(), WORK)?;
                            frame.expressions.insert(value.id, stored);
                            return Ok(stored);
                        }
                        _ => {}
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
                        local.live = false;
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
                    let expected = match self.receiving_for(frame, value.id, b)? {
                        Some(expected) => match self.state.arena[self.type_root(expected, b)?] {
                            Kind::Array { element, .. } => Some(element),
                            _ => None,
                        },
                        None => None,
                    };
                    let element = match expected {
                        Some(element) => element,
                        None => self.var(b)?,
                    };
                    for value in values {
                        let actual = if expected.is_some() {
                            self.expression_expected(frame, value, element, b, depth + 1)?
                        } else {
                            self.expression(frame, value, b, depth + 1)?
                        };
                        self.unify(actual, element, depth + 1, b)?;
                        self.prepare_call_value(frame.definition, value.id, actual, b)?;
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
                        let expected = match self.receiving_for(frame, value.id, b)? {
                            Some(expected) => match &self.state.arena[self.type_root(expected, b)?] {
                                Kind::Tuple(expected) if expected.len() == fields.len() => {
                                    b.spend(expected.len(), WORK)?;
                                    Some(expected.clone())
                                }
                                _ => None,
                            },
                            None => None,
                        };
                        let mut terms = Vec::new();
                        for (index, field) in fields.iter().enumerate() {
                            let term = if let Some(expected) = &expected {
                                self.expression_expected(frame, field, expected[index], b, depth + 1)?
                            } else {
                                self.expression(frame, field, b, depth + 1)?
                            };
                            self.prepare_call_value(frame.definition, field.id, term, b)?;
                            Self::push(&mut terms, term, b)?;
                        }
                        self.term(Kind::Tuple(terms), b)?
                    }
                }
                CoreExprKind::Group(inner) | CoreExprKind::CompileTime(inner) => {
                    match self.receiving_for(frame, value.id, b)? {
                        Some(expected) => self.expression_expected_with_projection(frame, inner, expected, projection, b, depth + 1)?,
                        None => self.expression_with_projection(frame, inner, projection, b, depth + 1)?,
                    }
                }
                CoreExprKind::Sequence { bindings, value: result } => {
                    let scope = frame.locals.len();
                    let mut prepared = self.prepared_shared_address(frame, bindings, result, b, depth + 1)?;
                    for (name, initializer) in bindings {
                        if !name.starts_with('$') || self.local(frame, name, b)?.is_some() {
                            return Err(fail("original sequence repeats or exposes a generated temporary"));
                        }
                        let ty = match prepared.take() {
                            Some(prepared) => prepared,
                            None => self.expression(frame, initializer, b, depth + 1)?,
                        };
                        self.bind(frame, name, ty, false, b)?;
                        let mut source = initializer;
                        while let CoreExprKind::Group(inner)
                            | CoreExprKind::Ascribed { value: inner, .. }
                            | CoreExprKind::Sequence { value: inner, .. } = &source.kind
                        {
                            self.step(depth, b)?;
                            source = inner;
                        }
                        frame.locals.last_mut().expect("prepared local").prepared_at = Some(source.id);
                    }
                    let result = match self.receiving_for(frame, value.id, b)? {
                        Some(expected) => self.expression_expected_with_projection(frame, result, expected, projection, b, depth + 1)?,
                        None => self.expression_with_projection(frame, result, projection, b, depth + 1)?,
                    };
                    if frame.alive {
                        self.cleanup(frame.definition, &frame.locals[scope..], b)?;
                    }
                    frame.locals.truncate(scope);
                    result
                }
                CoreExprKind::Try(value) => {
                    if !self.cleanup_actions.is_empty() {
                        b.lookup(self.cleanup_actions.len(), WORK)?;
                        if self.cleanup_actions.contains(&frame.definition) {
                            return Err(fail("original deferred cleanup exits before completing its action"));
                        }
                    }
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
                        self.cleanup_preparations(frame.definition, b)?;
                        self.cleanup(frame.definition, &frame.locals[frame.inherited..], b)?;
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
                    let expected = self.recipe(frame, ty, b, depth + 1)?;
                    let actual = self.expression_expected_with_projection(frame, value, expected, projection, b, depth + 1)?;
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
                            Self::push(&mut self.equality_operands, left, b)?;
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
                    let place = self.borrow_place(frame, operand, b, depth + 1)?;
                    if *op == UnaryOp::BorrowMut && !place.mutable {
                        return Err(fail(
                            "original mutable borrow requires an exclusive initialized place",
                        ));
                    }
                    let inner = place.inner;
                    let source_life = place.source_life;
                    let row = self.borrow_occurrence(frame.definition, value.id, b)?;
                    let occurrence = self.term(Kind::Life(Life::Implicit(frame.definition, row)), b)?;
                    self.remember_view_upper(frame.definition, row, source_life, b)?;
                    let wanted = match self.receiving_for(frame, value.id, b)? {
                        Some(expected) => match self.state.arena[self.type_root(expected, b)?] {
                            Kind::Address { life, inner: expected, .. } => { self.unify(inner, expected, depth + 1, b)?; Some(life) },
                            _ => None,
                        },
                        None => None,
                    };
                    let local = if !place.indirect && place.expression.is_none() && place.path.is_empty() {
                        place.anchor.filter(|anchor| matches!(anchor, LocalAnchor::Binding(_)))
                    } else { None };
                    let life = self.select_view_life(frame, occurrence, source_life, wanted, local, b, depth + 1)?;
                    b.lookup(self.state.borrows.len(), WORK)?;
                    if self
                        .state
                        .borrows
                        .insert(
                            (frame.definition, value.id),
                            Borrow {
                                role: Some(ViewSelection { local, occurrence, upper: source_life, selected: life }),
                                anchor: place.anchor,
                                expression: place.expression,
                                index: place.index,
                                root: place.root,
                                path: place.path,
                                indirect: place.indirect,
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
                    let mut inner = operand.as_ref();
                    while let CoreExprKind::Group(next) = &inner.kind {
                        self.step(depth, b)?;
                        inner = next;
                    }
                    let (manager, exclusive) = if let CoreExprKind::Name(name) = &inner.kind {
                        match self.local(frame, name, b)? {
                            Some(term) => match self.state.arena[self.type_root(term, b)?] {
                                Kind::Wrapped(Wrapper::Managed, _) => (true, false),
                                Kind::Address { mutable: true, .. } => (false, true),
                                _ => (false, false),
                            },
                            None => (false, false),
                        }
                    } else {
                        (false, false)
                    };
                    if matches!(&inner.kind, CoreExprKind::Member { object, .. }
                        if matches!(object.kind, CoreExprKind::Name(_)))
                    {
                        let place = self.give_inline_field(frame, operand, b, depth + 1)?;
                        let selected = place.selected;
                        b.lookup(self.state.owner_transfers.len(), WORK)?;
                        self.state.owner_transfers.insert((frame.definition, value.id), place);
                        selected
                    } else {
                        let value = if manager || exclusive {
                            let place = self.borrow_place(frame, operand, b, depth + 1)?;
                            self.consume_place(frame, &place, b)?;
                            b.lookup(self.state.owner_transfers.len(), WORK)?;
                            self.state.owner_transfers.insert((frame.definition, value.id), places::Place {
                                anchor: place.anchor, root: place.root, path: place.path,
                                selected: place.inner,
                            });
                            place.inner
                        } else {
                            self.expression(frame, operand, b, depth + 1)?
                        };
                        let transferable = match self.state.arena[self.type_root(value, b)?] {
                            Kind::Wrapped(Wrapper::Managed, _) | Kind::Closure { .. } => true,
                            Kind::Rigid(owner, ordinal) => {
                                // A callable premise identifies this original F,
                                // never one of its later concrete caller images.
                                let (kind, _, _) = self.quantified_callable_premise(owner, ordinal, b)?;
                                matches!(
                                    kind,
                                    CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                                )
                            }
                            _ => false,
                        };
                        if !transferable && !exclusive {
                            return Err(unknown(
                                "original give requires its complete owning manager or callable category",
                            ));
                        }
                        // The operand has already moved its real source occurrence
                        // through the ordinary availability/capture accounting.
                        // Keep its complete environment term: the enclosing call
                        // prepares it once and retains cleanup on every early exit.
                        // No Copy, SyncDrop or invocation permission follows here.
                        value
                    }
                }
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand,
                } => {
                    // Dereferencing observes the existing address head; it
                    // does not move that affine head out of its lexical slot.
                    let address = self.borrowed_argument(frame, operand, b, depth + 1)?;
                    let root = self.type_root(address, b)?;
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
                    if matches!(recipe.kind, SemanticSiteRecipeKind::Construct { .. }) {
                        let term = self.enum_call(frame, value.id, callee, arguments, b, depth + 1)?
                            .ok_or_else(|| fail("original formation lacks its exact enum constructor"))?;
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original constructor node is reused"));
                        }
                        return Ok(term);
                    }
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
                    let evaluated_head = matches!(head.kind,
                        CoreExprKind::Call { .. } | CoreExprKind::Closure { .. }
                        | CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
                        | CoreExprKind::Index { .. }
                        | CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, .. });
                    let local = if evaluated_head {
                        Some(self.expression(frame, head, b, depth + 1)?)
                    } else if let CoreExprKind::Name(name) = &head.kind {
                        self.local(frame, name, b)?
                    } else {
                        self.place(frame, head, false, b, depth + 1)?
                            .filter(|place| !place.path.is_empty())
                            .map(|place| place.selected)
                    };
                    let mut prepared_arguments = BTreeMap::new();
                    let (parameters, result) = if let Some(local) = local {
                        if !written.is_empty() {
                            return Err(unknown(
                                "original generic callable needs its complete method instantiation",
                            ));
                        }
                        self.demand_code_input(frame, head, local, arguments.len(), b, depth + 1)?;
                        if let Some((parameters, result)) =
                            self.abstract_scheme(frame, head, value.id, local, b, depth + 1)?
                        {
                            b.lookup(self.state.abstract_calls.len(), WORK)?;
                            if self.state.abstract_calls.get(&(frame.definition, value.id))
                                .is_some_and(|call| call.kind == CallableKind::FnOnce) {
                                self.prepare_call_value(frame.definition, head.id, local, b)?;
                            }
                            (parameters, result)
                        } else if let Some(call) =
                            self.code_call(frame, head, value.id, local, b, depth + 1)?
                        {
                            call
                        } else {
                            let callee_value = local;
                            let mut local = self.type_root(local, b)?;
                            let borrowed = if let Kind::Address { mutable, inner, .. } = self.state.arena[local] {
                                local = self.type_root(inner, b)?;
                                Some((mutable, inner))
                            } else { None };
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
                            // Resolve a real original owning/borrowed place once.
                            // This is the same complete source used by the actual
                            // field projection, not a Name/temporary reconstruction.
                            let receiver_place = if evaluated_head { None }
                                else { Some(self.borrow_place(frame, head, b, depth + 1)?) };
                            let receiver = if let Some(source) = self.explicit_callable_borrow(frame, head, b)? {
                                source
                            } else if let Some(place) = &receiver_place {
                                b.spend(place.path.len().saturating_add(8), WORK)?;
                                let inner = if let Some((_, inner)) = borrowed { inner } else { place.inner };
                                if borrowed.is_some() && (place.indirect || !place.path.is_empty()) {
                                    return Err(unknown("stored callable address needs its complete post-load pointer source"));
                                }
                                Borrow { role: None, anchor: place.anchor, expression: place.expression,
                                    index: place.index, root: place.root, path: place.path.clone(),
                                    indirect: place.indirect || borrowed.is_some(), inner, slice: None }
                            } else {
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.get(&head.id).copied() != Some(callee_value) {
                                    return Err(fail("temporary callable head loses its exact evaluated original expression"));
                                }
                                Borrow { role: None, anchor: None, expression: Some(head.id), index: None,
                                    root: callee_value, path: Vec::new(), indirect: borrowed.is_some(),
                                    inner: borrowed.map_or(callee_value, |(_, inner)| inner), slice: None }
                            };
                            let subject = borrowed.map_or(callee_value, |(_, inner)| inner);
                            let (requested_once, operation) = self.known_call_operation(subject, target, b)?;
                            let consuming = self.known_capture_call(frame, head, target, requested_once, operation,
                                borrowed.map(|(mutable, _)| mutable), receiver_place.as_ref(), b)?;
                            if consuming { self.prepare_call_value(frame.definition, head.id, callee_value, b)?; }
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
                            if parameters.len() != signature.parameters.len() {
                                return Err(fail("original known closure changes its declared parameter arity"));
                            }
                            let mut opened = Vec::new();
                            for (parameter, declared) in parameters.into_iter().zip(&signature.parameters) {
                                let parameter =
                                    self.open_invocations(parameter, target, &lives, b, depth + 1)?;
                                // Only the genuine original signature grants a
                                // call outer role. A whole T instantiated as an
                                // Address remains an invariant complete Type.
                                self.mark_call_parameter(parameter, &declared.ty, b)?;
                                Self::push(&mut opened, parameter, b)?;
                            }
                            let parameters = opened;
                            let result =
                                self.open_invocations(result, target, &lives, b, depth + 1)?;
                            let result = self.known_receiver_output(frame.definition, value.id,
                                subject, target, consuming, result, b)?;
                            b.spend(
                                self.state.calls.len().saturating_add(parameters.len() + 1),
                                WORK,
                            )?;
                            self.state.calls.insert(
                                (frame.definition, value.id),
                                Call {
                                    operation: Some(operation),
                                    target,
                                    actuals: Vec::new(),
                                    parameters: parameters.clone(),
                                    receiver: Some(receiver),
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
                            sealed::Resolution::Intrinsic(sealed::Intrinsic::CCallback) => {
                                let output = self.callback_value(
                                    frame,
                                    value.id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, output).is_some() {
                                    return Err(fail("original callback call node is reused"));
                                }
                                return Ok(output);
                            }
                            sealed::Resolution::Intrinsic(sealed::Intrinsic::TakeStored) => {
                                let [field] = arguments.as_slice() else { return Err(fail("move_out requires one original stored field")); };
                                if !written.is_empty() || !matches!(field.kind, CoreExprKind::Member { .. }) {
                                    return Err(fail("move_out requires its exact original field place"));
                                }
                                let slot = self.borrow_place(frame, field, b, depth + 1)?;
                                if !slot.mutable || !matches!(self.state.arena[self.type_root(slot.inner, b)?], Kind::Address { .. }) {
                                    return Err(fail("move_out requires an actual address slot of a mutable holder"));
                                }
                                b.lookup(self.state.intrinsics.len(), WORK)?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if self.state.intrinsics.insert((frame.definition, value.id), sealed::Intrinsic::TakeStored).is_some()
                                    || frame.expressions.insert(value.id, slot.inner).is_some() {
                                    return Err(fail("move_out repeats its actual original occurrence"));
                                }
                                return Ok(slot.inner);
                            }
                            sealed::Resolution::Intrinsic(sealed::Intrinsic::OwnedDyn) => {
                                let result = self.owned_dyn_call(
                                    frame,
                                    value.id,
                                    written,
                                    arguments,
                                    b,
                                    depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original owned Dyn call node is reused"));
                                }
                                return Ok(result);
                            }
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
                                | sealed::Intrinsic::BytesView { .. }
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
                            sealed::Resolution::Intrinsic(sealed::Intrinsic::ScopedForeignCallback) => {
                                let result = self.scoped_foreign_callback(
                                    frame, value.id, written, arguments, b, depth + 1,
                                )?;
                                b.lookup(frame.expressions.len(), WORK)?;
                                if frame.expressions.insert(value.id, result).is_some() {
                                    return Err(fail("original scoped foreign callback node is reused"));
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
                                | sealed::Intrinsic::TaskValid
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
                                | sealed::Intrinsic::SyncCellScope(_)
                                | sealed::Intrinsic::SyncCellOp(..)
                                | sealed::Intrinsic::SyncCellCompareExchange
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
                                id @ (sealed::Intrinsic::TaskAwait
                                | sealed::Intrinsic::TaskCreate
                                | sealed::Intrinsic::TaskCreateOwned
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
                                // Reading a lexical type for inference does
                                // not evaluate or consume the argument. Apply
                                // it before querying dependent result types;
                                // the ordinary call below still evaluates and
                                // checks each actual exactly once in order.
                                let mut hints = Vec::new();
                                for argument in arguments {
                                    let hint =
                                        self.lexical_input_hint(frame, argument, b, depth + 1)?;
                                    Self::push(&mut hints, hint, b)?;
                                }
                                let definition = self.inventory.definition(target, b)?;
                                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
                                    return Err(fail("original call inference has no full static signature"));
                                };
                                // A nested argument of an abstract Type must be
                                // evaluated once before dependent trait output
                                // formation. Stage the complete ordered vector
                                // only when all inputs have that direct Type
                                // role and no receiving context is being lost.
                                b.spend(signature.parameters.len().saturating_add(arguments.len().saturating_mul(2)), WORK)?;
                                let abstract_inputs = signature.parameters.iter().all(|parameter|
                                    matches!(parameter.ty, SemanticTypeRecipe::Formal(SemanticFormalRef {
                                        scope: SemanticScopeRef::Ancestor(0), .. })));
                                let nested = hints.iter().zip(arguments).any(|(hint, argument)| hint.is_none()
                                    && matches!(argument.kind, CoreExprKind::Call { .. }));
                                if written.is_empty() && abstract_inputs && nested
                                    && self.receiving_for(frame, value.id, b)?.is_none()
                                    && hints.iter().zip(arguments).all(|(hint, argument)| hint.is_some()
                                        || matches!(argument.kind, CoreExprKind::Call { .. }))
                                {
                                    for (ordinal, argument) in arguments.iter().enumerate() {
                                        let borrowed = if let Some(Some(hint)) = hints.get(ordinal) {
                                            matches!(self.state.arena[self.type_root(*hint, b)?], Kind::Address { .. })
                                        } else { false };
                                        let actual = if borrowed { self.borrowed_argument(frame, argument, b, depth + 1)? }
                                            else { self.expression(frame, argument, b, depth + 1)? };
                                        b.lookup(prepared_arguments.len(), WORK)?;
                                        if prepared_arguments.insert(argument.id, actual).is_some() {
                                            return Err(fail("staged call argument reuses an original source identity"));
                                        }
                                        self.prepare_call_value(frame.definition, argument.id, actual, b)?;
                                        if let Some(hint) = hints.get_mut(ordinal) { *hint = Some(actual); }
                                    }
                                }
                                let (actuals, parameters, result) = self.scheme(
                                    frame,
                                    Some(value.id),
                                    target,
                                    written,
                                    Some(&hints),
                                    b,
                                    depth + 1,
                                )?;
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
                                        operation: None,
                                        target,
                                        actuals,
                                        parameters: parameters.clone(),
                                        receiver: None,
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
                    // The exact receiving expression constrains this call's
                    // unresolved result roles before its arguments are checked.
                    // This opens no equation between two rigid occurrences.
                    if let Some(expected) = self.receiving_for(frame, value.id, b)? {
                        self.call_result_context(result, expected, b, depth + 1)?;
                    }
                    let mut outer_inputs = Vec::new();
                    for (ordinal, (argument, parameter)) in arguments.iter().zip(parameters).enumerate() {
                        b.lookup(prepared_arguments.len(), WORK)?;
                        let prepared = prepared_arguments.remove(&argument.id);
                        let actual = if let Some(actual) = self.temporary_description_argument(
                            frame, value.id, ordinal, argument, parameter, prepared, b, depth + 1,
                        )? {
                            actual
                        } else if let Some(actual) = prepared {
                            actual
                        } else {
                            self.call_input_value(frame, argument, parameter, b, depth + 1)?
                        };
                        if frame.alive {
                            self.observe_code_argument(frame.definition, value.id, ordinal, actual, b)?;
                        }
                        if let Some(input) = self.collect_call_input(frame, argument, actual, parameter, ordinal, b, depth + 1)? {
                            Self::push(&mut outer_inputs, input, b)?;
                        }
                    }
                    b.lookup(self.state.calls.len(), WORK)?;
                    b.lookup(self.state.abstract_calls.len(), WORK)?;
                    let prototype = self.state.calls.get(&(frame.definition, value.id)).map(|call| call.target)
                        .or_else(|| self.state.abstract_calls.get(&(frame.definition, value.id)).map(|call| call.signature_owner));
                    self.collect_result_inputs(prototype, result, &mut outer_inputs, b)?;
                    self.settle_call_inputs(frame, value.id, prototype, &outer_inputs, b, depth + 1)?;
                    if !prepared_arguments.is_empty() { return Err(fail("original staged call leaves an unchecked argument")); }
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
                CoreExprKind::RecordConstructor { definition, fields } => {
                    self.record_constructor(frame, value.id, *definition, fields, b, depth + 1)?
                }
                CoreExprKind::RecordUpdate { ty, base, fields } => {
                    let term = self.expression(frame, base, b, depth + 1)?;
                    self.prepare_call_value(frame.definition, base.id, term, b)?;
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
                    if let Some(term) = self.enum_call(frame, value.id, value, &[], b, depth + 1)? {
                        b.lookup(frame.expressions.len(), WORK)?;
                        if frame.expressions.insert(value.id, term).is_some() {
                            return Err(fail("original nullary constructor node is reused"));
                        }
                        return Ok(term);
                    }
                    if let Some(term) = self.observe_member(frame, value, b, depth + 1)? {
                        return Ok(term);
                    }
                    let mut receiver = None;
                    let term = self.expression_with_projection(frame, object, Some(&mut receiver), b, depth + 1)?;
                    if let Some(owner) = owner {
                        let owner = self.recipe(frame, owner, b, depth + 1)?;
                        self.unify(term, owner, depth + 1, b)?;
                    }
                    self.member(frame.definition, value.id, receiver.unwrap_or(term), member, projection, b, depth + 1)?
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
                    if !*move_capture {
                        self.infer_owner_captures(frame, child, value.id, b)?;
                    }
                    let mut inherited = self.copy_locals(&frame.locals, b)?;
                    if *move_capture {
                        for environment in &child.environment {
                            crate::dyn_metadata::recipe(&environment.source, b)?;
                            let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::NameAt {
                                    at: SemanticOriginalNode::Expression(at),
                                    name,
                                },
                            }) = &environment.source else { continue; };
                            if *at != value.id {
                                return Err(fail("moved capture selects another original lexical point"));
                            }
                            b.spend(frame.locals.iter()
                                .map(|local| local.name.len().saturating_add(name.len() + 1))
                                .sum(), WORK)?;
                            let index = frame.locals.iter().rposition(|local| local.name == *name)
                                .ok_or_else(|| fail("moved capture has no exact original lexical binding"))?;
                            inherited[index].ty = self.moved_reference_capture(
                                frame, &frame.locals[index], b, depth + 1,
                            )?;
                        }
                    }
                    let mut names = Vec::new();
                    for local in &inherited {
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
                    let goal = self.closure_goal(frame, value.id, target, b, depth + 1)?;
                    let term = self.function(target, Some(&inherited), goal, b, depth + 1)?;
                    let captured = &self
                        .state
                        .frames
                        .get(&target)
                        .ok_or_else(|| fail("original child capture footprint is absent"))?
                        .captured;
                    b.spend(captured.len(), WORK)?;
                    let has_captures = !captured.is_empty();
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
                        if frame.alive { self.local_available(&frame.locals[index], &[], b)?; }
                        if index < frame.inherited {
                            b.lookup(frame.captured.len(), WORK)?;
                            frame.captured.insert(index, ty);
                        }
                        if *move_capture || self.inferred_owner_capture(target, &frame.locals[index].name, b)? {
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
                                frame.locals[index].live = false;
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
                    // Embedded signature recipes are normalized at the child
                    // declaration. Ancestor(1) reaches the creator from there.
                    // Invocation ordinals name this literal's own signature.
                    let mut lives = Vec::new();
                    for ordinal in 0..child_signature.invocation_lifetimes {
                        let life = self.term(Kind::Life(Life::Invocation(target, ordinal)), b)?;
                        Self::push(&mut lives, life, b)?;
                    }
                    Self::push(
                        &mut self.role_frames,
                        roles::RoleFrame {
                            owner: target,
                            lifetimes: lives,
                            implicits: BTreeMap::new(),
                            receiver: None,
                        },
                        b,
                    )?;
                    b.lookup(self.state.frames.len(), WORK)?;
                    let child_frame = self.state.frames.remove(&target)
                        .ok_or_else(|| fail("closure annotations lack their checked child frame"))?;
                    let annotations: CompileResult<()> = (|| {
                        for (parameter, expected) in parameters.iter().zip(&expected) {
                            if let Some(annotation) = &parameter.ty {
                                let annotation = self.recipe(&child_frame, annotation, b, depth + 1)?;
                                self.unify(annotation, *expected, depth + 1, b)?;
                            }
                        }
                        if let Some(annotation) = return_type {
                            let annotation = self.recipe(&child_frame, annotation, b, depth + 1)?;
                            self.unify(annotation, result, depth + 1, b)?;
                        }
                        Ok(())
                    })();
                    self.state.frames.insert(target, child_frame);
                    self.role_frames.pop().ok_or_else(|| {
                        fail("original closure literal lost its own signature frame")
                    })?;
                    annotations?;
                    self.same_body(body, target, b)?;
                    self.thin_closure_value(
                        frame, value.id, target, has_captures, expected, result,
                        child_signature, term, b,
                    )?
                }
                CoreExprKind::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    let condition = self.expression(frame, condition, b, depth + 1)?;
                    let boolean = self.atom(Type::Bool, b)?;
                    self.unify(condition, boolean, depth + 1, b)?;
                    let expected = self.receiving_for(frame, value.id, b)?;
                    let result = self.var(b)?;
                    let wanted = expected.unwrap_or(result);
                    let open_join = matches!(self.state.arena[self.type_root(wanted,b)?], Kind::Infer(None));
                    let alive = frame.alive;
                    let before = self.copy_locals(&frame.locals, b)?;
                    // Infer both complete branches once. Literal terms remain
                    // unresolved until their common result is constrained, so
                    // a known integer peer supplies context without evaluating
                    // another branch just to discover its type.
                    let left = self.expression_expected(frame, then_value, wanted, b, depth + 1)?;
                    if !open_join { self.unify(left, result, depth + 1, b)?; }
                    let left_alive = frame.alive;
                    let left_locals = std::mem::replace(&mut frame.locals, before);
                    frame.alive = alive;
                    let right = self.expression_expected(frame, else_value, wanted, b, depth + 1)?;
                    if open_join
                        && let (Kind::Address {life:x,..},Kind::Address {life:y,..}) =
                            (&self.state.arena[self.type_root(left,b)?],&self.state.arena[self.type_root(right,b)?])
                        && !self.same_original(*x,*y,b,depth+1)? {
                        let (Kind::Address {mutable,life:a,inner},Kind::Address {mutable:other,life:z,inner:child}) =
                            (&self.state.arena[self.type_root(left,b)?],&self.state.arena[self.type_root(right,b)?]) else {unreachable!()};
                        let (mutable,a,z,inner,child)=(*mutable,*a,*z,*inner,*child);
                        if other != &mutable {return Err(fail("conditional changes its original pointer mode"));}
                        self.unify(inner,child,depth+1,b)?;
                        let ordinal=self.borrow_occurrence(frame.definition,value.id,b)?;
                        self.remember_view_upper(frame.definition,ordinal,Some(a),b)?;
                        self.remember_view_upper(frame.definition,ordinal,Some(z),b)?;
                        let life=self.term(Kind::Life(Life::Implicit(frame.definition,ordinal)),b)?;
                        let joined=self.term(Kind::Address {mutable,life,inner},b)?;
                        self.unify(joined,result,depth+1,b)?;
                    } else {
                        if open_join { self.unify(left,result,depth+1,b)?; }
                        self.unify(right,result,depth+1,b)?;
                    }
                    let right_alive = frame.alive;
                    if left_locals.len() != frame.locals.len() {
                        return Err(fail("original value branch changes its lexical binding inventory"));
                    }
                    for (left, right) in left_locals.iter().zip(&mut frame.locals) {
                        b.spend(1, WORK)?;
                        match (left_alive, right_alive) {
                            (true, true) => self.join_vacancies(right, left, b)?,
                            (true, false) => {
                                right.vacant = self.copy_vacancies(&left.vacant, b)?;
                                right.maybe_vacant = self.copy_vacancies(&left.maybe_vacant, b)?;
                                right.live = left.live;
                            }
                            _ => {},
                        }
                        right.used = match (left_alive, right_alive) {
                            (true, true) => left.used || right.used,
                            (true, false) => left.used,
                            _ => right.used,
                        };
                        right.initialized = match (left_alive, right_alive) {
                            (true, true) => left.initialized && right.initialized,
                            (true, false) => left.initialized,
                            _ => right.initialized,
                        };
                    }
                    frame.alive = left_alive || right_alive;
                    result
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
        if preparing {
            let current = self.preparations.last()
                .ok_or_else(|| fail("original call lost its preparation transaction"))?;
            if self.preparations.len() != preparation_depth + 1
                || current.definition != frame.definition || current.call != value.id {
                return Err(fail("original call preparation commits another expression transaction"));
            }
            // Success transferred the prepared operands to this operation;
            // compiler rejection publishes no partially checked transaction.
            self.preparations.pop();
        }
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
                    if let Some(initializer) = initializer {
                        let initializer_node = initializer.id;
                        let value = self.expression_expected(
                            frame,
                            initializer,
                            family.unwrap_or(local),
                            b,
                            depth + 1,
                        )?;
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
                        let value = self
                            .reference_binding(
                                frame,
                                statement.id,
                                initializer_node,
                                value,
                                *mutable,
                                ty.is_some(),
                                b,
                                depth + 1,
                            )?
                            .unwrap_or(value);
                        self.unify(local, value, depth + 1, b)?;
                        if !*mutable && !self.state.intrinsic_anchors.is_empty()
                            && matches!(self.state.arena[self.type_root(value, b)?], Kind::Address { .. })
                            && let Some(input) = self.store_view_input(frame, initializer, b)?
                        {
                            b.lookup(self.state.store_view_inputs.len(), WORK)?;
                            self.state.store_view_inputs.insert((frame.definition, statement.id), input);
                        }
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
                            local.live = false;
                        }
                    }
                }
                CoreStatementKind::TupleBinding {
                    pattern,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let mode = self.pattern_reference_mode(pattern, b, depth + 1)?;
                    let (subject, source) =
                        self.observed_pattern_subject(frame, initializer, mode, pattern_places::record_subject(pattern), b, depth + 1)?;
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
                        &source,
                        *mutable,
                        b,
                        depth + 1,
                    )?;
                }
                CoreStatementKind::Defer(body) => {
                    let (action, result, _) = body.deferred_action().ok_or_else(||
                        fail("original defer has no zero-parameter nonmoving lexical action"))?;
                    if result != &SemanticTypeRecipe::Atom(Type::Unit) {
                        return Err(fail("original defer action changes its Unit result"));
                    }
                    let registration = self.inventory.definition(frame.definition, b)?;
                    let recipe = self.original_site(registration, action.id, b)?;
                    let SemanticSiteRecipeKind::Closure { definition, .. } = recipe.kind else {
                        return Err(fail("original defer operand does not create its lexical closure"));
                    };
                    b.lookup(self.cleanup_actions.len(), WORK)?;
                    if self.cleanup_actions.contains(&frame.definition) {
                        return Err(fail("original cleanup action attempts a nested defer"));
                    }
                    let child = self.inventory.definition(definition, b)?;
                    if !matches!(&child.body, SemanticDefinitionBody::FunctionWithBody { signature, .. }
                        if signature.call_kind == CallKind::Direct && signature.parameters.is_empty()) {
                        return Err(fail("original defer action has an asynchronous or nonempty call signature"));
                    }
                    self.cleanup_actions.insert(definition);
                    // Normal closure formation binds the declared child, captures, ownership
                    // transfers and result. The registration retains it until scope exit.
                    self.expression(frame, action, b, depth + 1)?;
                }
                CoreStatementKind::Expression(value) => {
                    let discarded = self.expression(frame, value, b, depth + 1)?;
                    if frame.alive {
                        self.record_cleanup(discarded, &cleanup::Origin::expression(frame.definition, value.id), &[], b)?;
                    }
                }
                CoreStatementKind::Return(value) => {
                    if !self.cleanup_actions.is_empty() {
                        b.lookup(self.cleanup_actions.len(), WORK)?;
                        if self.cleanup_actions.contains(&frame.definition) {
                            return Err(fail("original deferred cleanup exits before completing its action"));
                        }
                    }
                    let returned_node = value.as_ref().map(|value| value.id);
                    let receiving = frame.result;
                    let value = match value {
                        Some(value) => {
                            if self.inferred_receiver_body(frame.definition, b)?
                                && matches!(self.state.arena[self.type_root(receiving, b)?],
                                    Kind::Infer(None)
                                    | Kind::Address { mutable: false, .. })
                            {
                                self.expression(frame, value, b, depth + 1)?
                            } else { self.expression_expected(frame, value, receiving, b, depth + 1)? }
                        }
                        None => self.atom(Type::Unit, b)?,
                    };
                    let receiver_return = self.inferred_receiver_return(frame, value, b, depth + 1)?;
                    let expected = self.type_root(frame.result, b)?;
                    let actual = self.type_root(value, b)?;
                    if let Kind::ErrorUnion { error, result } = self.state.arena[expected]
                        && !matches!(self.state.arena[actual], Kind::ErrorUnion { .. })
                    {
                        // Ordinary return values are the success arm of the
                        // declaration's real ErrorUnion, never a new error type
                        // inferred from a selected concrete caller.
                        let is_error = self.same_original(actual, error, b, depth + 1)?
                            && !self.same_original(actual, result, b, depth + 1)?;
                        let arm = if is_error { error } else { result };
                        self.unify(value, arm, depth + 1, b)?;
                        if let Some(node) = returned_node {
                            b.lookup(self.state.return_injections.len(), WORK)?;
                            if self
                                .state
                                .return_injections
                                .insert((frame.definition, node), (frame.result, is_error))
                                .is_some()
                            {
                                return Err(fail(
                                    "original return has competing implicit union injections",
                                ));
                            }
                        }
                    } else if !receiver_return {
                        let projected = match &statement.kind {
                            CoreStatementKind::Return(Some(expression)) =>
                                self.constructor_return(frame, expression, value, b, depth + 1)?,
                            _ => false,
                        };
                        if !projected { self.unify(value, frame.result, depth + 1, b)?; }
                    }
                    if frame.alive {
                        self.capture_exit(frame, b)?;
                        self.cleanup_preparations(frame.definition, b)?;
                        self.cleanup(frame.definition, &frame.locals[frame.inherited..], b)?;
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
                        match (then_alive, else_alive) {
                            (true, true) => self.join_vacancies(other, then, b)?,
                            (true, false) => {
                                other.vacant = self.copy_vacancies(&then.vacant, b)?;
                                other.maybe_vacant = self.copy_vacancies(&then.maybe_vacant, b)?;
                                other.live = then.live;
                            }
                            _ => {}
                        }
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
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.if_let_statement(
                        frame,
                        statement.id,
                        pattern,
                        value,
                        then_body,
                        else_body.as_ref(),
                        b,
                        depth + 1,
                    )?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    self.pattern_loop(frame, statement.id, pattern, value, body, b, depth + 1)?;
                }
                CoreStatementKind::ForRange { name, start, end, body } => {
                    self.range_loop(frame, statement.id, loops::RangeBinding::Name(name),
                        start, end, body, b, depth + 1)?;
                }
                CoreStatementKind::ForEach { pattern, iterator, body } => {
                    let (start, end) = self.range_parts(iterator, b, depth + 1)?;
                    self.range_loop(frame, statement.id, loops::RangeBinding::Pattern(pattern),
                        start, end, body, b, depth + 1)?;
                }
                CoreStatementKind::While { condition, body } => {
                    self.loop_statement(frame, statement.id, Some(condition), body, b, depth + 1)?;
                }
                CoreStatementKind::Loop(body) => {
                    self.loop_statement(frame, statement.id, None, body, b, depth + 1)?;
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    self.loop_jump(
                        frame,
                        matches!(&statement.kind, CoreStatementKind::Break),
                        b,
                    )?;
                }
                CoreStatementKind::CompoundAssign { target, op, value } => {
                    if self.projected_compound(frame, target, *op, value, b, depth + 1)? {
                        continue;
                    }
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
                    self.local_available(local, &[], b)?;
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
                    self.record_environment_read(frame, index, target.id, b)?;
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
                        frame.locals[index].live = true;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    if self.projected_assign(frame, statement.id, target, value, b, depth + 1)? {
                        continue;
                    }
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
                    let anchor = if index < frame.inherited {
                        Some(self.environment_anchor(frame, &frame.locals[index].name, b)?)
                    } else { frame.locals[index].anchor };
                    self.record_cleanup_write(frame.definition, statement.id, anchor, &[], value.id, b)?;
                    if let Some(binding) = self.reference_assignment_target(frame, index, b)? {
                        let actual = self.expression(frame, value, b, depth + 1)?;
                        self.reference_assignment(
                            frame,
                            binding,
                            statement.id,
                            value.id,
                            actual,
                            b,
                            depth + 1,
                        )?;
                    } else {
                        let actual =
                            self.expression_expected(frame, value, expected, b, depth + 1)?;
                        self.unify(actual, expected, depth + 1, b)?;
                    }
                    b.lookup(frame.expressions.len(), WORK)?;
                    if frame.expressions.insert(target.id, expected).is_some() {
                        return Err(fail("original assignment destination node is reused"));
                    }
                    if frame.alive {
                        let old = &frame.locals[index];
                        if old.live {
                            let origin = cleanup::Origin::local(frame.definition, anchor, cleanup::Event::Replace(statement.id));
                            self.cleanup_partial(old.ty, &[], &old.vacant, &origin, b, 0)?;
                        }
                        frame.locals[index].used = false;
                        frame.locals[index].initialized = true;
                        frame.locals[index].live = true;
                        frame.locals[index].vacant.clear();
                        frame.locals[index].maybe_vacant.clear();
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
            self.cleanup(frame.definition, &frame.locals[scope..], b)?;
        }
        frame.locals.truncate(scope);
        Ok(())
    }
    fn function(
        &mut self,
        id: SemanticDefinitionId,
        inherited: Option<&[Local]>,
        goal: Option<(Vec<Term>, Term)>,
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
        if let Some((inputs, _)) = &goal
            && (definition.kind != SemanticDefinitionKind::Closure
                || signature.call_kind != CallKind::Direct
                || inputs.len() != parameters.len())
        {
            return Err(fail("original closure changes its exact receiving signature"));
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
        // A nested nominal application may ask about this declaration's own
        // rigid where-context while its body is still being checked. Preserve
        // the real lexical header, without marking its body complete: active
        // recursion remains rejected and only the final snapshot is returned
        // from a successful whole-program proof.
        self.snapshot(&frame, b)?;
        self.original_trait_facts(&frame, b)?;
        if let Some(inherited) = inherited {
            for local in inherited {
                self.bind(&mut frame, &local.name, local.ty, local.mutable, b)?;
            }
            frame.inherited = frame.locals.len();
            let moved = self.owns_captures(id, b)?;
            for local in &mut frame.locals {
                local.borrowed = !(moved || self.inferred_owner_capture(id, &local.name, b)?);
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
            if let Some((inputs, _)) = &goal {
                self.unify(frame.parameters[index], inputs[index], depth + 1, b)?;
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
        if !matches!(
            signature.result,
            SemanticTypeRecipe::Opaque { .. } | SemanticTypeRecipe::OriginalType { .. }
        ) {
            let expected = self.recipe(&frame, &signature.result, b, depth + 1)?;
            self.unify(frame.result, expected, depth + 1, b)?;
        }
        if let Some((_, result)) = goal {
            self.unify(frame.result, result, depth + 1, b)?;
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
            self.cleanup(frame.definition, &frame.locals[frame.inherited..], b)?;
            let unit = self.atom(Type::Unit, b)?;
            if let Kind::ErrorUnion { result, .. } =
                self.state.arena[self.type_root(frame.result, b)?]
            {
                self.unify(result, unit, depth + 1, b)?;
            } else {
                self.unify(frame.result, unit, depth + 1, b)?;
            }
        }
        self.finish_inferred_receiver(&mut frame, b)?;
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
        self.compare_child_body(body, target, b)
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
            CoreStatementKind::ForRange { start, end, body, .. } => {
                normalize_expr(start, b, depth + 1)?;
                normalize_expr(end, b, depth + 1)?;
                normalize_block(body, b, depth + 1)?;
            }
            CoreStatementKind::ForEach { pattern, iterator, body } => {
                normalize_pattern(pattern, b, depth + 1)?;
                normalize_expr(iterator, b, depth + 1)?;
                normalize_block(body, b, depth + 1)?;
            }
            CoreStatementKind::Loop(body) => normalize_block(body, b, depth + 1)?,
            CoreStatementKind::Break | CoreStatementKind::Continue => {}
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) | CoreStatementKind::Defer(body) => {
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
        CoreExprKind::Binary { left, right, .. }
        | CoreExprKind::Range { start: left, end: right } => {
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
            let index = inventory.original_index(definition.id, b)?;
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
            closed_formations: Default::default(),
            closed_values: None,
            frames: BTreeMap::new(),
            calls: BTreeMap::new(),
            intrinsics: BTreeMap::new(),
            intrinsic_inputs: BTreeMap::new(),
            intrinsic_anchors: BTreeMap::new(),
            store_view_inputs: BTreeMap::new(),
            native_targets: BTreeMap::new(),
            registration_factories: BTreeMap::new(),
            scoped_foreign_operands: BTreeMap::new(),
            task_worker_actuals: BTreeMap::new(),
            native_paths: BTreeMap::new(),
            abstract_calls: BTreeMap::new(),
            trait_calls: BTreeMap::new(),
            task_templates: Vec::new(),
            task_address_heads: BTreeMap::new(),
            dyn_templates: Vec::new(),
            borrows: BTreeMap::new(),
            descriptions: BTreeMap::new(),
            temporary_descriptions: BTreeMap::new(),
            owner_transfers: BTreeMap::new(),
            captured_descriptions: BTreeMap::new(),
            inferred_owner_captures: BTreeMap::new(),
            enum_constructors: BTreeMap::new(),
            patterns: BTreeMap::new(),
            pattern_origins: Vec::new(),
            indirect_pattern_borrows: BTreeMap::new(),
            range_bindings: BTreeMap::new(),
            names_at: BTreeMap::new(),
            environment_reads: BTreeMap::new(),
            local_reads: BTreeMap::new(),
            prepared_reads: BTreeMap::new(),
            prepared_address_inputs: BTreeMap::new(),
            capture_origins: BTreeMap::new(),
            function_values: BTreeMap::new(),
            code_calls: BTreeMap::new(),
            string_literals: BTreeMap::new(),
            ffi_literals: BTreeMap::new(),
            return_injections: BTreeMap::new(),
            dyn_creations: BTreeMap::new(),
            receiving_places: BTreeMap::new(),
            received_result_roles: BTreeSet::new(),
            view_uppers: BTreeMap::new(),
            reference_roles: BTreeMap::new(),
            call_regions: BTreeMap::new(),
            call_uses: BTreeMap::new(),
            pending_cleanups: Vec::new(),
            cleanup_writes: BTreeMap::new(),
        },
        active: BTreeSet::new(),
        creators: BTreeMap::new(),
        numeric: Vec::new(),
        equality_operands: Vec::new(),
        literals: Vec::new(),
        integers: Vec::new(),
        needs: Vec::new(),
        cleanup_terms: Vec::new(),
        preparations: Vec::new(),
        callable_needs: Vec::new(),
        mutating_closures: BTreeSet::new(),
        cleanup_actions: BTreeSet::new(),
        role_frames: Vec::new(),
        bound_frames: Vec::new(),
        next_role: 0,
        loops: Vec::new(),
        trait_state: traits::State::default(),
        capture_state: captures::CaptureState::default(),
        code_inputs: code_inputs::CodeInputs::default(),
        receiving: Vec::new(),
        local_return_bounds: BTreeSet::new(),
        field_needs: Vec::new(),
        nominal_declaration: None,
        call_variables: BTreeMap::new(),
        call_parameters: BTreeSet::new(),
        constructor_templates: BTreeMap::new(),
        constructor_constants: BTreeMap::new(),
    };
    solver.trait_catalog(b)?;
    for definition in &program.definitions {
        if definition.kind == SemanticDefinitionKind::Method
            && definition.parent.is_some_and(|parent| {
                inventory
                    .definitions
                    .get(&parent)
                    .is_some_and(|owner| matches!(
                        owner.body,
                        SemanticDefinitionBody::Scope(_) | SemanticDefinitionBody::Nominal { .. }
                    ))
            })
        {
            solver.function(definition.id, None, None, b, 0)?;
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
            solver.nominal_declaration = Some(definition.id);
            let formed = solver.application(definition.id, arguments, b, 0);
            solver.nominal_declaration = None;
            formed?;
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
            solver.function(definition.id, None, None, b, 0)?;
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
        solver.function(wrapper, None, None, b, 0)?;
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
    solver.complete_code_inputs(b)?;
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
    if !solver.preparations.is_empty() {
        return Err(fail("original body checking left an uncommitted operand preparation"));
    }
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
    solver.check_record_constructor_actuals(b)?;
    for term in &solver.integers {
        let root = solver.type_root(*term, b)?;
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if integer_width(ty).is_some()) {
            return Err(fail(
                "original bit or remainder operation has a noninteger type",
            ));
        }
    }
    for term in &solver.equality_operands {
        let root = solver.type_root(*term, b)?;
        if !matches!(&solver.state.arena[root], Kind::Atom(ty) if ty.is_b0_scalar() || *ty == Type::ForeignHandle)
        {
            return Err(unknown(
                "original equality requires an admitted scalar or foreign handle",
            ));
        }
    }
    for need in &solver.needs {
        if !solver.satisfies(need.term, need.predicate, b, 0)? {
            let mut errors = fail(match need.predicate {
                Predicate::Copy => "original repeated use requires Copy in its declaration context",
                Predicate::SyncDrop => {
                    "original live cleanup edge requires SyncDrop in its declaration context"
                }
                Predicate::Value => "original actual does not establish Value",
                Predicate::StoreElement => "original actual does not establish StoreElement",
                Predicate::Capability(_) => {
                    "original actual does not establish its declared sealed capability"
                }
            });
            if matches!(need.predicate, Predicate::Copy) {
                let root = solver.type_root(need.term, b)?;
                let detail = match &solver.state.arena[root] {
                    Kind::Rigid(owner, ordinal) => format!("rigid {}:{} mixed {}", owner.module.0, owner.index, ordinal),
                    Kind::Nominal { definition, .. } | Kind::Closure { definition, .. } => format!("definition {}:{}", definition.module.0, definition.index),
                    kind => format!("kind {:?}", std::mem::discriminant(kind)),
                };
                errors[0].notes.push(format!("original Copy subject {}", detail));
                let mut shown = 0usize;
                'frames: for frame in solver.state.frames.values() {
                    for (node, term) in &frame.expressions {
                        b.spend(1, WORK)?;
                        if *term == need.term || solver.type_root(*term, b)? == root {
                            b.spend(64, WORK)?;
                            errors[0].notes.push(format!("original Copy value definition {}:{} expression {}", frame.definition.module.0, frame.definition.index, node.0));
                            shown += 1;
                            if shown == 16 { break 'frames; }
                        }
                    }
                }
            }
            if matches!(need.predicate, Predicate::SyncDrop) {
                let root = solver.type_root(need.term, b)?;
                let detail = match &solver.state.arena[root] {
                    Kind::Rigid(owner, ordinal) => format!("rigid {:?} mixed {}", owner, ordinal),
                    Kind::Nominal { definition, .. } | Kind::Closure { definition, .. } => {
                        format!("definition {:?}", definition)
                    }
                    kind => format!("kind {:?}", std::mem::discriminant(kind)),
                };
                errors[0]
                    .notes
                    .push(format!("original cleanup subject {}", detail));
            }
            if matches!(need.predicate, Predicate::StoreElement) {
                let root = solver.type_root(need.term, b)?;
                let detail = match &solver.state.arena[root] {
                    Kind::Nominal { definition, .. } => format!("nominal {:?}", definition),
                    kind => format!("kind {:?}", std::mem::discriminant(kind)),
                };
                errors[0]
                    .notes
                    .push(format!("original StoreElement subject {}", detail));
            }
            if matches!(need.predicate, Predicate::Value) {
                for (term, owner, field) in &solver.field_needs {
                    b.spend(1, WORK)?;
                    if solver.root(*term, b)? == solver.root(need.term, b)? {
                        b.spend(64, WORK)?;
                        errors[0].notes.push(format!(
                            "original inline field definition {}:{} field {}",
                            owner.module.0, owner.index, field
                        ));
                    }
                }
            }
            return Err(errors);
        }
    }
    solver.check_cleanups(module, b)?;
    for frame in solver.state.frames.values() {
        for value in &frame.parameters {
            solver.sized_original(*value, b, 0)?;
        }
        solver.sized_method_result(frame, b)?;
    }
    for (owner, source, target) in std::mem::take(&mut solver.local_return_bounds) {
        b.lookup(solver.state.frames.len(), WORK)?;
        let frame = solver.state.frames.get(&owner).ok_or_else(|| fail("returned reference role has no original frame"))?;
        b.spend(frame.formals.len(), WORK)?;
        let context = nominal::frame(owner, frame.formals.clone());
        if !solver.view_upper_allows(&context, source, target, &mut BTreeSet::new(), b, 0)? {
            return Err(fail("updated local reference sources exceed the required return bound"));
        }
    }
    solver.reference_role_escapes(b)?;
    effects.finish(&solver.state, inventory, b)?;
    protocol::check(&solver.state, inventory, b)?;
    solver.check_resume_capabilities(b)?;
    solver.check_scoped_threads(b)?;
    b.spend(module.functions.len(), WORK)?;
    let terms = solver.state.arena.len();
    b.spend(terms, WORK)?;
    let mut closed = Vec::new();
    closed.try_reserve_exact(terms).map_err(|_| resource("original value table allocation failed"))?;
    closed.resize_with(terms, || None);
    solver.state.closed_values = Some(std::cell::RefCell::new(closed));
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
                    prepared_at: local.prepared_at,
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
                    live: local.live,
                    vacant: self.copy_vacancies(&local.vacant, b)?,
                    maybe_vacant: self.copy_vacancies(&local.maybe_vacant, b)?,
                },
                b,
            )?;
        }
        Ok(output)
    }
    // spec/trait-owned-results.md keeps the concrete provider responsible for its Self layout.
    fn sized_method_result(&self, frame: &Frame, b: &mut Budget) -> CompileResult<()> {
        if let Kind::ErrorUnion { error, result } = &self.state.arena[self.root(frame.result, b)?] {
            if let Kind::SelfType(owner) = &self.state.arena[self.root(*result, b)?] {
                let method = self.inventory.definition(frame.definition, b)?;
                let declaration = self.inventory.definition(*owner, b)?;
                let fixed_member = method.parent == Some(*owner)
                    && method.kind == SemanticDefinitionKind::Method
                    && method.parameters.is_empty() && declaration.parameters.is_empty()
                    && matches!(declaration.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. }))
                    && matches!(method.body, SemanticDefinitionBody::FunctionWithBody {
                        body: SemanticOriginalFunctionBody::Declaration, receiver_parameter: Some(0), ..
                    });
                let plain_error = match &self.state.arena[self.root(*error, b)?] {
                    Kind::Atom(ty) => ty.is_b0_scalar(),
                    Kind::Nominal { arguments, .. } => arguments.is_empty(),
                    _ => false,
                };
                if fixed_member && plain_error && !frame.parameters.is_empty() {
                    let mut profile = true;
                    for (index, parameter) in frame.parameters.iter().enumerate() {
                        b.spend(1, WORK)?;
                        profile &= match &self.state.arena[self.root(*parameter, b)?] {
                            Kind::Address { inner, mutable, .. } if index == 0 || !mutable => {
                                matches!(&self.state.arena[self.root(*inner, b)?], Kind::SelfType(actual) if actual == owner)
                            }
                            Kind::Atom(ty) if index != 0 => ty.is_b0_scalar(),
                            _ => false,
                        };
                    }
                    if profile {
                        // Each concrete implementation supplies the complete Self layout.
                        // The abstract signature still supplies a concrete error arm.
                        return self.carrier_original(*error, true, b, 1);
                    }
                }
            }
        }
        self.sized_original(frame.result, b, 0)
    }
    fn sized_original(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.carrier_original(term, false, b, depth)
    }

    // Passing an abstract whole value does not claim an inline layout or a
    // structural Value capability. Aggregate occurrences require that extra
    // theorem in the original declaration context.
    fn carrier_original(
        &self,
        term: Term,
        inline: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
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
            Kind::Dyn(ty) if !ty.owned => Err(fail(
                "original borrowed Dyn payload requires its real reference carrier",
            )),
            Kind::Dyn(_) => Ok(()),
            Kind::Array { element, .. } => self.carrier_original(*element, true, b, depth + 1),
            Kind::ErrorUnion { error, result } => {
                self.carrier_original(*error, true, b, depth + 1)?;
                self.carrier_original(*result, true, b, depth + 1)
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
                    self.carrier_original(*field, true, b, depth + 1)?;
                }
                Ok(())
            }
            Kind::CallResult { hidden, .. } => self.carrier_original(*hidden, inline, b, depth + 1),
            Kind::Associated { .. } | Kind::SelfType(_) | Kind::Rigid(..)
                if !inline || self.satisfies(term, Predicate::Value, b, depth + 1)? =>
            {
                Ok(())
            }
            Kind::Associated { .. } | Kind::SelfType(_) | Kind::Rigid(..) => Err(unknown(
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
    fn cleanup(&mut self, definition: SemanticDefinitionId, locals: &[Local], b: &mut Budget) -> CompileResult<()> {
        for local in locals {
            b.spend(1, WORK)?;
            if local.live && !local.borrowed {
                let origin = local.prepared_at.map_or_else(
                    || cleanup::Origin::local(definition, local.anchor, cleanup::Event::Scope),
                    |producer| cleanup::Origin::expression(definition, producer),
                );
                self.cleanup_partial(local.ty, &[], &local.vacant, &origin, b, 0)?;
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
        self.premises_well_formed_at(context, requirements, b, 0)
    }
    fn premises_well_formed_at(
        &self,
        context: SemanticDefinitionId,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for requirement in requirements {
            b.spend(1, WORK)?;
            if let SemanticRequirementRecipe::ForAll { requirements, .. } = requirement {
                self.premises_well_formed_at(context, requirements, b, depth + 1)?;
                continue;
            }
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
            if let SemanticRequirementRecipe::Callable { ty, .. } = requirement {
                if let SemanticTypeRecipe::Formal(reference) = ty {
                    let owner = self.original_scope(context, &reference.scope, b)?;
                    if !matches!(
                        self.inventory.definition(owner, b)?.parameters.get(reference.ordinal as usize),
                        Some(SemanticFormal::Type { .. })
                    ) {
                        return Err(fail("original callable subject changes its formal kind"));
                    }
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
            if matches!(requirement, SemanticRequirementRecipe::Outlives { .. })
                && self.inventory.definition(context, b)?.elided_result_bound(requirement).is_some() {
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
        self.premise_needs_at(frame, requirements, b, 0)
    }
    fn premise_needs_at(
        &mut self,
        frame: &Frame,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for requirement in requirements {
            if let SemanticRequirementRecipe::ForAll { lifetimes, requirements } = requirement {
                let checkpoint = self.bound_frames.len();
                self.push_bound_frame(frame.definition, *lifetimes, false, b)?;
                let result = self.premise_needs_at(frame, requirements, b, depth + 1);
                self.bound_frames.truncate(checkpoint);
                result?;
                continue;
            }
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
            if matches!(requirement, SemanticRequirementRecipe::Outlives { .. })
                && self.inventory.definition(frame.definition, b)?.elided_result_bound(requirement).is_some() {
                // The actual call operands discharge this bound at their original Call.
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
        self.assumption_at(owner, ordinal, requirements, wanted, b, 0)
    }
    fn assumption_at(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        requirements: &[SemanticRequirementRecipe],
        wanted: Predicate,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        for requirement in requirements {
            b.spend(1, WORK)?;
            if let SemanticRequirementRecipe::ForAll { requirements, .. } = requirement {
                if self.assumption_at(owner, ordinal, requirements, wanted, b, depth + 1)? {
                    return Ok(true);
                }
                continue;
            }
            if let SemanticRequirementRecipe::Callable {
                ty: SemanticTypeRecipe::Formal(reference),
                ..
            } = requirement
            {
                if matches!(wanted, Predicate::Value)
                    && self.original_scope(owner, &reference.scope, b)? == owner
                    && reference.ordinal == ordinal
                {
                    // A callable premise describes an ordinary environment
                    // value. It supplies neither Copy nor synchronous cleanup.
                    return Ok(true);
                }
                continue;
            }
            if matches!(
                requirement,
                SemanticRequirementRecipe::Callable { .. }
                    | SemanticRequirementRecipe::Trait { .. }
            ) {
                continue;
            }
            if matches!(requirement, SemanticRequirementRecipe::Outlives { .. })
                && self.inventory.definition(owner, b)?.elided_result_bound(requirement).is_some() { continue; }
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
                || matches!(ty, Type::ForeignHandle | Type::CWideString)
                || *ty == crate::generic_mutex::error_type()
                || crate::dyn_dispatch::is_create_error(ty)
                || strings::is_byte_type(ty))
                && (!matches!(predicate, Predicate::StoreElement)
                    || (!matches!(ty, Type::Unit | Type::CWideString)
                        && !crate::dyn_dispatch::is_create_error(ty)
                        && !strings::is_byte_type(ty)))),
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
            Kind::Array { element, length } => {
                if matches!(predicate, Predicate::StoreElement) {
                    let length = self.root(*length, b)?;
                    match self.state.arena[length] {
                        Kind::ConstValue(Constant::Usize(0)) => return Ok(false),
                        Kind::ConstValue(Constant::Usize(_)) => {}
                        _ => return Err(unknown("array Store element requires its original concrete length")),
                    }
                }
                self.satisfies(*element, predicate, b, depth + 1)
            }
            Kind::Desc { .. } => Ok(!matches!(predicate, Predicate::StoreElement)),
            Kind::MutexGuard { .. } => {
                Ok(matches!(predicate, Predicate::Value | Predicate::SyncDrop))
            }
            Kind::Wrapped(Wrapper::Managed, payload) => match predicate {
                Predicate::Copy => Ok(false),
                Predicate::StoreElement => self.owned_sync(*payload, b, depth + 1),
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
            Kind::Dyn(ty) => match predicate {
                Predicate::Value => Ok(ty.owned),
                Predicate::SyncDrop if ty.owned => self.owned_dyn_sync(ty, b),
                _ => Ok(false),
            },
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
                    let mut callback = None;
                    let mut callbacks = 0usize;
                    for field in fields {
                        let root = self.type_root(*field, b)?;
                        if matches!(self.state.arena[root], Kind::Closure { .. }) {
                            callbacks += 1;
                            let same_callback = if let Some(old) = callback {
                                self.same_original(old, root, b, depth + 1)?
                            } else {
                                true
                            };
                            if *kind != LifeNominalKind::Package
                                || callbacks > 2
                                || !same_callback
                                || !self.capability(
                                    root,
                                    SemanticBuiltinTrait::ClosedEnvironment,
                                    b,
                                    depth + 1,
                                )?
                                || !self.satisfies(root, Predicate::SyncDrop, b, depth + 1)?
                            {
                                return Ok(false);
                            }
                            callback = Some(root);
                        } else if !self.satisfies(*field, Predicate::StoreElement, b, depth + 1)? {
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
            Kind::Closure { definition, environment, .. } => match predicate {
                Predicate::Copy => self.closure_copy(*definition, b, depth + 1),
                Predicate::StoreElement => Ok(false),
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

fn site_expression(
    owner: SemanticDefinitionId,
    site: SemanticSiteId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
    missing: &str,
    statement: &str,
) -> CompileResult<NodeId> {
    match inventory.original_index(owner, b)?.site_node(site, b)? {
        Some(SemanticOriginalNode::Expression(node)) => Ok(node),
        Some(SemanticOriginalNode::Statement(_)) => Err(fail(statement)),
        None => Err(fail(missing)),
    }
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
                .into_type(b)?),
            Kind::Array { .. }
            | Kind::Task(_)
            | Kind::Dyn(_)
            | Kind::MutexGuard { .. }
            | Kind::Code { .. }
            | Kind::ErrorUnion { .. }
            | Kind::Desc { .. }
            | Kind::Wrapped(_, _) => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .into_type(b)?),
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
                .into_type(b)?),
            Kind::Closure { .. } => self.value(term, context, program, inventory, b, depth + 1)?.into_type(b),
            Kind::Infer(_) => Err(unknown(
                "original Type remains unresolved independently of actual caller headers",
            )),
            Kind::Associated { .. } | Kind::SelfType(_) => Ok(self
                .value(term, context, program, inventory, b, depth + 1)?
                .into_type(b)?),
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
            .into_type(b)?)
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
        b.lookup(self.frames.len(), WORK)?;
        let frame = self
            .frames
            .get(&context.definition)
            .ok_or_else(|| fail("original Type source has no checked declaration frame"))?;
        let term = match source {
            SemanticOriginalTypeSource::Expression(id) => {
                b.lookup(frame.expressions.len(), WORK)?;
                frame.expressions.get(id).copied()
            }
            SemanticOriginalTypeSource::Binding(id) => {
                b.lookup(frame.bindings.len(), WORK)?;
                frame.bindings.get(id).copied()
            }
            SemanticOriginalTypeSource::BindingProjection { statement, pattern } => {
                let width = pattern.len().saturating_add(1);
                b.lookup_with_width(self.patterns.len(), width, WORK)?;
                b.spend(width, WORK)?;
                self.patterns.get(&(context.definition, *statement, pattern.clone())).copied()
            }
            SemanticOriginalTypeSource::Parameter(index) => frame.parameters.get(*index as usize).copied(),
            SemanticOriginalTypeSource::Result => Some(frame.result),
            SemanticOriginalTypeSource::NameAt { at:SemanticOriginalNode::Expression(at),name } => {
                b.lookup(self.names_at.len(),WORK)?;
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
        let node = site_expression(context.definition, site, inventory, b,
            "actual call inputs have no original node", "actual call input selects an original statement")?;
        b.lookup(self.calls.len(), WORK)?;
        let call = self
            .calls
            .get(&(context.definition, node))
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
    pub(super) fn return_injection(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<(crate::semantic::value::Value, bool)>> {
        let Some(SemanticOriginalNode::Expression(node)) = inventory.original_index(context.definition, b)?.site_node(site, b)? else { return Ok(None); };
        b.lookup(self.return_injections.len(), WORK)?;
        let Some((whole, error)) = self.return_injections.get(&(context.definition, node)) else { return Ok(None); };
        Ok(Some((self.value(*whole, context, program, inventory, b, 0)?, *error)))
    }

    pub(super) fn borrowed_source(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<borrow_places::Source>> {
        let Some(SemanticOriginalNode::Expression(node)) = inventory.original_index(context.definition, b)?.site_node(site, b)? else { return Ok(None); };
        b.lookup(self.borrows.len(), WORK)?;
        let Some(source) = self.borrows.get(&(context.definition, node)) else { return Ok(None); };
        b.spend(source.path.len(), WORK)?;
        Ok(Some(borrow_places::Source {
            role: source.role.as_ref().map(|role| -> CompileResult<_> {
                Ok((
                    self.life_value(role.occurrence, context, inventory, b)?,
                    role.upper.map(|upper| self.life_value(upper, context, inventory, b)).transpose()?,
                    self.life_value(role.selected, context, inventory, b)?,
                    role.local,
                ))
            }).transpose()?,
            anchor: source.anchor,
            expression: source.expression,
            index: source.index,
            root: self.value(source.root, context, program, inventory, b, 0)?,
            path: source.path.clone(),
            inner: self.value(source.inner, context, program, inventory, b, 0)?,
            indirect: source.indirect,
            slice: source.slice,
        }))
    }
    pub(super) fn intrinsic(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<sealed::Intrinsic>> {
        let node = site_expression(context.definition, site, inventory, b,
            "actual core operation has no original anchor", "actual core operation selects a statement")?;
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
        let node = site_expression(context.definition, site, inventory, b,
            "actual native head omits its source occurrence", "actual native head selects an original statement")?;
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
        let node = site_expression(context.definition, site, inventory, b,
            "actual native result omits its original occurrence", "actual native result selects an original statement")?;
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
        let node = site_expression(context.definition, site, inventory, b,
            "actual native inputs omit their original node", "actual native call selects an original statement")?;
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
        Option<CallKind>,
    )> {
        let node = site_expression(context.definition, site, inventory, b,
            "actual BodyCall has no original node", "actual BodyCall selects an original statement")?;
        b.lookup(self.calls.len(), WORK)?;
        let call = self
            .calls
            .get(&(context.definition, node))
            .ok_or_else(|| fail("actual BodyCall has no independently resolved original callee"))?;
        let mut actuals = Vec::new();
        for actual in &call.actuals {
            let actual = self.argument(*actual, context, program, inventory, b, 0)?;
            Solver::push(&mut actuals, actual, b)?;
        }
        Ok((call.target, call.receiver.is_some(), actuals, call.operation))
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
        let node = site_expression(instance.definition, site, inventory, b,
            "actual installation loses its original call anchor", "actual installation selects an original statement")?;
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
