//! Bounded index of the actual immutable Core trees, including all branches.
//! This index grants no execution, body or declaration authority.
use super::*;
#[derive(Default)]
pub(super) struct CoreNodes<'a> {
    pub(super) expressions:BTreeMap<NodeId,&'a CoreExpr>,
    statements:BTreeMap<NodeId,&'a CoreStatement>,
    bindings:BTreeMap<NodeId,&'a CoreStatement>,
}
enum Work<'a> {
    Block(&'a CoreBlock),
    Statement(&'a CoreStatement),
    Expression(&'a CoreExpr),
    Pattern(&'a CorePattern),
    Template(&'a CoreQuoteTemplate),
    Tree(&'a CoreSyntaxTree, Option<usize>),
    Entry(&'a CoreSyntaxTreeEntry, Option<usize>),
    Argument(&'a CoreGenericArgument),
}

impl<'a> CoreNodes<'a> {
    pub(super) fn walk(&mut self, body: &'a CoreBlock, b: &mut Budget) -> Check<()> {
        let mut pending = vec![Work::Block(body)];
        let mut ids = BTreeSet::new();
        while let Some(work) = pending.pop() {
            b.spend(2)?;
            match work {
                Work::Block(block) => {
                    b.spend(block.statements.len())?;
                    pending.extend(block.statements.iter().rev().map(Work::Statement));
                }
                Work::Statement(statement) => {
                    if !ids.insert(statement.id) { return Err(bad("duplicate frozen Core body node identity")); }
                    self.statements.insert(statement.id, statement);
                    charge_statement(statement,b)?;
                    use CoreStatementKind::*;
                    match &statement.kind {
                        Binding { initializer, .. } => {
                            self.bindings.insert(statement.id, statement);
                            if let Some(value) = initializer { pending.push(Work::Expression(value)); }
                        }
                        TupleBinding { pattern, initializer, .. } => {
                            self.bindings.insert(statement.id, statement);
                            pending.push(Work::Expression(initializer)); pending.push(Work::Pattern(pattern));
                        }
                        Assign { target, value } | CompoundAssign { target, value, .. } => {
                            pending.push(Work::Expression(value)); pending.push(Work::Expression(target));
                        }
                        Expression(value) | Yield(value) => pending.push(Work::Expression(value)),
                        Return(value) => { if let Some(value) = value { pending.push(Work::Expression(value)); } }
                        If { condition, then_body, else_body } => {
                            if let Some(body) = else_body { pending.push(Work::Block(body)); }
                            pending.push(Work::Block(then_body)); pending.push(Work::Expression(condition));
                        }
                        IfLet { pattern, value, then_body, else_body } => {
                            self.bindings.insert(statement.id, statement);
                            if let Some(body) = else_body { pending.push(Work::Block(body)); }
                            pending.push(Work::Block(then_body)); pending.push(Work::Expression(value)); pending.push(Work::Pattern(pattern));
                        }
                        While { condition, body } => { pending.push(Work::Block(body)); pending.push(Work::Expression(condition)); }
                        WhileLet { pattern, value, body } => {
                            self.bindings.insert(statement.id, statement);
                            pending.push(Work::Block(body)); pending.push(Work::Expression(value)); pending.push(Work::Pattern(pattern));
                        }
                        ForRange { start, end, body, .. } => {
                            self.bindings.insert(statement.id, statement);
                            pending.push(Work::Block(body)); pending.push(Work::Expression(end)); pending.push(Work::Expression(start));
                        }
                        ForEach { pattern, iterator, body } => {
                            self.bindings.insert(statement.id, statement);
                            pending.push(Work::Block(body)); pending.push(Work::Expression(iterator)); pending.push(Work::Pattern(pattern));
                        }
                        Match { value, arms } => {
                            b.spend(arms.len().saturating_mul(2))?;
                            for arm in arms.iter().rev() { pending.push(Work::Block(&arm.body)); pending.push(Work::Pattern(&arm.pattern)); }
                            pending.push(Work::Expression(value));
                        }
                        Block(body) | Defer(body) | Unsafe(body) | Loop(body) => pending.push(Work::Block(body)),
                        With { value, body, .. } => {
                            self.bindings.insert(statement.id, statement);
                            pending.push(Work::Block(body)); pending.push(Work::Expression(value));
                        }
                        CompileTime(statement) => pending.push(Work::Statement(statement)),
                        Break | Continue => {}
                        Unsupported(_) => return Err(bad("original statement retains an unsupported operation placeholder")),
                    }
                }
                Work::Expression(expression) => {
                    if !ids.insert(expression.id) { return Err(bad("duplicate frozen Core body node identity")); }
                    self.expressions.insert(expression.id, expression);
                    charge_expression(expression,b)?;
                    use CoreExprKind::*;
                    match &expression.kind {
                        MakeDynBorrow { source, .. } | MakeDynOwned { source, .. } => pending.push(Work::Expression(source)),
                        CallDynSlot { callee, arguments, .. } | Call { callee, arguments } => {
                            b.spend(arguments.len())?;
                            pending.extend(arguments.iter().rev().map(Work::Expression)); pending.push(Work::Expression(callee));
                        }
                        Ascribed { value, .. } | NumericConvert { value, .. } | IntegerConvert { value, .. }
                        | CompileTime(value) | Try(value) | Spread(value) | Group(value) | Splice(value) => pending.push(Work::Expression(value)),
                        Unary { operand, .. } => pending.push(Work::Expression(operand)),
                        Binary { left, right, .. } | Range { start: left, end: right } | Pipeline { input: left, target: right } => {
                            pending.push(Work::Expression(right)); pending.push(Work::Expression(left));
                        }
                        TypeApply { callee, .. } => pending.push(Work::Expression(callee)),
                        GenericApply { callee, arguments } => {
                            b.spend(arguments.len())?;
                            pending.extend(arguments.iter().rev().map(Work::Argument)); pending.push(Work::Expression(callee));
                        }
                        Tuple(values) | Array(values) => { b.spend(values.len())?; pending.extend(values.iter().rev().map(Work::Expression)); }
                        Index { object, index } => { pending.push(Work::Expression(index)); pending.push(Work::Expression(object)); }
                        Slice { object, start, end } => {
                            if let Some(value) = end { pending.push(Work::Expression(value)); }
                            if let Some(value) = start { pending.push(Work::Expression(value)); }
                            pending.push(Work::Expression(object));
                        }
                        Member { object, .. } => pending.push(Work::Expression(object)),
                        Record { fields, .. } | RecordConstructor { fields, .. } => {
                            b.spend(fields.len())?; pending.extend(fields.iter().rev().map(|(_, value)| Work::Expression(value)));
                        }
                        RecordUpdate { base, fields, .. } => {
                            b.spend(fields.len())?; pending.extend(fields.iter().rev().map(|(_, value)| Work::Expression(value))); pending.push(Work::Expression(base));
                        }
                        Sequence { bindings, value } => {
                            b.spend(bindings.len())?; pending.push(Work::Expression(value)); pending.extend(bindings.iter().rev().map(|(_, value)| Work::Expression(value)));
                        }
                        // A separately indexed child definition owns its body,
                        // parameters and local bindings. The complete raw body
                        // was still traversed by the bounded metadata prepay.
                        Closure { parameters,body,.. } => {for parameter in parameters {if let Some(ty)=&parameter.ty {dynamic_metadata::ty(ty,b)?;}}pending.push(Work::Block(body));}
                        If { condition, then_value, else_value } => {
                            pending.push(Work::Expression(else_value)); pending.push(Work::Expression(then_value)); pending.push(Work::Expression(condition));
                        }
                        CompileTimeBlock(body) => pending.push(Work::Block(body)),
                        SyntaxQuote { template, .. } => pending.push(Work::Template(template)),
                        SyntaxIsland { tree, .. } => pending.push(Work::Tree(tree, None)),
                        Literal(_) | Integer(_) | Float(_) | Bool(_) | String(_) | Unit | Name(_) | ReceiverMember(_) | Character(_) => {}
                        Unsupported(_) => return Err(bad("original expression retains an unsupported operation placeholder")),
                    }
                }
                Work::Pattern(pattern) => {
                    use CorePatternKind::*;
                    match &pattern.kind {
                        Tuple(patterns) | Variant { fields: patterns, .. } | Constructor { fields: patterns, .. } => {
                            b.spend(patterns.len())?; pending.extend(patterns.iter().rev().map(Work::Pattern));
                        }
                        Record { fields, .. } | RecordConstructor { fields, .. } => {
                            b.spend(fields.len())?; pending.extend(fields.iter().rev().map(|(_, pattern)| Work::Pattern(pattern)));
                        }
                        Reference { pattern, .. } | ErrorArm { pattern, .. } => pending.push(Work::Pattern(pattern)),
                        Name(_) | Wildcard | Integer(_) | Bool(_) | Float(_) | String(_) | Character(_) => {}
                    }
                }
                Work::Template(template) => {
                    b.spend(template.holes.len().saturating_add(template.name_sites.len()))?;
                    for hole in template.holes.iter().rev() { pending.push(Work::Expression(&hole.value)); }
                    pending.push(Work::Tree(&template.tree, Some(template.holes.len())));
                }
                Work::Tree(tree, holes) => {
                    b.spend(tree.children.len())?; pending.extend(tree.children.iter().rev().map(|entry| Work::Entry(entry, holes)));
                }
                Work::Entry(entry, holes) => match entry {
                    CoreSyntaxTreeEntry::Token { .. } => {}
                    CoreSyntaxTreeEntry::Tree(tree) => pending.push(Work::Tree(tree, holes)),
                    CoreSyntaxTreeEntry::Hole { index, .. } => {
                        if holes.is_none_or(|count| *index as usize >= count) { return Err(bad("original syntax hole has no actual template value")); }
                    }
                },
                Work::Argument(argument) => match argument {
                    CoreGenericArgument::Const(value) => pending.push(Work::Expression(value)),
                    CoreGenericArgument::Type(ty) => dynamic_metadata::ty(ty,b)?,
                    CoreGenericArgument::Lifetime(name) => b.spend(name.len())?,
                },
            }
        }
        Ok(())
    }

}
fn charge_statement(statement:&CoreStatement,b:&mut Budget)->Check<()> {
    match &statement.kind {
        CoreStatementKind::Binding{name,ty,..}=>{b.spend(name.len())?;if let Some(ty)=ty{dynamic_metadata::ty(ty,b)?;}},
        CoreStatementKind::ForRange{name,..}|CoreStatementKind::With{name,..}=>b.spend(name.len())?,
        _=>{},
    }Ok(())
}
fn charge_expression(expression:&CoreExpr,b:&mut Budget)->Check<()> {
    match &expression.kind {
        CoreExprKind::Ascribed{ty,..}|CoreExprKind::NumericConvert{target:ty,..}|CoreExprKind::IntegerConvert{target:ty,..}|CoreExprKind::Record{ty,..}=>dynamic_metadata::ty(ty,b)?,
        CoreExprKind::TypeApply{type_arguments,..}=>for ty in type_arguments{dynamic_metadata::ty(ty,b)?;},
        CoreExprKind::Member{member,owner,..}=>{b.spend(member.len())?;if let Some(ty)=owner{dynamic_metadata::ty(ty,b)?;}},
        CoreExprKind::RecordUpdate{ty:Some(ty),..}=>dynamic_metadata::ty(ty,b)?,
        CoreExprKind::Integer(text)|CoreExprKind::Float(text)|CoreExprKind::String(text)|CoreExprKind::Name(text)|CoreExprKind::ReceiverMember(text)|CoreExprKind::Unsupported(text)=>b.spend(text.len())?,
        CoreExprKind::Literal(Constant::WideString(units))=>b.spend(units.len())?,
        _=>{},
    }Ok(())
}
