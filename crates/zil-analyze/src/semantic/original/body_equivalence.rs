//! Exact original closure-body correspondence. Only source positions and local
//! node numbering are alpha-renamed. Operators, declarations, value binders,
//! mixed arguments and every complete annotation remain comparison operands.
use super::*;

#[derive(Clone, Copy)]
struct Node {
    canonical: NodeId,
    statement: bool,
}
struct Compare<'s, 'a, 'i> {
    solver: &'s Solver<'a, 'i>,
    left: BTreeMap<(SemanticDefinitionId, NodeId), Node>,
    right: BTreeMap<(SemanticDefinitionId, NodeId), Node>,
    next: u32,
    annotations: Vec<(SemanticDefinitionId, SemanticTypeRecipe, SemanticTypeRecipe)>,
}
impl Compare<'_, '_, '_> {
    fn step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original full body correspondence exceeds its complete depth",
            ));
        }
        Ok(())
    }
    fn node(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut NodeId,
        right: &mut NodeId,
        statement: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(
            self.left
                .len()
                .saturating_add(self.right.len())
                .saturating_add(1),
            WORK,
        )?;
        let canonical = NodeId(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| resource("original body node correspondence domain overflow"))?;
        let node = Node {
            canonical,
            statement,
        };
        if self.left.insert((owner, *left), node).is_some()
            || self.right.insert((owner, *right), node).is_some()
        {
            return Err(fail(
                "original body correspondence repeats a real declaration node identity",
            ));
        }
        *left = canonical;
        *right = canonical;
        Ok(())
    }
    fn annotation(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut SemanticTypeRecipe,
        right: &mut SemanticTypeRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        crate::dyn_metadata::recipe(left, b)?;
        crate::dyn_metadata::recipe(right, b)?;
        let a = std::mem::replace(left, SemanticTypeRecipe::Atom(Type::Unit));
        let z = std::mem::replace(right, SemanticTypeRecipe::Atom(Type::Unit));
        // These placeholders exist only in private comparison copies. The full
        // recipes are compared separately below, never evaluated as Unit.
        Solver::push(&mut self.annotations, (owner, a, z), b)
    }
    fn optional_annotation(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut Option<SemanticTypeRecipe>,
        right: &mut Option<SemanticTypeRecipe>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        match (left, right) {
            (Some(a), Some(z)) => self.annotation(owner, a, z, b),
            (None, None) => Ok(()),
            _ => Err(fail(
                "original child body changes whether a source annotation was written",
            )),
        }
    }
    fn exprs(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut [SemanticOriginalExpr],
        right: &mut [SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if left.len() != right.len() {
            return Err(fail(
                "original child body changes its ordered operand arity",
            ));
        }
        for (a, z) in left.iter_mut().zip(right) {
            self.expr(owner, a, z, b, depth + 1)?;
        }
        Ok(())
    }
    fn fields(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut [(String, SemanticOriginalExpr)],
        right: &mut [(String, SemanticOriginalExpr)],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if left.len() != right.len() {
            return Err(fail(
                "original child body changes its complete ordered field inventory",
            ));
        }
        for ((an, a), (zn, z)) in left.iter_mut().zip(right) {
            b.spend(an.len().saturating_add(zn.len()), WORK)?;
            if an != zn {
                return Err(fail(
                    "original child body changes an actual source field binding",
                ));
            }
            self.expr(owner, a, z, b, depth + 1)?;
        }
        Ok(())
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut SemanticOriginalBlock,
        right: &mut SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        left.span = zil_base::Span::default();
        right.span = zil_base::Span::default();
        if left.statements.len() != right.statements.len() {
            return Err(fail(
                "original child body changes its complete statement inventory",
            ));
        }
        for (a, z) in left.statements.iter_mut().zip(&mut right.statements) {
            self.statement(owner, a, z, b, depth + 1)?;
        }
        Ok(())
    }
    fn statement(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut SemanticOriginalStatement,
        right: &mut SemanticOriginalStatement,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        self.node(owner, &mut left.id, &mut right.id, true, b)?;
        left.span = zil_base::Span::default();
        right.span = zil_base::Span::default();
        match (&mut left.kind, &mut right.kind) {
            (
                CoreStatementKind::Binding {
                    ty: a,
                    initializer: av,
                    ..
                },
                CoreStatementKind::Binding {
                    ty: z,
                    initializer: zv,
                    ..
                },
            ) => {
                self.optional_annotation(owner, a, z, b)?;
                match (av, zv) {
                    (Some(a), Some(z)) => self.expr(owner, a, z, b, depth + 1)?,
                    (None, None) => {}
                    _ => {
                        return Err(fail(
                            "original child body changes an actual binding initializer",
                        ));
                    }
                }
            }
            (
                CoreStatementKind::TupleBinding {
                    pattern: a,
                    ty: at,
                    initializer: av,
                    ..
                },
                CoreStatementKind::TupleBinding {
                    pattern: z,
                    ty: zt,
                    initializer: zv,
                    ..
                },
            ) => {
                self.pattern(owner, a, z, b, depth + 1)?;
                self.optional_annotation(owner, at, zt, b)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreStatementKind::Assign {
                    target: a,
                    value: av,
                },
                CoreStatementKind::Assign {
                    target: z,
                    value: zv,
                },
            )
            | (
                CoreStatementKind::CompoundAssign {
                    target: a,
                    value: av,
                    ..
                },
                CoreStatementKind::CompoundAssign {
                    target: z,
                    value: zv,
                    ..
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (CoreStatementKind::Expression(a), CoreStatementKind::Expression(z))
            | (CoreStatementKind::Yield(a), CoreStatementKind::Yield(z)) => {
                self.expr(owner, a, z, b, depth + 1)?
            }
            (CoreStatementKind::Return(a), CoreStatementKind::Return(z)) => match (a, z) {
                (Some(a), Some(z)) => self.expr(owner, a, z, b, depth + 1)?,
                (None, None) => {}
                _ => return Err(fail("original child body changes its actual return value")),
            },
            (
                CoreStatementKind::If {
                    condition: a,
                    then_body: ab,
                    else_body: ae,
                },
                CoreStatementKind::If {
                    condition: z,
                    then_body: zb,
                    else_body: ze,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
                self.optional_block(owner, ae, ze, b, depth + 1)?;
            }
            (
                CoreStatementKind::IfLet {
                    pattern: a,
                    value: av,
                    then_body: ab,
                    else_body: ae,
                },
                CoreStatementKind::IfLet {
                    pattern: z,
                    value: zv,
                    then_body: zb,
                    else_body: ze,
                },
            ) => {
                self.pattern(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
                self.optional_block(owner, ae, ze, b, depth + 1)?;
            }
            (
                CoreStatementKind::While {
                    condition: a,
                    body: ab,
                },
                CoreStatementKind::While {
                    condition: z,
                    body: zb,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
            }
            (
                CoreStatementKind::WhileLet {
                    pattern: a,
                    value: av,
                    body: ab,
                },
                CoreStatementKind::WhileLet {
                    pattern: z,
                    value: zv,
                    body: zb,
                },
            ) => {
                self.pattern(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
            }
            (
                CoreStatementKind::ForRange {
                    start: a,
                    end: av,
                    body: ab,
                    ..
                },
                CoreStatementKind::ForRange {
                    start: z,
                    end: zv,
                    body: zb,
                    ..
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
            }
            (
                CoreStatementKind::ForEach {
                    pattern: a,
                    iterator: av,
                    body: ab,
                },
                CoreStatementKind::ForEach {
                    pattern: z,
                    iterator: zv,
                    body: zb,
                },
            ) => {
                self.pattern(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
            }
            (
                CoreStatementKind::Match { value: a, arms: aa },
                CoreStatementKind::Match { value: z, arms: za },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                if aa.len() != za.len() {
                    return Err(fail(
                        "original child body changes its complete source arm inventory",
                    ));
                }
                for (a, z) in aa.iter_mut().zip(za) {
                    a.span = zil_base::Span::default();
                    z.span = zil_base::Span::default();
                    self.pattern(owner, &mut a.pattern, &mut z.pattern, b, depth + 1)?;
                    self.block(owner, &mut a.body, &mut z.body, b, depth + 1)?;
                }
            }
            (CoreStatementKind::Block(a), CoreStatementKind::Block(z))
            | (CoreStatementKind::Unsafe(a), CoreStatementKind::Unsafe(z))
            | (CoreStatementKind::Loop(a), CoreStatementKind::Loop(z))
            | (CoreStatementKind::Defer(a), CoreStatementKind::Defer(z)) => {
                self.block(owner, a, z, b, depth + 1)?
            }
            (
                CoreStatementKind::With {
                    value: a, body: ab, ..
                },
                CoreStatementKind::With {
                    value: z, body: zb, ..
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.block(owner, ab, zb, b, depth + 1)?;
            }
            (CoreStatementKind::CompileTime(a), CoreStatementKind::CompileTime(z)) => {
                self.statement(owner, a, z, b, depth + 1)?
            }
            (CoreStatementKind::Break, CoreStatementKind::Break)
            | (CoreStatementKind::Continue, CoreStatementKind::Continue)
            | (CoreStatementKind::Unsupported(_), CoreStatementKind::Unsupported(_)) => {}
            _ => {
                return Err(fail(
                    "original child body changes an actual source statement constructor",
                ));
            }
        }
        Ok(())
    }
    fn optional_block(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut Option<SemanticOriginalBlock>,
        right: &mut Option<SemanticOriginalBlock>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        match (left, right) {
            (Some(a), Some(z)) => self.block(owner, a, z, b, depth + 1),
            (None, None) => Ok(()),
            _ => Err(fail("original child body changes its source else branch")),
        }
    }
    fn pattern(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut SemanticOriginalPattern,
        right: &mut SemanticOriginalPattern,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        left.span = zil_base::Span::default();
        right.span = zil_base::Span::default();
        match (&mut left.kind, &mut right.kind) {
            (CorePatternKind::Tuple(a), CorePatternKind::Tuple(z))
            | (
                CorePatternKind::Constructor { fields: a, .. },
                CorePatternKind::Constructor { fields: z, .. },
            ) => self.patterns(owner, a, z, b, depth + 1)?,
            (
                CorePatternKind::Variant {
                    ty: a, fields: af, ..
                },
                CorePatternKind::Variant {
                    ty: z, fields: zf, ..
                },
            ) => {
                self.annotation(owner, a, z, b)?;
                self.patterns(owner, af, zf, b, depth + 1)?;
            }
            (
                CorePatternKind::Record { ty: a, fields: af },
                CorePatternKind::Record { ty: z, fields: zf },
            ) => {
                self.annotation(owner, a, z, b)?;
                self.pattern_fields(owner, af, zf, b, depth + 1)?;
            }
            (
                CorePatternKind::RecordConstructor { fields: a, .. },
                CorePatternKind::RecordConstructor { fields: z, .. },
            ) => self.pattern_fields(owner, a, z, b, depth + 1)?,
            (
                CorePatternKind::Reference { pattern: a, .. },
                CorePatternKind::Reference { pattern: z, .. },
            )
            | (
                CorePatternKind::ErrorArm { pattern: a, .. },
                CorePatternKind::ErrorArm { pattern: z, .. },
            ) => self.pattern(owner, a, z, b, depth + 1)?,
            (a, z) if std::mem::discriminant(a) == std::mem::discriminant(z) => {}
            _ => {
                return Err(fail(
                    "original child body changes its exact source pattern constructor",
                ));
            }
        }
        Ok(())
    }
    fn patterns(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut [SemanticOriginalPattern],
        right: &mut [SemanticOriginalPattern],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if left.len() != right.len() {
            return Err(fail(
                "original child body changes its ordered pattern children",
            ));
        }
        for (a, z) in left.iter_mut().zip(right) {
            self.pattern(owner, a, z, b, depth + 1)?;
        }
        Ok(())
    }
    fn pattern_fields(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut [(String, SemanticOriginalPattern)],
        right: &mut [(String, SemanticOriginalPattern)],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        if left.len() != right.len() {
            return Err(fail(
                "original child body changes its complete pattern field directory",
            ));
        }
        for ((an, a), (zn, z)) in left.iter_mut().zip(right) {
            b.spend(an.len().saturating_add(zn.len()), WORK)?;
            if an != zn {
                return Err(fail(
                    "original child body changes an actual pattern field name",
                ));
            }
            self.pattern(owner, a, z, b, depth + 1)?;
        }
        Ok(())
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut SemanticOriginalExpr,
        right: &mut SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let source = right.id;
        self.node(owner, &mut left.id, &mut right.id, false, b)?;
        left.span = zil_base::Span::default();
        right.span = zil_base::Span::default();
        match (&mut left.kind, &mut right.kind) {
            (CoreExprKind::Group(a), CoreExprKind::Group(z))
            | (CoreExprKind::CompileTime(a), CoreExprKind::CompileTime(z))
            | (CoreExprKind::Try(a), CoreExprKind::Try(z))
            | (CoreExprKind::Spread(a), CoreExprKind::Spread(z))
            | (CoreExprKind::Splice(a), CoreExprKind::Splice(z))
            | (CoreExprKind::Unary { operand: a, .. }, CoreExprKind::Unary { operand: z, .. }) => {
                self.expr(owner, a, z, b, depth + 1)?
            }
            (
                CoreExprKind::Ascribed { ty: a, value: av },
                CoreExprKind::Ascribed { ty: z, value: zv },
            ) => {
                self.annotation(owner, a, z, b)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::NumericConvert {
                    target: a,
                    value: av,
                },
                CoreExprKind::NumericConvert {
                    target: z,
                    value: zv,
                },
            )
            | (
                CoreExprKind::IntegerConvert {
                    target: a,
                    value: av,
                },
                CoreExprKind::IntegerConvert {
                    target: z,
                    value: zv,
                },
            ) => {
                self.annotation(owner, a, z, b)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::Binary {
                    left: a, right: av, ..
                },
                CoreExprKind::Binary {
                    left: z, right: zv, ..
                },
            )
            | (
                CoreExprKind::Index {
                    object: a,
                    index: av,
                },
                CoreExprKind::Index {
                    object: z,
                    index: zv,
                },
            )
            | (
                CoreExprKind::Range { start: a, end: av },
                CoreExprKind::Range { start: z, end: zv },
            )
            | (
                CoreExprKind::Pipeline {
                    input: a,
                    target: av,
                },
                CoreExprKind::Pipeline {
                    input: z,
                    target: zv,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::Call {
                    callee: a,
                    arguments: av,
                },
                CoreExprKind::Call {
                    callee: z,
                    arguments: zv,
                },
            )
            | (
                CoreExprKind::CallDynSlot {
                    callee: a,
                    arguments: av,
                    ..
                },
                CoreExprKind::CallDynSlot {
                    callee: z,
                    arguments: zv,
                    ..
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.exprs(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::TypeApply {
                    callee: a,
                    type_arguments: av,
                },
                CoreExprKind::TypeApply {
                    callee: z,
                    type_arguments: zv,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                if av.len() != zv.len() {
                    return Err(fail(
                        "original child body changes its full written Type application arity",
                    ));
                }
                for (a, z) in av.iter_mut().zip(zv) {
                    self.annotation(owner, a, z, b)?;
                }
            }
            (
                CoreExprKind::GenericApply {
                    callee: a,
                    arguments: av,
                },
                CoreExprKind::GenericApply {
                    callee: z,
                    arguments: zv,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                if av.len() != zv.len() {
                    return Err(fail(
                        "original child body changes its full written mixed actual order",
                    ));
                }
                for (a, z) in av.iter_mut().zip(zv) {
                    match (a, z) {
                        (CoreGenericArgument::Type(a), CoreGenericArgument::Type(z)) => {
                            self.annotation(owner, a, z, b)?
                        }
                        (CoreGenericArgument::Const(a), CoreGenericArgument::Const(z)) => {
                            self.expr(owner, a, z, b, depth + 1)?
                        }
                        (CoreGenericArgument::Lifetime(_), CoreGenericArgument::Lifetime(_)) => {}
                        _ => {
                            return Err(fail(
                                "original child body changes a written mixed actual kind",
                            ));
                        }
                    }
                }
            }
            (
                CoreExprKind::Member {
                    object: a,
                    owner: at,
                    ..
                },
                CoreExprKind::Member {
                    object: z,
                    owner: zt,
                    ..
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.optional_annotation(owner, at, zt, b)?;
            }
            (
                CoreExprKind::Record { ty: a, fields: af },
                CoreExprKind::Record { ty: z, fields: zf },
            ) => {
                self.annotation(owner, a, z, b)?;
                self.fields(owner, af, zf, b, depth + 1)?;
            }
            (
                CoreExprKind::RecordConstructor { fields: a, .. },
                CoreExprKind::RecordConstructor { fields: z, .. },
            ) => self.fields(owner, a, z, b, depth + 1)?,
            (
                CoreExprKind::RecordUpdate {
                    ty: a,
                    base: av,
                    fields: af,
                },
                CoreExprKind::RecordUpdate {
                    ty: z,
                    base: zv,
                    fields: zf,
                },
            ) => {
                self.optional_annotation(owner, a, z, b)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.fields(owner, af, zf, b, depth + 1)?;
            }
            (CoreExprKind::Tuple(a), CoreExprKind::Tuple(z))
            | (CoreExprKind::Array(a), CoreExprKind::Array(z)) => {
                self.exprs(owner, a, z, b, depth + 1)?
            }
            (
                CoreExprKind::Slice {
                    object: a,
                    start: av,
                    end: ae,
                },
                CoreExprKind::Slice {
                    object: z,
                    start: zv,
                    end: ze,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.optional_expr(owner, av, zv, b, depth + 1)?;
                self.optional_expr(owner, ae, ze, b, depth + 1)?;
            }
            (
                CoreExprKind::Sequence {
                    bindings: a,
                    value: av,
                },
                CoreExprKind::Sequence {
                    bindings: z,
                    value: zv,
                },
            ) => {
                self.fields(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
            }
            (
                CoreExprKind::Closure {
                    parameters: a,
                    return_type: at,
                    body: ab,
                    ..
                },
                CoreExprKind::Closure {
                    parameters: z,
                    return_type: zt,
                    body: zb,
                    ..
                },
            ) => {
                if a.len() != z.len() {
                    return Err(fail(
                        "original nested closure changes its full source parameter inventory",
                    ));
                }
                for (a, z) in a.iter_mut().zip(z) {
                    a.span = zil_base::Span::default();
                    z.span = zil_base::Span::default();
                    self.optional_annotation(owner, &mut a.ty, &mut z.ty, b)?;
                }
                self.optional_annotation(owner, at, zt, b)?;
                let original = self.solver.inventory.definition(owner, b)?;
                let site = self.solver.original_site(original, source, b)?;
                let SemanticSiteRecipeKind::Closure {
                    definition: child, ..
                } = site.kind
                else {
                    return Err(fail(
                        "original nested closure is linked to another source operation",
                    ));
                };
                let child_definition = self.solver.inventory.definition(child, b)?;
                if child_definition.parent != Some(owner)
                    || child_definition.kind != SemanticDefinitionKind::Closure
                {
                    return Err(fail(
                        "original nested closure changes its exact lexical child declaration",
                    ));
                }
                self.block(child, ab, zb, b, depth + 1)?;
            }
            (
                CoreExprKind::If {
                    condition: a,
                    then_value: av,
                    else_value: ae,
                },
                CoreExprKind::If {
                    condition: z,
                    then_value: zv,
                    else_value: ze,
                },
            ) => {
                self.expr(owner, a, z, b, depth + 1)?;
                self.expr(owner, av, zv, b, depth + 1)?;
                self.expr(owner, ae, ze, b, depth + 1)?;
            }
            (
                CoreExprKind::MakeDynBorrow { source: a, .. },
                CoreExprKind::MakeDynBorrow { source: z, .. },
            )
            | (
                CoreExprKind::MakeDynOwned { source: a, .. },
                CoreExprKind::MakeDynOwned { source: z, .. },
            ) => self.expr(owner, a, z, b, depth + 1)?,
            (CoreExprKind::CompileTimeBlock(a), CoreExprKind::CompileTimeBlock(z)) => {
                self.block(owner, a, z, b, depth + 1)?
            }
            (
                CoreExprKind::SyntaxQuote { template: a, .. },
                CoreExprKind::SyntaxQuote { template: z, .. },
            ) => {
                self.syntax(&mut a.tree, &mut z.tree, b, depth + 1)?;
                if a.holes.len() != z.holes.len() || a.name_sites.len() != z.name_sites.len() {
                    return Err(fail(
                        "original syntax quote changes its full hole or binding directory",
                    ));
                }
                for (a, z) in a.holes.iter_mut().zip(&mut z.holes) {
                    a.span = zil_base::Span::default();
                    z.span = zil_base::Span::default();
                    self.expr(owner, &mut a.value, &mut z.value, b, depth + 1)?;
                }
                for (a, z) in a.name_sites.iter_mut().zip(&mut z.name_sites) {
                    a.span = zil_base::Span::default();
                    z.span = zil_base::Span::default();
                }
            }
            (
                CoreExprKind::SyntaxIsland { tree: a, .. },
                CoreExprKind::SyntaxIsland { tree: z, .. },
            ) => self.syntax(a, z, b, depth + 1)?,
            (a, z) if std::mem::discriminant(a) == std::mem::discriminant(z) => {}
            _ => {
                return Err(fail(
                    "original child body changes an actual source expression constructor",
                ));
            }
        }
        Ok(())
    }
    fn optional_expr(
        &mut self,
        owner: SemanticDefinitionId,
        left: &mut Option<Box<SemanticOriginalExpr>>,
        right: &mut Option<Box<SemanticOriginalExpr>>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        match (left, right) {
            (Some(a), Some(z)) => self.expr(owner, a, z, b, depth + 1),
            (None, None) => Ok(()),
            _ => Err(fail("original child body changes a source range bound")),
        }
    }
    fn syntax(
        &self,
        left: &mut CoreSyntaxTree,
        right: &mut CoreSyntaxTree,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        left.span = zil_base::Span::default();
        right.span = zil_base::Span::default();
        if left.children.len() != right.children.len() {
            return Err(fail(
                "original syntax body changes its complete source tree",
            ));
        }
        for (a, z) in left.children.iter_mut().zip(&mut right.children) {
            match (a, z) {
                (
                    CoreSyntaxTreeEntry::Token { span: a, .. },
                    CoreSyntaxTreeEntry::Token { span: z, .. },
                ) => {
                    *a = zil_base::Span::default();
                    *z = zil_base::Span::default();
                }
                (CoreSyntaxTreeEntry::Tree(a), CoreSyntaxTreeEntry::Tree(z)) => {
                    self.syntax(a, z, b, depth + 1)?
                }
                (CoreSyntaxTreeEntry::Hole { .. }, CoreSyntaxTreeEntry::Hole { .. }) => {}
                _ => {
                    return Err(fail(
                        "original syntax body changes an actual token/tree/hole constructor",
                    ));
                }
            }
        }
        Ok(())
    }
}

struct Canonical<'s, 'a, 'i> {
    solver: &'s Solver<'a, 'i>,
    scopes: BTreeMap<SemanticDefinitionId, u32>,
    external: BTreeMap<(SemanticDefinitionId, NodeId, bool), NodeId>,
    next: u32,
}
impl Canonical<'_, '_, '_> {
    fn step(&self, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original annotation correspondence exceeds its complete depth",
            ));
        }
        Ok(())
    }
    fn scope(
        &mut self,
        owner: SemanticDefinitionId,
        scope: &mut SemanticScopeRef,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        let actual = self.solver.original_scope(owner, scope, b)?;
        b.lookup(self.scopes.len(), WORK)?;
        let symbol = if let Some(symbol) = self.scopes.get(&actual) {
            *symbol
        } else {
            let symbol = u32::try_from(self.scopes.len())
                .map_err(|_| resource("original annotation owner correspondence overflow"))?;
            self.scopes.insert(actual, symbol);
            symbol
        };
        // Comparison-only injective owner symbols. These copies never enter
        // recipe evaluation, inference, a semantic instance or an actual type.
        *scope = SemanticScopeRef::Ancestor(symbol);
        Ok(actual)
    }
    fn node(
        &mut self,
        owner: SemanticDefinitionId,
        node: &mut NodeId,
        statement: bool,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(map.len(), WORK)?;
        if let Some(mapped) = map.get(&(owner, *node)) {
            if mapped.statement != statement {
                return Err(fail(
                    "original annotation changes an expression versus binding source identity",
                ));
            }
            *node = mapped.canonical;
            return Ok(());
        }
        let index = self.solver.indexes.get(&owner).ok_or_else(|| {
            fail("original annotation references no complete lexical definition index")
        })?;
        if statement {
            index.get_stmt(*node, b)?;
        } else {
            index.get_expr(*node, b)?;
        }
        let key = (owner, *node, statement);
        b.lookup(self.external.len(), WORK)?;
        let canonical = if let Some(node) = self.external.get(&key) {
            *node
        } else {
            let canonical = NodeId(self.next);
            self.next = self.next.checked_add(1).ok_or_else(|| {
                resource("original external source correspondence domain overflow")
            })?;
            self.external.insert(key, canonical);
            canonical
        };
        *node = canonical;
        Ok(())
    }
    fn formal(
        &mut self,
        owner: SemanticDefinitionId,
        formal: &mut SemanticFormalRef,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.scope(owner, &mut formal.scope, b)?;
        Ok(())
    }
    fn operand(
        &mut self,
        owner: SemanticDefinitionId,
        operand: &mut SemanticRecipeOperand,
        b: &mut Budget,
    ) -> CompileResult<()> {
        match operand {
            SemanticRecipeOperand::TypeFormal(formal) => self.formal(owner, formal, b),
            SemanticRecipeOperand::Environment { scope, .. } => {
                self.scope(owner, scope, b)?;
                Ok(())
            }
            SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => Ok(()),
        }
    }
    fn life(
        &mut self,
        owner: SemanticDefinitionId,
        life: &mut SemanticLifeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match life {
            SemanticLifeRecipe::Formal(formal) => self.formal(owner, formal, b),
            SemanticLifeRecipe::Implicit { scope, .. }
            | SemanticLifeRecipe::Environment { scope, .. } => {
                self.scope(owner, scope, b)?;
                Ok(())
            }
            SemanticLifeRecipe::OuterOccurrence { operand, .. }
            | SemanticLifeRecipe::IntrinsicClass { operand, .. } => self.operand(owner, operand, b),
            SemanticLifeRecipe::Bound { .. } | SemanticLifeRecipe::Invocation(_) => Ok(()),
        }
    }
    fn outer(
        &mut self,
        owner: SemanticDefinitionId,
        life: &mut SemanticOuterLifeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        match life {
            SemanticOuterLifeRecipe::Class(life) => self.life(owner, life, b, depth + 1),
            SemanticOuterLifeRecipe::Static => Ok(()),
        }
    }
    fn actual(
        &mut self,
        owner: SemanticDefinitionId,
        actual: &mut SemanticActualRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match actual {
            SemanticActualRecipe::Type(ty) => self.ty(owner, ty, map, b, depth + 1),
            SemanticActualRecipe::Const { carrier, value } => {
                self.ty(owner, carrier, map, b, depth + 1)?;
                self.constant(owner, value, map, b, depth + 1)
            }
            SemanticActualRecipe::Life(life) => self.life(owner, life, b, depth + 1),
            SemanticActualRecipe::Infer => Ok(()),
        }
    }
    fn application(
        &mut self,
        owner: SemanticDefinitionId,
        application: &mut SemanticTraitRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        for actual in &mut application.arguments {
            self.actual(owner, actual, map, b, depth + 1)?;
        }
        Ok(())
    }
    fn constant(
        &mut self,
        owner: SemanticDefinitionId,
        constant: &mut SemanticConstRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match constant {
            SemanticConstRecipe::Formal(formal) => self.formal(owner, formal, b),
            SemanticConstRecipe::Environment { scope, .. } => {
                self.scope(owner, scope, b)?;
                Ok(())
            }
            SemanticConstRecipe::Unary {
                operand_type,
                value,
                ..
            } => {
                self.ty(owner, operand_type, map, b, depth + 1)?;
                self.constant(owner, value, map, b, depth + 1)
            }
            SemanticConstRecipe::Binary {
                operand_type,
                left,
                right,
                ..
            } => {
                self.ty(owner, operand_type, map, b, depth + 1)?;
                self.constant(owner, left, map, b, depth + 1)?;
                self.constant(owner, right, map, b, depth + 1)
            }
            SemanticConstRecipe::Convert {
                source_type,
                target_type,
                value,
            } => {
                self.ty(owner, source_type, map, b, depth + 1)?;
                self.ty(owner, target_type, map, b, depth + 1)?;
                self.constant(owner, value, map, b, depth + 1)
            }
            SemanticConstRecipe::AssociatedConst {
                receiver,
                application,
                ..
            } => {
                self.ty(owner, receiver, map, b, depth + 1)?;
                self.application(owner, application, map, b, depth + 1)
            }
            SemanticConstRecipe::Value(_)
            | SemanticConstRecipe::IntegerLiteral(_)
            | SemanticConstRecipe::FloatLiteral(_) => Ok(()),
        }
    }
    fn sequence(
        &mut self,
        owner: SemanticDefinitionId,
        sequence: &mut SemanticClassSequenceRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        for row in &mut sequence.parts {
            match row {
                SemanticClassSequencePart::One(life) => self.life(owner, life, b, depth + 1)?,
                SemanticClassSequencePart::AllClasses { ty, .. } => {
                    self.ty(owner, ty, map, b, depth + 1)?
                }
            }
        }
        Ok(())
    }
    fn retain(
        &mut self,
        owner: SemanticDefinitionId,
        retained: &mut [SemanticRetainedPart],
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        for row in retained {
            match row {
                SemanticRetainedPart::One { parent, .. } => {
                    self.life(owner, parent, b, depth + 1)?
                }
                SemanticRetainedPart::All { parents, .. } => {
                    self.sequence(owner, parents, map, b, depth + 1)?
                }
            }
        }
        Ok(())
    }
    fn nominal(
        &mut self,
        owner: SemanticDefinitionId,
        nominal: &mut [SemanticNominalPart],
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        for row in nominal {
            match row {
                SemanticNominalPart::One { class, .. } => self.life(owner, class, b, depth + 1)?,
                SemanticNominalPart::FromType { ty, classes, .. } => {
                    self.ty(owner, ty, map, b, depth + 1)?;
                    self.sequence(owner, classes, map, b, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn retained(
        &mut self,
        owner: SemanticDefinitionId,
        shape: &mut SemanticRetainedShapeRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        self.sequence(owner, &mut shape.domain, map, b, depth + 1)?;
        for port in &mut shape.ports {
            match port {
                SemanticPortPart::One { leaf, class, .. } => {
                    self.ty(owner, leaf, map, b, depth + 1)?;
                    self.life(owner, class, b, depth + 1)?;
                }
                SemanticPortPart::FromType { ty, classes } => {
                    self.ty(owner, ty, map, b, depth + 1)?;
                    self.sequence(owner, classes, map, b, depth + 1)?;
                }
            }
        }
        self.retain(owner, &mut shape.retained, map, b, depth + 1)?;
        self.nominal(owner, &mut shape.nominal, map, b, depth + 1)
    }
    fn storage(
        &mut self,
        owner: SemanticDefinitionId,
        shape: &mut SemanticStorageShapeRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        self.sequence(owner, &mut shape.domain, map, b, depth + 1)?;
        for slot in &mut shape.slots {
            self.life(owner, &mut slot.class, b, depth + 1)?;
        }
        self.retain(owner, &mut shape.retained, map, b, depth + 1)?;
        self.nominal(owner, &mut shape.nominal, map, b, depth + 1)
    }
    fn ty(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &mut SemanticTypeRecipe,
        map: &BTreeMap<(SemanticDefinitionId, NodeId), Node>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match ty {
            SemanticTypeRecipe::Formal(formal) => self.formal(owner, formal, b),
            SemanticTypeRecipe::Environment { scope, .. }
            | SemanticTypeRecipe::SelfType { scope } => {
                self.scope(owner, scope, b)?;
                Ok(())
            }
            SemanticTypeRecipe::Exact(exact) => {
                for life in &mut exact.classes {
                    self.life(owner, life, b, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Nominal { arguments, .. } => {
                for actual in arguments {
                    self.actual(owner, actual, map, b, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                ..
            } => {
                self.ty(owner, receiver, map, b, depth + 1)?;
                self.application(owner, application, map, b, depth + 1)
            }
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                for field in fields {
                    self.ty(owner, field, map, b, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Array { element, length } => {
                self.ty(owner, element, map, b, depth + 1)?;
                self.constant(owner, length, map, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                self.outer(owner, lifetime, b, depth + 1)?;
                self.ty(owner, inner, map, b, depth + 1)
            }
            SemanticTypeRecipe::Slice(inner)
            | SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                self.ty(owner, inner, map, b, depth + 1)
            }
            SemanticTypeRecipe::MutexGuard { payload, retained } => {
                self.ty(owner, payload, map, b, depth + 1)?;
                self.retained(owner, retained, map, b, depth + 1)
            }
            SemanticTypeRecipe::Task { result, retained } => {
                self.ty(owner, result, map, b, depth + 1)?;
                self.retained(owner, retained, map, b, depth + 1)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.ty(owner, error, map, b, depth + 1)?;
                self.ty(owner, result, map, b, depth + 1)
            }
            SemanticTypeRecipe::Qualified { storage, life } => {
                self.ty(owner, storage, map, b, depth + 1)?;
                self.storage(owner, life, map, b, depth + 1)
            }
            SemanticTypeRecipe::Code { signature, .. } => {
                for parameter in &mut signature.parameters {
                    self.ty(owner, &mut parameter.ty, map, b, depth + 1)?;
                }
                self.ty(owner, &mut signature.result, map, b, depth + 1)
            }
            SemanticTypeRecipe::Closure {
                arguments,
                environment,
                ..
            } => {
                for actual in arguments.iter_mut().chain(environment) {
                    self.actual(owner, actual, map, b, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Dyn { interface, .. } => {
                self.application(owner, &mut interface.application, map, b, depth + 1)?;
                match &mut interface.context {
                    SemanticDynContextRecipe::Closed => {}
                    SemanticDynContextRecipe::Type(ty) => self.ty(owner, ty, map, b, depth + 1)?,
                    SemanticDynContextRecipe::Shape(shape) => {
                        self.retained(owner, shape, map, b, depth + 1)?
                    }
                }
                for binding in &mut interface.associated {
                    self.application(owner, &mut binding.application, map, b, depth + 1)?;
                    match &mut binding.value {
                        SemanticAssociatedRecipe::Type(ty) => {
                            self.ty(owner, ty, map, b, depth + 1)?
                        }
                        SemanticAssociatedRecipe::Const { carrier, value } => {
                            self.ty(owner, carrier, map, b, depth + 1)?;
                            self.constant(owner, value, map, b, depth + 1)?;
                        }
                    }
                }
                Ok(())
            }
            SemanticTypeRecipe::Opaque {
                arguments, hidden, ..
            } => {
                for actual in arguments {
                    self.actual(owner, actual, map, b, depth + 1)?;
                }
                self.ty(owner, hidden, map, b, depth + 1)
            }
            SemanticTypeRecipe::OriginalType { scope, source } => {
                let actual = self.scope(owner, scope, b)?;
                match source {
                    SemanticOriginalTypeSource::Expression(node) => {
                        self.node(actual, node, false, map, b)?
                    }
                    SemanticOriginalTypeSource::Binding(node) => {
                        self.node(actual, node, true, map, b)?
                    }
                    SemanticOriginalTypeSource::CallConstCarrier { call, .. } => {
                        self.node(actual, call, false, map, b)?
                    }
                    SemanticOriginalTypeSource::BindingProjection { statement, .. } => {
                        self.node(actual, statement, true, map, b)?
                    }
                    SemanticOriginalTypeSource::NameAt { at, .. } => match at {
                        SemanticOriginalNode::Expression(node) => {
                            self.node(actual, node, false, map, b)?
                        }
                        SemanticOriginalNode::Statement(node) => {
                            self.node(actual, node, true, map, b)?
                        }
                    },
                    SemanticOriginalTypeSource::Parameter(_)
                    | SemanticOriginalTypeSource::Result => {}
                }
                Ok(())
            }
            SemanticTypeRecipe::Atom(_)
            | SemanticTypeRecipe::UnresolvedNamed(_)
            | SemanticTypeRecipe::Unsupported(_) => Ok(()),
        }
    }
}
impl Solver<'_, '_> {
    pub(super) fn compare_child_body(
        &self,
        body: &SemanticOriginalBlock,
        target: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let child = self.inventory.definition(target, b)?;
        let (creator, _) = self.creators.get(&target).ok_or_else(|| {
            fail("original child-body correspondence has no actual creator occurrence")
        })?;
        if child.kind != SemanticDefinitionKind::Closure || child.parent != Some(*creator) {
            return Err(fail(
                "original child-body correspondence changes its exact lexical declaration",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(other),
            ..
        } = &child.body
        else {
            return Err(fail("original closure child omits its actual source body"));
        };
        crate::dyn_metadata::recipe(body, b)?;
        crate::dyn_metadata::recipe(other, b)?;
        let mut left = body.clone();
        let mut right = other.clone();
        let mut compare = Compare {
            solver: self,
            left: BTreeMap::new(),
            right: BTreeMap::new(),
            next: 0,
            annotations: Vec::new(),
        };
        // An inline body's node namespace is the actual child declaration.
        // Outer parameter/return annotations were checked separately in the
        // real creator plus the child's fresh invocation role before this call.
        compare.block(target, &mut left, &mut right, b, 0)?;
        if left != right {
            return Err(fail(
                "original closure literal and child differ in actual operations, operators or source bindings",
            ));
        }
        let Compare {
            left,
            right,
            next,
            annotations,
            ..
        } = compare;
        let mut canonical = Canonical {
            solver: self,
            scopes: BTreeMap::new(),
            external: BTreeMap::new(),
            next,
        };
        for (owner, mut a, mut z) in annotations {
            canonical.ty(owner, &mut a, &left, b, 0)?;
            canonical.ty(owner, &mut z, &right, b, 0)?;
            if a != z {
                return Err(fail(
                    "original closure literal and child differ in complete original annotation, scope or source-node binding",
                ));
            }
        }
        Ok(())
    }
}
