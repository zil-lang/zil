//! Type original operations in their declaration domain. Runtime operands and
//! a particular caller's concrete Type actuals never substitute rigid formals.
use super::*;
use zil_base::NodeId;

mod mixed;
pub(super) mod nominal;
use mixed::Life;

type Term = usize;
#[derive(Clone)]
enum Kind {
    Atom(Type),
    Rigid(SemanticDefinitionId, u32),
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
    },
}
struct Local {
    name: String,
    ty: Term,
    mutable: bool,
    used: bool,
    borrowed: bool,
    initialized: bool,
}
struct Frame {
    definition: SemanticDefinitionId,
    formals: Vec<Term>,
    parameters: Vec<Term>,
    result: Term,
    locals: Vec<Local>,
    inherited: usize,
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
}
struct Need {
    term: Term,
    predicate: Predicate,
}
struct Literal {
    term: Term,
    text: String,
    float: bool,
    negative_context: bool,
}
struct Call {
    target: SemanticDefinitionId,
    actuals: Vec<Term>,
    closure: bool,
}
pub(super) struct Checked {
    arena: Vec<Kind>,
    frames: BTreeMap<SemanticDefinitionId, Frame>,
    calls: BTreeMap<(SemanticDefinitionId, NodeId), Call>,
    names_at: BTreeMap<(SemanticDefinitionId, NodeId), Vec<(String, Term)>>,
}
struct Solver<'a, 'i> {
    inventory: &'i Inventory<'a>,
    indexes: BTreeMap<SemanticDefinitionId, original_index::Index<'a>>,
    state: Checked,
    active: BTreeSet<SemanticDefinitionId>,
    creators: BTreeMap<SemanticDefinitionId, (SemanticDefinitionId, NodeId)>,
    numeric: Vec<Term>,
    scalar_captures: Vec<Term>,
    literals: Vec<Literal>,
    integers: Vec<Term>,
    needs: Vec<Need>,
}
fn resource(message: &str) -> Vec<zil_base::Diagnostic> {
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
    fn push<T>(rows: &mut Vec<T>, row: T, b: &mut Budget) -> CompileResult<()> {
        b.spend(rows.len().saturating_add(1), WORK)?;
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
        if !ty.is_b0_scalar() {
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
            Kind::Address { life, inner, .. } => {
                if self.occurs(needle, *life, depth + 1, b)?
                    || self.occurs(needle, *inner, depth + 1, b)?
                {
                    return Ok(true);
                }
            }
            Kind::ConstRigid { carrier, .. } => {
                if self.occurs(needle, *carrier, depth + 1, b)? {
                    return Ok(true);
                }
            }
            Kind::Closure {
                parameters, result, ..
            } => {
                for field in parameters.iter().chain(std::iter::once(result)) {
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
                return Err(fail("original type equation is recursive"));
            }
            self.state.arena[left] = Kind::Infer(Some(right));
            return Ok(());
        }
        if matches!(self.state.arena[right], Kind::Infer(None)) {
            return self.unify(right, left, depth + 1, b);
        }
        // These clones contain only arena indices or a scalar. Pay all child
        // rows before allocating them; no complete Type clone hides here.
        let width = |kind: &Kind| match kind {
            Kind::Tuple(fields) => fields.len(),
            Kind::Nominal {
                arguments, fields, ..
            } => arguments.len().saturating_add(fields.len()),
            Kind::Closure { parameters, .. } => parameters.len() + 1,
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
                },
                Kind::Closure {
                    definition: z,
                    parameters: zp,
                    result: zr,
                },
            ) if a == z && ap.len() == zp.len() => {
                for (a, z) in ap.into_iter().zip(zp) {
                    self.unify(a, z, depth + 1, b)?;
                }
                self.unify(ar, zr, depth + 1, b)
            }
            _ => Err(fail(
                "original operation does not typecheck under its rigid declaration",
            )),
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
                name: name.to_owned(),
                ty,
                mutable,
                used: false,
                borrowed: false,
                initialized: true,
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
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => self.atom(ty.clone(), b),
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
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let life = self.life(frame, life, b, depth + 1)?;
                let inner = self.recipe(frame, inner, b, depth + 1)?;
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
                if exact.ty.is_b0_scalar()
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
                .saturating_add(self.state.frames.len()),
            WORK,
        )?;
        let mut locals = Vec::new();
        for local in &frame.locals {
            b.spend(local.name.len(), WORK)?;
            Self::push(
                &mut locals,
                Local {
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
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
                alive: frame.alive,
                environment: frame.environment.clone(),
                expressions: frame.expressions.clone(),
                bindings: frame.bindings.clone(),
            },
        );
        Ok(())
    }
    fn scalar_literal(&mut self, text: &str, float: bool, b: &mut Budget) -> CompileResult<Term> {
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
        self.indexes
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
            ) {
                if found.replace(recipe).is_some() {
                    return Err(fail("original operation has ambiguous resolution sites"));
                }
            }
        }
        found.ok_or_else(|| fail("original call or closure has no exact resolution site"))
    }
    fn static_name(
        &self,
        caller: SemanticDefinitionId,
        name: &str,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        let definition = self.inventory.definition(caller, b)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body else {
            return Err(fail(
                "original callee lookup has no complete name inventory",
            ));
        };
        let mut found = None;
        for binding in names {
            b.spend(
                binding
                    .path
                    .iter()
                    .fold(name.len().saturating_add(1), |count, part| {
                        count.saturating_add(part.len())
                    }),
                WORK,
            )?;
            if binding.namespace != SemanticOriginalNamespace::Value
                || binding.path.len() != 1
                || binding.path[0] != name
            {
                continue;
            }
            let SemanticOriginalNameValue::Definition(target) = &binding.value else {
                return Err(unknown(
                    "original callee is a symbolic value requiring its complete callable premise",
                ));
            };
            self.inventory.definition(*target, b)?;
            if found.replace(*target).is_some() {
                return Err(fail(
                    "original value name has multiple declaration bindings",
                ));
            }
        }
        found.ok_or_else(|| {
            fail("original static callee is not in its actual lexical name inventory")
        })
    }
    fn scheme(
        &mut self,
        caller: &Frame,
        target: SemanticDefinitionId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Vec<Term>, Term)> {
        self.step(depth, b)?;
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
            return Err(fail("original callee has no complete declared scheme"));
        };
        if definition.parent.is_some()
            || !definition.environment.is_empty()
            || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
        {
            return Err(unknown(
                "original lexical/member call needs its complete declared scope and quantifier instantiation",
            ));
        }
        if !written.is_empty() && written.len() != definition.parameters.len() {
            return Err(fail(
                "original written actual vector has another mixed arity",
            ));
        }
        let mut formals = Vec::new();
        for (index, formal) in definition.parameters.iter().enumerate() {
            let SemanticFormal::Type { requirements } = formal else {
                return Err(unknown(
                    "original call needs its complete constant or lifetime actual inference",
                ));
            };
            self.premises_well_formed(target, requirements, b)?;
            let term = match written.get(index) {
                Some(CoreGenericArgument::Type(recipe)) => {
                    self.recipe(caller, recipe, b, depth + 1)?
                }
                None => self.var(b)?,
                _ => return Err(fail("original explicit actual changes the formal kind")),
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
            alive: true,
            environment: Vec::new(),
            expressions: BTreeMap::new(),
            bindings: BTreeMap::new(),
        };
        for parameter in &signature.parameters {
            let term = self.recipe(&frame, &parameter.ty, b, depth + 1)?;
            Self::push(&mut frame.parameters, term, b)?;
        }
        for formal in &definition.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                self.premise_needs(&frame, requirements, b)?;
            }
        }
        self.premise_needs(&frame, &definition.requirements, b)?;
        let result = self.recipe(&frame, &signature.result, b, depth + 1)?;
        Ok((frame.formals, frame.parameters, result))
    }
    fn expression(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        b.lookup(self.indexes.len(), WORK)?;
        self.indexes
            .get(&frame.definition)
            .ok_or_else(|| fail("original expression has no definition index"))?
            .get_expr(value.id, b)?;
        if frame.expressions.contains_key(&value.id) {
            return Err(fail("original expression identity is reused"));
        }
        let ty = match &value.kind {
            CoreExprKind::Integer(text) => self.scalar_literal(text, false, b)?,
            CoreExprKind::Float(text) => self.scalar_literal(text, true, b)?,
            CoreExprKind::Literal(value) if value.ty().is_b0_scalar() => {
                self.atom(value.ty(), b)?
            }
            CoreExprKind::Bool(_) => self.atom(Type::Bool, b)?,
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
            CoreExprKind::Tuple(fields) => {
                if fields.is_empty() {
                    self.atom(Type::Unit, b)?
                } else {
                    let mut terms = Vec::new();
                    for field in fields {
                        let term = self.expression(frame, field, b, depth + 1)?;
                        Self::push(&mut terms, term, b)?;
                    }
                    self.term(Kind::Tuple(terms), b)?
                }
            }
            CoreExprKind::Group(value) => self.expression(frame, value, b, depth + 1)?,
            CoreExprKind::Ascribed { ty, value } => {
                let actual = self.expression(frame, value, b, depth + 1)?;
                let expected = self.recipe(frame, ty, b, depth + 1)?;
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
                        Self::push(&mut self.scalar_captures, left, b)?;
                        self.atom(Type::Bool, b)?
                    }
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual => {
                        Self::push(&mut self.numeric, left, b)?;
                        self.atom(Type::Bool, b)?
                    }
                    BinaryOp::BitAnd
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
                let CoreExprKind::Name(name) = &operand.kind else {
                    return Err(unknown(
                        "original borrow needs its exact initialized place projection",
                    ));
                };
                let inner = self
                    .local(frame, name, b)?
                    .ok_or_else(|| fail("original borrow selects no lexical local"))?;
                let row = self.borrow_occurrence(frame.definition, value.id, b)?;
                // Read the original Name's type without moving its occupant.
                b.spend(frame.expressions.len().saturating_add(1), WORK)?;
                if frame.expressions.insert(operand.id, inner).is_some() {
                    return Err(fail("original borrowed place node is reused"));
                }
                let life = self.term(Kind::Life(Life::Implicit(frame.definition, row)), b)?;
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
                op: UnaryOp::Dereference,
                operand,
            } => {
                let address = self.expression(frame, operand, b, depth + 1)?;
                let root = self.root(address, b)?;
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
                if !matches!(
                    recipe.kind,
                    SemanticSiteRecipeKind::OriginalCall
                        | SemanticSiteRecipeKind::Call { .. }
                        | SemanticSiteRecipeKind::CallableCall { .. }
                ) {
                    return Err(fail("original call is linked to another operation kind"));
                }
                let mut head = callee.as_ref();
                while let CoreExprKind::Group(inner) = &head.kind {
                    self.step(depth, b)?;
                    head = inner;
                }
                let (head, written) = match &head.kind {
                    CoreExprKind::GenericApply { callee, arguments } => {
                        (callee.as_ref(), arguments.as_slice())
                    }
                    _ => (head, &[][..]),
                };
                let (parameters, result) = if let CoreExprKind::Name(name) = &head.kind {
                    if let Some(local) = self.local(frame, name, b)? {
                        if !written.is_empty() {
                            return Err(unknown(
                                "original generic callable needs its complete method instantiation",
                            ));
                        }
                        let local = self.root(local, b)?;
                        b.spend(
                            match &self.state.arena[local] {
                                Kind::Closure { parameters, .. } => parameters.len() + 1,
                                _ => 1,
                            },
                            WORK,
                        )?;
                        let Kind::Closure {
                            definition: target,
                            parameters,
                            result,
                        } = self.state.arena[local].clone()
                        else {
                            return Err(unknown(
                                "original indirect call needs its full callable premise",
                            ));
                        };
                        b.spend(self.state.calls.len().saturating_add(1), WORK)?;
                        self.state.calls.insert(
                            (frame.definition, value.id),
                            Call {
                                target,
                                actuals: Vec::new(),
                                closure: true,
                            },
                        );
                        (parameters, result)
                    } else {
                        let target = self.static_name(frame.definition, name, b)?;
                        let (actuals, parameters, result) =
                            self.scheme(frame, target, written, b, depth + 1)?;
                        b.spend(self.state.calls.len().saturating_add(actuals.len()), WORK)?;
                        self.state.calls.insert(
                            (frame.definition, value.id),
                            Call {
                                target,
                                actuals,
                                closure: false,
                            },
                        );
                        (parameters, result)
                    }
                } else {
                    return Err(unknown(
                        "original member or expression call needs its full source resolution",
                    ));
                };
                if parameters.len() != arguments.len() {
                    return Err(fail(
                        "original call value arity differs from its declared scheme",
                    ));
                }
                for (argument, parameter) in arguments.iter().zip(parameters) {
                    let argument = self.expression(frame, argument, b, depth + 1)?;
                    self.unify(argument, parameter, depth + 1, b)?;
                }
                result
            }
            CoreExprKind::Record { ty, fields } => {
                let term = self.recipe(frame, ty, b, depth + 1)?;
                self.record_fields(frame, term, fields, true, b, depth + 1)?;
                term
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                let term = self.expression(frame, base, b, depth + 1)?;
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
                let term = self.expression(frame, object, b, depth + 1)?;
                if let Some(owner) = owner {
                    let owner = self.recipe(frame, owner, b, depth + 1)?;
                    self.unify(term, owner, depth + 1, b)?;
                }
                self.member(term, member, b, depth + 1)?
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
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
                let mut names = Vec::new();
                for local in &frame.locals {
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
                let term = self.function(target, Some(&frame.locals), b, depth + 1)?;
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
                for (parameter, expected) in parameters.iter().zip(expected) {
                    if let Some(annotation) = &parameter.ty {
                        let annotation = self.recipe(frame, annotation, b, depth + 1)?;
                        self.unify(annotation, expected, depth + 1, b)?;
                    }
                }
                if let Some(annotation) = return_type {
                    let annotation = self.recipe(frame, annotation, b, depth + 1)?;
                    self.unify(annotation, result, depth + 1, b)?;
                }
                self.same_body(body, target, b)?;
                term
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expression(frame, condition, b, depth + 1)?;
                let boolean = self.atom(Type::Bool, b)?;
                self.unify(condition, boolean, depth + 1, b)?;
                let left = self.expression(frame, then_value, b, depth + 1)?;
                let right = self.expression(frame, else_value, b, depth + 1)?;
                self.unify(left, right, depth + 1, b)?;
                left
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
                    if let Some(annotation) = ty {
                        let expected = self.recipe(frame, annotation, b, depth + 1)?;
                        self.unify(local, expected, depth + 1, b)?;
                    }
                    if let Some(value) = initializer {
                        let value = self.expression(frame, value, b, depth + 1)?;
                        self.unify(local, value, depth + 1, b)?;
                    }
                    self.bind(frame, name, local, *mutable, b)?;
                    if initializer.is_none() {
                        if let Some(local) = frame.locals.last_mut() {
                            local.initialized = false;
                        }
                    }
                }
                CoreStatementKind::Expression(value) => {
                    let discarded = self.expression(frame, value, b, depth + 1)?;
                    if frame.alive {
                        Self::push(
                            &mut self.needs,
                            Need {
                                term: discarded,
                                predicate: Predicate::SyncDrop,
                            },
                            b,
                        )?;
                    }
                }
                CoreStatementKind::Return(value) => {
                    let value = match value {
                        Some(value) => self.expression(frame, value, b, depth + 1)?,
                        None => self.atom(Type::Unit, b)?,
                    };
                    self.unify(value, frame.result, depth + 1, b)?;
                    if frame.alive {
                        self.cleanup(&frame.locals, b)?;
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
                CoreStatementKind::Assign { target, value } => {
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
                    let expected = frame.locals[index].ty;
                    let value = self.expression(frame, value, b, depth + 1)?;
                    self.unify(value, expected, depth + 1, b)?;
                    if frame.alive {
                        let old = &frame.locals[index];
                        if old.initialized && !old.used {
                            Self::push(
                                &mut self.needs,
                                Need {
                                    term: old.ty,
                                    predicate: Predicate::SyncDrop,
                                },
                                b,
                            )?;
                        }
                        frame.locals[index].used = false;
                        frame.locals[index].initialized = true;
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
            self.cleanup(&frame.locals[scope..], b)?;
        }
        frame.locals.truncate(scope);
        Ok(())
    }
    fn function(
        &mut self,
        id: SemanticDefinitionId,
        inherited: Option<&[Local]>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if !self.active.insert(id) {
            return Err(fail("original lexical body graph is cyclic"));
        }
        let definition = self.inventory.definition(id, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        else {
            return Err(fail("original function omits its complete body"));
        };
        if !matches!(
            definition.kind,
            SemanticDefinitionKind::Function | SemanticDefinitionKind::Closure
        ) || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
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
        self.premises_well_formed(id, &definition.requirements, b)?;
        let mut frame = Frame {
            definition: id,
            formals: Vec::new(),
            parameters: Vec::new(),
            result: self.var(b)?,
            locals: Vec::new(),
            inherited: 0,
            alive: true,
            environment: Vec::new(),
            expressions: BTreeMap::new(),
            bindings: BTreeMap::new(),
        };
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            let term = self.rigid_formal(id, ordinal, formal, &frame.formals, b)?;
            Self::push(&mut frame.formals, term, b)?;
        }
        if let Some(inherited) = inherited {
            for local in inherited {
                self.bind(&mut frame, &local.name, local.ty, local.mutable, b)?;
            }
            frame.inherited = frame.locals.len();
            for local in &mut frame.locals {
                local.borrowed = true;
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
            let declared = self.recipe(&frame, &signature.parameters[index].ty, b, depth + 1)?;
            self.unify(frame.parameters[index], declared, depth + 1, b)?;
            if let Some(annotation) = &parameter.ty {
                let annotation = self.recipe(&frame, annotation, b, depth + 1)?;
                self.unify(frame.parameters[index], annotation, depth + 1, b)?;
            }
            let term = frame.parameters[index];
            self.bind(&mut frame, &parameter.name, term, false, b)?;
        }
        match body {
            SemanticOriginalFunctionBody::Defined(body) => {
                self.block(&mut frame, body, b, depth + 1)?
            }
            SemanticOriginalFunctionBody::External => {
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
                    if self.inventory.function(target, b)?.foreign.is_none() {
                        return Err(fail(
                            "actual implementation cannot claim an external original body",
                        ));
                    }
                }
            }
            SemanticOriginalFunctionBody::Declaration => {
                return Err(unknown(
                    "abstract original member needs its actual trait and implementation witness",
                ));
            }
        }
        if frame.alive && matches!(body, SemanticOriginalFunctionBody::Defined(_)) {
            self.cleanup(&frame.locals, b)?;
            let unit = self.atom(Type::Unit, b)?;
            self.unify(frame.result, unit, depth + 1, b)?;
        }
        let result = self.recipe(&frame, &signature.result, b, depth + 1)?;
        self.unify(frame.result, result, depth + 1, b)?;
        let mut parameter_terms = Vec::new();
        for parameter in &frame.parameters {
            Self::push(&mut parameter_terms, *parameter, b)?;
        }
        let term = self.term(
            Kind::Closure {
                definition: id,
                parameters: parameter_terms,
                result: frame.result,
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
        let definition = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(other),
            ..
        } = &definition.body
        else {
            return Err(fail("original closure definition omits its literal body"));
        };
        crate::dyn_metadata::recipe(body, b)?;
        crate::dyn_metadata::recipe(other, b)?;
        let mut body = body.clone();
        let mut other = other.clone();
        normalize_block(&mut body, b, 0)?;
        normalize_block(&mut other, b, 0)?;
        if body != other {
            return Err(fail(
                "original closure literal and child declaration have different operations",
            ));
        }
        Ok(())
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
            CoreStatementKind::Assign { target, value } => {
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
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
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
fn normalize_expr(
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
        | CoreExprKind::Unary { operand: value, .. }
        | CoreExprKind::Ascribed { value, .. } => normalize_expr(value, b, depth + 1)?,
        CoreExprKind::Binary { left, right, .. } => {
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
        CoreExprKind::Tuple(fields) => {
            for field in fields {
                normalize_expr(field, b, depth + 1)?;
            }
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
    let mut indexes = BTreeMap::new();
    for definition in inventory.definitions.values() {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) {
            let index = original_index::index(definition, b)?;
            b.lookup(indexes.len(), WORK)?;
            indexes.insert(definition.id, index);
        }
    }
    let mut solver = Solver {
        inventory,
        indexes,
        state: Checked {
            arena: Vec::new(),
            frames: BTreeMap::new(),
            calls: BTreeMap::new(),
            names_at: BTreeMap::new(),
        },
        active: BTreeSet::new(),
        creators: BTreeMap::new(),
        numeric: Vec::new(),
        scalar_captures: Vec::new(),
        literals: Vec::new(),
        integers: Vec::new(),
        needs: Vec::new(),
    };
    for definition in &program.definitions {
        b.spend(1, WORK)?;
        if matches!(definition.body, SemanticDefinitionBody::Nominal { .. }) {
            let mut arguments = Vec::new();
            for (ordinal, formal) in definition.parameters.iter().enumerate() {
                let term = solver.rigid_formal(definition.id, ordinal, formal, &arguments, b)?;
                Solver::push(&mut arguments, term, b)?;
            }
            solver.application(definition.id, arguments, b, 0)?;
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
            solver.function(definition.id, None, b, 0)?;
        }
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
    for literal in &solver.literals {
        let root = solver.root(literal.term, b)?;
        if matches!(solver.state.arena[root], Kind::Infer(None)) {
            solver.state.arena[root] =
                Kind::Atom(if literal.float { Type::F64 } else { Type::I32 });
        }
    }
    // Numeric constraints arise from original operators, not actual headers.
    for term in &solver.numeric {
        let root = solver.root(*term, b)?;
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
        let root = solver.root(literal.term, b)?;
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
    for term in &solver.integers {
        let root = solver.root(*term, b)?;
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if integer_width(ty).is_some()) {
            return Err(fail(
                "original bit or remainder operation has a noninteger type",
            ));
        }
    }
    for term in &solver.scalar_captures {
        let root = solver.root(*term, b)?;
        if !matches!(&solver.state.arena[root],Kind::Atom(ty) if ty.is_b0_scalar()) {
            return Err(unknown(
                "original capture or assignment needs its complete abstract value and cleanup proof",
            ));
        }
    }
    for need in &solver.needs {
        if !solver.satisfies(need.term, need.predicate, b, 0)? {
            return Err(fail(match need.predicate {
                Predicate::Copy => "original repeated use requires Copy in its declaration context",
                Predicate::SyncDrop => {
                    "original live cleanup edge requires SyncDrop in its declaration context"
                }
                Predicate::Value => "original actual does not establish Value",
                Predicate::StoreElement => "original actual does not establish StoreElement",
            }));
        }
    }
    b.spend(module.functions.len(), WORK)?;
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
                    name: local.name.clone(),
                    ty: local.ty,
                    mutable: local.mutable,
                    used: local.used,
                    borrowed: local.borrowed,
                    initialized: local.initialized,
                },
                b,
            )?;
        }
        Ok(output)
    }
    fn cleanup(&mut self, locals: &[Local], b: &mut Budget) -> CompileResult<()> {
        for local in locals {
            b.spend(1, WORK)?;
            if local.initialized && !local.used && !local.borrowed {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: local.ty,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
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
                    _ => {
                        return Err(unknown(
                            "original sealed provider or thread premise needs its complete declaration/body proof",
                        ));
                    }
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
        for requirement in requirements {
            b.spend(1, WORK)?;
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
        for requirement in requirements {
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
        for requirement in requirements {
            b.spend(1, WORK)?;
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
        match &self.state.arena[term] {
            Kind::Atom(ty) => Ok(ty.is_b0_scalar()
                && (!matches!(predicate, Predicate::StoreElement) || *ty != Type::Unit)),
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
                    return Err(unknown(
                        "original nominal Store qualification needs its complete layout and teardown proof",
                    ));
                }
                for field in fields {
                    if !self.satisfies(*field, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Kind::Closure { .. } => match predicate {
                Predicate::Copy | Predicate::StoreElement => Ok(false),
                // All actually used captures are separately checked scalar
                // values/borrowed scalar heads. This grants no head authority.
                Predicate::Value | Predicate::SyncDrop => Ok(true),
            },
            Kind::Infer(_) => Err(unknown(
                "original structural predicate has an unresolved type variable",
            )),
        }
    }
}
fn entails(available: Predicate, wanted: Predicate) -> bool {
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
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                let mut actuals = Vec::new();
                for term in arguments {
                    let ty = self.materialize(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut actuals, ty, b)?;
                }
                let mut physical = Vec::new();
                for term in fields {
                    let ty = self.materialize(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut physical, ty, b)?;
                }
                nominal::materialize(*definition, &actuals, &physical, inventory, b, depth + 1)
            }
            Kind::Closure { definition, .. } => {
                let mut selected = None;
                for instance in &program.instances {
                    b.spend(1, WORK)?;
                    if instance.definition != *definition
                        || (instance.parent != Some(context.id) && instance.id != context.id)
                    {
                        continue;
                    }
                    let Some(environment) = &instance.environment else {
                        continue;
                    };
                    let SemanticClosureTarget::Typed(id) = environment.closure else {
                        return Err(fail("original closure type retains a Core target"));
                    };
                    if selected.is_some_and(|previous| previous != id) {
                        return Err(fail(
                            "original closure Type has multiple actual entry identities",
                        ));
                    }
                    selected = Some(id);
                }
                selected.map(Type::Closure).ok_or_else(|| {
                    fail("original closure Type has no exact actual creator instance")
                })
            }
            Kind::Infer(_) => Err(unknown(
                "original Type remains unresolved independently of actual caller headers",
            )),
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
        b.spend(self.frames.len().saturating_add(1), WORK)?;
        let frame = self
            .frames
            .get(&context.definition)
            .ok_or_else(|| fail("original Type source has no checked declaration frame"))?;
        let term = match source {
            SemanticOriginalTypeSource::Expression(id) => { b.spend(frame.expressions.len().saturating_add(1),WORK)?; frame.expressions.get(id).copied() },
            SemanticOriginalTypeSource::Binding(id) => { b.spend(frame.bindings.len().saturating_add(1),WORK)?; frame.bindings.get(id).copied() },
            SemanticOriginalTypeSource::Parameter(index) => frame.parameters.get(*index as usize).copied(),
            SemanticOriginalTypeSource::Result => Some(frame.result),
            SemanticOriginalTypeSource::NameAt { at:SemanticOriginalNode::Expression(at),name } => {
                b.spend(self.names_at.len().saturating_add(1),WORK)?;
                let rows = self.names_at.get(&(context.definition,*at)).ok_or_else(|| fail("original name Type is not at its checked creator"))?;
                let mut selected = None;
                for (key,term) in rows { b.spend(key.len().saturating_add(name.len()),WORK)?; if key == name { selected = Some(*term); } }
                selected
            }
            _ => return Err(unknown("original projected binder or constant carrier needs its complete inference evidence")),
        }.ok_or_else(|| fail("original Type query selects no actual original node or binding"))?;
        self.materialize(term, context, program, inventory, b, 0)
    }
    pub(super) fn call(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<(SemanticDefinitionId, bool, Vec<Type>)> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "actual BodyCall has no complete original function body",
            ));
        };
        b.spend(site_nodes.len(), WORK)?;
        let mut selected = None;
        for node in site_nodes {
            if node.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(id) = node.node else {
                return Err(fail("actual BodyCall selects an original statement"));
            };
            if selected.replace(id).is_some() {
                return Err(fail("actual BodyCall has multiple original nodes"));
            }
        }
        let node = selected.ok_or_else(|| fail("actual BodyCall has no original node"))?;
        b.spend(self.calls.len().saturating_add(1), WORK)?;
        let call = self
            .calls
            .get(&(context.definition, node))
            .ok_or_else(|| fail("actual BodyCall has no independently resolved original callee"))?;
        let mut actuals = Vec::new();
        for actual in &call.actuals {
            let actual = self.materialize(*actual, context, program, inventory, b, 0)?;
            Solver::push(&mut actuals, actual, b)?;
        }
        Ok((call.target, call.closure, actuals))
    }
}
