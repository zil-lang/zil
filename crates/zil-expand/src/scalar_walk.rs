//! Current-request scalar body views. Normalize borrows the real parser tree;
//! only renamed expressions are owned. Its complete walk also observes the
//! structural Restore demand. No observation or view survives this request.
use super::*;
use crate::incremental::{Reuse, Stage};
use zil_syntax::ParsedSource;

pub(crate) struct BodyView {
    parsed: ParsedSource,
    ordinal: usize,
    pub(crate) name: String,
    // Actual expression identity, not Span equality. Two distinct parser nodes
    // may have equal spans; they must never select each other's replacement.
    // The strong parser owner fixes these addresses for the entire request.
    pub(crate) expressions: Arc<BTreeMap<usize, Expr>>,
}
impl BodyView {
    pub(crate) fn item(&self) -> &Item {
        &self.parsed.parsed().module.items[self.ordinal]
    }
    pub(crate) fn replacement(&self, expression: &Expr) -> Option<&Expr> {
        self.expressions
            .get(&(std::ptr::from_ref(expression) as usize))
    }
    pub(crate) fn materialize(&self) -> Item {
        // Materialization is an explicit compatibility operation. Copy from
        // original syntax with a simultaneous immutable-source walk below;
        // cloned node addresses cannot address the current view's table.
        struct Copy<'a>(&'a BodyView);
        impl Copy<'_> {
            fn block(&self, source: &Block, target: &mut Block) {
                for (a, b) in source.statements.iter().zip(&mut target.statements) {
                    self.statement(a, b);
                }
            }
            fn statement(&self, source: &Stmt, target: &mut Stmt) {
                match (&source.kind, &mut target.kind) {
                    (StmtKind::Binding { value: a, .. }, StmtKind::Binding { value: b, .. })
                    | (StmtKind::Return(Some(a)), StmtKind::Return(Some(b)))
                    | (StmtKind::Expression(a), StmtKind::Expression(b)) => self.expression(a, b),
                    (
                        StmtKind::Assign {
                            target: a,
                            value: b,
                            ..
                        },
                        StmtKind::Assign {
                            target: c,
                            value: d,
                            ..
                        },
                    ) => {
                        self.expression(a, c);
                        self.expression(b, d);
                    }
                    (
                        StmtKind::If {
                            condition: Condition::Expression(a),
                            then_body: b,
                            else_body: c,
                        },
                        StmtKind::If {
                            condition: Condition::Expression(d),
                            then_body: e,
                            else_body: f,
                        },
                    ) => {
                        self.expression(a, d);
                        self.block(b, e);
                        match (c, f) {
                            (Some(ElseBranch::Block(a)), Some(ElseBranch::Block(b))) => {
                                self.block(a, b)
                            }
                            (Some(ElseBranch::If(a)), Some(ElseBranch::If(b))) => {
                                self.statement(a, b)
                            }
                            _ => {}
                        }
                    }
                    (
                        StmtKind::While {
                            condition: Condition::Expression(a),
                            body: b,
                        },
                        StmtKind::While {
                            condition: Condition::Expression(c),
                            body: d,
                        },
                    ) => {
                        self.expression(a, c);
                        self.block(b, d);
                    }
                    (StmtKind::Loop(a), StmtKind::Loop(b))
                    | (StmtKind::Unsafe(a), StmtKind::Unsafe(b)) => self.block(a, b),
                    _ => {}
                }
            }
            fn expression(&self, source: &Expr, target: &mut Expr) {
                if let Some(replacement) = self.0.replacement(source) {
                    *target = replacement.clone();
                    return;
                }
                match (&source.kind, &mut target.kind) {
                    (ExprKind::Group(a), ExprKind::Group(b))
                    | (ExprKind::Unary { operand: a, .. }, ExprKind::Unary { operand: b, .. }) => {
                        self.expression(a, b)
                    }
                    (
                        ExprKind::Binary {
                            left: a, right: b, ..
                        },
                        ExprKind::Binary {
                            left: c, right: d, ..
                        },
                    ) => {
                        self.expression(a, c);
                        self.expression(b, d);
                    }
                    (
                        ExprKind::Pipeline {
                            input: a,
                            target: b,
                        },
                        ExprKind::Pipeline {
                            input: c,
                            target: d,
                        },
                    ) => {
                        self.expression(a, c);
                        self.expression(b, d);
                    }
                    (ExprKind::Member { object: a, .. }, ExprKind::Member { object: b, .. }) => {
                        self.expression(a, b)
                    }
                    (
                        ExprKind::Call {
                            callee: a,
                            arguments: b,
                        },
                        ExprKind::Call {
                            callee: c,
                            arguments: d,
                        },
                    ) => {
                        self.expression(a, c);
                        for (a, b) in b.iter().zip(d) {
                            self.expression(&a.value, &mut b.value);
                        }
                    }
                    (
                        ExprKind::If {
                            condition: a,
                            then_value: b,
                            else_value: c,
                        },
                        ExprKind::If {
                            condition: d,
                            then_value: e,
                            else_value: f,
                        },
                    ) => {
                        self.expression(a, d);
                        self.expression(b, e);
                        self.expression(c, f);
                    }
                    _ => {}
                }
            }
        }
        let mut item = self.item().clone();
        let source = match &self.item().kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f,
            _ => unreachable!("scalar view"),
        };
        let target = match &mut item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f,
            _ => unreachable!("current scalar declaration"),
        };
        target.name.clone_from(&self.name);
        match (&source.body, &mut target.body) {
            (FunctionBody::Block(a), FunctionBody::Block(b)) => Copy(self).block(a, b),
            (FunctionBody::Expression(a), FunctionBody::Expression(b)) => {
                Copy(self).expression(a, b)
            }
            _ => {}
        }
        item
    }
}
pub(crate) struct ModuleView {
    pub(crate) id: ModuleId,
    pub(crate) span: Span,
    pub(crate) items: Vec<Arc<BodyView>>,
    pub(crate) imports: Vec<CoreImport>,
}

struct Reference {
    key: usize,
    expression: Expr,
    depth: usize,
}
struct RestoreDemand {
    nodes: usize,
    work: usize,
    depth: usize,
    references: Vec<Reference>,
}
// The ordinary scalar Budget has no temporary-byte ledger or feature effects.
// All scope copies and structural nodes have identical charges in Normalize
// and Restore. Only canonical-name text is removed from this observed demand;
// Restore's actual declaration/reference operations supply its replacement.
struct Walk<'a, 'b, 'c, 'l> {
    visitor: &'a mut Visitor<'b, 'c, 'l>,
    input: Option<&'a BodyView>,
    expressions: BTreeMap<usize, Expr>,
    canonical_bytes: usize,
    depth: usize,
    references: Vec<Reference>,
}
impl Walk<'_, '_, '_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> R<()> {
        self.visitor.budget.node(span, depth)?;
        self.depth = self.depth.max(depth);
        Ok(())
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
        self.node(ty.span, depth)?;
        match &ty.kind {
            TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                // Inventory rejects declarations named after primitives. In
                // the admitted domain this lookup cannot rewrite a type.
                debug_assert!(
                    integer_name(&path[0]) || matches!(path[0].as_str(), "bool" | "void")
                );
                debug_assert!(match &self.visitor.mode {
                    Mode::Normalize(index) =>
                        !index.symbols[&self.visitor.module].contains_key(&path[0]),
                    _ => true,
                });
            }
            TypeKind::Group(inner) => self.ty(inner, depth + 1)?,
            _ => unreachable!("complete scalar admission precedes borrowed normalization"),
        }
        self.visitor.scalar_output(ty.span, || true)
    }
    fn item(&mut self, item: &Item) -> R<String> {
        self.node(item.span, 0)?;
        let types = self.visitor.names(&Names::new(), item.span)?;
        let f = match &item.kind {
            ItemKind::Function(f) | ItemKind::Extern { function: f, .. } => f,
            _ => unreachable!("scalar body view"),
        };
        let mut name = self
            .input
            .map_or_else(|| f.name.clone(), |view| view.name.clone());
        self.visitor.declaration_name(&mut name, true, item.span)?;
        if matches!(self.visitor.mode, Mode::Normalize(_)) {
            self.canonical_bytes += name.len();
        }
        let mut values = Names::new();
        for parameter in &f.params {
            self.ty(&parameter.ty, 1)?;
            values.insert(parameter.name.clone());
        }
        if let Some(result) = &f.return_type {
            self.ty(result, 1)?;
        }
        match &f.body {
            FunctionBody::Block(block) => self.block(block, &types, &values, 1)?,
            FunctionBody::Expression(expression) => self.expr(expression, &types, &values, 1)?,
            FunctionBody::Declaration => {}
        }
        self.visitor.scalar_output(item.span, || {
            crate::incremental::scalar_admission::item_shape(item)
        })?;
        Ok(name)
    }
    fn pattern(&mut self, pattern: &Pattern, values: &mut Names, depth: usize) -> R<()> {
        self.node(pattern.span, depth)?;
        match &pattern.kind {
            PatternKind::Name(name) => {
                values.insert(name.clone());
            }
            PatternKind::Wildcard => {}
            _ => unreachable!("scalar binding pattern"),
        }
        self.visitor.scalar_output(pattern.span, || true)
    }
    fn block(&mut self, block: &Block, types: &Names, values: &Names, depth: usize) -> R<()> {
        self.node(block.span, depth)?;
        let mut scope = self.visitor.names(values, block.span)?;
        for statement in &block.statements {
            self.statement(statement, types, &mut scope, depth + 1)?;
        }
        self.visitor.scalar_output(block.span, || true)
    }
    fn condition(
        &mut self,
        condition: &Condition,
        types: &Names,
        values: &Names,
        depth: usize,
    ) -> R<()> {
        let Condition::Expression(expression) = condition else {
            unreachable!("scalar condition")
        };
        self.expr(expression, types, values, depth)
    }
    fn statement(
        &mut self,
        statement: &Stmt,
        types: &Names,
        values: &mut Names,
        depth: usize,
    ) -> R<()> {
        self.node(statement.span, depth)?;
        match &statement.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(ty) = ty {
                    self.ty(ty, depth + 1)?;
                }
                self.expr(value, types, values, depth + 1)?;
                self.pattern(pattern, values, depth + 1)?;
            }
            StmtKind::Return(Some(expression)) | StmtKind::Expression(expression) => {
                self.expr(expression, types, values, depth + 1)?
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, types, values, depth + 1)?;
                self.expr(value, types, values, depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let success = self.visitor.names(values, statement.span)?;
                self.condition(condition, types, &success, depth + 1)?;
                self.block(then_body, types, &success, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(block)) => {
                        self.block(block, types, values, depth + 1)?
                    }
                    Some(ElseBranch::If(statement)) => {
                        let mut scope = self.visitor.names(values, statement.span)?;
                        self.statement(statement, types, &mut scope, depth + 1)?;
                    }
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let success = self.visitor.names(values, statement.span)?;
                self.condition(condition, types, &success, depth + 1)?;
                self.block(body, types, &success, depth + 1)?;
            }
            StmtKind::Loop(block) | StmtKind::Unsafe(block) => {
                self.block(block, types, values, depth + 1)?
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            _ => unreachable!("scalar statement"),
        }
        self.visitor.scalar_output(statement.span, || {
            crate::incremental::scalar_admission::statement_shape(statement)
        })
    }
    fn expr(&mut self, original: &Expr, types: &Names, values: &Names, depth: usize) -> R<()> {
        let expression = self
            .input
            .and_then(|view| view.replacement(original))
            .unwrap_or(original);
        self.node(expression.span, depth)?;
        // expr_name can replace only these small leaves in the scalar domain.
        // In particular, never clone a Call, branch, block or expression tree.
        if matches!(&expression.kind, ExprKind::Name(_))
            || matches!(&expression.kind, ExprKind::Member { object, member: MemberName::Name(_) } if matches!(object.kind, ExprKind::Name(_)))
        {
            let mut leaf = expression.clone();
            if self.visitor.expr_name(&mut leaf, types, values, depth)? {
                if matches!(self.visitor.mode, Mode::Normalize(_)) {
                    let ExprKind::Name(name) = &leaf.kind else {
                        unreachable!("normalized scalar symbol")
                    };
                    self.canonical_bytes += name.len();
                    self.references.push(Reference {
                        key: std::ptr::from_ref(original) as usize,
                        expression: leaf.clone(),
                        depth,
                    });
                }
                self.visitor.scalar_output(leaf.span, || {
                    crate::incremental::scalar_admission::expression_shape(&leaf)
                })?;
                let prior = self
                    .expressions
                    .insert(std::ptr::from_ref(original) as usize, leaf.clone());
                debug_assert!(prior.as_ref().is_none_or(|prior| prior == &leaf));
                return Ok(());
            }
        }
        match &expression.kind {
            ExprKind::Group(inner) | ExprKind::Unary { operand: inner, .. } => {
                self.expr(inner, types, values, depth + 1)?
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, types, values, depth + 1)?;
                self.expr(right, types, values, depth + 1)?;
            }
            ExprKind::Pipeline { input, target } => {
                self.expr(input, types, values, depth + 1)?;
                self.expr(target, types, values, depth + 1)?;
            }
            ExprKind::Member { object, member } => {
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner, depth + 1)?;
                }
                self.expr(object, types, values, depth + 1)?;
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee, types, values, depth + 1)?;
                for argument in arguments {
                    self.expr(&argument.value, types, values, depth + 1)?;
                }
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, types, values, depth + 1)?;
                self.expr(then_value, types, values, depth + 1)?;
                self.expr(else_value, types, values, depth + 1)?;
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
            _ => unreachable!("scalar expression"),
        }
        self.visitor.scalar_output(expression.span, || {
            crate::incremental::scalar_admission::expression_shape(expression)
        })
    }
}

fn restore(
    view: &BodyView,
    demand: RestoreDemand,
    functions: &Functions,
    edges: &mut Vec<CoreImport>,
    budget: &mut Budget<'_>,
) -> R<(BodyView, bool)> {
    let before = budget.remaining_limits();
    let mut allowance = before.clone();
    let fits = demand.depth <= allowance.max_call_depth
        && allowance.max_nodes >= demand.nodes
        && allowance.max_analysis_steps >= demand.work;
    if fits {
        allowance.max_nodes -= demand.nodes;
        allowance.max_analysis_steps -= demand.work;
        let mut trial = Budget::new(&allowance);
        trial.target_pointer_width = budget.target_pointer_width;
        let mut next_edges = edges.clone();
        let mut visitor = Visitor {
            scalar_file: None,
            default_nominal: None,
            const_scope: Names::new(),
            mode: Mode::Restore {
                functions,
                edges: &mut next_edges,
            },
            module: view.parsed.source().module_id,
            budget: &mut trial,
        };
        let mut name = view.name.clone();
        let mut result = visitor.declaration_name(&mut name, true, view.item().span);
        let mut expressions = BTreeMap::new();
        for reference in demand.references {
            if result.is_err() {
                break;
            }
            let mut leaf = reference.expression;
            result = visitor
                .expr_name(&mut leaf, &Names::new(), &Names::new(), reference.depth)
                .map(|_| ());
            let prior = expressions.insert(reference.key, leaf.clone());
            debug_assert!(prior.as_ref().is_none_or(|prior| prior == &leaf));
        }
        if result.is_ok() {
            budget.adopt_remaining(&trial.remaining_limits());
            *edges = next_edges;
            return Ok((
                BodyView {
                    parsed: view.parsed.clone(),
                    ordinal: view.ordinal,
                    name,
                    expressions: Arc::new(expressions),
                },
                true,
            ));
        }
    }
    // A failed aggregate debit cannot choose a diagnostic or spend anything.
    // Re-enter the actual ordered borrowed walk with the original current
    // allowance and import list, including failures between two references.
    let mut visitor = Visitor {
        scalar_file: None,
        default_nominal: None,
        const_scope: Names::new(),
        mode: Mode::Restore { functions, edges },
        module: view.parsed.source().module_id,
        budget,
    };
    let mut walk = Walk {
        visitor: &mut visitor,
        input: Some(view),
        expressions: BTreeMap::new(),
        canonical_bytes: 0,
        depth: 0,
        references: Vec::new(),
    };
    let name = walk.item(view.item())?;
    Ok((
        BodyView {
            parsed: view.parsed.clone(),
            ordinal: view.ordinal,
            name,
            expressions: Arc::new(walk.expressions),
        },
        false,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn normalize(
    modules: &[&Module],
    owners: &[ParsedSource],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
    products: &mut generics::products::Products,
    index: &Index,
    functions: &Functions,
    mut budget: Budget<'_>,
    reuse: &mut Reuse<'_>,
) -> R<Option<SpecializedModules>> {
    use scalar_products::NormalizedProduct;
    let mut normalized = Vec::new();
    let mut demands = Vec::new();
    for (module, parsed) in modules.iter().zip(owners) {
        for (ordinal, item) in module.items.iter().enumerate() {
            if matches!(item.kind, ItemKind::Import(_)) {
                continue;
            }
            let before = budget.remaining_limits();
            let clock = reuse.clock();
            let mut visitor = Visitor {
                scalar_file: Some(module.span.file),
                default_nominal: None,
                const_scope: Names::new(),
                mode: Mode::Normalize(index),
                module: module.id,
                budget: &mut budget,
            };
            let mut walk = Walk {
                visitor: &mut visitor,
                input: None,
                expressions: BTreeMap::new(),
                canonical_bytes: 0,
                depth: 0,
                references: Vec::new(),
            };
            let name = walk.item(item)?;
            let after = walk.visitor.budget.remaining_limits();
            let demand = RestoreDemand {
                nodes: before.max_nodes - after.max_nodes,
                work: before.max_analysis_steps - after.max_analysis_steps - walk.canonical_bytes,
                depth: walk.depth,
                references: walk.references,
            };
            let view = Arc::new(BodyView {
                parsed: parsed.clone(),
                ordinal,
                name,
                expressions: Arc::new(walk.expressions),
            });
            reuse.production(clock);
            reuse.borrowed_scalar_item();
            reuse.record(Stage::Normalize, item.span, before, &budget);
            let declaration = match item.kind {
                ItemKind::Function(_) => scalar_products::Declaration::Function(view.name.clone()),
                ItemKind::Extern { .. } => scalar_products::Declaration::Extern,
                _ => unreachable!("scalar declaration"),
            };
            reuse.save_normalized_recipe(item.span, ordinal, declaration);
            demands.push(demand);
            normalized.push(NormalizedProduct::View(view));
        }
    }
    if !generics::validate_plain_products(&mut normalized, limits, products, reuse)
        .map_err(|mut errors| errors.remove(0))?
    {
        return Ok(None);
    }
    let mut output = Vec::with_capacity(modules.len());
    let mut normalized = normalized.into_iter().zip(demands).peekable();
    for (module, imports) in modules.iter().zip(imports) {
        let before = budget.remaining_limits();
        let mut edges = imports.clone();
        let mut items = Vec::new();
        while normalized
            .peek()
            .is_some_and(|(view, _)| view.span().file == module.span.file)
        {
            let (NormalizedProduct::View(view), demand) =
                normalized.next().expect("current scalar view")
            else {
                unreachable!("borrowed producer")
            };
            reuse.fused_distributed_item();
            let item_before = budget.remaining_limits();
            let clock = reuse.clock();
            let nodes = demand.nodes;
            let (restored, fused) = restore(&view, demand, functions, &mut edges, &mut budget)?;
            if fused {
                reuse.fused_restore(nodes);
            } else {
                reuse.borrowed_restore_fallback();
            }
            reuse.production(clock);
            reuse.record(Stage::RestoreItem, view.item().span, item_before, &budget);
            items.push(Arc::new(restored));
        }
        reuse.record_restore_module(module.span, before, &budget);
        output.push(ModuleView {
            id: module.id,
            span: module.span,
            items,
            imports: edges,
        });
    }
    products.storage.restore_functions(functions, &mut budget)?;
    products.restore_owners(functions)?;
    Ok(Some(SpecializedModules::BorrowedScalar(output)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_syntax::attributes::AttributeContext;

    fn view(
        parsed: &ParsedSource,
        ordinal: usize,
        index: &Index,
        budget: &mut Budget<'_>,
    ) -> R<(BodyView, RestoreDemand)> {
        let item = &parsed.parsed().module.items[ordinal];
        let before = budget.remaining_limits();
        let mut visitor = Visitor {
            scalar_file: Some(item.span.file),
            default_nominal: None,
            const_scope: Names::new(),
            mode: Mode::Normalize(index),
            module: parsed.source().module_id,
            budget,
        };
        let mut walk = Walk {
            visitor: &mut visitor,
            input: None,
            expressions: BTreeMap::new(),
            canonical_bytes: 0,
            depth: 0,
            references: Vec::new(),
        };
        let name = walk.item(item)?;
        let after = walk.visitor.budget.remaining_limits();
        Ok((
            BodyView {
                parsed: parsed.clone(),
                ordinal,
                name,
                expressions: Arc::new(walk.expressions),
            },
            RestoreDemand {
                nodes: before.max_nodes - after.max_nodes,
                work: before.max_analysis_steps - after.max_analysis_steps - walk.canonical_bytes,
                depth: walk.depth,
                references: walk.references,
            },
        ))
    }

    #[test]
    fn borrowed_normalize_and_restore_match_actual_owned_walks_at_every_quota_prefix() {
        let limits = Limits::default();
        let parse = |id, text| {
            ParsedSource::parse(
                Arc::new(SourceUnit::new(
                    FileId(id),
                    ModuleId(id),
                    format!("m{id}.zil"),
                    text,
                )),
                &limits,
                &AttributeContext::default(),
            )
        };
        let inputs = [
            parse(
                0,
                "import dep; extern \"C\" fn get()->i32; #[test] fn f(x:(i32),y:bool)->i32{let z=dep.g(x);if y{let dep=x;while dep>0{if dep==2{break;}else{continue;}}}else if x<0{unsafe{get();}}else{loop{break;}}return if y{z}else{x};} fn unused(x:i32)->i32=>x |> dep.g;",
            ),
            parse(1, "pub fn g(x:i32)->i32=>x+1;"),
        ];
        for input in &inputs {
            assert!(input.parsed().is_ok());
        }
        let sources = inputs.iter().map(ParsedSource::source).collect::<Vec<_>>();
        let modules = inputs
            .iter()
            .map(|input| &input.parsed().module)
            .collect::<Vec<_>>();
        let imports = [
            vec![CoreImport {
                alias: "dep".into(),
                module: ModuleId(1),
                span: modules[0].items[0].span,
            }],
            vec![],
        ];
        let (index, _, functions) =
            declaration_inventory(&sources, &modules, &imports, &mut Budget::new(&limits)).unwrap();
        for (position, input) in inputs.iter().enumerate() {
            for (ordinal, item) in input.parsed().module.items.iter().enumerate() {
                if matches!(item.kind, ItemKind::Import(_)) {
                    continue;
                }
                let mut normalized = item.clone();
                let mut full = Budget::new(&limits);
                Visitor {
                    scalar_file: Some(item.span.file),
                    default_nominal: None,
                    const_scope: Names::new(),
                    mode: Mode::Normalize(&index),
                    module: input.source().module_id,
                    budget: &mut full,
                }
                .item(&mut normalized, &Names::new(), true, 0)
                .unwrap();
                let mut borrowed = Budget::new(&limits);
                let (current, _) = view(input, ordinal, &index, &mut borrowed).unwrap();
                assert_eq!(current.materialize(), normalized);
                assert_eq!(borrowed.remaining_limits(), full.remaining_limits());
                // Include an empty import list: Restore must form the original
                // generated alias at the first actual external occurrence.
                for initial_edges in [imports[position].clone(), vec![]] {
                    let mut actual = normalized.clone();
                    let mut actual_edges = initial_edges.clone();
                    let mut actual_budget = Budget::new(&limits);
                    Visitor {
                        scalar_file: None,
                        default_nominal: None,
                        const_scope: Names::new(),
                        mode: Mode::Restore {
                            functions: &functions,
                            edges: &mut actual_edges,
                        },
                        module: input.source().module_id,
                        budget: &mut actual_budget,
                    }
                    .item(&mut actual, &Names::new(), true, 0)
                    .unwrap();
                    let cost = actual_budget.remaining_limits();
                    for nodes in [false, true] {
                        let maximum = if nodes {
                            limits.max_nodes - cost.max_nodes
                        } else {
                            limits.max_analysis_steps - cost.max_analysis_steps
                        };
                        for amount in 0..=maximum + 1 {
                            let allowance = if nodes {
                                Limits {
                                    max_nodes: amount,
                                    ..limits.clone()
                                }
                            } else {
                                Limits {
                                    max_analysis_steps: amount,
                                    ..limits.clone()
                                }
                            };
                            let (current, demand) =
                                view(input, ordinal, &index, &mut Budget::new(&limits)).unwrap();
                            let mut shared = Budget::new(&allowance);
                            let mut edges = initial_edges.clone();
                            let result =
                                restore(&current, demand, &functions, &mut edges, &mut shared)
                                    .map(|(item, _)| item.materialize());
                            let mut expected = normalized.clone();
                            let mut expected_edges = initial_edges.clone();
                            let mut budget = Budget::new(&allowance);
                            let reference = Visitor {
                                scalar_file: None,
                                default_nominal: None,
                                const_scope: Names::new(),
                                mode: Mode::Restore {
                                    functions: &functions,
                                    edges: &mut expected_edges,
                                },
                                module: input.source().module_id,
                                budget: &mut budget,
                            }
                            .item(&mut expected, &Names::new(), true, 0)
                            .map(|()| expected);
                            assert_eq!(result, reference, "nodes={nodes} amount={amount}");
                            assert_eq!(shared.remaining_limits(), budget.remaining_limits());
                            assert_eq!(edges, expected_edges);
                        }
                    }
                }
            }
        }
    }
}
