//! Private receipts from actual Builder operations. These records do not prove
//! a recipe, lifetime, capability or a public semantic claim. In particular a
//! Core node is never selected by Span, a name, or equality of result types.
use crate::*;
use zil_base::{ModuleId, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Origin {
    pub module: ModuleId,
    /// Real outer CoreFunction, also for a nested closure body. Synthetic
    /// closure entry functions have no independent CoreFunction identity.
    pub creator: NodeId,
    pub closure: Option<NodeId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Point {
    pub block: BlockId,
    pub instruction: usize,
}
#[derive(Default)]
pub(super) struct Expression {
    pub value: Option<ValueId>,
    pub callee: Option<NodeId>,
    pub end: Option<Point>,
    pub instructions: Vec<Point>,
    complete: bool,
}
pub(super) struct Declaration {
    pub local: LocalId,
    pub at: Point,
}
pub(super) struct Frame {
    pub origin: Origin,
    pub expressions: BTreeMap<NodeId, Expression>,
    pub declarations: BTreeMap<NodeId, Declaration>,
    /// Actual direct creator and both actual environment/entry identities.
    pub parent: Option<(FunctionId, ClosureId)>,
    stack: Vec<NodeId>,
    live_locals: BTreeMap<LocalId, Point>,
}
#[derive(Default)]
pub(super) struct Ledger {
    pub frames: BTreeMap<FunctionId, Frame>,
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
                .saturating_add(frame.declarations.len())
                .saturating_add(frame.live_locals.len()),
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
                .saturating_add(frame.declarations.len()),
            span,
        )?;
        for row in frame.expressions.values_mut() {
            pay(left, row.instructions.len(), span)?;
            row.instructions.retain_mut(&mut change);
            if let Some(point) = &mut row.end {
                boundary(point);
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
