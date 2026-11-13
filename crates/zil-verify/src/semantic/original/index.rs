//! Bounded structural indices for the complete original operation grammar.
//! These cursors prove neither types nor the fidelity of an actual instruction.
use super::*;

pub(super) struct Index<'a> {
    pub(super) definitions: BTreeMap<SemanticDefinitionId, Function<'a>>,
}

#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Expression(&'a SemanticOriginalExpr),
    Statement(&'a SemanticOriginalStatement),
}

pub(super) struct Function<'a> {
    pub(super) definition: &'a SemanticDefinition,
    pub(super) signature: &'a SemanticSignatureRecipe,
    pub(super) sites: &'a [SemanticSiteRecipe],
    pub(super) parameters: &'a [SemanticOriginalClosureParameter],
    pub(super) receiver_parameter: Option<u32>,
    pub(super) site_nodes: &'a [SemanticOriginalSiteNode],
    pub(super) body: &'a SemanticOriginalFunctionBody,
    pub(super) expressions: BTreeMap<NodeId, &'a SemanticOriginalExpr>,
    pub(super) statements: BTreeMap<NodeId, &'a SemanticOriginalStatement>,
    /// The statement is the original binder occurrence, not a Typed LocalId.
    /// Pattern projections and parameter ordinals remain separate identities.
    pub(super) bindings: BTreeMap<NodeId, &'a SemanticOriginalStatement>,
    pub(super) nodes: BTreeMap<SemanticSiteId, Node<'a>>,
    pub(super) closures: BTreeMap<NodeId, SemanticDefinitionId>,
}

pub(super) fn build<'a>(module: &'a TypedModule, b: &mut Budget) -> Check<Index<'a>> {
    let program = module.semantic.as_ref().ok_or_else(|| bad("original body inventory is absent"))?;
    // Includes annotations, constants, quote payloads, nested closure bodies and
    // every allocation-bearing row, before recursive comparisons below.
    dynamic_metadata::recipe(program, b)?;
    let mut result = Index { definitions: BTreeMap::new() };
    let mut ids = BTreeSet::new();
    for definition in &program.definitions {
        b.spend(2)?;
        if !ids.insert(definition.id) {
            return Err(bad("duplicate original definition identity in body index"));
        }
        let (signature, sites, parameters, receiver_parameter, site_nodes, body) = match &definition.body {
            SemanticDefinitionBody::Function { .. } => return Err(bad("original function inventory omits its complete body")),
            SemanticDefinitionBody::FunctionWithBody { signature, sites, parameters, receiver_parameter, site_nodes, body, .. } =>
                (signature, sites.as_slice(), parameters.as_slice(), *receiver_parameter, site_nodes.as_slice(), body),
            SemanticDefinitionBody::Nominal { .. } | SemanticDefinitionBody::Scope(_) => {
                if matches!(definition.kind, SemanticDefinitionKind::Function | SemanticDefinitionKind::Method | SemanticDefinitionKind::Closure | SemanticDefinitionKind::TaskEntry) {
                    return Err(bad("original callable definition has a nonfunction body"));
                }
                continue;
            }
        };
        if !matches!(definition.kind, SemanticDefinitionKind::Function | SemanticDefinitionKind::Method | SemanticDefinitionKind::Closure | SemanticDefinitionKind::TaskEntry) {
            return Err(bad("original function body has a noncallable definition kind"));
        }
        if receiver_parameter.is_some_and(|ordinal| ordinal as usize >= parameters.len()) {
            return Err(bad("original receiver selects no actual parameter ordinal"));
        }
        b.spend(parameters.len().saturating_add(1))?;
        let mut function = Function {
            definition, signature, sites, parameters, receiver_parameter, site_nodes, body,
            expressions: BTreeMap::new(), statements: BTreeMap::new(), bindings: BTreeMap::new(),
            nodes: BTreeMap::new(), closures: BTreeMap::new(),
        };
        if let SemanticOriginalFunctionBody::Defined(body) = body {
            function.walk(body, b)?;
        }
        function.bind_sites(b)?;
        result.definitions.insert(definition.id, function);
    }
    // A closure's source body belongs to its exact child definition. No
    // first matching name, span, layout or concrete instance selects a child.
    let mut children = BTreeSet::new();
    for function in result.definitions.values() {
        for (&node, &child_id) in &function.closures {
            b.spend(result.definitions.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
            let child = result.definitions.get(&child_id).ok_or_else(|| bad("original closure site has no child function body"))?;
            if child.definition.kind != SemanticDefinitionKind::Closure || child.definition.parent != Some(function.definition.id) || !children.insert(child_id) {
                return Err(bad("original closure site does not uniquely name its real child definition"));
            }
            let expression = function.expressions.get(&node).ok_or_else(|| bad("original closure node is absent"))?;
            let SemanticOriginalExprKind::Closure { parameters, body, .. } = &expression.kind else {
                return Err(bad("original closure site selects another operation"));
            };
            if child.parameters != parameters || child.receiver_parameter.is_some() || !matches!(child.body, SemanticOriginalFunctionBody::Defined(actual) if actual == body) {
                return Err(bad("original closure child changes its parameters or complete source body"));
            }
        }
    }
    for function in result.definitions.values() {
        b.spend(1)?;
        if function.definition.kind == SemanticDefinitionKind::Closure && !children.contains(&function.definition.id) {
            return Err(bad("original closure definition has no actual parent closure occurrence"));
        }
    }
    Ok(result)
}

enum Work<'a> {
    Block(&'a SemanticOriginalBlock),
    Statement(&'a SemanticOriginalStatement),
    Expression(&'a SemanticOriginalExpr),
    Pattern(&'a SemanticOriginalPattern),
    Template(&'a SemanticOriginalQuoteTemplate),
    Tree(&'a SemanticOriginalSyntaxTree, Option<usize>),
    Entry(&'a SemanticOriginalSyntaxTreeEntry, Option<usize>),
    Argument(&'a SemanticOriginalGenericArgument),
}

impl<'a> Function<'a> {
    fn walk(&mut self, body: &'a SemanticOriginalBlock, b: &mut Budget) -> Check<()> {
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
                    if !ids.insert(statement.id) { return Err(bad("duplicate original body node identity")); }
                    self.statements.insert(statement.id, statement);
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
                    if !ids.insert(expression.id) { return Err(bad("duplicate original body node identity")); }
                    self.expressions.insert(expression.id, expression);
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
                        Closure { .. } => {}
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
                    SemanticOriginalSyntaxTreeEntry::Token { .. } => {}
                    SemanticOriginalSyntaxTreeEntry::Tree(tree) => pending.push(Work::Tree(tree, holes)),
                    SemanticOriginalSyntaxTreeEntry::Hole { index, .. } => {
                        if holes.is_none_or(|count| *index as usize >= count) { return Err(bad("original syntax hole has no actual template value")); }
                    }
                },
                Work::Argument(argument) => match argument {
                    SemanticOriginalGenericArgument::Const(value) => pending.push(Work::Expression(value)),
                    SemanticOriginalGenericArgument::Type(_) | SemanticOriginalGenericArgument::Lifetime(_) => {}
                },
            }
        }
        Ok(())
    }

    fn bind_sites(&mut self, b: &mut Budget) -> Check<()> {
        if self.sites.len() != self.site_nodes.len() { return Err(bad("original site directory omits or adds a node correspondence")); }
        b.spend(self.sites.len().saturating_add(self.site_nodes.len()))?;
        let mut recipes = BTreeMap::new();
        let mut paths = BTreeSet::new();
        for site in self.sites {
            b.spend(site.occurrence_path.len().saturating_add(1))?;
            if recipes.insert(site.id, site).is_some() || !paths.insert(site.occurrence_path.as_slice()) {
                return Err(bad("duplicate original site recipe identity or source occurrence"));
            }
        }
        for entry in self.site_nodes {
            let recipe = recipes.get(&entry.site).ok_or_else(|| bad("original site node selects no recipe"))?;
            let node = match entry.node {
                SemanticOriginalNode::Expression(id) => Node::Expression(*self.expressions.get(&id).ok_or_else(|| bad("original site selects no expression in its own lexical body"))?),
                SemanticOriginalNode::Statement(id) => Node::Statement(*self.statements.get(&id).ok_or_else(|| bad("original site selects no statement in its own lexical body"))?),
            };
            if !compatible(&recipe.kind, node) { return Err(bad("original site kind disagrees with its exact operation")); }
            if self.nodes.insert(entry.site, node).is_some() { return Err(bad("duplicate original site node correspondence")); }
            if let (SemanticSiteRecipeKind::Closure { definition, .. }, Node::Expression(expression)) = (&recipe.kind, node) {
                if self.closures.insert(expression.id, *definition).is_some() { return Err(bad("one original closure occurrence has multiple child definitions")); }
            }
        }
        for expression in self.expressions.values() {
            b.spend(1)?;
            if matches!(expression.kind, SemanticOriginalExprKind::Closure { .. }) && !self.closures.contains_key(&expression.id) {
                return Err(bad("original closure occurrence omits its real child definition site"));
            }
        }
        Ok(())
    }
}

fn compatible(recipe: &SemanticSiteRecipeKind, node: Node<'_>) -> bool {
    match (recipe, node) {
        (SemanticSiteRecipeKind::TypeUse { .. }, Node::Expression(_)) => true,
        (SemanticSiteRecipeKind::DeclareLocal { .. }, Node::Statement(statement)) => matches!(statement.kind, SemanticOriginalStatementKind::Binding { .. } | SemanticOriginalStatementKind::TupleBinding { .. }),
        (SemanticSiteRecipeKind::Closure { .. }, Node::Expression(expression)) => matches!(expression.kind, SemanticOriginalExprKind::Closure { .. }),
        (SemanticSiteRecipeKind::Call { .. } | SemanticSiteRecipeKind::CallableCall { .. } | SemanticSiteRecipeKind::OriginalCall, Node::Expression(expression)) => matches!(expression.kind, SemanticOriginalExprKind::Call { .. } | SemanticOriginalExprKind::CallDynSlot { .. } | SemanticOriginalExprKind::Pipeline { .. }),
        (SemanticSiteRecipeKind::Construct { .. }, Node::Expression(expression)) => matches!(expression.kind, SemanticOriginalExprKind::Tuple(_) | SemanticOriginalExprKind::Array(_) | SemanticOriginalExprKind::Record { .. } | SemanticOriginalExprKind::RecordUpdate { .. } | SemanticOriginalExprKind::RecordConstructor { .. } | SemanticOriginalExprKind::MakeDynOwned { .. } | SemanticOriginalExprKind::Call { .. } | SemanticOriginalExprKind::Literal(_)),
        _ => false,
    }
}
