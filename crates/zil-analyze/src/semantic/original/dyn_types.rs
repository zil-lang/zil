//! Original dynamic type domains. These terms retain the complete mixed
//! application and Context; an interface ID or vtable is never a premise.
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
pub(super) struct ContextShape {
    domain: Vec<Sequence>,
    ports: Vec<Port>,
    retained: Vec<Retain>,
    nominal: Vec<Nominal>,
    // Slots are canonicalized in the original domain, before actual images can
    // coincide. They continue to distinguish phantom and repeated actuals.
    leaves: Vec<Term>,
}
#[derive(Clone)]
pub(super) enum Context {
    Closed,
    Type(Term),
    Shape(ContextShape),
}
#[derive(Clone)]
pub(super) enum AssociatedValue {
    Type(Term),
    Const { carrier: Term, value: Term },
}
#[derive(Clone)]
pub(super) struct Associated {
    pub(super) declaration: LifeDeclarationId,
    pub(super) arguments: Vec<Term>,
    pub(super) member: u32,
    pub(super) value: AssociatedValue,
}
#[derive(Clone)]
pub(super) struct Application {
    pub(super) declaration: LifeDeclarationId,
    pub(super) arguments: Vec<Term>,
}
pub(super) struct Template {
    pub(super) owner: SemanticDefinitionId,
    pub(super) owned: bool,
    pub(super) recipe: SemanticDynInterfaceRecipe,
    pub(super) term: Term,
}
#[derive(Clone)]
pub(super) struct DynType {
    pub(super) owned: bool,
    pub(super) declaration: LifeDeclarationId,
    pub(super) arguments: Vec<Term>,
    pub(super) inherited: Vec<Application>,
    pub(super) associated: Vec<Associated>,
    pub(super) context: Context,
}
impl AssociatedValue {
    fn terms(&self) -> impl Iterator<Item = Term> + '_ {
        let (first, second) = match self {
            Self::Type(term) => (*term, None),
            Self::Const { carrier, value } => (*carrier, Some(*value)),
        };
        std::iter::once(first).chain(second)
    }
}
impl DynType {
    pub(super) fn terms(&self) -> impl Iterator<Item = Term> + '_ {
        self.arguments
            .iter()
            .copied()
            .chain(
                self.inherited
                    .iter()
                    .flat_map(|row| row.arguments.iter().copied()),
            )
            .chain(
                self.associated
                    .iter()
                    .flat_map(|row| row.arguments.iter().copied().chain(row.value.terms())),
            )
            .chain(
                match &self.context {
                    Context::Closed => &[][..],
                    Context::Type(term) => std::slice::from_ref(term),
                    Context::Shape(shape) => shape.leaves.as_slice(),
                }
                .iter()
                .copied(),
            )
    }
    pub(super) fn work(&self) -> usize {
        let mut work = self.arguments.len().saturating_add(1);
        for row in &self.inherited {
            work = work.saturating_add(row.arguments.len().saturating_add(1));
        }
        for row in &self.associated {
            work = work.saturating_add(row.arguments.len().saturating_add(3));
        }
        if let Context::Shape(shape) = &self.context {
            fn sequence(rows: &[Sequence]) -> usize {
                rows.iter().fold(0usize, |n, row| {
                    n.saturating_add(match row {
                        Sequence::One(_) => 1,
                        Sequence::All(_, path) => path.len().saturating_add(1),
                    })
                })
            }
            work = work
                .saturating_add(shape.leaves.len())
                .saturating_add(sequence(&shape.domain));
            for row in &shape.ports {
                work = work.saturating_add(match row {
                    Port::One(..) => 1,
                    Port::From(_, rows) => sequence(rows).saturating_add(1),
                });
            }
            for row in &shape.retained {
                work = work.saturating_add(match row {
                    Retain::One(path, ..) => path.len().saturating_add(1),
                    Retain::All(path, rows) => {
                        path.len().saturating_add(sequence(rows)).saturating_add(1)
                    }
                });
            }
            for row in &shape.nominal {
                work = work.saturating_add(match row {
                    Nominal::One(path, ..) => path.len().saturating_add(1),
                    Nominal::From(path, _, rows) => {
                        path.len().saturating_add(sequence(rows)).saturating_add(1)
                    }
                });
            }
        }
        work
    }
    fn map_terms(&mut self, mut map: impl FnMut(Term) -> CompileResult<Term>) -> CompileResult<()> {
        for term in &mut self.arguments {
            *term = map(*term)?;
        }
        for row in &mut self.inherited {
            for term in &mut row.arguments {
                *term = map(*term)?;
            }
        }
        for row in &mut self.associated {
            for term in &mut row.arguments {
                *term = map(*term)?;
            }
            match &mut row.value {
                AssociatedValue::Type(term) => *term = map(*term)?,
                AssociatedValue::Const { carrier, value } => {
                    *carrier = map(*carrier)?;
                    *value = map(*value)?;
                }
            }
        }
        match &mut self.context {
            Context::Closed => {}
            Context::Type(term) => *term = map(*term)?,
            Context::Shape(shape) => {
                for term in &mut shape.leaves {
                    *term = map(*term)?;
                }
            }
        }
        Ok(())
    }
    fn same_structure(&self, other: &Self) -> bool {
        self.owned == other.owned
            && self.declaration == other.declaration
            && self.arguments.len() == other.arguments.len()
            && self.inherited.len() == other.inherited.len()
            && self.inherited.iter().zip(&other.inherited).all(|(a, z)| {
                a.declaration == z.declaration && a.arguments.len() == z.arguments.len()
            })
            && self.associated.len() == other.associated.len()
            && self.associated.iter().zip(&other.associated).all(|(a, z)| {
                a.declaration == z.declaration
                    && a.member == z.member
                    && a.arguments.len() == z.arguments.len()
                    && matches!(
                        (&a.value, &z.value),
                        (AssociatedValue::Type(_), AssociatedValue::Type(_))
                            | (AssociatedValue::Const { .. }, AssociatedValue::Const { .. })
                    )
            })
            && match (&self.context, &other.context) {
                (Context::Closed, Context::Closed) | (Context::Type(_), Context::Type(_)) => true,
                (Context::Shape(a), Context::Shape(z)) => {
                    a.domain == z.domain
                        && a.ports == z.ports
                        && a.retained == z.retained
                        && a.nominal == z.nominal
                        && a.leaves.len() == z.leaves.len()
                }
                _ => false,
            }
    }
}
fn frame(definition: SemanticDefinitionId, formals: Vec<Term>) -> Frame {
    Frame {
        definition,
        formals,
        parameters: Vec::new(),
        result: 0,
        locals: Vec::new(),
        inherited: 0,
        captured: BTreeMap::new(),
        alive: true,
        environment: Vec::new(),
        expressions: BTreeMap::new(),
        bindings: BTreeMap::new(),
    }
}
impl Solver<'_, '_> {
    fn dyn_shape_leaf(
        &self,
        leaves: &mut Vec<Term>,
        term: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<usize> {
        for (slot, previous) in leaves.iter().enumerate() {
            if self.same_original(*previous, term, b, depth + 1)? {
                return Ok(slot);
            }
        }
        let slot = leaves.len();
        Self::push(leaves, term, b)?;
        Ok(slot)
    }
    fn dyn_sequence(
        &mut self,
        frame: &Frame,
        source: &SemanticClassSequenceRecipe,
        leaves: &mut Vec<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Sequence>> {
        self.step(depth, b)?;
        let mut rows = Vec::new();
        for row in &source.parts {
            let row = match row {
                SemanticClassSequencePart::One(life) => {
                    let term = self.life(frame, life, b, depth + 1)?;
                    Sequence::One(self.dyn_shape_leaf(leaves, term, b, depth + 1)?)
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    let term = self.recipe(frame, ty, b, depth + 1)?;
                    b.spend(path.len(), WORK)?;
                    Sequence::All(
                        self.dyn_shape_leaf(leaves, term, b, depth + 1)?,
                        path.clone(),
                    )
                }
            };
            Self::push(&mut rows, row, b)?;
        }
        Ok(rows)
    }
    fn dyn_shape(
        &mut self,
        frame: &Frame,
        source: &SemanticRetainedShapeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<ContextShape> {
        self.step(depth, b)?;
        let mut shape = ContextShape::default();
        shape.domain = self.dyn_sequence(frame, &source.domain, &mut shape.leaves, b, depth + 1)?;
        for row in &source.ports {
            let row = match row {
                SemanticPortPart::One { leaf, mode, class } => {
                    let leaf = self.recipe(frame, leaf, b, depth + 1)?;
                    let class = self.life(frame, class, b, depth + 1)?;
                    Port::One(
                        self.dyn_shape_leaf(&mut shape.leaves, leaf, b, depth + 1)?,
                        *mode,
                        self.dyn_shape_leaf(&mut shape.leaves, class, b, depth + 1)?,
                    )
                }
                SemanticPortPart::FromType { ty, classes } => {
                    let ty = self.recipe(frame, ty, b, depth + 1)?;
                    let classes =
                        self.dyn_sequence(frame, classes, &mut shape.leaves, b, depth + 1)?;
                    Port::From(
                        self.dyn_shape_leaf(&mut shape.leaves, ty, b, depth + 1)?,
                        classes,
                    )
                }
            };
            Self::push(&mut shape.ports, row, b)?;
        }
        for row in &source.retained {
            let row = match row {
                SemanticRetainedPart::One {
                    path,
                    child_class,
                    parent,
                } => {
                    b.spend(path.len(), WORK)?;
                    let term = self.life(frame, parent, b, depth + 1)?;
                    Retain::One(
                        path.clone(),
                        *child_class,
                        self.dyn_shape_leaf(&mut shape.leaves, term, b, depth + 1)?,
                    )
                }
                SemanticRetainedPart::All { path, parents } => {
                    b.spend(path.len(), WORK)?;
                    Retain::All(
                        path.clone(),
                        self.dyn_sequence(frame, parents, &mut shape.leaves, b, depth + 1)?,
                    )
                }
            };
            Self::push(&mut shape.retained, row, b)?;
        }
        for row in &source.nominal {
            let row = match row {
                SemanticNominalPart::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    b.spend(path.len(), WORK)?;
                    let term = self.life(frame, class, b, depth + 1)?;
                    Nominal::One(
                        path.clone(),
                        *declaration,
                        *argument,
                        self.dyn_shape_leaf(&mut shape.leaves, term, b, depth + 1)?,
                    )
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    b.spend(path.len(), WORK)?;
                    let term = self.recipe(frame, ty, b, depth + 1)?;
                    let classes =
                        self.dyn_sequence(frame, classes, &mut shape.leaves, b, depth + 1)?;
                    Nominal::From(
                        path.clone(),
                        self.dyn_shape_leaf(&mut shape.leaves, term, b, depth + 1)?,
                        classes,
                    )
                }
            };
            Self::push(&mut shape.nominal, row, b)?;
        }
        Ok(shape)
    }
    pub(super) fn dyn_type(
        &mut self,
        original: &Frame,
        owned: bool,
        recipe: &SemanticDynInterfaceRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let arguments = self.trait_arguments(original, &recipe.application, b, depth + 1)?;
        let mut inherited = Vec::new();
        self.dyn_inherited(
            recipe.application.declaration,
            &arguments,
            &mut Vec::new(),
            &mut inherited,
            b,
            depth + 1,
        )?;
        let context = match &recipe.context {
            SemanticDynContextRecipe::Closed => Context::Closed,
            SemanticDynContextRecipe::Type(ty) => {
                Context::Type(self.recipe(original, ty, b, depth + 1)?)
            }
            SemanticDynContextRecipe::Shape(shape) => {
                Context::Shape(self.dyn_shape(original, shape, b, depth + 1)?)
            }
        };
        let mut associated = Vec::<Associated>::new();
        for binding in &recipe.associated {
            let actuals = self.trait_arguments(original, &binding.application, b, depth + 1)?;
            if !self.dyn_super_application(
                recipe.application.declaration,
                &arguments,
                binding.application.declaration,
                &actuals,
                b,
                depth + 1,
            )? {
                return Err(fail(
                    "original Dyn associated binding selects no full inherited application",
                ));
            }
            let raw = self
                .inventory
                .original_dyn(binding.application.declaration, b)?;
            let member = raw
                .members
                .get(binding.member as usize)
                .ok_or_else(|| fail("original Dyn associated binding selects no member"))?;
            let value = match (&member.kind, &binding.value) {
                (DynMemberKind::Type, SemanticAssociatedRecipe::Type(ty)) => {
                    AssociatedValue::Type(self.recipe(original, ty, b, depth + 1)?)
                }
                (DynMemberKind::Constant, SemanticAssociatedRecipe::Const { carrier, value }) => {
                    let carrier = self.recipe(original, carrier, b, depth + 1)?;
                    let scope = declaration_scope(
                        self.program,
                        binding.application.declaration,
                        self.inventory,
                        b,
                    )?;
                    let target = frame(scope, actuals.clone());
                    let row = raw
                        .associated_definitions
                        .as_ref()
                        .and_then(|rows| rows.iter().find(|row| row.member == binding.member))
                        .ok_or_else(|| {
                            unknown(
                                "original Dyn associated constant omits its declaration carrier",
                            )
                        })?;
                    let raw_carrier = row.carrier.as_ref().ok_or_else(|| {
                        fail("original Dyn associated constant omits its carrier")
                    })?;
                    let expected = self.raw_dyn_type(&target, raw_carrier, None, b, depth + 1)?;
                    self.unify(carrier, expected, depth + 1, b)?;
                    AssociatedValue::Const {
                        carrier,
                        value: self.constant(original, value, carrier, b, depth + 1)?,
                    }
                }
                _ => {
                    return Err(fail(
                        "original Dyn associated binding changes its declared member kind",
                    ));
                }
            };
            for old in &associated {
                if old.declaration == binding.application.declaration
                    && old.member == binding.member
                    && self.dyn_same_rows(&old.arguments, &actuals, b, depth + 1)?
                {
                    return Err(fail("original Dyn repeats an associated equation"));
                }
            }
            Self::push(
                &mut associated,
                Associated {
                    declaration: binding.application.declaration,
                    arguments: actuals,
                    member: binding.member,
                    value,
                },
                b,
            )?;
        }
        self.dyn_parameter_needs(recipe.application.declaration, &arguments, b, depth + 1)?;
        for application in &inherited {
            self.dyn_parameter_needs(
                application.declaration,
                &application.arguments,
                b,
                depth + 1,
            )?;
        }
        for row in &associated {
            self.dyn_parameter_needs(row.declaration, &row.arguments, b, depth + 1)?;
            self.dyn_associated_needs(row, b, depth + 1)?;
        }
        self.dyn_associated_complete(
            recipe.application.declaration,
            &arguments,
            &associated,
            b,
            depth + 1,
        )?;
        let ty = DynType {
            owned,
            declaration: recipe.application.declaration,
            arguments,
            inherited,
            associated,
            context,
        };
        self.dyn_domain_covered(&ty, b, depth + 1)?;
        let term = self.term(Kind::Dyn(ty), b)?;
        // Cache only the declaration's complete original domain. A concrete
        // call or a temporary trait Self specialization cannot certify it.
        if self.dyn_original_frame(original, b)? {
            crate::dyn_metadata::recipe(recipe, b)?;
            Self::push(
                &mut self.state.dyn_templates,
                Template {
                    owner: original.definition,
                    owned,
                    recipe: recipe.clone(),
                    term,
                },
                b,
            )?;
        }
        Ok(term)
    }
    fn dyn_original_frame(&self, context: &Frame, b: &mut Budget) -> CompileResult<bool> {
        let declaration = self.inventory.definition(context.definition, b)?;
        if !self.role_frames.is_empty() || declaration.parameters.len() != context.formals.len() {
            return Ok(false);
        }
        for (owner, _) in &self.trait_state.selves {
            b.spend(self.trait_state.selves.len().saturating_mul(2), WORK)?;
            let first = self
                .trait_state
                .selves
                .iter()
                .find(|(scope, _)| scope == owner)
                .map(|(_, term)| term);
            let last = self
                .trait_state
                .selves
                .iter()
                .rev()
                .find(|(scope, _)| scope == owner)
                .map(|(_, term)| term);
            if first != last {
                return Ok(false);
            }
        }
        for (ordinal, (formal, actual)) in declaration
            .parameters
            .iter()
            .zip(&context.formals)
            .enumerate()
        {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| fail("original Dyn template formal ordinal overflows"))?;
            let original = match (formal, &self.state.arena[self.root(*actual, b)?]) {
                (SemanticFormal::Type { .. }, Kind::Rigid(owner, index)) => {
                    *owner == context.definition && *index == ordinal
                }
                (SemanticFormal::Lifetime, Kind::Life(Life::Formal(owner, index))) => {
                    *owner == context.definition && *index == ordinal
                }
                (
                    SemanticFormal::Const { .. },
                    Kind::ConstRigid {
                        owner,
                        ordinal: index,
                        ..
                    },
                ) => *owner == context.definition && *index == ordinal,
                _ => false,
            };
            if !original {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn dyn_inherited(
        &mut self,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        active: &mut Vec<LifeDeclarationId>,
        result: &mut Vec<Application>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        b.spend(active.len().saturating_add(1), WORK)?;
        if active.contains(&declaration) {
            return Err(fail(
                "original Dyn inheritance contains a declaration cycle",
            ));
        }
        Self::push(active, declaration, b)?;
        let raw = self.inventory.original_dyn(declaration, b)?;
        if raw.kind != DynDeclarationKind::Trait {
            return Err(fail(
                "original Dyn inherited application selects a nontrait declaration",
            ));
        }
        let scope = declaration_scope(self.program, declaration, self.inventory, b)?;
        b.spend(arguments.len(), WORK)?;
        let context = frame(scope, arguments.to_vec());
        for edge in raw.direct_supertraits.as_ref().ok_or_else(|| {
            unknown("original Dyn inheritance omits its complete direct declaration recipes")
        })? {
            let actuals = self.raw_dyn_application(&context, edge, b, depth + 1)?;
            b.spend(active.len(), WORK)?;
            if active.contains(&edge.declaration) {
                return Err(fail(
                    "original Dyn inheritance contains a declaration cycle",
                ));
            }
            let mut duplicate = false;
            for old in result.iter() {
                if old.declaration == edge.declaration
                    && self.dyn_same_rows(&old.arguments, &actuals, b, depth + 1)?
                {
                    duplicate = true;
                }
            }
            if duplicate {
                continue;
            }
            Self::push(
                result,
                Application {
                    declaration: edge.declaration,
                    arguments: actuals.clone(),
                },
                b,
            )?;
            self.dyn_inherited(edge.declaration, &actuals, active, result, b, depth + 1)?;
        }
        active.pop();
        Ok(())
    }
    pub(super) fn dyn_unify(
        &mut self,
        left: &DynType,
        right: &DynType,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        b.spend(left.work().saturating_add(right.work()), WORK)?;
        if !left.same_structure(right) {
            return Err(fail(
                "original Dyn equation changes its full application or Context shape",
            ));
        }
        for (a, z) in left.terms().zip(right.terms()) {
            self.unify(a, z, depth + 1, b)?;
        }
        Ok(())
    }
    pub(super) fn dyn_same(
        &self,
        left: &DynType,
        right: &DynType,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        b.spend(left.work().saturating_add(right.work()), WORK)?;
        if !left.same_structure(right) {
            return Ok(false);
        }
        for (a, z) in left.terms().zip(right.terms()) {
            if !self.same_original(a, z, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(super) fn dyn_instantiate(
        &mut self,
        mut ty: DynType,
        owner: SemanticDefinitionId,
        actuals: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        ty.map_terms(|term| self.instantiate(term, owner, actuals, b, depth + 1))?;
        self.term(Kind::Dyn(ty), b)
    }
    pub(super) fn dyn_open_invocations(
        &mut self,
        mut ty: DynType,
        owner: SemanticDefinitionId,
        actuals: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        ty.map_terms(|term| self.open_invocations(term, owner, actuals, b, depth + 1))?;
        self.term(Kind::Dyn(ty), b)
    }
    fn dyn_super_application(
        &mut self,
        source: LifeDeclarationId,
        arguments: &[Term],
        target: LifeDeclarationId,
        expected: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if source == target && self.dyn_same_rows(arguments, expected, b, depth + 1)? {
            return Ok(true);
        }
        let raw = self.inventory.original_dyn(source, b)?;
        let edges = raw
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| unknown("original Dyn supertrait application inventory is unknown"))?;
        let scope = declaration_scope(self.program, source, self.inventory, b)?;
        b.spend(arguments.len(), WORK)?;
        let context = frame(scope, arguments.to_vec());
        for edge in edges {
            let actuals = self.raw_dyn_application(&context, edge, b, depth + 1)?;
            if self.dyn_super_application(
                edge.declaration,
                &actuals,
                target,
                expected,
                b,
                depth + 1,
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Solver<'_, '_> {
    fn dyn_same_rows(
        &self,
        left: &[Term],
        right: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if left.len() != right.len() {
            return Ok(false);
        }
        for (a, z) in left.iter().zip(right) {
            if !self.same_original(*a, *z, b, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn dyn_pattern_scope(
        &self,
        declaration: LifeDeclarationId,
        b: &mut Budget,
    ) -> CompileResult<SemanticDefinitionId> {
        let mut found = None;
        for row in self.inventory.definitions.values() {
            b.spend(1, WORK)?;
            if matches!(row.body,SemanticDefinitionBody::Nominal { declaration:id,.. } if id==declaration)
                && found.replace(row.id).is_some()
            {
                return Err(fail(
                    "original raw nominal has competing complete declarations",
                ));
            }
        }
        if let Some(scope) = found {
            return Ok(scope);
        }
        declaration_scope(self.program, declaration, self.inventory, b)
    }
    fn dyn_raw_formal(
        &self,
        context: &Frame,
        ordinal: u32,
        method: bool,
        kind: u8,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let declaration = self.inventory.definition(context.definition, b)?;
        let target = if !method && matches!(declaration.kind, SemanticDefinitionKind::Method) {
            let parent = declaration.parent.ok_or_else(|| {
                fail("original raw declaration parameter has no lexical declaration")
            })?;
            self.state.frames.get(&parent).ok_or_else(|| {
                fail("original raw declaration parameter has no checked parent frame")
            })?
        } else {
            context
        };
        let row = self.inventory.definition(target.definition, b)?;
        if method && !matches!(row.body, SemanticDefinitionBody::FunctionWithBody { .. }) {
            return Err(fail(
                "original raw method parameter has no method declaration",
            ));
        }
        if !matches!(
            (kind, row.parameters.get(ordinal as usize)),
            (0, Some(SemanticFormal::Type { .. }))
                | (1, Some(SemanticFormal::Lifetime))
                | (2, Some(SemanticFormal::Const { .. }))
        ) {
            return Err(fail(
                "original raw parameter changes its full ordinal or mixed kind",
            ));
        }
        target
            .formals
            .get(ordinal as usize)
            .copied()
            .ok_or_else(|| fail("original raw full-ordinal actual is absent"))
    }
    pub(super) fn raw_dyn_life(
        &mut self,
        context: &Frame,
        recipe: &DynLifePattern,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            DynLifePattern::DeclarationParameter(index) => {
                self.dyn_raw_formal(context, *index, false, 1, b)
            }
            DynLifePattern::MethodParameter(index) => {
                self.dyn_raw_formal(context, *index, true, 1, b)
            }
            DynLifePattern::Invocation(index) => self.life(
                context,
                &SemanticLifeRecipe::Invocation(*index),
                b,
                depth + 1,
            ),
            DynLifePattern::ImplicitDeclaration(index) => self.life(
                context,
                &SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: *index,
                },
                b,
                depth + 1,
            ),
            DynLifePattern::Bound { .. } => Err(unknown(
                "original raw quantified lifetime requires its full bound role frame",
            )),
        }
    }
    pub(super) fn raw_dyn_application(
        &mut self,
        context: &Frame,
        recipe: &DynSelfPattern,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Term>> {
        self.step(depth, b)?;
        let scope = self.dyn_pattern_scope(recipe.declaration, b)?;
        let declaration = self.inventory.definition(scope, b)?;
        if declaration.parameters.len() != recipe.arguments.len() {
            return Err(fail(
                "original raw application changes its complete mixed arity",
            ));
        }
        let mut arguments = Vec::new();
        for (formal, actual) in declaration.parameters.iter().zip(&recipe.arguments) {
            let term = match (formal, actual) {
                (SemanticFormal::Type { .. }, DynPatternActual::Type(ty)) => {
                    self.raw_dyn_type(context, ty, None, b, depth + 1)?
                }
                (SemanticFormal::Lifetime, DynPatternActual::Life(life)) => {
                    self.raw_dyn_life(context, life, b, depth + 1)?
                }
                (SemanticFormal::Const { carrier }, DynPatternActual::Const(constant)) => {
                    b.spend(arguments.len(), WORK)?;
                    let target = frame(scope, arguments.clone());
                    let carrier = self.recipe(&target, carrier, b, depth + 1)?;
                    self.raw_dyn_constant(context, constant, carrier, b, depth + 1)?
                }
                _ => {
                    return Err(fail(
                        "original raw application changes its full Type/Const/Life actual kind",
                    ));
                }
            };
            Self::push(&mut arguments, term, b)?;
        }
        Ok(arguments)
    }
    pub(super) fn raw_dyn_constant(
        &mut self,
        context: &Frame,
        recipe: &DynConstPattern,
        carrier: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            DynConstPattern::Value(value) => self.constant(
                context,
                &SemanticConstRecipe::Value(value.clone()),
                carrier,
                b,
                depth + 1,
            ),
            DynConstPattern::IntegerLiteral(value) => self.constant(
                context,
                &SemanticConstRecipe::IntegerLiteral(value.clone()),
                carrier,
                b,
                depth + 1,
            ),
            DynConstPattern::FloatLiteral(value) => self.constant(
                context,
                &SemanticConstRecipe::FloatLiteral(value.clone()),
                carrier,
                b,
                depth + 1,
            ),
            DynConstPattern::Parameter(ordinal) | DynConstPattern::MethodParameter(ordinal) => {
                let term = self.dyn_raw_formal(
                    context,
                    *ordinal,
                    matches!(recipe, DynConstPattern::MethodParameter(_)),
                    2,
                    b,
                )?;
                let expected = match &self.state.arena[self.root(term, b)?] {
                    Kind::ConstRigid { carrier, .. } => *carrier,
                    Kind::ConstValue(value) => self.atom(value.ty(), b)?,
                    _ => return Err(fail("original raw constant parameter has no constant term")),
                };
                self.unify(carrier, expected, depth + 1, b)?;
                Ok(term)
            }
            // Symbolic arithmetic/projection must first exist in the original
            // constant algebra. Raw structure and an actual folded result do
            // not prove this expression for every original actual.
            DynConstPattern::Unary { .. }
            | DynConstPattern::Binary { .. }
            | DynConstPattern::Convert { .. }
            | DynConstPattern::AssociatedProjection { .. } => Err(unknown(
                "original raw constant expression requires its complete symbolic carrier-aware algebra",
            )),
        }
    }
    pub(super) fn raw_dyn_type(
        &mut self,
        context: &Frame,
        recipe: &DynTypePattern,
        receiver: Option<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        match recipe {
            DynTypePattern::SelfType => {
                receiver.ok_or_else(|| fail("original raw Self cannot establish its own receiver"))
            }
            DynTypePattern::Parameter(ordinal) => {
                self.dyn_raw_formal(context, *ordinal, false, 0, b)
            }
            DynTypePattern::MethodParameter(ordinal) => {
                self.dyn_raw_formal(context, *ordinal, true, 0, b)
            }
            DynTypePattern::Concrete(ty) => {
                self.recipe(context, &SemanticTypeRecipe::Atom(ty.clone()), b, depth + 1)
            }
            DynTypePattern::Semantic { scope, ty } => {
                if *scope == context.definition {
                    return self.recipe(context, ty, b, depth + 1);
                }
                let mut owner = self.inventory.definition(context.definition, b)?.parent;
                for _ in 0..=b.partial_depth {
                    b.spend(1, WORK)?;
                    let Some(id) = owner else {
                        return Err(fail(
                            "original raw semantic type changes its exact lexical owner",
                        ));
                    };
                    if id == *scope {
                        let parent = self.state.frames.get(scope).ok_or_else(|| {
                            fail("original raw semantic type has no checked lexical frame")
                        })?;
                        b.spend(parent.formals.len(), WORK)?;
                        let parent = frame(*scope, parent.formals.clone());
                        return self.recipe(&parent, ty, b, depth + 1);
                    }
                    owner = self.inventory.definition(id, b)?.parent;
                }
                Err(resource(
                    "original raw semantic owner exceeds its lexical depth",
                ))
            }
            DynTypePattern::Nominal(application) => {
                let definition = self.dyn_pattern_scope(application.declaration, b)?;
                let arguments = self.raw_dyn_application(context, application, b, depth + 1)?;
                self.application(definition, arguments, b, depth + 1)
            }
            DynTypePattern::AssociatedProjection {
                receiver: subject,
                application,
                member,
            } => {
                let subject = self.raw_dyn_type(context, subject, receiver, b, depth + 1)?;
                let arguments = self.raw_dyn_application(context, application, b, depth + 1)?;
                self.associated_type(
                    context,
                    subject,
                    application.declaration,
                    arguments,
                    *member,
                    b,
                    depth + 1,
                )
            }
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => {
                let life = self.raw_dyn_life(
                    context,
                    lifetime.as_ref().ok_or_else(|| {
                        unknown("original raw address omits its actual lifetime occurrence")
                    })?,
                    b,
                    depth + 1,
                )?;
                let inner = self.raw_dyn_type(context, inner, receiver, b, depth + 1)?;
                self.term(
                    Kind::Address {
                        mutable: *mutable,
                        life,
                        inner,
                    },
                    b,
                )
            }
            DynTypePattern::Desc { lifetime, inner } => {
                let life = self.raw_dyn_life(
                    context,
                    lifetime.as_ref().ok_or_else(|| {
                        unknown("original raw Desc omits its actual lifetime occurrence")
                    })?,
                    b,
                    depth + 1,
                )?;
                let inner = self.raw_dyn_type(context, inner, receiver, b, depth + 1)?;
                self.term(Kind::Desc { life, inner }, b)
            }
            DynTypePattern::Tuple(fields) => {
                let mut terms = Vec::new();
                for field in fields {
                    let term = self.raw_dyn_type(context, field, receiver, b, depth + 1)?;
                    Self::push(&mut terms, term, b)?;
                }
                if terms.is_empty() {
                    self.atom(Type::Unit, b)
                } else {
                    self.term(Kind::Tuple(terms), b)
                }
            }
            DynTypePattern::Array { element, length } => {
                let element = self.raw_dyn_type(context, element, receiver, b, depth + 1)?;
                let carrier = self.atom(Type::Usize, b)?;
                let length = self.raw_dyn_constant(context, length, carrier, b, depth + 1)?;
                self.term(Kind::Array { element, length }, b)
            }
            DynTypePattern::Slice(inner) => {
                let inner = self.raw_dyn_type(context, inner, receiver, b, depth + 1)?;
                self.term(Kind::Slice(inner), b)
            }
            DynTypePattern::Managed(inner) => {
                let inner = self.raw_dyn_type(context, inner, receiver, b, depth + 1)?;
                self.term(Kind::Wrapped(Wrapper::Managed, inner), b)
            }
            DynTypePattern::OwnedStore(inner) => {
                let inner = self.raw_dyn_type(context, inner, receiver, b, depth + 1)?;
                Self::push(
                    &mut self.needs,
                    Need {
                        term: inner,
                        predicate: Predicate::StoreElement,
                    },
                    b,
                )?;
                let store = self.term(Kind::Wrapped(Wrapper::Store, inner), b)?;
                self.term(Kind::Wrapped(Wrapper::Managed, store), b)
            }
            DynTypePattern::ErrorUnion { error, result } => {
                let error = self.raw_dyn_type(context, error, receiver, b, depth + 1)?;
                let result = self.raw_dyn_type(context, result, receiver, b, depth + 1)?;
                self.term(Kind::ErrorUnion { error, result }, b)
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Origin {
    One(usize),
    All(usize, Vec<LifeTypeProjection>, u32),
}
struct Image {
    origin: Origin,
    class: Class,
}
struct Materialize<'a, 'i> {
    checked: &'a Checked,
    shape: &'a ContextShape,
    context: &'a SemanticInstance,
    program: &'a SemanticModule,
    inventory: &'a Inventory<'i>,
}
impl Materialize<'_, '_> {
    fn leaf(&self, slot: usize) -> CompileResult<Term> {
        self.shape
            .leaves
            .get(slot)
            .copied()
            .ok_or_else(|| fail("original Dyn Context leaf is absent"))
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
        rows: &[Sequence],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Image>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original Dyn Context expansion exceeds its complete depth",
            ));
        }
        let mut result = Vec::new();
        for row in rows {
            match row {
                Sequence::One(slot) => Solver::push(
                    &mut result,
                    Image {
                        origin: Origin::One(*slot),
                        class: self.life(*slot, b)?,
                    },
                    b,
                )?,
                Sequence::All(slot, path) => {
                    let value = self.value(*slot, b, depth + 1)?;
                    if path.is_empty() {
                        for (ordinal, class) in value.classes.into_iter().enumerate() {
                            let ordinal = u32::try_from(ordinal)
                                .map_err(|_| fail("original Dyn Context child class overflow"))?;
                            Solver::push(
                                &mut result,
                                Image {
                                    origin: Origin::All(*slot, path.clone(), ordinal),
                                    class,
                                },
                                b,
                            )?;
                        }
                    } else {
                        b.spend(
                            path.len()
                                .saturating_mul(value.life.retained.len().saturating_add(1)),
                            WORK,
                        )?;
                        let mut children: Vec<_> = value
                            .life
                            .retained
                            .iter()
                            .filter(|row| row.path == *path)
                            .collect();
                        b.spend(
                            children
                                .len()
                                .saturating_mul(children.len().saturating_add(1)),
                            WORK,
                        )?;
                        children.sort_by_key(|row| row.class);
                        if children.is_empty() {
                            return Err(unknown(
                                "original Dyn Context projection has no complete child domain",
                            ));
                        }
                        for (ordinal, child) in children.into_iter().enumerate() {
                            if child.class as usize != ordinal {
                                return Err(fail(
                                    "original Dyn Context child domain is sparse or repeated",
                                ));
                            }
                            let class=value.classes.get(child.binder as usize).ok_or_else(||fail("original Dyn Context child binding leaves its parent domain"))?;
                            Solver::push(
                                &mut result,
                                Image {
                                    origin: Origin::All(*slot, path.clone(), child.class),
                                    class: class.clone(),
                                },
                                b,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(result)
    }
    fn binder(&self, domain: &[Image], image: &Image, b: &mut Budget) -> CompileResult<u32> {
        b.spend(domain.len(), WORK)?;
        let mut found = None;
        for (index, candidate) in domain.iter().enumerate() {
            if candidate.origin == image.origin {
                if candidate.class != image.class {
                    return Err(fail(
                        "original Dyn Context occurrence has contradictory actual images",
                    ));
                }
                if found.replace(index).is_some() {
                    return Err(fail(
                        "original Dyn Context repeats an original domain selector",
                    ));
                }
            }
        }
        u32::try_from(found.ok_or_else(|| {
            unknown("original Dyn Context occurrence needs its exact original domain selector")
        })?)
        .map_err(|_| fail("original Dyn Context ordinal overflow"))
    }
    fn one(&self, slot: usize, b: &mut Budget) -> CompileResult<Image> {
        Ok(Image {
            origin: Origin::One(slot),
            class: self.life(slot, b)?,
        })
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
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                Some(if *is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            _ => None,
        }
        .ok_or_else(|| fail("original Dyn Context port selects no complete physical child"))?;
    }
    Ok(ty)
}
fn push_port(
    ports: &mut Vec<RetainedBorrowPort>,
    port: RetainedBorrowPort,
    b: &mut Budget,
) -> CompileResult<()> {
    // Public uppers normalize equal typed ports in first-occurrence order.
    for previous in ports.iter() {
        crate::dyn_metadata::ty(&previous.leaf, b)?;
        crate::dyn_metadata::ty(&port.leaf, b)?;
        if *previous == port {
            return Ok(());
        }
    }
    Solver::push(ports, port, b)
}
impl Checked {
    pub(super) fn dyn_context_value(
        &self,
        ty: &DynType,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(RetainedLifeShape, Vec<Class>)> {
        b.spend(ty.work(), WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original Dyn Context value exceeds its complete depth",
            ));
        }
        match &ty.context {
            Context::Closed => Ok((
                RetainedLifeShape {
                    binder_count: 0,
                    ports: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                },
                Vec::new(),
            )),
            Context::Type(term) => {
                let value = self.value(*term, context, program, inventory, b, depth + 1)?;
                if value.life.binder_count as usize != value.classes.len() {
                    return Err(fail(
                        "original Dyn Context omits part of its intrinsic class domain",
                    ));
                }
                let mut shape = RetainedLifeShape {
                    binder_count: value.life.binder_count,
                    ports: Vec::new(),
                    retained: value.life.retained,
                    nominal_arguments: value.life.nominal_arguments,
                };
                for slot in &value.life.slots {
                    let leaf = physical_leaf(&value.ty, &slot.path, b)?;
                    crate::dyn_metadata::ty(leaf, b)?;
                    push_port(
                        &mut shape.ports,
                        RetainedBorrowPort {
                            leaf: leaf.clone(),
                            mode: slot.mode,
                            binder: slot.binder,
                        },
                        b,
                    )?;
                }
                crate::dyn_metadata::retained_shape(&shape, b)?;
                Ok((shape, value.classes))
            }
            Context::Shape(source) => {
                let materialize = Materialize {
                    checked: self,
                    shape: source,
                    context,
                    program,
                    inventory,
                };
                let domain = materialize.sequence(&source.domain, b, depth + 1)?;
                // Domain multiplicity is semantic; equal actual class images
                // do not merge originally distinct domain selectors.
                for (index, image) in domain.iter().enumerate() {
                    b.spend(index.saturating_add(1), WORK)?;
                    if domain[..index].iter().any(|old| old.origin == image.origin) {
                        return Err(fail(
                            "original Dyn Context repeats its original class selector",
                        ));
                    }
                }
                let binder_count = u32::try_from(domain.len())
                    .map_err(|_| fail("original Dyn Context domain overflow"))?;
                let mut shape = RetainedLifeShape {
                    binder_count,
                    ports: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                };
                for port in &source.ports {
                    match port {
                        Port::One(leaf, mode, class) => {
                            let leaf = materialize.value(*leaf, b, depth + 1)?;
                            let image = materialize.one(*class, b)?;
                            let binder = materialize.binder(&domain, &image, b)?;
                            push_port(
                                &mut shape.ports,
                                RetainedBorrowPort {
                                    leaf: leaf.ty,
                                    mode: *mode,
                                    binder,
                                },
                                b,
                            )?;
                        }
                        Port::From(ty, classes) => {
                            let value = materialize.value(*ty, b, depth + 1)?;
                            let images = materialize.sequence(classes, b, depth + 1)?;
                            if images.len() != value.classes.len() {
                                return Err(fail(
                                    "original Dyn Context port expansion changes its complete child domain",
                                ));
                            }
                            for slot in &value.life.slots {
                                let leaf = physical_leaf(&value.ty, &slot.path, b)?;
                                crate::dyn_metadata::ty(leaf, b)?;
                                let image = images.get(slot.binder as usize).ok_or_else(|| {
                                    fail("original Dyn Context port child class is absent")
                                })?;
                                let binder = materialize.binder(&domain, image, b)?;
                                push_port(
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
                for part in &source.retained {
                    match part {
                        Retain::One(path, class, parent) => {
                            let image = materialize.one(*parent, b)?;
                            let binder = materialize.binder(&domain, &image, b)?;
                            b.spend(path.len(), WORK)?;
                            Solver::push(
                                &mut shape.retained,
                                StorageRetainedBinding {
                                    path: path.clone(),
                                    class: *class,
                                    binder,
                                },
                                b,
                            )?;
                        }
                        Retain::All(path, parents) => {
                            let images = materialize.sequence(parents, b, depth + 1)?;
                            for (class, image) in images.iter().enumerate() {
                                let class = u32::try_from(class).map_err(|_| {
                                    fail("original Dyn Context retained child ordinal overflow")
                                })?;
                                let binder = materialize.binder(&domain, image, b)?;
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
                    }
                }
                for part in &source.nominal {
                    match part {
                        Nominal::One(path, declaration, argument, class) => {
                            let image = materialize.one(*class, b)?;
                            let binder = materialize.binder(&domain, &image, b)?;
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
                        Nominal::From(path, ty, classes) => {
                            let value = materialize.value(*ty, b, depth + 1)?;
                            let images = materialize.sequence(classes, b, depth + 1)?;
                            if images.len() != value.classes.len() {
                                return Err(fail(
                                    "original Dyn Context nominal expansion changes its complete child domain",
                                ));
                            }
                            for row in value.life.nominal_arguments {
                                let image = images.get(row.binder as usize).ok_or_else(|| {
                                    fail("original Dyn Context nominal child class is absent")
                                })?;
                                let binder = materialize.binder(&domain, image, b)?;
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
                crate::dyn_metadata::retained_shape(&shape, b)?;
                let mut classes = Vec::new();
                for image in domain {
                    Solver::push(&mut classes, image.class, b)?;
                }
                Ok((shape, classes))
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Domain {
    Life(Term),
    Type(Term),
}
impl Solver<'_, '_> {
    fn dyn_domain(
        &self,
        term: Term,
        domain: &mut Vec<Domain>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let term = self.type_root(term, b)?;
        match &self.state.arena[term] {
            Kind::Atom(_) | Kind::ConstValue(_) => Ok(()),
            Kind::Life(_) | Kind::LifeVariable(_) => Self::push(domain, Domain::Life(term), b),
            Kind::ConstRigid { carrier, .. } => self.dyn_domain(*carrier, domain, b, depth + 1),
            Kind::Address { life, inner, .. }
            | Kind::Desc { life, inner }
            | Kind::MutexGuard {
                life,
                payload: inner,
            } => {
                self.dyn_domain(*life, domain, b, depth + 1)?;
                self.dyn_domain(*inner, domain, b, depth + 1)
            }
            Kind::Slice(element) | Kind::Array { element, .. } | Kind::Wrapped(_, element) => {
                self.dyn_domain(*element, domain, b, depth + 1)
            }
            Kind::Tuple(fields) => {
                for field in fields {
                    self.dyn_domain(*field, domain, b, depth + 1)?;
                }
                Ok(())
            }
            Kind::Nominal {
                arguments, fields, ..
            } => {
                for field in arguments.iter().chain(fields) {
                    self.dyn_domain(*field, domain, b, depth + 1)?;
                }
                Ok(())
            }
            Kind::ErrorUnion { error, result } => {
                self.dyn_domain(*error, domain, b, depth + 1)?;
                self.dyn_domain(*result, domain, b, depth + 1)
            }
            Kind::Closure { environment, .. } => {
                for field in environment {
                    self.dyn_domain(*field, domain, b, depth + 1)?;
                }
                Ok(())
            }
            Kind::Dyn(ty) => self.dyn_context_domain(&ty.context, domain, b, depth + 1),
            Kind::Task(_) => {
                // The retained shape's symbolic AllClasses domain is opaque
                // here. Preserve the whole original Task domain as a unit.
                if self.capability(term, SemanticBuiltinTrait::ClosedEnvironment, b, depth + 1)? {
                    Ok(())
                } else {
                    Self::push(domain, Domain::Type(term), b)
                }
            }
            Kind::Code { .. } | Kind::Rigid(..) => {
                if self.capability(term, SemanticBuiltinTrait::ClosedEnvironment, b, depth + 1)? {
                    Ok(())
                } else {
                    Self::push(domain, Domain::Type(term), b)
                }
            }
            Kind::Associated { .. } | Kind::SelfType(_) => {
                Self::push(domain, Domain::Type(term), b)
            }
            Kind::Infer(_) => Err(unknown(
                "original Dyn class domain contains an unresolved Type",
            )),
            Kind::CallResult { hidden, .. } => self.dyn_domain(*hidden, domain, b, depth + 1),
        }
    }
    fn dyn_project_domain(
        &self,
        mut term: Term,
        path: &[LifeTypeProjection],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        for projection in path {
            self.step(depth.saturating_add(path.len()), b)?;
            term = match (&self.state.arena[self.type_root(term, b)?], projection) {
                (
                    Kind::Address { inner, .. } | Kind::Desc { inner, .. },
                    LifeTypeProjection::Pointee,
                ) => *inner,
                (
                    Kind::Array { element, .. } | Kind::Slice(element) | Kind::Wrapped(_, element),
                    LifeTypeProjection::Element,
                ) => *element,
                (Kind::Nominal { arguments, .. }, LifeTypeProjection::TypeArgument(index)) => {
                    *arguments.get(*index as usize).ok_or_else(|| {
                        fail("original Dyn class projection selects no full mixed argument")
                    })?
                }
                (
                    Kind::Tuple(fields) | Kind::Nominal { fields, .. },
                    LifeTypeProjection::Place(PlaceProjection::Field(index)),
                ) => *fields
                    .get(*index as usize)
                    .ok_or_else(|| fail("original Dyn class projection selects no field"))?,
                (
                    Kind::ErrorUnion { error, result },
                    LifeTypeProjection::Place(PlaceProjection::ErrorArm(error_arm)),
                ) => {
                    if *error_arm {
                        *error
                    } else {
                        *result
                    }
                }
                (
                    Kind::Code { parameters, .. } | Kind::Closure { parameters, .. },
                    LifeTypeProjection::Parameter(index),
                ) => *parameters
                    .get(*index as usize)
                    .ok_or_else(|| fail("original Dyn class projection selects no parameter"))?,
                (
                    Kind::Code { result, .. } | Kind::Closure { result, .. },
                    LifeTypeProjection::Result,
                ) => *result,
                _ => {
                    return Err(unknown(
                        "original Dyn projected class domain requires its complete symbolic child signature",
                    ));
                }
            };
        }
        Ok(term)
    }
    fn dyn_context_domain(
        &self,
        context: &Context,
        domain: &mut Vec<Domain>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        match context {
            Context::Closed => Ok(()),
            Context::Type(term) => self.dyn_domain(*term, domain, b, depth + 1),
            Context::Shape(shape) => {
                for row in &shape.domain {
                    match row {
                        Sequence::One(slot) => Self::push(
                            domain,
                            Domain::Life(*shape.leaves.get(*slot).ok_or_else(|| {
                                fail("original Dyn Context domain leaf is absent")
                            })?),
                            b,
                        )?,
                        Sequence::All(slot, path) => {
                            let term = *shape.leaves.get(*slot).ok_or_else(|| {
                                fail("original Dyn Context domain leaf is absent")
                            })?;
                            let term = self.dyn_project_domain(term, path, b, depth + 1)?;
                            self.dyn_domain(term, domain, b, depth + 1)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
    fn dyn_domain_covered(&self, ty: &DynType, b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.step(depth, b)?;
        let mut available = Vec::new();
        self.dyn_context_domain(&ty.context, &mut available, b, depth + 1)?;
        let mut required = Vec::new();
        for argument in &ty.arguments {
            self.dyn_domain(*argument, &mut required, b, depth + 1)?;
        }
        for row in &ty.inherited {
            for argument in &row.arguments {
                self.dyn_domain(*argument, &mut required, b, depth + 1)?;
            }
        }
        for row in &ty.associated {
            for argument in row.arguments.iter().copied().chain(row.value.terms()) {
                self.dyn_domain(argument, &mut required, b, depth + 1)?;
            }
        }
        for need in required {
            let mut found = false;
            for have in &available {
                match (need, *have) {
                    (Domain::Life(a), Domain::Life(z)) | (Domain::Type(a), Domain::Type(z)) => {
                        found |= self.same_original(a, z, b, depth + 1)?;
                    }
                    _ => {}
                }
            }
            if !found {
                return Err(fail(
                    "original Dyn Context omits a persistent trait or associated class domain, including phantom actuals",
                ));
            }
        }
        Ok(())
    }
    fn dyn_parameter_needs(
        &mut self,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let scope = declaration_scope(self.program, declaration, self.inventory, b)?;
        let original = self.inventory.definition(scope, b)?;
        b.spend(arguments.len(), WORK)?;
        let target = frame(scope, arguments.to_vec());
        for formal in &original.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                self.premise_needs(&target, requirements, b)?;
            }
        }
        Ok(())
    }
}

impl Solver<'_, '_> {
    fn dyn_associated_complete(
        &mut self,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        bindings: &[Associated],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let raw = self.inventory.original_dyn(declaration, b)?;
        for (ordinal, member) in raw.members.iter().enumerate() {
            b.spend(1, WORK)?;
            if matches!(member.kind, DynMemberKind::Method(_)) {
                continue;
            }
            let member =
                u32::try_from(ordinal).map_err(|_| fail("original Dyn member ordinal overflow"))?;
            let mut found = false;
            for row in bindings {
                if row.declaration == declaration
                    && row.member == member
                    && self.dyn_same_rows(arguments, &row.arguments, b, depth + 1)?
                {
                    found = true;
                }
            }
            if !found {
                return Err(unknown(
                    "original Dyn associated member needs its complete fixed equation, including any declaration default",
                ));
            }
        }
        let scope = declaration_scope(self.program, declaration, self.inventory, b)?;
        b.spend(arguments.len(), WORK)?;
        let context = frame(scope, arguments.to_vec());
        for edge in raw
            .direct_supertraits
            .as_ref()
            .ok_or_else(|| unknown("original Dyn supertrait inventory is unknown"))?
        {
            let actuals = self.raw_dyn_application(&context, edge, b, depth + 1)?;
            self.dyn_associated_complete(edge.declaration, &actuals, bindings, b, depth + 1)?;
        }
        Ok(())
    }
    fn dyn_associated_needs(
        &mut self,
        binding: &Associated,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let raw = self.inventory.original_dyn(binding.declaration, b)?;
        let row = raw
            .associated_definitions
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.member == binding.member))
            .ok_or_else(|| {
                unknown("original Dyn associated binding omits its complete declaration bounds")
            })?;
        let scope = declaration_scope(self.program, binding.declaration, self.inventory, b)?;
        b.spend(binding.arguments.len(), WORK)?;
        let context = frame(scope, binding.arguments.clone());
        for requirement in &row.requirements {
            let DynObligation::Structural {
                receiver: DynTypePattern::Semantic { scope: owner, ty },
                bound,
            } = requirement
            else {
                return Err(unknown(
                    "original Dyn associated equation needs its complete trait, callable or dependent equality theorem",
                ));
            };
            if *owner != scope {
                return Err(fail(
                    "original Dyn associated bound changes its exact declaration scope",
                ));
            }
            let SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } = ty.as_ref()
            else {
                return Err(unknown(
                    "original Dyn associated bound needs its complete substituted original subject",
                ));
            };
            if !matches!(
                receiver.as_ref(),
                SemanticTypeRecipe::SelfType {
                    scope: SemanticScopeRef::Ancestor(0)
                }
            ) || application.declaration != binding.declaration
                || *member != binding.member
            {
                return Err(unknown(
                    "original Dyn associated bound needs its exact original member equation",
                ));
            }
            let actuals = self.trait_arguments(&context, application, b, depth + 1)?;
            if !self.dyn_same_rows(&actuals, &binding.arguments, b, depth + 1)? {
                return Err(fail(
                    "original Dyn associated bound changes its complete mixed application",
                ));
            }
            let AssociatedValue::Type(term) = binding.value else {
                return Err(fail("original Dyn type bound selects a constant member"));
            };
            let predicate = match bound {
                DynStructuralBound::Copy => Predicate::Copy,
                DynStructuralBound::Value => Predicate::Value,
                DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                DynStructuralBound::StoreElement => Predicate::StoreElement,
            };
            Self::push(&mut self.needs, Need { term, predicate }, b)?;
        }
        Ok(())
    }
}
