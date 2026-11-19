//! Private receipts from actual Builder operations. These records do not prove
//! a recipe, lifetime, capability or a public semantic claim. In particular a
//! Core node is never selected by Span, a name, or equality of result types.
use crate::*;
use zil_base::{ModuleId, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Origin {
    pub module: ModuleId,
    /// Real outer CoreFunction, also for a nested closure body. Synthetic
    /// closure entry functions have no independent CoreFunction identity.
    pub creator: NodeId,
    pub closure: Option<NodeId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Point {
    pub block: BlockId,
    pub instruction: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Construction {
    pub value: ValueId,
    pub at: Point,
}
#[derive(Default)]
pub(crate) struct Expression {
    pub value: Option<ValueId>,
    pub place: Option<SemanticOperand>,
    pub callee: Option<NodeId>,
    /// The exact address prepared for this call's whole callee place, before
    /// arguments. This is not a logical Closure TypeUse or a type premise.
    pub callee_place: Option<ValueId>,
    /// The actual owner place selected by this dynamic call, before borrowing.
    pub dyn_receiver: Option<NodeId>,
    pub end: Option<Point>,
    pub instructions: Vec<Point>,
    /// The real literal producer result, before a later contextual observation
    /// or error injection. This is independent of the final expression value.
    pub construction: Option<Construction>,
    complete: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct ClosureSlot {
    pub initializer: NodeId,
    pub input: ValueId,
    pub array: ValueId,
    pub construction: Point,
    pub initialization: Point,
}
/// A source local's logical type, selected from its exact emitted storage
/// representation. Only Ledger can construct this view; a physical Array type
/// by itself can never be used as a logical Closure hint.
pub(crate) struct LogicalLocalType<'f> {
    ty: &'f Type,
    slot: Option<ClosureSlot>,
}
impl<'f> LogicalLocalType<'f> {
    pub(crate) fn as_type(&self) -> &'f Type { self.ty }
    pub(crate) fn is_persistent_closure(&self) -> bool { self.slot.is_some() }
}
pub(crate) struct Declaration {
    pub local: LocalId,
    pub at: Point,
}
pub(crate) struct Frame {
    pub origin: Origin,
    pub expressions: BTreeMap<NodeId, Expression>,
    pub declarations: BTreeMap<NodeId, Declaration>,
    pub closure_slots: BTreeMap<LocalId, ClosureSlot>,
    /// Lexical creator and the runtime environment, when materialized. Thin
    /// FunctionRefs retain the creator after their temporary closure slot is released.
    pub parent: Option<(FunctionId, Option<ClosureId>)>,
    stack: Vec<NodeId>,
    live_locals: BTreeMap<LocalId, Point>,
}
#[path = "emission_callable_modes.rs"]
mod callable_modes;
pub(crate) use callable_modes::CallableBodyModes;

#[derive(Default)]
pub(crate) struct Ledger {
    pub(crate) ordinary_stats: std::rc::Rc<std::cell::RefCell<stored::OrdinaryStats>>,
    pub(super) fixed_abis: Option<super::abi::Choices>,
    pub(super) abi_origins: Vec<super::abi::SourceOrigin>,
    /// Only the explicit frozen-context entry may install this independently
    /// checked immutable source interface. It contributes no emitted frames.
    pub(super) frozen: Option<std::rc::Rc<super::frozen_context::FrozenSource>>,
    pub(super) original: super::original_context::Slot,
    pub frames: BTreeMap<FunctionId, Frame>,
    // Each specialization has its own actual target. An origin alone can
    // select a target only when its complete set has one member.
    pub(super) targets: BTreeMap<Origin, Vec<FunctionId>>,
    closure_entries: BTreeMap<ClosureId, FunctionId>,
    /// Pure original use facts, bound to this request's immutable context.
    original_callable_modes: std::cell::RefCell<Option<std::rc::Rc<callable_modes::Directory>>>,
    /// Full nominal proposals attached only to genuine TaskPoll results.
    pub(crate) formed_polls: BTreeMap<(FunctionId, ValueId), LifeNominalInstance>,
    /// Bound only by relocation of the exact source request and emitted call.
    pub(super) instance_requests: std::cell::RefCell<
        BTreeMap<SemanticInstanceId, (SemanticInstanceId, SemanticSiteId, FunctionId)>,
    >,
    /// Lexical selections recorded while emitting this exact MakeClosure.
    pub(super) closure_captures: BTreeMap<ClosureId, Vec<(String, ValueId)>>,
    /// Candidate headers formed at real MakeClosure operations. Publication
    /// rebuilds and compares them; they confer no closedness or permissions.
    pub(super) formed_headers: BTreeMap<
        ClosureId,
        (
            StorageLifeContract,
            Vec<SemanticHeaderBinding>,
            Vec<(StorageBase, SemanticTypeValue)>,
        ),
    >,
    /// Actual Core headers deliberately retained for a later callable/value
    /// specialization. A missing frame by itself is never this fact.
    pub templates: std::collections::BTreeSet<Origin>,
}
fn invalid(span: Span, message: &str) -> Diagnostic {
    error("E-SEMANTIC-EMISSION", span, message)
}
fn pay(left: &mut usize, n: usize, span: Span) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic emission work exceeded", Some(span))
    })?;
    Ok(())
}
fn lookup_cost(n: usize) -> usize {
    n.saturating_add(1).ilog2() as usize + 1
}
impl Ledger {
    /// A finished inner expression can be used while its enclosing Task call
    /// is still being prepared. The active outer expression is never evidence
    /// for its not-yet-emitted result.
    pub(crate) fn completed_expression(
        &self,
        function: FunctionId,
        expression: NodeId,
        left: &mut usize,
        span: Span,
    ) -> R<&Expression> {
        pay(left, lookup_cost(self.frames.len()), span)?;
        let frame = self.frames.get(&function)
            .ok_or_else(|| invalid(span, "completed result has no actual emission frame"))?;
        pay(left, lookup_cost(frame.expressions.len()), span)?;
        let row = frame.expressions.get(&expression)
            .ok_or_else(|| invalid(span, "completed result has no actual expression receipt"))?;
        if !row.complete || row.end.is_none() || row.value.is_none() {
            return Err(invalid(span, "result context precedes completion of its real expression"));
        }
        Ok(row)
    }

    pub(crate) fn has_original_context(&self) -> bool {
        self.original.context().is_some()
    }

    pub(crate) fn shared_parameter_at_prepared_call(
        &self,
        function: FunctionId,
        invocation: NodeId,
        left: &mut usize,
        span: Span,
    ) -> R<bool> {
        let Some(context) = self.original.context() else { return Ok(false) };
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get(&function) else { return Ok(false) };
        let program = &context.semantic;
        let mut callers = BTreeMap::new();
        pay(left, program.instances.len(), span)?;
        for row in &program.instances {
            let actual = match row.target {
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =>
                    Origin { module, creator: function, closure: None },
                SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                    module, creator, expression, occurrence: 0,
                }) => Origin { module, creator, closure: Some(expression) },
                _ => continue,
            };
            if actual == frame.origin {
                pay(left, lookup_cost(callers.len()).saturating_add(
                    std::mem::size_of::<(SemanticInstanceId, SemanticDefinitionId)>(),
                ), span)?;
                callers.insert(row.id, row.definition);
            }
        }
        if callers.is_empty() {
            return Ok(false);
        }
        let mut resolved = None;
        pay(left, program.sites.len(), span)?;
        for site in &program.sites {
            if site.at != (SemanticSiteAddress::Core {
                expression: invocation, occurrence: 0, role: SemanticEmissionRole::Call,
            }) {
                continue;
            }
            pay(left, lookup_cost(callers.len()), span)?;
            let Some(caller) = callers.get(&site.instance).copied() else { continue };
            pay(left, lookup_cost(context.original.complete.len())
                .saturating_add(lookup_cost(context.original.call_targets.len())), span)?;
            if !context.original.complete.contains(&caller) {
                return Ok(false);
            }
            let Some(target) = context.original.call_targets.get(&(caller, site.recipe)).copied()
            else { return Ok(false) };
            let definition = context.definition(target, left)?;
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
            else { return Ok(false) };
            let [parameter] = signature.parameters.as_slice() else { return Ok(false) };
            if parameter.mode != PassingMode::Borrow
                || !matches!(parameter.ty, SemanticTypeRecipe::Address { mutable: false, .. })
                || resolved.is_some_and(|previous| previous != (caller, site.recipe, target))
            {
                return Ok(false);
            }
            resolved = Some((caller, site.recipe, target));
        }
        Ok(resolved.is_some())
    }

    pub(crate) fn record_formed_header(
        &mut self,
        closure: ClosureId,
        header: StorageLifeContract,
        embedding: Vec<SemanticHeaderBinding>,
        values: Vec<(StorageBase, SemanticTypeValue)>,
    ) -> R<()> {
        if let Some(previous) = self.formed_headers.get(&closure) {
            if previous != &(header, embedding, values) {
                return Err(invalid(
                    Span::default(),
                    "actual Task preparation changes an earlier complete closure header",
                ));
            }
        } else {
            self.formed_headers
                .insert(closure, (header, embedding, values));
        }
        Ok(())
    }

    pub(crate) fn validate_formed_capture(
        &self,
        closure: ClosureId,
        parameters: &[Type],
        captures: &[Type],
        result: &Type,
        header: &StorageLifeContract,
        limits: &Limits,
        left: &mut usize,
    ) -> R<bool> {
        pay(
            left,
            lookup_cost(self.formed_headers.len()),
            Span::default(),
        )?;
        let Some((proposed, embedding, values)) = self.formed_headers.get(&closure) else {
            return Ok(false);
        };
        crate::borrowed_fp::charge_contract(proposed, |work| pay(left, work, Span::default()), 1)?;
        if proposed != header {
            return Err(invalid(
                Span::default(),
                "actual closure changed its early formed header",
            ));
        }
        let (program, owner) = self.original_closure_definition(closure, left)?
            .ok_or_else(|| invalid(Span::default(), "formed capture has no immutable original closure origin"))?;
        super::headers::validate_formed_header(
            program, self, owner, parameters, captures, result, header, embedding, values, limits, left,
        )?;
        Ok(true)
    }
    fn closure_frame(&self, closure: ClosureId, left: &mut usize, span: Span) -> R<Option<&Frame>> {
        pay(left, lookup_cost(self.closure_entries.len()), span)?;
        let Some(entry) = self.closure_entries.get(&closure) else { return Ok(None) };
        pay(left, lookup_cost(self.frames.len()), span)?;
        self.frames.get(entry).filter(|frame| frame.parent.is_some_and(|(_, actual)| actual == Some(closure)))
            .map(Some).ok_or_else(|| invalid(span, "closure does not match its actual emitted entry"))
    }

    fn original_closure_definition(
        &self, closure: ClosureId, left: &mut usize,
    ) -> R<Option<(&SemanticModule, SemanticDefinitionId)>> {
        let program = self.original.context().map(|context| &context.semantic)
            .or_else(|| self.frozen.as_deref().map(|frozen| &frozen.semantic))
            ;
        let Some(program) = program else { return Ok(None); };
        let Some(frame) = self.closure_frame(closure, left, Span::default())? else { return Ok(None) };
        let origin = frame.origin;
        pay(left, program.instances.len(), Span::default())?;
        let mut owner = None;
        for instance in &program.instances {
            if matches!(&instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                module, creator, expression, occurrence: 0,
            }) if *module == origin.module && *creator == origin.creator && Some(*expression) == origin.closure) {
                if owner.is_some_and(|owner| owner != instance.definition) {
                    return Err(invalid(Span::default(), "formed capture source origin has several original definitions"));
                }
                owner = Some(instance.definition);
            }
        }
        Ok(owner.map(|owner| (program, owner)))
    }

    /// Formation of an already emitted environment remains traceable when it
    /// has no early source_context proposal. This authenticates its actual
    /// source/creator occurrence only; finish still checks MakeClosure, every
    /// operand, full header/domain and actual caller permissions.
    pub(crate) fn require_original_closure_transport(
        &self, closure: ClosureId, left: &mut usize, span: Span,
    ) -> R<()> {
        let (program, owner) = self.original_closure_definition(closure, left)?
            .ok_or_else(|| invalid(span, "Package callback has no genuine original closure occurrence"))?;
        pay(left, program.definitions.len(), span)?;
        let original = program.definitions.iter().find(|row| row.id == owner)
            .ok_or_else(|| invalid(span, "Package callback loses its original definition"))?;
        if original.kind != SemanticDefinitionKind::Closure {
            return Err(invalid(span, "Package callback origin is not its original Closure definition"));
        }
        let entry = self.closure_frame(closure, left, span)?
            .ok_or_else(|| invalid(span, "Package callback has no actual closure frame"))?;
        let (creator, _) = entry.parent.expect("filtered actual closure frame");
        pay(left, lookup_cost(self.frames.len()), span)?;
        let parent = self.frames.get(&creator)
            .ok_or_else(|| invalid(span, "Package callback has no real creator frame"))?;
        let expression = entry.origin.closure
            .ok_or_else(|| invalid(span, "Package callback origin has no real source expression"))?;
        if entry.origin.module != parent.origin.module || entry.origin.creator != parent.origin.creator {
            return Err(invalid(span, "Package callback changes its actual lexical source owner"));
        }
        pay(left, program.instances.len(), span)?;
        let has_parent = program.instances.iter().any(|row| Some(row.definition) == original.parent
            && match &row.target {
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =>
                    parent.origin.closure.is_none() && *module == parent.origin.module && *function == parent.origin.creator,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure { module, creator, expression, occurrence: 0 }) =>
                    *module == parent.origin.module && *creator == parent.origin.creator && Some(*expression) == parent.origin.closure,
                _ => false,
            });
        if !has_parent { return Err(invalid(span, "Package callback changes its original true creator")); }
        pay(left, lookup_cost(parent.expressions.len()), span)?;
        let receipt = parent.expressions.get(&expression)
            .ok_or_else(|| invalid(span, "Package callback lacks its actual creator expression receipt"))?;
        if !receipt.complete || receipt.value.is_none() || receipt.end.is_none() || receipt.instructions.is_empty() {
            return Err(invalid(span, "Package callback precedes its genuine completed source occurrence"));
        }
        Ok(())
    }

    /// Only an immutable original capability theorem for this exact actual
    /// closure. It grants neither synchronous cleanup nor a registration ABI.
    pub(crate) fn original_closed_environment(
        &self, closure: ClosureId, limits: &Limits, left: &mut usize,
    ) -> R<Option<bool>> {
        let Some((program, owner)) = self.original_closure_definition(closure, left)? else { return Ok(None); };
        let proof = self.original.borrow();
        if let Some(proof) = proof.as_ref() {
            return proof.closed_closure_environment(program, owner, limits, left);
        }
        if let Some(frozen) = self.frozen.as_deref() {
            return frozen.original.closed_closure_environment(program, owner, limits, left);
        }
        Ok(None)
    }

    pub(crate) fn active_expression(
        &self,
        function: FunctionId,
        left: &mut usize,
        span: Span,
    ) -> R<Option<NodeId>> {
        pay(left, lookup_cost(self.frames.len()), span)?;
        Ok(self
            .frames
            .get(&function)
            .and_then(|frame| frame.stack.last().copied()))
    }
    pub(crate) fn capture_sources(
        &mut self,
        closure: ClosureId,
        captures: Vec<(String, ValueId)>,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(
            left,
            lookup_cost(self.closure_captures.len()).saturating_add(captures.len()),
            span,
        )?;
        for (name, _) in &captures {
            pay(left, name.len(), span)?;
        }
        if self.closure_captures.insert(closure, captures).is_some() {
            return Err(invalid(
                span,
                "one actual closure repeats its capture selections",
            ));
        }
        Ok(())
    }
    pub(crate) fn dynamic_receiver(
        &mut self,
        function: FunctionId,
        receiver: NodeId,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        let call = *frame
            .stack
            .last()
            .ok_or_else(|| invalid(span, "dynamic owner selection has no active real call"))?;
        pay(
            left,
            lookup_cost(frame.expressions.len()).saturating_mul(2),
            span,
        )?;
        let owner = frame
            .expressions
            .get(&receiver)
            .ok_or_else(|| invalid(span, "dynamic owner selection has no actual place receipt"))?;
        if owner.place.is_none() || owner.end.is_none() || !owner.complete {
            return Err(invalid(
                span,
                "dynamic owner selection is not its completed actual place occurrence",
            ));
        }
        let row = frame
            .expressions
            .get_mut(&call)
            .ok_or_else(|| invalid(span, "dynamic owner selection lost its active call receipt"))?;
        if row
            .dyn_receiver
            .replace(receiver)
            .is_some_and(|old| old != receiver)
        {
            return Err(invalid(
                span,
                "one dynamic call changes its actual owner place selection",
            ));
        }
        Ok(())
    }
    pub fn register_template(&mut self, origin: Origin, left: &mut usize, span: Span) -> R<()> {
        pay(
            left,
            lookup_cost(self.templates.len()).saturating_add(1),
            span,
        )?;
        if !self.templates.insert(origin) {
            return Err(invalid(span, "one Core template was registered twice"));
        }
        Ok(())
    }
    pub(crate) fn install_frozen(
        &mut self,
        source: std::rc::Rc<super::frozen_context::FrozenSource>,
    ) -> R<()> {
        if self.frozen.is_some() || !self.frames.is_empty() {
            return Err(invalid(
                Span::default(),
                "frozen source context changed after actual emission began",
            ));
        }
        self.frozen = Some(source);
        Ok(())
    }
    pub fn bind(
        &mut self,
        function: FunctionId,
        origin: Origin,
        parent: Option<(FunctionId, ClosureId)>,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        pay(left, lookup_cost(self.frames.len()).saturating_add(1), span)?;
        if self.frames.contains_key(&function) {
            return Err(invalid(
                span,
                "actual Typed function has two semantic emission owners",
            ));
        }
        pay(left, lookup_cost(self.targets.len()), span)?;
        if let Some((_, closure)) = parent {
            pay(left, lookup_cost(self.closure_entries.len()), span)?;
            if self.closure_entries.contains_key(&closure) {
                return Err(invalid(span, "one emitted closure has several actual entry functions"));
            }
        }
        let targets = self.targets.entry(origin).or_default();
        pay(left, 1 + if targets.len() == targets.capacity() { targets.len() } else { 0 }, span)?;
        targets.try_reserve(1).map_err(|_| invalid(span, "cannot grow the emitted target directory"))?;
        targets.push(function);
        if let Some((_, closure)) = parent {
            self.closure_entries.insert(closure, function);
        }
        self.frames.insert(
            function,
            Frame {
                origin,
                parent: parent.map(|(creator, environment)| (creator, Some(environment))),
                expressions: BTreeMap::new(),
                declarations: BTreeMap::new(),
                closure_slots: BTreeMap::new(),
                stack: Vec::new(),
                live_locals: BTreeMap::new(),
            },
        );
        Ok(())
    }
    pub(crate) fn convert_to_function_ref(
        &mut self, entry: FunctionId, closure: ClosureId, left: &mut usize, span: Span,
    ) -> R<()> {
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&entry) else { return Ok(()) };
        let Some((creator, Some(actual))) = frame.parent else {
            return Err(invalid(span, "thin conversion has no emitted closure environment"));
        };
        pay(left, lookup_cost(self.closure_entries.len()), span)?;
        if actual != closure || self.closure_entries.get(&closure) != Some(&entry) {
            return Err(invalid(span, "thin conversion changes the emitted closure entry"));
        }
        self.closure_entries.remove(&closure);
        frame.parent = Some((creator, None));
        Ok(())
    }
    pub fn bind_closure(
        &mut self,
        creator: FunctionId,
        expression: NodeId,
        closure: ClosureId,
        entry: FunctionId,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(parent) = self.frames.get(&creator) else {
            return Ok(());
        };
        let origin = Origin {
            module: parent.origin.module,
            creator: parent.origin.creator,
            closure: Some(expression),
        };
        self.bind(entry, origin, Some((creator, closure)), left, span)
    }
    /// A qualified capture may await complete emitted-header formation only
    /// at its genuine source operation. A factory bound or an equal Type is
    /// not evidence that this actual entry belongs to this creator/site.
    pub(crate) fn require_closure_creator(
        &self,
        creator: FunctionRef<'_>,
        expression: NodeId,
        closure: ClosureId,
        entry: FunctionId,
        at: Point,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        pay(left, lookup_cost(self.frames.len()).saturating_mul(2), span)?;
        let parent = self
            .frames
            .get(&creator.id)
            .ok_or_else(|| invalid(span, "qualified capture has no actual creator frame"))?;
        let child = self
            .frames
            .get(&entry)
            .ok_or_else(|| invalid(span, "qualified capture has no actual closure frame"))?;
        if creator.foreign.is_some()
            || child.parent != Some((creator.id, Some(closure)))
            || child.origin.module != parent.origin.module
            || child.origin.creator != parent.origin.creator
            || child.origin.closure != Some(expression)
            || parent.stack.last() != Some(&expression)
        {
            return Err(invalid(
                span,
                "qualified capture changes its actual creator/site",
            ));
        }
        pay(
            left,
            lookup_cost(parent.expressions.len()).saturating_add(1),
            span,
        )?;
        let receipt = parent
            .expressions
            .get(&expression)
            .ok_or_else(|| invalid(span, "qualified capture has no active source expression"))?;
        let body = creator
            .blocks
            .get(at.block.0 as usize)
            .ok_or_else(|| invalid(span, "qualified capture leaves its actual creator CFG"))?;
        if receipt.complete
            || receipt.end.is_some()
            || body.id != at.block
            || at.instruction != body.instructions.len()
        {
            return Err(invalid(
                span,
                "qualified capture has no current preparation boundary",
            ));
        }
        if self.original_closure_definition(closure, left)?.is_none() {
            return Err(invalid(
                span,
                "qualified capture has no immutable original closure site",
            ));
        }
        Ok(())
    }
    pub fn begin(
        &mut self,
        function: FunctionId,
        expression: NodeId,
        callee: Option<NodeId>,
        limits: &Limits,
        left: &mut usize,
        span: Span,
    ) -> R<bool> {
        if self.frames.is_empty() {
            return Ok(false);
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(false);
        };
        // A wrapper re-entering the same real expression is one occurrence,
        // not another Source mint or another parallel traversal ordinal.
        if frame.stack.last() == Some(&expression) {
            return Ok(false);
        }
        if frame.stack.len() >= limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic emitter depth exceeded",
                Some(span),
            ));
        }
        pay(
            left,
            lookup_cost(frame.expressions.len()).saturating_add(2),
            span,
        )?;
        if frame.expressions.contains_key(&expression) {
            return Err(invalid(
                span,
                "one Core expression identity was emitted in two actual positions",
            ));
        }
        if frame.expressions.len() >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic emitted node count exceeded",
                Some(span),
            ));
        }
        frame.expressions.insert(
            expression,
            Expression {
                callee,
                ..Expression::default()
            },
        );
        frame.stack.push(expression);
        Ok(true)
    }
    pub fn end(
        &mut self,
        function: FunctionId,
        expression: NodeId,
        entered: bool,
        value: Option<ValueId>,
        boundary: Option<Point>,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if !entered {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let frame = self
            .frames
            .get_mut(&function)
            .ok_or_else(|| invalid(span, "missing emission frame"))?;
        if frame.stack.pop() != Some(expression) {
            return Err(invalid(
                span,
                "semantic emission context does not match the actual Core expression",
            ));
        }
        pay(left, lookup_cost(frame.expressions.len()), span)?;
        let row = frame
            .expressions
            .get_mut(&expression)
            .expect("begun expression");
        row.value = value;
        row.end = boundary;
        row.complete = true;
        Ok(())
    }
    pub fn constructed(
        &mut self,
        function: FunctionId,
        expression: NodeId,
        value: ValueId,
        at: Point,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        if frame.stack.last() != Some(&expression) {
            return Err(invalid(
                span,
                "constructor receipt is outside its real source expression",
            ));
        }
        pay(left, lookup_cost(frame.expressions.len()), span)?;
        let row = frame
            .expressions
            .get_mut(&expression)
            .ok_or_else(|| invalid(span, "constructor has no active expression receipt"))?;
        pay(left, row.instructions.len().saturating_add(1), span)?;
        if row.complete || !row.instructions.contains(&at) || row.construction.is_some() {
            return Err(invalid(
                span,
                "constructor receipt repeats or is not its actual emitted instruction",
            ));
        }
        row.construction = Some(Construction { value, at });
        Ok(())
    }
    /// A direct place lookup does not load or move its whole value merely to
    /// supply a type-use witness. Record the actual initialized location at
    /// the operation that selected it; no type/name lookup at lowering time.
    pub fn place(
        &mut self,
        function: FunctionId,
        expression: NodeId,
        place: SemanticOperand,
        boundary: Point,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        pay(
            left,
            lookup_cost(frame.expressions.len()).saturating_add(1),
            span,
        )?;
        if let Some(row) = frame.expressions.get_mut(&expression) {
            // A give/borrow can begin the real Name expression before it
            // selects its initialized place. Attach that lookup to this one
            // active operation; end() will publish its real SSA and boundary.
            // A completed receipt, prior lookup or already emitted operation
            // remains a distinct use and cannot be silently reused.
            pay(left, 1, span)?;
            if frame.stack.last() == Some(&expression)
                && !row.complete
                && row.value.is_none()
                && row.place.is_none()
                && row.end.is_none()
                && row.instructions.is_empty()
                && row.construction.is_none()
                && row.callee.is_none()
                && row.callee_place.is_none()
                && row.dyn_receiver.is_none()
            {
                row.place = Some(place);
                return Ok(());
            }
            pay(left, frame.stack.len().saturating_add(1), span)?;
            return Err(invalid(
                span,
                &format!(
                    "a direct place reuses an already emitted expression identity: function {function:?}, node {expression:?}, stack {:?}, complete {}, value {:?}, place {:?}, end {:?}, instructions {}, requested {boundary:?}",
                    frame.stack,
                    row.complete,
                    row.value,
                    row.place,
                    row.end,
                    row.instructions.len()
                ),
            ));
        }
        frame.expressions.insert(
            expression,
            Expression {
                place: Some(place),
                end: Some(boundary),
                complete: true,
                ..Expression::default()
            },
        );
        Ok(())
    }
    /// Original body nodes and actual Core emission nodes are separate
    /// coordinate domains. The immutable Core site maps this definition and
    /// recipe to the actual call key; no callee Type/image supplies that map.
    pub(crate) fn original_call_expression(
        &self, function: FunctionId, definition: &SemanticDefinition,
        recipe: SemanticSiteId, left: &mut usize,
    ) -> R<NodeId> {
        pay(left, lookup_cost(self.frames.len()), Span::default())?;
        let frame = self.frames.get(&function)
            .ok_or_else(|| invalid(Span::default(), "prepared call has no actual source frame"))?;
        let mut selected = None;
        for program in self.original.context().map(|context| &context.semantic).into_iter()
            .chain(self.frozen.as_deref().map(|source| &source.semantic)) {
            pay(left, program.definitions.len().saturating_add(program.instances.len()), Span::default())?;
            let Some(original) = program.definitions.iter().find(|row| row.id == definition.id) else { continue; };
            for identity in [&original.identity, &definition.identity] {
                pay(left, identity.package.len().saturating_add(identity.module_path.len())
                    .saturating_add(identity.declaration_path.len()).saturating_add(identity.generation_path.len()).saturating_add(1), Span::default())?;
                for part in identity.module_path.iter().chain(&identity.declaration_path) {
                    pay(left, part.len(), Span::default())?;
                }
            }
            if original.identity != definition.identity { continue; }
            let mut scopes = Vec::new();
            for row in &program.instances {
                if row.definition != definition.id { continue; }
                let matches = match row.target {
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =>
                        frame.origin.closure.is_none() && module == frame.origin.module && function == frame.origin.creator,
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure { module, creator, expression, occurrence: 0 }) =>
                        module == frame.origin.module && creator == frame.origin.creator && Some(expression) == frame.origin.closure,
                    _ => false,
                };
                if matches { scopes.push(row.id); }
            }
            pay(left, program.sites.len().saturating_mul(scopes.len().saturating_add(1)), Span::default())?;
            for site in &program.sites {
                if site.recipe != recipe || !scopes.contains(&site.instance) { continue; }
                let SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Call } = site.at else {
                    return Err(invalid(Span::default(), "original prepared call has no actual Core occurrence"));
                };
                if selected.replace(expression).is_some_and(|old| old != expression) {
                    return Err(invalid(Span::default(), "one original Call recipe names different actual Core occurrences"));
                }
            }
        }
        selected.ok_or_else(|| invalid(Span::default(), "prepared call has no immutable original-to-Core occurrence mapping"))
    }
    pub(crate) fn callee_place(
        &mut self, function: FunctionId, callee: NodeId, address: ValueId,
        left: &mut usize, span: Span,
    ) -> R<()> {
        if self.frames.is_empty() { return Ok(()); }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else { return Ok(()); };
        let call = *frame.stack.last().ok_or_else(|| invalid(span,
            "callee place was prepared outside its actual call"))?;
        pay(left, lookup_cost(frame.expressions.len()).saturating_add(1), span)?;
        let row = frame.expressions.get_mut(&call).ok_or_else(|| invalid(span,
            "callee place lost its active call receipt"))?;
        if row.complete || row.callee != Some(callee) || row.callee_place.replace(address).is_some() {
            return Err(invalid(span, "callee place changes its actual call preparation"));
        }
        Ok(())
    }
    pub fn instruction(
        &mut self,
        function: FunctionId,
        at: Point,
        storage: Option<LocalId>,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        if let Some(local) = storage {
            pay(
                left,
                lookup_cost(frame.live_locals.len()).saturating_add(1),
                span,
            )?;
            if frame.live_locals.insert(local, at).is_some() {
                return Err(invalid(
                    span,
                    "one actual local has two semantic StorageLive points",
                ));
            }
        }
        let Some(expression) = frame.stack.last() else {
            return Ok(());
        };
        pay(
            left,
            lookup_cost(frame.expressions.len()).saturating_add(1),
            span,
        )?;
        frame
            .expressions
            .get_mut(expression)
            .expect("active expression")
            .instructions
            .push(at);
        Ok(())
    }
    /// A compiler temporary's StorageLive was removed and then inserted at
    /// its real lexical entry. Restore this exact local/point association;
    /// do not infer it later from an equal opcode or a declaration's type.
    pub fn restored_temporary_live(
        &mut self,
        function: FunctionId,
        local: LocalId,
        point: Point,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        pay(
            left,
            lookup_cost(frame.live_locals.len()).saturating_add(1),
            span,
        )?;
        if frame.live_locals.insert(local, point).is_some() {
            return Err(invalid(
                span,
                "temporary StorageLive relocation duplicates an actual local",
            ));
        }
        Ok(())
    }
    pub(crate) fn closure_slot(
        &mut self, function: FunctionId, local: LocalId, initializer: NodeId, input: ValueId, array: ValueId,
        construction: Point, initialization: Point, left: &mut usize, span: Span,
    ) -> R<()> {
        if self.frames.is_empty() { return Ok(()); }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else { return Ok(()); };
        pay(left, lookup_cost(frame.live_locals.len()) + lookup_cost(frame.closure_slots.len()) + 1, span)?;
        if !frame.live_locals.contains_key(&local)
            || frame.closure_slots.insert(local, ClosureSlot { initializer, input, array, construction, initialization }).is_some()
        {
            return Err(invalid(span, "callback slot does not have one real live local"));
        }
        Ok(())
    }
    /// This private emitter receipt locates a representation, not a type,
    /// lifetime or cleanup proof. Every consumer rechecks the actual operations.
    fn persistent_closure_type<'f>(
        &self, function: impl Into<crate::FunctionRef<'f>>, local: LocalId, limits: &Limits, left: &mut usize,
    ) -> R<Option<LogicalLocalType<'f>>> {
        let function = function.into();
        let span = function.span;
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get(&function.id) else { return Ok(None); };
        pay(left, lookup_cost(frame.closure_slots.len()), span)?;
        let Some(slot) = frame.closure_slots.get(&local) else { return Ok(None); };
        let source = self.completed_expression(function.id, slot.initializer, left, span)?;
        if source.value != Some(slot.input) {
            return Err(invalid(span, "callback slot does not store its exact original initializer occurrence"));
        }
        let at = |point: Point| -> R<&Instruction> {
            function.blocks.get(point.block.0 as usize).filter(|block| block.id == point.block)
                .and_then(|block| block.instructions.get(point.instruction))
                .ok_or_else(|| invalid(span, "callback slot receipt leaves its actual CFG"))
        };
        pay(left, lookup_cost(frame.live_locals.len()).saturating_add(4), span)?;
        let live = *frame.live_locals.get(&local)
            .ok_or_else(|| invalid(span, "callback slot has no exact StorageLive receipt"))?;
        if slot.construction.block != live.block || live.block != slot.initialization.block
            || slot.construction.instruction >= live.instruction
            || live.instruction.checked_add(1) != Some(slot.initialization.instruction)
            || !matches!(at(live)?.kind, InstructionKind::StorageLive(id) if id == local)
            || !matches!(&at(slot.construction)?.kind, InstructionKind::MakeOwnedArray { elements } if elements.as_slice() == [slot.input])
            || at(slot.construction)?.result != Some(slot.array)
            || !matches!(at(slot.initialization)?.kind, InstructionKind::Store { local: id, value } if id == local && value == slot.array)
        {
            return Err(invalid(span, "callback slot changes its actual construction/live/initialization chain"));
        }
        let physical = &function.locals.get(local.0 as usize)
            .ok_or_else(|| invalid(span, "callback slot local is absent"))?.ty;
        let Type::Array { element, length: 1 } = physical else {
            return Err(invalid(span, "callback slot is not its actual single-element storage"));
        };
        if !matches!(element.as_ref(), Type::Closure(_)) {
            return Err(invalid(span, "callback slot element is not its real closure"));
        }
        let input = function.values.get(slot.input.0 as usize)
            .ok_or_else(|| invalid(span, "callback slot initializer is absent"))?;
        let array = function.values.get(slot.array.0 as usize)
            .ok_or_else(|| invalid(span, "callback slot aggregate result is absent"))?;
        inference::charge_type_snapshot(input, span, limits, left, 1)?;
        inference::charge_type_snapshot(array, span, limits, left, 1)?;
        inference::charge_type_snapshot(physical, span, limits, left, 1)?;
        if input != element.as_ref() || array != physical {
            return Err(invalid(span, "callback slot changes its exact original closure occurrence type"));
        }
        Ok(Some(LogicalLocalType { ty: element, slot: Some(*slot) }))
    }
    pub(crate) fn logical_local_type<'f>(
        &self, function: impl Into<crate::FunctionRef<'f>>, local: LocalId, limits: &Limits, left: &mut usize,
    ) -> R<LogicalLocalType<'f>> {
        let function = function.into();
        let physical = &function.locals.get(local.0 as usize)
            .ok_or_else(|| invalid(function.span, "logical local has no actual physical storage"))?.ty;
        Ok(self.persistent_closure_type(function, local, limits, left)?
            .unwrap_or(LogicalLocalType { ty: physical, slot: None }))
    }
    pub(crate) fn closure_slot_constructor(
        &self, function: &TypedFunction, point: Point, limits: &Limits, left: &mut usize,
    ) -> R<Option<ClosureSlot>> {
        pay(left, lookup_cost(self.frames.len()), function.span)?;
        let Some(frame) = self.frames.get(&function.id) else { return Ok(None); };
        pay(left, frame.closure_slots.len(), function.span)?;
        let mut selected = None;
        for (local, slot) in &frame.closure_slots {
            if slot.construction == point {
                if selected.replace((*local, *slot)).is_some() {
                    return Err(invalid(function.span, "one actual callback slot constructor initializes several source locals"));
                }
            }
        }
        let Some((local, slot)) = selected else { return Ok(None); };
        self.persistent_closure_type(function, local, limits, left)?
            .ok_or_else(|| invalid(function.span, "callback slot constructor loses its local receipt"))?;
        Ok(Some(slot))
    }
    pub fn declaration(
        &mut self,
        function: FunctionId,
        statement: NodeId,
        local: LocalId,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        pay(left, lookup_cost(frame.live_locals.len()), span)?;
        let at = *frame
            .live_locals
            .get(&local)
            .ok_or_else(|| invalid(span, "actual binding lacks its emitted StorageLive"))?;
        pay(
            left,
            lookup_cost(frame.declarations.len()).saturating_add(1),
            span,
        )?;
        if frame
            .declarations
            .insert(statement, Declaration { local, at })
            .is_some()
        {
            return Err(invalid(
                span,
                "one Core binding identity declares two actual locals",
            ));
        }
        Ok(())
    }
    pub fn finish_function(&self, function: &TypedFunction, left: &mut usize) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        let span = function.span;
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get(&function.id) else {
            return Ok(());
        };
        if !frame.stack.is_empty() {
            return Err(invalid(
                span,
                "actual function completed inside an open semantic expression",
            ));
        }
        if let Some((parent, closure)) = frame.parent {
            pay(left, lookup_cost(self.frames.len()), span)?;
            if !self.frames.contains_key(&parent)
                || function.environment != closure
            {
                return Err(invalid(
                    span,
                    "closure entry receipt lost its actual creator or environment",
                ));
            }
        }
        pay(
            left,
            frame
                .expressions
                .len()
                .saturating_add(frame.declarations.len()),
            span,
        )?;
        for row in frame.expressions.values() {
            if !row.complete
                || row
                    .value
                    .is_some_and(|id| id.0 as usize >= function.values.len())
            {
                return Err(invalid(
                    span,
                    "semantic expression did not complete with its actual typed result",
                ));
            }
            if matches!(&row.place, Some(SemanticOperand::Local(local)) if local.0 as usize >= function.locals.len())
            {
                return Err(invalid(
                    span,
                    "semantic place receipt names an absent actual local",
                ));
            }
            if row.end.is_some_and(|point| {
                function
                    .blocks
                    .get(point.block.0 as usize)
                    .is_none_or(|block| point.instruction > block.instructions.len())
            }) {
                return Err(invalid(
                    span,
                    "semantic expression boundary was not relocated with the actual CFG",
                ));
            }
            pay(left, row.instructions.len(), span)?;
            for point in &row.instructions {
                if function
                    .blocks
                    .get(point.block.0 as usize)
                    .and_then(|block| block.instructions.get(point.instruction))
                    .is_none()
                {
                    return Err(invalid(
                        span,
                        "semantic instruction receipt was not relocated with the actual CFG",
                    ));
                }
            }
            if let Some(construction) = row.construction {
                pay(left, row.instructions.len().saturating_add(1), span)?;
                if !row.instructions.contains(&construction.at)
                    || !matches!(function.blocks.get(construction.at.block.0 as usize)
                        .and_then(|block| block.instructions.get(construction.at.instruction)),
                        Some(Instruction { result: Some(value), kind, .. })
                            if *value == construction.value && matches!(kind,
                                InstructionKind::Constant(Constant::Unit)
                                | InstructionKind::MakeVariant { .. }
                                | InstructionKind::MakeErrorUnion { .. }
                                | InstructionKind::MakeAggregate { .. }
                                | InstructionKind::MakeManaged { .. }
                                | InstructionKind::MakePackage { .. }
                                | InstructionKind::MakeOwnedTuple { .. }
                                | InstructionKind::MakeOwnedArray { .. }))
                {
                    return Err(invalid(
                        span,
                        "constructor receipt lost its actual creation result/opcode",
                    ));
                }
            }
        }
        for (local, at) in &frame.live_locals {
            pay(left, 1, span)?;
            if !matches!(function.blocks.get(at.block.0 as usize)
                .and_then(|block| block.instructions.get(at.instruction))
                .map(|instruction| &instruction.kind), Some(InstructionKind::StorageLive(actual)) if actual == local)
            {
                return Err(invalid(
                    span,
                    "actual local receipt lost its exact StorageLive instruction",
                ));
            }
        }
        for row in frame.declarations.values() {
            if function.locals.get(row.local.0 as usize).is_none()
                || !matches!(function.blocks.get(row.at.block.0 as usize)
                    .and_then(|block| block.instructions.get(row.at.instruction))
                    .map(|instruction| &instruction.kind),
                    Some(InstructionKind::StorageLive(actual)) if *actual == row.local)
            {
                return Err(invalid(
                    span,
                    "semantic binding does not name its actual StorageLive local",
                ));
            }
        }
        Ok(())
    }
    /// Insertions happen after the emitter recorded actual ordinals. Rebase
    /// those receipts at the mutation, never by finding an equal opcode later.
    pub fn inserted(
        &mut self,
        function: FunctionId,
        block: BlockId,
        index: usize,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        self.relocate(
            function,
            left,
            span,
            |point| {
                if point.block == block && point.instruction >= index {
                    point.instruction += 1;
                }
                true
            },
            |point| {
                if point.block == block && point.instruction >= index {
                    point.instruction += 1;
                }
            },
        )
    }
    /// Compiler temporary StorageLive can move to its lexical entry. It is not
    /// an original DeclareLocal receipt. Removing an original binding receipt
    /// requires an explicit transform and is rejected here.
    pub fn removed_temporary(
        &mut self,
        function: FunctionId,
        block: BlockId,
        index: usize,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        if !self.frames.is_empty() {
            pay(left, lookup_cost(self.frames.len()), span)?;
        }
        if let Some(frame) = self.frames.get(&function) {
            pay(left, frame.declarations.len().saturating_add(1), span)?;
            if frame.declarations.values().any(|row| {
                row.at
                    == (Point {
                        block,
                        instruction: index,
                    })
            }) {
                return Err(invalid(
                    span,
                    "temporary relocation selected an original binding",
                ));
            }
        }
        self.relocate(
            function,
            left,
            span,
            |point| {
                if point.block != block {
                    return true;
                }
                if point.instruction == index {
                    return false;
                }
                if point.instruction > index {
                    point.instruction -= 1;
                }
                true
            },
            |point| {
                if point.block == block && point.instruction > index {
                    point.instruction -= 1;
                }
            },
        )
    }
    fn relocate(
        &mut self,
        function: FunctionId,
        left: &mut usize,
        span: Span,
        mut change: impl FnMut(&mut Point) -> bool,
        mut boundary: impl FnMut(&mut Point),
    ) -> R<()> {
        if self.frames.is_empty() {
            return Ok(());
        }
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get_mut(&function) else {
            return Ok(());
        };
        pay(
            left,
            frame
                .expressions
                .len()
                .saturating_add(frame.declarations.len())
                .saturating_add(frame.live_locals.len())
                .saturating_add(frame.closure_slots.len()),
            span,
        )?;
        for row in frame.expressions.values_mut() {
            pay(left, row.instructions.len(), span)?;
            row.instructions.retain_mut(&mut change);
            if let Some(point) = &mut row.end {
                boundary(point);
            }
            if let Some(construction) = &mut row.construction {
                if !change(&mut construction.at) {
                    return Err(invalid(
                        span,
                        "constructor receipt was removed without a transform",
                    ));
                }
            }
        }
        frame.live_locals.retain(|_, point| change(point));
        for slot in frame.closure_slots.values_mut() {
            if !change(&mut slot.construction) || !change(&mut slot.initialization) {
                return Err(invalid(span, "callback slot receipt was removed without its exact transform"));
            }
        }
        for row in frame.declarations.values_mut() {
            if !change(&mut row.at) {
                return Err(invalid(
                    span,
                    "semantic binding receipt was removed without a transform",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_entries_release_the_slot_and_keep_the_lexical_creator() {
        let mut ledger = Ledger::default();
        let mut left = 1000;
        let span = Span::default();
        let creator = FunctionId(0);
        let slot = ClosureId(0);
        let origin = Origin { module: ModuleId(0), creator: NodeId(1), closure: None };
        ledger.bind(creator, origin, None, &mut left, span).unwrap();
        for id in [1, 2] {
            let entry = FunctionId(id);
            ledger.bind_closure(creator, NodeId(id), slot, entry, &mut left, span).unwrap();
            assert_eq!(ledger.closure_entries[&slot], entry);
            ledger.convert_to_function_ref(entry, slot, &mut left, span).unwrap();
            assert_eq!(ledger.frames[&entry].parent, Some((creator, None)));
            assert!(!ledger.closure_entries.contains_key(&slot));
        }
        let retained = FunctionId(3);
        ledger.bind_closure(creator, NodeId(3), slot, retained, &mut left, span).unwrap();
        assert!(ledger.convert_to_function_ref(FunctionId(1), slot, &mut left, span).is_err());
        assert_eq!(ledger.closure_entries[&slot], retained);
        assert_eq!(ledger.frames[&retained].parent, Some((creator, Some(slot))));
    }

    #[test]
    fn actual_creation_receipt_survives_a_distinct_contextual_result() {
        let mut ledger = Ledger::default();
        let mut left = 1000;
        let span = Span::default();
        let function = FunctionId(0);
        let node = NodeId(0);
        let at = Point {
            block: BlockId(0),
            instruction: 0,
        };
        ledger
            .bind(
                function,
                Origin {
                    module: ModuleId(0),
                    creator: node,
                    closure: None,
                },
                None,
                &mut left,
                span,
            )
            .unwrap();
        let entered = ledger
            .begin(function, node, None, &Limits::default(), &mut left, span)
            .unwrap();
        assert!(
            ledger
                .constructed(function, node, ValueId(3), at, &mut left, span)
                .is_err()
        );
        ledger
            .instruction(function, at, None, &mut left, span)
            .unwrap();
        ledger
            .constructed(function, node, ValueId(3), at, &mut left, span)
            .unwrap();
        assert!(
            ledger
                .constructed(function, node, ValueId(4), at, &mut left, span)
                .is_err()
        );
        assert!(
            ledger
                .constructed(function, NodeId(1), ValueId(3), at, &mut left, span)
                .is_err()
        );
        let observed = Point {
            block: BlockId(0),
            instruction: 1,
        };
        ledger
            .instruction(function, observed, None, &mut left, span)
            .unwrap();
        ledger
            .end(
                function,
                node,
                entered,
                Some(ValueId(4)),
                Some(Point {
                    block: BlockId(0),
                    instruction: 2,
                }),
                &mut left,
                span,
            )
            .unwrap();
        let expression = &ledger.frames[&function].expressions[&node];
        assert_eq!(expression.value, Some(ValueId(4)));
        assert_eq!(
            expression.construction,
            Some(Construction {
                value: ValueId(3),
                at
            })
        );
        assert_eq!(expression.instructions, [at, observed]);
    }
}
