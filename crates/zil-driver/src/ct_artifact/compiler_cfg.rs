use super::*;
#[path = "compiler_cfg_prune.rs"]
mod prune;
#[derive(Clone)]
pub(super) struct Value {
    pub id: CtValueId,
    pub ty: CtTy,
}
#[derive(Clone)]
struct Binding {
    name: String,
    local: CtLocalId,
    ty: CtTy,
    mutable: bool,
}
struct Scope {
    bindings: Vec<Binding>,
}
struct Block {
    instructions: Vec<CtInstruction>,
    terminator: Option<CtTerminator>,
    reachable: bool,
}
#[derive(Clone, Copy)]
struct Loop {
    stop: CtBlockId,
    next: CtBlockId,
    depth: usize,
}
struct Yield {
    end: CtBlockId,
    local: CtLocalId,
    ty: Option<CtTy>,
    depth: usize,
}
pub(super) struct Builder<'c, 'a> {
    pub compiler: &'c mut Compiler<'a>,
    pub module: usize,
    pub definition: CtSymbolId,
    id: CtFunctionId,
    result: CtTy,
    parameters: Vec<CtLocalId>,
    parameter_types: Vec<CtTy>,
    pub locals: Vec<CtTy>,
    pub values: Vec<CtTy>,
    blocks: Vec<Block>,
    current: CtBlockId,
    scopes: Vec<Scope>,
    loops: Vec<Loop>,
    yields: Vec<Yield>,
    span: Span,
}
impl<'c, 'a> Builder<'c, 'a> {
    fn new(
        compiler: &'c mut Compiler<'a>,
        id: CtFunctionId,
        module: usize,
        definition: CtSymbolId,
        result: CtTy,
        span: Span,
    ) -> R<Self> {
        let mut result = Self {
            compiler,
            module,
            definition,
            id,
            result,
            parameters: Vec::new(),
            parameter_types: Vec::new(),
            locals: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            current: CtBlockId(0),
            scopes: vec![Scope {
                bindings: Vec::new(),
            }],
            loops: Vec::new(),
            yields: Vec::new(),
            span,
        };
        result.current = result.new_block(span)?;
        Ok(result)
    }
    pub(super) fn source_alias_type_ref(
        &mut self,
        module: usize,
        actual: &s::TypeRef,
        depth: usize,
    ) -> R<Value> {
        self.compiler.budget.work(1, actual.span)?;
        // A top-level alias has no access to its user's lexical/control scopes.
        // Move the stacks aside; alias blocks create their own ordinary scopes.
        let scopes = std::mem::take(&mut self.scopes);
        let loops = std::mem::take(&mut self.loops);
        let yields = std::mem::take(&mut self.yields);
        let previous_module = self.module;
        self.module = module;
        let result = self.type_ref(module, actual, depth);
        self.module = previous_module;
        self.scopes = scopes;
        self.loops = loops;
        self.yields = yields;
        result
    }
    pub fn new_block(&mut self, span: Span) -> R<CtBlockId> {
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.blocks,
            1,
            span,
            "CT CFG block budget exhausted",
        )?;
        let id = CtBlockId(self.blocks.len() as u32);
        self.blocks.push(Block {
            instructions: Vec::new(),
            terminator: None,
            reachable: id.0 == 0,
        });
        Ok(id)
    }
    pub fn terminated(&self) -> bool {
        self.blocks[self.current.0 as usize].terminator.is_some()
    }
    pub fn terminate(&mut self, term: CtTerminator, span: Span) -> R<()> {
        self.compiler.budget.node(span)?;
        if self.terminated() {
            return Err(error(span, "CT block already has a control transfer"));
        }
        if self.blocks[self.current.0 as usize].reachable {
            match &term {
                CtTerminator::Jump(target) => self.blocks[target.0 as usize].reachable = true,
                CtTerminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.blocks[then_block.0 as usize].reachable = true;
                    self.blocks[else_block.0 as usize].reachable = true;
                }
                _ => {}
            }
        }
        self.blocks[self.current.0 as usize].terminator = Some(term);
        Ok(())
    }
    pub fn op(&mut self, kind: CtInstructionKind, span: Span) -> R<()> {
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.instructions,
            1,
            span,
            "CT instruction budget exhausted",
        )?;
        self.blocks[self.current.0 as usize]
            .instructions
            .push(CtInstruction {
                result: None,
                kind,
                span,
            });
        Ok(())
    }
    pub fn emit(&mut self, kind: CtInstructionKind, ty: CtTy, span: Span) -> R<Value> {
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.values,
            1,
            span,
            "CT value-table budget exhausted",
        )?;
        self.compiler.budget.ty(&ty, 0, span)?;
        let id = CtValueId(self.values.len() as u32);
        self.values.push(ty.clone());
        self.op(kind, span)?;
        self.blocks[self.current.0 as usize]
            .instructions
            .last_mut()
            .unwrap()
            .result = Some(id);
        Ok(Value { id, ty })
    }
    pub fn constant(&mut self, c: CtConstant, ty: CtTy, span: Span) -> R<Value> {
        self.compiler.budget.node(span)?;
        let id = CtConstantId(self.compiler.constants.len() as u32);
        self.compiler.constants.push(c);
        self.emit(CtInstructionKind::Constant(id), ty, span)
    }
    pub fn integer(&mut self, n: i64, span: Span) -> R<Value> {
        self.constant(
            CtConstant::Scalar(Constant::I64(n)),
            CtTy::Scalar(CtScalarTy::I64),
            span,
        )
    }
    pub fn new_local(&mut self, ty: CtTy, span: Span) -> R<CtLocalId> {
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.locals,
            1,
            span,
            "CT local-table budget exhausted",
        )?;
        let id = CtLocalId(self.locals.len() as u32);
        self.locals.push(ty);
        Ok(id)
    }
    pub fn lookup(&mut self, name: &str, span: Span) -> R<Option<Value>> {
        for scope in self.scopes.iter().rev() {
            for binding in scope.bindings.iter().rev() {
                self.compiler.budget.work(1, span)?;
                if binding.name == name {
                    let local = binding.local;
                    self.compiler.budget.ty(&binding.ty, 0, span)?;
                    let ty = binding.ty.clone();
                    return self
                        .emit(CtInstructionKind::Load(local), ty, span)
                        .map(Some);
                }
            }
        }
        Ok(None)
    }
    pub fn has_local(&mut self, name: &str, span: Span) -> R<bool> {
        for scope in self.scopes.iter().rev() {
            for binding in &scope.bindings {
                self.compiler.budget.work(1, span)?;
                if binding.name == name {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn bind(&mut self, name: &str, ty: CtTy, mutable: bool, span: Span) -> R<CtLocalId> {
        for binding in &self.scopes.last().unwrap().bindings {
            self.compiler.budget.work(1, span)?;
            if binding.name == name {
                return Err(error(span, "duplicate CT lexical binding"));
            }
        }
        self.compiler.budget.text(name, span)?;
        self.compiler.budget.ty(&ty, 0, span)?;
        let local = self.new_local(ty.clone(), span)?;
        self.compiler.budget.node(span)?;
        self.scopes.last_mut().unwrap().bindings.push(Binding {
            name: name.into(),
            local,
            ty,
            mutable,
        });
        Ok(local)
    }
    fn pattern(&mut self, pattern: &s::Pattern, value: Value, mutable: bool) -> R<()> {
        match &pattern.kind {
            s::PatternKind::Name(name) => {
                let local = self.bind(name, value.ty, mutable, pattern.span)?;
                self.op(CtInstructionKind::StorageLive(local), pattern.span)?;
                self.op(
                    CtInstructionKind::Store {
                        local,
                        value: value.id,
                    },
                    pattern.span,
                )
            }
            s::PatternKind::Wildcard => Ok(()),
            s::PatternKind::Tuple(fields) => {
                let CtTy::Tuple(types) = &value.ty else {
                    return Err(error(pattern.span, "CT tuple binding requires a tuple"));
                };
                if fields.len() != types.len() {
                    return Err(error(pattern.span, "CT tuple binding arity differs"));
                }
                for (index, (field, ty)) in fields.iter().zip(types).enumerate() {
                    let projected = self.emit(
                        CtInstructionKind::ExtractField {
                            aggregate: value.id,
                            index: index as u32,
                        },
                        ty.clone(),
                        field.span,
                    )?;
                    self.pattern(field, projected, mutable)?;
                }
                Ok(())
            }
            _ => Err(error(
                pattern.span,
                "CT binding requires an ordinary name, tuple or wildcard",
            )),
        }
    }
    fn cleanup(&mut self, depth: usize, span: Span) -> R<()> {
        let mut locals = Vec::new();
        for scope in &self.scopes[depth..] {
            self.compiler.budget.work(scope.bindings.len(), span)?;
            for binding in scope.bindings.iter().rev() {
                self.compiler.budget.node(span)?;
                locals.push(binding.local);
            }
        }
        for local in locals {
            self.op(CtInstructionKind::StorageDead(local), span)?;
        }
        Ok(())
    }
    fn scoped(&mut self, body: &s::Block, depth: usize) -> R<()> {
        self.compiler.budget.depth(depth, body.span)?;
        self.compiler.budget.node(body.span)?;
        self.scopes.push(Scope {
            bindings: Vec::new(),
        });
        self.block(body, depth + 1)?;
        if !self.terminated() {
            self.cleanup(self.scopes.len() - 1, body.span)?;
        }
        self.scopes.pop();
        Ok(())
    }
    fn condition(&mut self, condition: &s::Condition, depth: usize) -> R<Value> {
        match condition {
            s::Condition::Expression(e) => {
                self.expr(e, Some(&CtTy::Scalar(CtScalarTy::Bool)), depth)
            }
            _ => Err(error(
                self.span,
                "CT let-condition requires enum pattern lowering",
            )),
        }
    }
    pub fn expect(&mut self, value: Value, expected: Option<&CtTy>, span: Span) -> R<Value> {
        self.compiler.budget.ty(&value.ty, 0, span)?;
        if let Some(ty) = expected {
            self.compiler.budget.ty(ty, 0, span)?;
        }
        if expected.is_some_and(|t| *t != value.ty) {
            return Err(error(
                span,
                "CT expression type does not match its actual expected type",
            ));
        }
        Ok(value)
    }
    pub fn block(&mut self, body: &s::Block, depth: usize) -> R<()> {
        for statement in &body.statements {
            self.compiler.budget.depth(depth, statement.span)?;
            if self.terminated() {
                // Still attribute unexecuted source; no executable predecessor is added.
                self.current = self.new_block(statement.span)?;
            }
            self.statement(statement, depth + 1)?;
        }
        Ok(())
    }
    fn statement(&mut self, statement: &s::Stmt, depth: usize) -> R<()> {
        let span = statement.span;
        match &statement.kind {
            s::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                let expected = ty
                    .as_ref()
                    .map(|t| self.compiler.ty(self.module, t, depth))
                    .transpose()?;
                let value = self.expr(value, expected.as_ref(), depth)?;
                self.pattern(pattern, value, *mutable)
            }
            s::StmtKind::Expression(e) => {
                self.expr(e, None, depth)?;
                Ok(())
            }
            s::StmtKind::Assign { target, op, value } => {
                let s::ExprKind::Name(name) = &target.kind else {
                    return Err(error(span, "CT assignment needs a mutable lexical local"));
                };
                let mut binding = None;
                for scope in self.scopes.iter().rev() {
                    for b in scope.bindings.iter().rev() {
                        self.compiler.budget.work(1, span)?;
                        if b.name == *name {
                            self.compiler.budget.text(&b.name, span)?;
                            self.compiler.budget.ty(&b.ty, 0, span)?;
                            binding = Some(b.clone());
                            break;
                        }
                    }
                    if binding.is_some() {
                        break;
                    }
                }
                let binding = binding.ok_or_else(|| error(span, "unknown CT assignment target"))?;
                if !binding.mutable {
                    return Err(error(span, "CT assignment target is immutable"));
                }
                let right = self.expr(value, Some(&binding.ty), depth)?;
                let value = if *op == s::AssignOp::Set {
                    right
                } else {
                    let left = self.emit(
                        CtInstructionKind::Load(binding.local),
                        binding.ty.clone(),
                        span,
                    )?;
                    let op = match op {
                        s::AssignOp::Add => BinaryOp::Add,
                        s::AssignOp::Subtract => BinaryOp::Subtract,
                        s::AssignOp::Multiply => BinaryOp::Multiply,
                        s::AssignOp::Divide => BinaryOp::Divide,
                        s::AssignOp::Remainder => BinaryOp::Remainder,
                        s::AssignOp::BitAnd => BinaryOp::BitAnd,
                        s::AssignOp::BitOr => BinaryOp::BitOr,
                        s::AssignOp::BitXor => BinaryOp::BitXor,
                        _ => unreachable!(),
                    };
                    self.emit(
                        CtInstructionKind::Binary {
                            op,
                            left: left.id,
                            right: right.id,
                        },
                        binding.ty,
                        span,
                    )?
                };
                self.op(
                    CtInstructionKind::Store {
                        local: binding.local,
                        value: value.id,
                    },
                    span,
                )
            }
            s::StmtKind::Return(expression) => {
                if !self.yields.is_empty() {
                    return Err(error(
                        span,
                        "return cannot leave a CT block expression; use yield",
                    ));
                }
                self.compiler.budget.ty(&self.result, 0, span)?;
                let expected = self.result.clone();
                let value = match expression {
                    Some(e) => Some(self.expr(e, Some(&expected), depth)?.id),
                    None if expected == CtTy::Scalar(CtScalarTy::Unit) => None,
                    None => return Err(error(span, "CT function return value is missing")),
                };
                self.cleanup(0, span)?;
                self.terminate(CtTerminator::Return(value), span)
            }
            s::StmtKind::Yield(expression) => {
                let Some(y) = self.yields.last() else {
                    return Err(error(
                        span,
                        "yield is only valid inside a CT block expression",
                    ));
                };
                let expected = y.ty.clone();
                let value = self.expr(expression, expected.as_ref(), depth)?;
                let y = self.yields.last_mut().unwrap();
                if y.ty.is_none() {
                    self.locals[y.local.0 as usize] = value.ty.clone();
                    y.ty = Some(value.ty.clone());
                }
                let (local, end, d) = (y.local, y.end, y.depth);
                self.op(
                    CtInstructionKind::Store {
                        local,
                        value: value.id,
                    },
                    span,
                )?;
                self.cleanup(d, span)?;
                self.terminate(CtTerminator::Jump(end), span)
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.condition(condition, depth)?;
                let yes = self.new_block(span)?;
                let no = self.new_block(span)?;
                let end = self.new_block(span)?;
                self.terminate(
                    CtTerminator::Branch {
                        condition: condition.id,
                        then_block: yes,
                        else_block: no,
                    },
                    span,
                )?;
                self.current = yes;
                self.scoped(then_body, depth)?;
                if !self.terminated() {
                    self.terminate(CtTerminator::Jump(end), span)?;
                }
                self.current = no;
                if let Some(other) = else_body {
                    match other {
                        s::ElseBranch::Block(b) => self.scoped(b, depth)?,
                        s::ElseBranch::If(s) => self.statement(s, depth)?,
                    }
                }
                if !self.terminated() {
                    self.terminate(CtTerminator::Jump(end), span)?;
                }
                self.current = end;
                Ok(())
            }
            s::StmtKind::While { condition, body } => {
                let test = self.new_block(span)?;
                let work = self.new_block(span)?;
                let end = self.new_block(span)?;
                self.terminate(CtTerminator::Jump(test), span)?;
                self.current = test;
                let condition = self.condition(condition, depth)?;
                self.terminate(
                    CtTerminator::Branch {
                        condition: condition.id,
                        then_block: work,
                        else_block: end,
                    },
                    span,
                )?;
                self.loops.push(Loop {
                    stop: end,
                    next: test,
                    depth: self.scopes.len(),
                });
                self.current = work;
                self.scoped(body, depth)?;
                if !self.terminated() {
                    self.terminate(CtTerminator::Jump(test), span)?;
                }
                self.loops.pop();
                self.current = end;
                Ok(())
            }
            s::StmtKind::Loop(body) => {
                let work = self.new_block(span)?;
                let end = self.new_block(span)?;
                self.terminate(CtTerminator::Jump(work), span)?;
                self.current = work;
                self.loops.push(Loop {
                    stop: end,
                    next: work,
                    depth: self.scopes.len(),
                });
                self.scoped(body, depth)?;
                if !self.terminated() {
                    self.terminate(CtTerminator::Jump(work), span)?;
                }
                self.loops.pop();
                self.current = end;
                Ok(())
            }
            s::StmtKind::Break | s::StmtKind::Continue => {
                let Some(target) = self.loops.last().copied() else {
                    return Err(error(span, "CT loop control is outside a loop"));
                };
                self.cleanup(target.depth, span)?;
                self.terminate(
                    CtTerminator::Jump(if matches!(statement.kind, s::StmtKind::Break) {
                        target.stop
                    } else {
                        target.next
                    }),
                    span,
                )
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => self.for_loop(pattern, iterable, body, depth),
            s::StmtKind::Match { value, arms } => self.match_statement(value, arms, depth),
            _ => Err(error(
                span,
                "runtime resources, defer, unsafe and generation statements cannot execute as CT data operations",
            )),
        }
    }
    pub fn expr_if(
        &mut self,
        condition: &s::Expr,
        then_value: &s::Expr,
        else_value: &s::Expr,
        expected: Option<&CtTy>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let cond = self.expr(condition, Some(&CtTy::Scalar(CtScalarTy::Bool)), depth)?;
        let yes = self.new_block(span)?;
        let no = self.new_block(span)?;
        let end = self.new_block(span)?;
        let local = self.new_local(
            expected.cloned().unwrap_or(CtTy::Scalar(CtScalarTy::Unit)),
            span,
        )?;
        self.op(CtInstructionKind::StorageLive(local), span)?;
        self.terminate(
            CtTerminator::Branch {
                condition: cond.id,
                then_block: yes,
                else_block: no,
            },
            span,
        )?;
        self.current = yes;
        let a = self.expr(then_value, expected, depth)?;
        self.locals[local.0 as usize] = a.ty.clone();
        self.op(CtInstructionKind::Store { local, value: a.id }, span)?;
        self.terminate(CtTerminator::Jump(end), span)?;
        self.current = no;
        let b = self.expr(else_value, Some(&a.ty), depth)?;
        self.op(CtInstructionKind::Store { local, value: b.id }, span)?;
        self.terminate(CtTerminator::Jump(end), span)?;
        self.current = end;
        let result = self.emit(CtInstructionKind::Load(local), a.ty, span)?;
        self.op(CtInstructionKind::StorageDead(local), span)?;
        Ok(result)
    }
    pub fn meta_block(
        &mut self,
        body: &s::Block,
        expected: Option<&CtTy>,
        depth: usize,
    ) -> R<Value> {
        let span = body.span;
        let end = self.new_block(span)?;
        let local = self.new_local(
            expected.cloned().unwrap_or(CtTy::Scalar(CtScalarTy::Unit)),
            span,
        )?;
        self.op(CtInstructionKind::StorageLive(local), span)?;
        self.yields.push(Yield {
            end,
            local,
            ty: expected.cloned(),
            depth: self.scopes.len(),
        });
        self.scoped(body, depth)?;
        if !self.terminated() && self.blocks[self.current.0 as usize].reachable {
            return Err(error(span, "every normal CT block path must yield a value"));
        }
        let y = self.yields.pop().unwrap();
        let ty =
            y.ty.ok_or_else(|| error(span, "CT block has no yielded value"))?;
        self.current = end;
        let result = self.emit(CtInstructionKind::Load(local), ty, span)?;
        self.op(CtInstructionKind::StorageDead(local), span)?;
        Ok(result)
    }
    fn finish(mut self) -> R<CtFunction> {
        if !self.terminated() {
            if !self.blocks[self.current.0 as usize].reachable {
                self.terminate(CtTerminator::Trap, self.span)?;
            } else if self.result == CtTy::Scalar(CtScalarTy::Unit) {
                self.cleanup(0, self.span)?;
                self.terminate(CtTerminator::Return(None), self.span)?;
            } else {
                return Err(error(
                    self.span,
                    "CT function has a normal path without its declared result",
                ));
            }
        }
        let blocks = self.closed_blocks()?;
        Ok(CtFunction {
            id: self.id,
            definition: self.definition,
            parameters: self.parameters,
            parameter_types: self.parameter_types,
            result_type: self.result,
            locals: self.locals,
            values: self.values,
            blocks,
            entry: CtBlockId(0),
            span: self.span,
        })
    }
}

pub(super) fn function(
    c: &mut Compiler<'_>,
    id: CtFunctionId,
    declaration: usize,
) -> R<CtFunction> {
    let d = c.declarations[declaration];
    let (parameters, result) = c.header(declaration)?;
    let definition = c.symbol(d.module, d.item.span)?;
    let mut b = Builder::new(c, id, d.module, definition, result, d.function.span)?;
    for (parameter, ty) in d.function.params.iter().zip(parameters) {
        let local = b.bind(&parameter.name, ty.clone(), false, parameter.span)?;
        b.parameters.push(local);
        b.parameter_types.push(ty);
    }
    match &d.function.body {
        s::FunctionBody::Block(body) => b.block(body, 0)?,
        s::FunctionBody::Expression(expression) => {
            let expected = b.result.clone();
            let value = b.expr(expression, Some(&expected), 0)?;
            b.cleanup(0, expression.span)?;
            b.terminate(CtTerminator::Return(Some(value.id)), expression.span)?;
        }
        s::FunctionBody::Declaration => {
            return Err(error(d.function.span, "CT function body is absent"));
        }
    }
    b.finish()
}
pub(super) fn expression(
    c: &mut Compiler<'_>,
    id: CtFunctionId,
    module: usize,
    definition: CtSymbolId,
    expression: &s::Expr,
    expected: Option<CtTy>,
    parameters: &[(String, CtTy)],
) -> R<CtFunction> {
    if let Some(ty) = &expected {
        c.budget.ty(ty, 0, expression.span)?;
    }
    let mut b = Builder::new(
        c,
        id,
        module,
        definition,
        expected.clone().unwrap_or(CtTy::Scalar(CtScalarTy::Unit)),
        expression.span,
    )?;
    for (name, ty) in parameters {
        b.compiler.budget.ty(ty, 0, expression.span)?;
        let local = b.bind(name, ty.clone(), false, expression.span)?;
        b.parameters.push(local);
        b.parameter_types.push(ty.clone());
    }
    let value = b.expr(expression, expected.as_ref(), 0)?;
    b.result = value.ty;
    b.cleanup(0, expression.span)?;
    b.terminate(CtTerminator::Return(Some(value.id)), expression.span)?;
    b.finish()
}
impl Builder<'_, '_> {
    fn for_loop(
        &mut self,
        pattern: &s::Pattern,
        iterable: &s::Expr,
        body: &s::Block,
        depth: usize,
    ) -> R<()> {
        let span = iterable.span;
        let (initial, bound, source, element) = if let s::ExprKind::Binary {
            op: s::BinaryOp::Range,
            left,
            right,
        } = &iterable.kind
        {
            let initial = self.expr(left, None, depth + 1)?;
            let bound = self.expr(right, Some(&initial.ty), depth + 1)?;
            if !matches!(
                initial.ty,
                CtTy::Scalar(CtScalarTy::U8 | CtScalarTy::U16 | CtScalarTy::I32 | CtScalarTy::I64)
            ) {
                return Err(error(span, "CT range requires target integers"));
            }
            (initial, bound, None, None)
        } else {
            let value = self.expr(iterable, None, depth + 1)?;
            let element = match &value.ty {
                CtTy::Array { element, .. } | CtTy::List(element) => element.as_ref().clone(),
                _ => return Err(error(span, "CT for-loop requires a range, Array or List")),
            };
            let source = self.new_local(value.ty.clone(), span)?;
            self.op(CtInstructionKind::StorageLive(source), span)?;
            self.op(
                CtInstructionKind::Store {
                    local: source,
                    value: value.id,
                },
                span,
            )?;
            let bound = self.emit(
                CtInstructionKind::Length(value.id),
                CtTy::Scalar(CtScalarTy::I64),
                span,
            )?;
            let initial = self.integer(0, span)?;
            (initial, bound, Some((source, value.ty)), Some(element))
        };
        let ty = initial.ty.clone();
        let counter = self.new_local(ty.clone(), span)?;
        let limit = self.new_local(bound.ty, span)?;
        for local in [counter, limit] {
            self.op(CtInstructionKind::StorageLive(local), span)?;
        }
        self.op(
            CtInstructionKind::Store {
                local: counter,
                value: initial.id,
            },
            span,
        )?;
        self.op(
            CtInstructionKind::Store {
                local: limit,
                value: bound.id,
            },
            span,
        )?;
        let test = self.new_block(span)?;
        let work = self.new_block(span)?;
        let step = self.new_block(span)?;
        let end = self.new_block(span)?;
        self.terminate(CtTerminator::Jump(test), span)?;
        self.current = test;
        let index = self.emit(CtInstructionKind::Load(counter), ty.clone(), span)?;
        let bound = self.emit(CtInstructionKind::Load(limit), ty.clone(), span)?;
        let condition = self.emit(
            CtInstructionKind::Binary {
                op: BinaryOp::Less,
                left: index.id,
                right: bound.id,
            },
            CtTy::Scalar(CtScalarTy::Bool),
            span,
        )?;
        self.terminate(
            CtTerminator::Branch {
                condition: condition.id,
                then_block: work,
                else_block: end,
            },
            span,
        )?;
        self.current = work;
        let value = if let Some((source, source_ty)) = &source {
            let array = self.emit(CtInstructionKind::Load(*source), source_ty.clone(), span)?;
            self.emit(
                CtInstructionKind::Extract {
                    aggregate: array.id,
                    index: index.id,
                },
                element.unwrap(),
                span,
            )?
        } else {
            index
        };
        self.loops.push(Loop {
            stop: end,
            next: step,
            depth: self.scopes.len(),
        });
        self.scopes.push(Scope {
            bindings: Vec::new(),
        });
        self.pattern(pattern, value, false)?;
        self.block(body, depth + 1)?;
        if !self.terminated() {
            self.cleanup(self.scopes.len() - 1, span)?;
            self.terminate(CtTerminator::Jump(step), span)?;
        }
        self.scopes.pop();
        self.loops.pop();
        self.current = step;
        let index = self.emit(CtInstructionKind::Load(counter), ty.clone(), span)?;
        let one = match ty {
            CtTy::Scalar(CtScalarTy::U8) => Constant::U8(1),
            CtTy::Scalar(CtScalarTy::U16) => Constant::U16(1),
            CtTy::Scalar(CtScalarTy::I32) => Constant::I32(1),
            _ => Constant::I64(1),
        };
        let one = self.constant(CtConstant::Scalar(one), ty.clone(), span)?;
        let next = self.emit(
            CtInstructionKind::Binary {
                op: BinaryOp::Add,
                left: index.id,
                right: one.id,
            },
            ty,
            span,
        )?;
        self.op(
            CtInstructionKind::Store {
                local: counter,
                value: next.id,
            },
            span,
        )?;
        self.terminate(CtTerminator::Jump(test), span)?;
        self.current = end;
        for local in [limit, counter] {
            self.op(CtInstructionKind::StorageDead(local), span)?;
        }
        if let Some((source, _)) = source {
            self.op(CtInstructionKind::StorageDead(source), span)?;
        }
        Ok(())
    }
    fn match_statement(
        &mut self,
        expression: &s::Expr,
        arms: &[s::MatchArm],
        depth: usize,
    ) -> R<()> {
        let value = self.expr(expression, None, depth + 1)?;
        let span = expression.span;
        let end = self.new_block(span)?;
        let mut wildcard = false;
        let mut covered = Vec::new();
        for arm in arms {
            self.compiler.budget.depth(depth, arm.span)?;
            if wildcard {
                return Err(error(
                    arm.span,
                    "CT match arm follows an exhaustive wildcard",
                ));
            }
            let body = self.new_block(arm.span)?;
            let next = self.new_block(arm.span)?;
            let variant = match &arm.pattern.kind {
                s::PatternKind::Wildcard | s::PatternKind::Name(_) => {
                    wildcard = true;
                    self.terminate(CtTerminator::Jump(body), arm.span)?;
                    None
                }
                s::PatternKind::Constructor { path, .. } => {
                    let CtTy::Enum(schema) = value.ty else {
                        return Err(error(
                            arm.span,
                            "constructor pattern requires a CT enum value",
                        ));
                    };
                    let (_, decl) = self.compiler.enum_sources[schema.0 as usize];
                    let name = path
                        .last()
                        .ok_or_else(|| error(arm.span, "empty CT enum pattern path"))?;
                    let mut found = None;
                    for (index, variant) in decl.variants.iter().enumerate() {
                        self.compiler.budget.work(1, arm.span)?;
                        if variant.name == *name {
                            found = Some(index as u32);
                            break;
                        }
                    }
                    let tag = found.ok_or_else(|| error(arm.span, "CT enum variant is absent"))?;
                    self.compiler.budget.work(covered.len(), arm.span)?;
                    if covered.contains(&tag) {
                        return Err(error(arm.span, "duplicate CT enum match arm"));
                    }
                    self.compiler.budget.node(arm.span)?;
                    covered.push(tag);
                    let actual = self.emit(
                        CtInstructionKind::VariantTag(value.id),
                        CtTy::Scalar(CtScalarTy::I32),
                        arm.span,
                    )?;
                    let expected = self.constant(
                        CtConstant::Scalar(Constant::I32(tag as i32)),
                        CtTy::Scalar(CtScalarTy::I32),
                        arm.span,
                    )?;
                    let condition = self.emit(
                        CtInstructionKind::Binary {
                            op: BinaryOp::Equal,
                            left: actual.id,
                            right: expected.id,
                        },
                        CtTy::Scalar(CtScalarTy::Bool),
                        arm.span,
                    )?;
                    self.terminate(
                        CtTerminator::Branch {
                            condition: condition.id,
                            then_block: body,
                            else_block: next,
                        },
                        arm.span,
                    )?;
                    Some((schema, tag))
                }
                s::PatternKind::Literal(literal) => {
                    let expected = s::Expr {
                        kind: s::ExprKind::Literal(literal.clone()),
                        span: arm.pattern.span,
                    };
                    let expected = self.expr(&expected, Some(&value.ty), depth + 1)?;
                    let condition = self.emit(
                        CtInstructionKind::Binary {
                            op: BinaryOp::Equal,
                            left: value.id,
                            right: expected.id,
                        },
                        CtTy::Scalar(CtScalarTy::Bool),
                        arm.span,
                    )?;
                    self.terminate(
                        CtTerminator::Branch {
                            condition: condition.id,
                            then_block: body,
                            else_block: next,
                        },
                        arm.span,
                    )?;
                    None
                }
                _ => return Err(error(arm.span, "unsupported CT match pattern")),
            };
            self.current = body;
            self.scopes.push(Scope {
                bindings: Vec::new(),
            });
            if let Some((schema, tag)) = variant {
                let s::PatternKind::Constructor { arguments, .. } = &arm.pattern.kind else {
                    unreachable!()
                };
                let types = self.compiler.enums[schema.0 as usize].variants[tag as usize].clone();
                if arguments.len() != types.len() {
                    return Err(error(arm.span, "CT enum pattern field count differs"));
                }
                for (index, (pattern, ty)) in arguments.iter().zip(types).enumerate() {
                    let field = self.emit(
                        CtInstructionKind::VariantField {
                            value: value.id,
                            variant: tag,
                            field: index as u32,
                        },
                        ty,
                        pattern.span,
                    )?;
                    self.pattern(pattern, field, false)?;
                }
            } else if let s::PatternKind::Name(name) = &arm.pattern.kind {
                let local = self.bind(name, value.ty.clone(), false, arm.span)?;
                self.op(CtInstructionKind::StorageLive(local), arm.span)?;
                self.op(
                    CtInstructionKind::Store {
                        local,
                        value: value.id,
                    },
                    arm.span,
                )?;
            }
            match &arm.body {
                s::ArrowBody::Block(block) => self.block(block, depth + 1)?,
                s::ArrowBody::Expression(expression) => {
                    self.expr(expression, None, depth + 1)?;
                }
            }
            if !self.terminated() {
                self.cleanup(self.scopes.len() - 1, arm.span)?;
                self.terminate(CtTerminator::Jump(end), arm.span)?;
            }
            self.scopes.pop();
            self.current = next;
        }
        if !wildcard {
            match value.ty {
                CtTy::Enum(schema)
                    if covered.len() == self.compiler.enums[schema.0 as usize].variants.len() => {}
                _ => return Err(error(span, "CT match is not exhaustive")),
            }
        }
        self.terminate(CtTerminator::Trap, span)?;
        self.current = end;
        Ok(())
    }
}
