//! Original Task equations. A retained shape describes a type domain; it is
//! never a proof that an occupant is Closed, has no loans, or may be dropped.
use super::*;
use crate::semantic::value::{Class, Value};

#[derive(Clone, PartialEq, Eq)]
enum Sequence {
    One(usize),
    All(usize, Vec<LifeTypeProjection>),
}
#[derive(Clone, PartialEq, Eq)]
enum Port {
    One(usize, StoredBorrowMode, usize),
    From(usize, Vec<Sequence>),
}
#[derive(Clone, PartialEq, Eq)]
enum Retain {
    One(Vec<LifeTypeProjection>, u32, usize),
    All(Vec<LifeTypeProjection>, Vec<Sequence>),
}
#[derive(Clone, PartialEq, Eq)]
enum Nominal {
    One(Vec<LifeTypeProjection>, LifeDeclarationId, u32, usize),
    From(Vec<LifeTypeProjection>, usize, Vec<Sequence>),
}
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Retained {
    domain: Vec<Sequence>,
    ports: Vec<Port>,
    retained: Vec<Retain>,
    nominal: Vec<Nominal>,
}
#[derive(Clone)]
pub(super) struct TaskType {
    pub(super) result: Term,
    pub(super) retained: Retained,
    // All leaves, including phantom and FromType leaves, are original terms.
    // Indices in Retained select this vector, never actual caller ordinals.
    leaves: Vec<Term>,
}
impl TaskType {
    pub(super) fn terms(&self) -> impl Iterator<Item = Term> + '_ {
        std::iter::once(self.result).chain(self.leaves.iter().copied())
    }
    pub(super) fn work(&self) -> usize {
        fn sequence(parts: &[Sequence]) -> usize {
            parts.iter().fold(0usize, |total, part| {
                total.saturating_add(match part {
                    Sequence::One(_) => 1,
                    Sequence::All(_, path) => path.len().saturating_add(1),
                })
            })
        }
        let mut count = self
            .leaves
            .len()
            .saturating_add(1)
            .saturating_add(sequence(&self.retained.domain));
        for port in &self.retained.ports {
            count = count.saturating_add(match port {
                Port::One(..) => 1,
                Port::From(_, classes) => sequence(classes).saturating_add(1),
            });
        }
        for part in &self.retained.retained {
            count = count.saturating_add(match part {
                Retain::One(path, ..) => path.len().saturating_add(1),
                Retain::All(path, classes) => path
                    .len()
                    .saturating_add(sequence(classes))
                    .saturating_add(1),
            });
        }
        for part in &self.retained.nominal {
            count = count.saturating_add(match part {
                Nominal::One(path, ..) => path.len().saturating_add(1),
                Nominal::From(path, _, classes) => path
                    .len()
                    .saturating_add(sequence(classes))
                    .saturating_add(1),
            });
        }
        count
    }
}

impl Solver<'_, '_> {
    fn task_leaf(
        &self,
        leaves: &mut Vec<Term>,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<usize> {
        let slot = leaves.len();
        Self::push(leaves, term, b)?;
        Ok(slot)
    }
    fn task_sequence(
        &mut self,
        frame: &Frame,
        source: &SemanticClassSequenceRecipe,
        leaves: &mut Vec<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Sequence>> {
        self.step(depth, b)?;
        let mut result = Vec::new();
        for part in &source.parts {
            let part = match part {
                SemanticClassSequencePart::One(life) => {
                    let term = self.life(frame, life, b, depth + 1)?;
                    Sequence::One(self.task_leaf(leaves, term, b)?)
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    let term = self.recipe(frame, ty, b, depth + 1)?;
                    b.spend(path.len(), WORK)?;
                    Sequence::All(self.task_leaf(leaves, term, b)?, path.clone())
                }
            };
            Self::push(&mut result, part, b)?;
        }
        Ok(result)
    }
    pub(super) fn task_type(
        &mut self,
        frame: &Frame,
        result: &SemanticTypeRecipe,
        shape: &SemanticRetainedShapeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let result = self.recipe(frame, result, b, depth + 1)?;
        let mut leaves = Vec::new();
        let domain = self.task_sequence(frame, &shape.domain, &mut leaves, b, depth + 1)?;
        let mut retained = Retained {
            domain,
            ..Retained::default()
        };
        for port in &shape.ports {
            let port = match port {
                SemanticPortPart::One { leaf, mode, class } => {
                    let leaf = self.recipe(frame, leaf, b, depth + 1)?;
                    let class = self.life(frame, class, b, depth + 1)?;
                    Port::One(
                        self.task_leaf(&mut leaves, leaf, b)?,
                        *mode,
                        self.task_leaf(&mut leaves, class, b)?,
                    )
                }
                SemanticPortPart::FromType { ty, classes } => {
                    let ty = self.recipe(frame, ty, b, depth + 1)?;
                    let classes = self.task_sequence(frame, classes, &mut leaves, b, depth + 1)?;
                    Port::From(self.task_leaf(&mut leaves, ty, b)?, classes)
                }
            };
            Self::push(&mut retained.ports, port, b)?;
        }
        for part in &shape.retained {
            let part = match part {
                SemanticRetainedPart::One {
                    path,
                    child_class,
                    parent,
                } => {
                    b.spend(path.len(), WORK)?;
                    let parent = self.life(frame, parent, b, depth + 1)?;
                    Retain::One(
                        path.clone(),
                        *child_class,
                        self.task_leaf(&mut leaves, parent, b)?,
                    )
                }
                SemanticRetainedPart::All { path, parents } => {
                    b.spend(path.len(), WORK)?;
                    Retain::All(
                        path.clone(),
                        self.task_sequence(frame, parents, &mut leaves, b, depth + 1)?,
                    )
                }
            };
            Self::push(&mut retained.retained, part, b)?;
        }
        for part in &shape.nominal {
            let part = match part {
                SemanticNominalPart::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    b.spend(path.len(), WORK)?;
                    let class = self.life(frame, class, b, depth + 1)?;
                    Nominal::One(
                        path.clone(),
                        *declaration,
                        *argument,
                        self.task_leaf(&mut leaves, class, b)?,
                    )
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    b.spend(path.len(), WORK)?;
                    let ty = self.recipe(frame, ty, b, depth + 1)?;
                    let classes = self.task_sequence(frame, classes, &mut leaves, b, depth + 1)?;
                    Nominal::From(path.clone(), self.task_leaf(&mut leaves, ty, b)?, classes)
                }
            };
            Self::push(&mut retained.nominal, part, b)?;
        }
        self.term(
            Kind::Task(TaskType {
                result,
                retained,
                leaves,
            }),
            b,
        )
    }
    pub(super) fn task_unify(
        &mut self,
        left: &TaskType,
        right: &TaskType,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        b.spend(left.work().saturating_add(right.work()), WORK)?;
        if left.retained != right.retained || left.leaves.len() != right.leaves.len() {
            return Err(fail(
                "original Task equation changes its complete retained domain",
            ));
        }
        for (left, right) in left.terms().zip(right.terms()) {
            self.unify(left, right, depth + 1, b)?;
        }
        Ok(())
    }
    pub(super) fn task_instantiate(
        &mut self,
        mut task: TaskType,
        owner: SemanticDefinitionId,
        actuals: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        task.result = self.instantiate(task.result, owner, actuals, b, depth + 1)?;
        for leaf in &mut task.leaves {
            *leaf = self.instantiate(*leaf, owner, actuals, b, depth + 1)?;
        }
        self.term(Kind::Task(task), b)
    }
    pub(super) fn task_open_invocations(
        &mut self,
        mut task: TaskType,
        owner: SemanticDefinitionId,
        actuals: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        task.result = self.open_invocations(task.result, owner, actuals, b, depth + 1)?;
        for leaf in &mut task.leaves {
            *leaf = self.open_invocations(*leaf, owner, actuals, b, depth + 1)?;
        }
        self.term(Kind::Task(task), b)
    }

    fn task_builtin(
        &mut self,
        identity: LifeDeclarationId,
        actuals: Vec<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let mut selected = None;
        for candidate in self.inventory.definitions.values() {
            b.spend(1, WORK)?;
            if matches!(candidate.body, SemanticDefinitionBody::Nominal { declaration, kind: LifeNominalKind::Enum, .. } if declaration == identity)
            {
                if selected.replace(candidate.id).is_some() {
                    return Err(fail(
                        "original Task nominal role has competing declarations",
                    ));
                }
            }
        }
        self.application(
            selected.ok_or_else(|| {
                fail("original Task operation lacks its canonical nominal declaration")
            })?,
            actuals,
            b,
            depth + 1,
        )
    }

    fn task_worker(
        &mut self,
        frame: &Frame,
        expression: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(
        SemanticDefinitionId,
        Vec<Term>,
        Term,
        Vec<PassingMode>,
        PassingMode,
        CallKind,
    )> {
        self.step(depth, b)?;
        let mut head = expression;
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
                for ty in type_arguments {
                    crate::dyn_metadata::recipe(ty, b)?;
                    Self::push(&mut type_written, CoreGenericArgument::Type(ty.clone()), b)?;
                }
                (callee.as_ref(), type_written.as_slice())
            }
            _ => (head, &[][..]),
        };
        let path = sealed::path(head, b, depth + 1)?
            .ok_or_else(|| fail("Task creation requires an actual static worker declaration"))?;
        let mut locals = BTreeSet::new();
        for local in &frame.locals {
            b.spend(local.name.len().saturating_add(locals.len() + 1), WORK)?;
            locals.insert(local.name.clone());
        }
        let sealed::Resolution::Definition(target) =
            sealed::resolve(self.inventory, frame.definition, &path, &locals, b)?
        else {
            return Err(fail(
                "Task worker is shadowed or has no original declaration",
            ));
        };
        let declaration = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            foreign,
            body,
            resume_bound,
            ..
        } = &declaration.body
        else {
            return Err(fail(
                "Task worker omits its original full signature and body",
            ));
        };
        crate::dyn_metadata::recipe(signature, b)?;
        if foreign.is_some()
            || !matches!(body, SemanticOriginalFunctionBody::Defined(_))
            || declaration.kind != SemanticDefinitionKind::Function
        {
            return Err(fail(
                "Task worker is not an original defined static function",
            ));
        }
        b.spend(signature.parameters.len(), WORK)?;
        let modes: Vec<_> = signature.parameters.iter().map(|p| p.mode).collect();
        let result_mode = signature.result_mode;
        let call_kind = signature.call_kind;
        // Read the original resume bound; actual frame/thread proof is an
        // independent obligation of the Task kernel, never a type equation.
        match resume_bound {
            TaskResumeBound::Local | TaskResumeBound::Transfer => {}
        }
        let (_, parameters, result) = self.scheme(frame, target, written, b, depth + 1)?;
        Ok((target, parameters, result, modes, result_mode, call_kind))
    }

    // Collect explicit original input occurrences. The actual kernel checks
    // which physical inputs/keepers realize these occurrences at every PC.
    fn task_input_shape(
        &self,
        term: Term,
        task: &mut TaskType,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        match &self.state.arena[self.type_root(term, b)?] {
            Kind::Atom(_) | Kind::ConstValue(_) | Kind::Code { .. } => Ok(()),
            Kind::Address { mutable, life, .. } => {
                let leaf = self.task_leaf(&mut task.leaves, term, b)?;
                let class = self.task_leaf(&mut task.leaves, *life, b)?;
                Self::push(&mut task.retained.domain, Sequence::One(class), b)?;
                Self::push(
                    &mut task.retained.ports,
                    Port::One(
                        leaf,
                        if *mutable {
                            StoredBorrowMode::ExclusiveAddress
                        } else {
                            StoredBorrowMode::SharedAddress
                        },
                        class,
                    ),
                    b,
                )
            }
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                for field in fields {
                    self.task_input_shape(*field, task, b, depth + 1)?;
                }
                // Nominal phantom classes are retained separately, even when
                // they supply no input port or concrete borrowed target.
                if let Kind::Nominal { arguments, .. } =
                    &self.state.arena[self.type_root(term, b)?]
                {
                    for argument in arguments {
                        if matches!(
                            self.state.arena[self.root(*argument, b)?],
                            Kind::Life(_) | Kind::LifeVariable(_)
                        ) {
                            let class = self.task_leaf(&mut task.leaves, *argument, b)?;
                            Self::push(&mut task.retained.domain, Sequence::One(class), b)?;
                        }
                    }
                }
                Ok(())
            }
            Kind::Array { element, .. } | Kind::Wrapped(_, element) => {
                self.task_input_shape(*element, task, b, depth + 1)
            }
            Kind::ErrorUnion { error, result } => {
                self.task_input_shape(*error, task, b, depth + 1)?;
                self.task_input_shape(*result, task, b, depth + 1)
            }
            _ => Err(unknown(
                "Task creation needs the complete original input-port expansion for this type",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn task_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        intrinsic: sealed::Intrinsic,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if !written.is_empty() {
            return Err(fail(
                "Task intrinsic has no independent generic argument vector",
            ));
        }
        let mut inputs = Vec::new();
        let result = if intrinsic == sealed::Intrinsic::TaskCreate {
            let (worker, arguments) = arguments
                .split_first()
                .ok_or_else(|| fail("Task creation omits its static worker"))?;
            let (target, parameters, result, modes, result_mode, call_kind) =
                self.task_worker(frame, worker, b, depth + 1)?;
            if parameters.len() != arguments.len() || modes.len() != arguments.len() {
                return Err(fail(
                    "Task creation changes its worker's ordered input arity",
                ));
            }
            let mut task = TaskType {
                result,
                retained: Retained::default(),
                leaves: Vec::new(),
            };
            for ((argument, parameter), mode) in arguments.iter().zip(&parameters).zip(&modes) {
                let actual = self.expression(frame, argument, b, depth + 1)?;
                self.unify(actual, *parameter, depth + 1, b)?;
                if *mode == PassingMode::Copy {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: actual,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
                self.task_input_shape(*parameter, &mut task, b, depth + 1)?;
                Self::push(&mut inputs, *parameter, b)?;
            }
            // A borrowed result needs an exact retained Result mapping; no
            // result-pointee matching is allowed to invent its provenance.
            if let Kind::Address { life, .. } = self.state.arena[self.type_root(result, b)?] {
                let class = self.task_leaf(&mut task.leaves, life, b)?;
                Self::push(
                    &mut task.retained.retained,
                    Retain::One(vec![LifeTypeProjection::Result], 0, class),
                    b,
                )?;
            }
            b.lookup(self.state.native_targets.len(), WORK)?;
            if self
                .state
                .native_targets
                .insert((frame.definition, node), vec![target])
                .is_some()
            {
                return Err(fail(
                    "Task creation has competing original worker identities",
                ));
            }
            // This is the worker's complete reference type, not a Direct call.
            let reference = self.term(
                Kind::Code {
                    kind: SemanticCodeKind::FunctionPointer,
                    parameters,
                    result,
                    modes,
                    result_mode,
                    call_kind,
                },
                b,
            )?;
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(worker.id, reference).is_some() {
                return Err(fail("Task worker source node is reused"));
            }
            let task = self.term(Kind::Task(task), b)?;
            let owner = self.term(Kind::Wrapped(Wrapper::Managed, task), b)?;
            let error = self.task_builtin(
                zil_ir::builtin_life::TASK_CREATE_ERROR,
                Vec::new(),
                b,
                depth + 1,
            )?;
            self.term(
                Kind::ErrorUnion {
                    error,
                    result: owner,
                },
                b,
            )?
        } else {
            if arguments.len() != 1 {
                return Err(fail(
                    "Task operation requires exactly its actual owning head",
                ));
            }
            let input = self.expression(frame, &arguments[0], b, depth + 1)?;
            let owner = if intrinsic == sealed::Intrinsic::TaskFinish {
                input
            } else {
                let Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } = self.state.arena[self.type_root(input, b)?]
                else {
                    return Err(fail(
                        "Task activation requires an exclusive address of its owning head",
                    ));
                };
                inner
            };
            let Kind::Wrapped(Wrapper::Managed, payload) =
                self.state.arena[self.type_root(owner, b)?]
            else {
                return Err(fail("Task operation requires the complete actual owner"));
            };
            let Kind::Task(task) = &self.state.arena[self.type_root(payload, b)?] else {
                return Err(fail("Task operation selects another resource family"));
            };
            let payload = task.result;
            Self::push(&mut inputs, input, b)?;
            let anchor = self.registration_anchor(frame, &arguments[0], b, depth + 1)?;
            b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
            if self
                .state
                .intrinsic_anchors
                .insert((frame.definition, node), anchor)
                .is_some()
            {
                return Err(fail("Task operation has competing original owner anchors"));
            }
            match intrinsic {
                sealed::Intrinsic::TaskPoll => {
                    let poll =
                        self.task_builtin(zil_ir::builtin_life::POLL, vec![payload], b, depth + 1)?;
                    let error = self.task_builtin(
                        zil_ir::builtin_life::TASK_POLL_STATE,
                        Vec::new(),
                        b,
                        depth + 1,
                    )?;
                    self.term(
                        Kind::ErrorUnion {
                            error,
                            result: poll,
                        },
                        b,
                    )?
                }
                sealed::Intrinsic::TaskPollClose => {
                    let unit = self.atom(Type::Unit, b)?;
                    self.task_builtin(zil_ir::builtin_life::POLL, vec![unit], b, depth + 1)?
                }
                sealed::Intrinsic::TaskCancel | sealed::Intrinsic::TaskFinish => {
                    self.atom(Type::Unit, b)?
                }
                _ => return Err(fail("non-Task intrinsic entered the Task scheme")),
            }
        };
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), intrinsic)
            .is_some()
        {
            return Err(fail("Task operation has competing original schemes"));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        if self
            .state
            .intrinsic_inputs
            .insert((frame.definition, node), inputs)
            .is_some()
        {
            return Err(fail("Task operation has competing ordered original inputs"));
        }
        Ok(result)
    }
}

struct Materialize<'a, 'i> {
    checked: &'a Checked,
    task: &'a TaskType,
    context: &'a SemanticInstance,
    program: &'a SemanticModule,
    inventory: &'a Inventory<'i>,
}
impl Materialize<'_, '_> {
    fn leaf(&self, slot: usize) -> CompileResult<Term> {
        self.task
            .leaves
            .get(slot)
            .copied()
            .ok_or_else(|| fail("original Task shape leaf is absent"))
    }
    fn value(&self, slot: usize, b: &mut Budget, depth: usize) -> CompileResult<Value> {
        self.checked.value(
            self.leaf(slot)?,
            self.context,
            self.program,
            self.inventory,
            b,
            depth + 1,
        )
    }
    fn life(&self, slot: usize, b: &mut Budget) -> CompileResult<Class> {
        self.checked
            .life_value(self.leaf(slot)?, self.context, self.inventory, b)
    }
    fn sequence(
        &self,
        sequence: &[Sequence],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Class>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original Task shape expansion exceeds its depth"));
        }
        let mut classes = Vec::new();
        for part in sequence {
            match part {
                Sequence::One(slot) => Solver::push(&mut classes, self.life(*slot, b)?, b)?,
                Sequence::All(slot, path) => {
                    let value = self.value(*slot, b, depth + 1)?;
                    if path.is_empty() {
                        // An actual class image may repeat: the original child
                        // domain is complete and is never deduplicated here.
                        for class in value.classes {
                            Solver::push(&mut classes, class, b)?;
                        }
                    } else {
                        b.spend(path.len().saturating_add(value.life.retained.len()), WORK)?;
                        let mut rows: Vec<_> = value
                            .life
                            .retained
                            .iter()
                            .filter(|row| row.path == *path)
                            .collect();
                        rows.sort_by_key(|row| row.class);
                        if rows.is_empty() {
                            return Err(unknown(
                                "projected Task class expansion needs its complete child-domain inventory",
                            ));
                        }
                        for (ordinal, row) in rows.iter().enumerate() {
                            if row.class as usize != ordinal {
                                return Err(fail(
                                    "Task child-domain expansion is sparse or repeats a class",
                                ));
                            }
                            let class =
                                value.classes.get(row.binder as usize).ok_or_else(|| {
                                    fail("Task child-domain binding leaves the full parent domain")
                                })?;
                            Solver::push(&mut classes, class.clone(), b)?;
                        }
                    }
                }
            }
        }
        Ok(classes)
    }
    fn binder(&self, classes: &[Class], class: &Class, b: &mut Budget) -> CompileResult<u32> {
        b.spend(classes.len(), WORK)?;
        let mut selected = None;
        for (ordinal, candidate) in classes.iter().enumerate() {
            if candidate == class && selected.replace(ordinal).is_some() {
                return Err(unknown(
                    "Task repeated actual lifetime images need their distinct original domain selectors",
                ));
            }
        }
        u32::try_from(
            selected.ok_or_else(|| {
                fail("Task occurrence is absent from its declared retained domain")
            })?,
        )
        .map_err(|_| fail("Task retained domain ordinal overflow"))
    }
}

fn physical_leaf<'a>(
    mut ty: &'a Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<&'a Type> {
    for projection in path {
        b.spend(1, WORK)?;
        while let Type::BorrowedStorage { storage, .. } = ty {
            b.spend(1, WORK)?;
            ty = storage;
        }
        ty = match (ty, projection) {
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(*index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            (Type::Enum { variants, .. }, PlaceProjection::Variant(index)) => variants
                .get(index.variant as usize)
                .and_then(|(_, fields)| fields.get(index.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(success)) => {
                Some(if *success { result } else { error })
            }
            _ => None,
        }
        .ok_or_else(|| fail("Task port expansion selects no actual physical child"))?;
    }
    Ok(ty)
}

impl Checked {
    pub(super) fn task_value(
        &self,
        task: &TaskType,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original Task value exceeds its complete depth"));
        }
        let materialize = Materialize {
            checked: self,
            task,
            context,
            program,
            inventory,
        };
        let classes = materialize.sequence(&task.retained.domain, b, depth + 1)?;
        let binder_count =
            u32::try_from(classes.len()).map_err(|_| fail("Task retained domain overflow"))?;
        let result = self.value(task.result, context, program, inventory, b, depth + 1)?;
        let mut shape = RetainedLifeShape {
            binder_count,
            ports: Vec::new(),
            retained: Vec::new(),
            nominal_arguments: Vec::new(),
        };
        for port in &task.retained.ports {
            match port {
                Port::One(leaf, mode, class) => {
                    let leaf = materialize.value(*leaf, b, depth + 1)?;
                    let class = materialize.life(*class, b)?;
                    let binder = materialize.binder(&classes, &class, b)?;
                    Solver::push(
                        &mut shape.ports,
                        RetainedBorrowPort {
                            leaf: leaf.ty,
                            mode: *mode,
                            binder,
                        },
                        b,
                    )?;
                }
                Port::From(ty, sequence) => {
                    let value = materialize.value(*ty, b, depth + 1)?;
                    let images = materialize.sequence(sequence, b, depth + 1)?;
                    if images.len() != value.classes.len() {
                        return Err(fail(
                            "Task port expansion changes its complete child domain",
                        ));
                    }
                    for slot in &value.life.slots {
                        let leaf = physical_leaf(&value.ty, &slot.path, b)?;
                        crate::dyn_metadata::ty(leaf, b)?;
                        let class = images
                            .get(slot.binder as usize)
                            .ok_or_else(|| fail("Task port's child class is absent"))?;
                        let binder = materialize.binder(&classes, class, b)?;
                        Solver::push(
                            &mut shape.ports,
                            RetainedBorrowPort {
                                leaf: leaf.clone(),
                                mode: slot.mode,
                                binder,
                            },
                            b,
                        )?;
                    }
                }
            }
        }
        for part in &task.retained.retained {
            let (path, images) = match part {
                Retain::One(path, child, parent) => {
                    let image = materialize.life(*parent, b)?;
                    let binder = materialize.binder(&classes, &image, b)?;
                    b.spend(path.len(), WORK)?;
                    Solver::push(
                        &mut shape.retained,
                        StorageRetainedBinding {
                            path: path.clone(),
                            class: *child,
                            binder,
                        },
                        b,
                    )?;
                    continue;
                }
                Retain::All(path, parents) => (path, materialize.sequence(parents, b, depth + 1)?),
            };
            for (child, image) in images.iter().enumerate() {
                let binder = materialize.binder(&classes, image, b)?;
                let class = u32::try_from(child)
                    .map_err(|_| fail("Task retained child ordinal overflow"))?;
                b.spend(path.len(), WORK)?;
                Solver::push(
                    &mut shape.retained,
                    StorageRetainedBinding {
                        path: path.clone(),
                        class,
                        binder,
                    },
                    b,
                )?;
            }
        }
        for part in &task.retained.nominal {
            match part {
                Nominal::One(path, declaration, argument, slot) => {
                    let image = materialize.life(*slot, b)?;
                    let binder = materialize.binder(&classes, &image, b)?;
                    b.spend(path.len(), WORK)?;
                    Solver::push(
                        &mut shape.nominal_arguments,
                        NominalLifeArgument {
                            path: path.clone(),
                            declaration: *declaration,
                            argument: *argument,
                            binder,
                        },
                        b,
                    )?;
                }
                Nominal::From(path, ty, sequence) => {
                    let value = materialize.value(*ty, b, depth + 1)?;
                    let images = materialize.sequence(sequence, b, depth + 1)?;
                    if images.len() != value.classes.len() {
                        return Err(fail("Task nominal expansion changes the full child domain"));
                    }
                    for row in value.life.nominal_arguments {
                        let image = images
                            .get(row.binder as usize)
                            .ok_or_else(|| fail("Task nominal child class is absent"))?;
                        let binder = materialize.binder(&classes, image, b)?;
                        b.spend(path.len().saturating_add(row.path.len()), WORK)?;
                        let mut full_path = path.clone();
                        full_path.extend(row.path);
                        Solver::push(
                            &mut shape.nominal_arguments,
                            NominalLifeArgument {
                                path: full_path,
                                declaration: row.declaration,
                                argument: row.argument,
                                binder,
                            },
                            b,
                        )?;
                    }
                }
            }
        }
        // Every result class is represented, including classes carried only by
        // a nominal argument. A result Type alone cannot erase its context.
        for (class, image) in result.classes.iter().enumerate() {
            let mut found = None;
            for row in &shape.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.path.as_slice() == [LifeTypeProjection::Result]
                    && row.class as usize == class
                {
                    if found.replace(row.binder).is_some() {
                        return Err(fail("Task repeats a retained result-class mapping"));
                    }
                }
            }
            let binder = found.ok_or_else(|| fail("Task omits a complete result-class mapping"))?;
            if classes.get(binder as usize) != Some(image) {
                return Err(fail(
                    "Task result context differs from its retained relation",
                ));
            }
        }
        crate::dyn_metadata::retained_shape(&shape, b)?;
        let ty = Type::Task {
            result: Box::new(result.ty),
            retained_life: shape,
        };
        Ok(Value {
            ty,
            classes,
            life: StorageLifeShape {
                binder_count,
                slots: Vec::new(),
                retained: Vec::new(),
                nominal_arguments: Vec::new(),
            },
        })
    }
}
