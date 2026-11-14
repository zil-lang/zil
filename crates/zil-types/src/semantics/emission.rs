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
    pub end: Option<Point>,
    pub instructions: Vec<Point>,
    /// The real literal producer result, before a later contextual observation
    /// or error injection. This is independent of the final expression value.
    pub construction: Option<Construction>,
    complete: bool,
}
pub(crate) struct Declaration {
    pub local: LocalId,
    pub at: Point,
}
pub(crate) struct Frame {
    pub origin: Origin,
    pub expressions: BTreeMap<NodeId, Expression>,
    pub declarations: BTreeMap<NodeId, Declaration>,
    /// Actual direct creator and both actual environment/entry identities.
    pub parent: Option<(FunctionId, ClosureId)>,
    stack: Vec<NodeId>,
    live_locals: BTreeMap<LocalId, Point>,
}
#[derive(Default)]
pub(crate) struct Ledger {
    pub(super) fixed_abis: Option<super::abi::Choices>,
    pub(super) abi_origins: Vec<super::abi::SourceOrigin>,
    /// Only the explicit frozen-context entry may install this independently
    /// checked immutable source interface. It contributes no emitted frames.
    pub(super) frozen: Option<std::rc::Rc<super::frozen_context::FrozenSource>>,
    pub(super) original: super::original_context::Slot,
    pub frames: BTreeMap<FunctionId, Frame>,
    /// Bound only by relocation of the exact source request and emitted call.
    pub(super) instance_requests: std::cell::RefCell<
        BTreeMap<SemanticInstanceId, (SemanticInstanceId, SemanticSiteId, FunctionId)>,
    >,
    /// Lexical selections recorded while emitting this exact MakeClosure.
    pub(super) closure_captures: BTreeMap<ClosureId, Vec<(String, ValueId)>>,
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
        self.frames.insert(
            function,
            Frame {
                origin,
                parent,
                expressions: BTreeMap::new(),
                declarations: BTreeMap::new(),
                stack: Vec::new(),
                live_locals: BTreeMap::new(),
            },
        );
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
        if let Some(row) = frame.expressions.get(&expression) {
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
                || function.environment.is_some_and(|actual| actual != closure)
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
                .saturating_add(frame.live_locals.len()),
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
