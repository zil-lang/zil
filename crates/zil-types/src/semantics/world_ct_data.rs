//! Recover the checked nominal identities of a reified CT value from the
//! original query's actual TypeUse. Reification adds fresh physical producers;
//! it cannot add source declarations, permissions, or constructor premises.
use super::*;
use std::collections::BTreeSet;

type Constructors = BTreeMap<ValueId, SharedType>;

pub(super) fn constructors(
    world: &mut World<'_, '_>,
    cfg: &flow::Shape,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    function: &TypedFunction,
    sites: &[&SemanticSite],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Constructors> {
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof.as_ref().ok_or_else(|| bad("reified CT data has no checked original program"))?;
    world.pay(index::search(proof.compile_time_results.len()))?;
    if !proof.compile_time_results.range((original.id, zil_base::NodeId(0))..)
        .next().is_some_and(|(owner, _)| *owner == original.id)
    {
        return Ok(BTreeMap::new());
    }
    let mut constructors = BTreeMap::new();
    let mut sources = BTreeSet::new();
    world.pay(sites.len())?;
    for site in sites {
        let SemanticSiteAction::TypeUse { subject, .. } = &site.action else { continue; };
        let Some(SemanticOriginalNode::Expression(query)) =
            occurrences::original_node(world, original.id, site.recipe)?
        else { continue; };
        world.pay(index::search(proof.compile_time_results.len()))?;
        if !proof.compile_time_results.contains(&(original.id, query)) {
            continue;
        }
        world.pay(index::search(sources.len()))?;
        if !sources.insert(query) {
            return Err(bad("one original CT result has multiple actual data roots"));
        }
        let SemanticSiteRecipeKind::TypeUse { expected: recipe @ SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(node),
        } } = &world.original_recipe(original, site.recipe)?.kind else {
            return Err(bad("reified CT result loses its exact original expression recipe"));
        };
        if *node != query {
            return Err(bad("reified CT result changes its original query identity"));
        }
        let SemanticOperand::Value(value) = subject else {
            return Err(bad("reified CT result needs its actual value producer"));
        };
        let SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::Before } = site.at else {
            return Err(bad("reified CT result needs its actual use boundary"));
        };
        let physical = function.values.get(value.0 as usize)
            .ok_or_else(|| bad("reified CT result selects an absent whole type"))?;
        let expected = world.physical_type(instance.id, recipe, physical)?;
        let ty = World::type_of(&expected)?;
        if !matches!(ty, Type::Record { .. } | Type::Tuple(_) | Type::Array { .. } | Type::Enum { .. }) {
            // Scalar TypeUse and any enclosing non-data adaptation are checked
            // by the ordinary world. They introduce no nominal data producer.
            continue;
        }
        serializable(world, ty, 0)?;
        let zero = world.zero(ty)?;
        world.same(instance.id, &expected, &zero, implicit)?;

        let mut seen = BTreeSet::new();
        let (producer_block, producer, _) = cfg.producer(world, function, *value)?;
        if producer_block != block || producer + 1 != instruction as usize {
            return Err(bad("reified CT result does not immediately follow its actual producer"));
        }
        let mut tree = Tree { cfg, function, block, seen: &mut seen, constructors: &mut constructors };
        tree.visit(world, *value, ty, instruction as usize, 0)?;
    }
    Ok(constructors)
}

fn serializable(world: &mut World<'_, '_>, ty: &Type, depth: usize) -> R<()> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types, "reified CT result shape depth exceeded", None));
    }
    match ty {
        Type::Unit | Type::Bool => {}
        ty if ty.is_integer() || ty.is_float() => {}
        Type::Tuple(fields) => {
            world.pay(fields.len())?;
            for ty in fields {
                serializable(world, ty, depth + 1)?;
            }
        }
        Type::Record { fields, .. } => {
            world.pay(fields.len())?;
            for (_, ty) in fields {
                serializable(world, ty, depth + 1)?;
            }
        }
        Type::Array { element, .. } => serializable(world, element, depth + 1)?,
        Type::Enum { variants, .. } => {
            world.pay(variants.len())?;
            for (_, fields) in variants {
                world.pay(fields.len())?;
                for ty in fields {
                    serializable(world, ty, depth + 1)?;
                }
            }
        }
        _ => return Err(bad("reified CT result is outside the closed serializable value domain")),
    }
    Ok(())
}

struct Tree<'a> {
    cfg: &'a flow::Shape,
    function: &'a TypedFunction,
    block: BlockId,
    seen: &'a mut BTreeSet<ValueId>,
    constructors: &'a mut Constructors,
}

impl Tree<'_> {
    fn visit(
        &mut self,
        world: &mut World<'_, '_>,
        value: ValueId,
        expected: &Type,
        before: usize,
        depth: usize,
    ) -> R<()> {
        world.pay(index::search(self.seen.len()))?;
        if depth > world.limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types, "reified CT producer depth exceeded", None));
        }
        if !self.seen.insert(value) {
            return Err(bad("reified CT data reuses an actual producer"));
        }
        let (block, ordinal, instruction) = self.cfg.producer(world, self.function, value)?;
        if block != self.block || ordinal >= before {
            return Err(bad("reified CT data producer is outside its result construction"));
        }
        let actual = &self.function.values[value.0 as usize];
        world.pay(inference::type_snapshot_work(expected, instruction.span, world.limits)?)?;
        world.pay(inference::type_snapshot_work(actual, instruction.span, world.limits)?)?;
        if actual != expected {
            return Err(bad("reified CT data changes an exact original field whole type"));
        }
        match (expected, &instruction.kind) {
            (Type::Tuple(types), InstructionKind::MakeAggregate { fields }) => {
                self.fields(world, fields, types.iter(), ordinal, depth)?;
            }
            (ty @ Type::Record { fields: types, .. }, InstructionKind::MakeAggregate { fields }) => {
                self.fields(world, fields, types.iter().map(|(_, ty)| ty), ordinal, depth)?;
                self.record_nominal(world, value, ty)?;
            }
            (Type::Array { element, length }, InstructionKind::MakeAggregate { fields })
                if fields.len() == *length as usize =>
            {
                self.fields(world, fields, std::iter::repeat_n(element.as_ref(), fields.len()), ordinal, depth)?;
            }
            (ty @ Type::Enum { variants, .. }, InstructionKind::MakeVariant { variant, fields }) => {
                let (_, types) = variants.get(*variant as usize)
                    .ok_or_else(|| bad("reified CT value selects an absent original variant"))?;
                self.fields(world, fields, types.iter(), ordinal, depth)?;
                self.record_nominal(world, value, ty)?;
            }
            (ty, InstructionKind::Constant(constant)) if constant.ty() == *ty => {}
            _ => return Err(bad("reified CT result changes the serialized data producer")),
        }
        Ok(())
    }

    fn fields<'a>(
        &mut self,
        world: &mut World<'_, '_>,
        fields: &[ValueId],
        types: impl ExactSizeIterator<Item = &'a Type>,
        before: usize,
        depth: usize,
    ) -> R<()> {
        if fields.len() != types.len() {
            return Err(bad("reified CT value changes the complete original constructor arity"));
        }
        world.pay(fields.len())?;
        for (field, ty) in fields.iter().zip(types) {
            self.visit(world, *field, ty, before, depth + 1)?;
        }
        Ok(())
    }

    fn record_nominal(&mut self, world: &mut World<'_, '_>, value: ValueId, ty: &Type) -> R<()> {
        world.pay(index::search(self.constructors.len()))?;
        let checked = world.zero(ty)?;
        if self.constructors.insert(value, checked).is_some() {
            return Err(bad("reified CT nominal producer belongs to several original query results"));
        }
        Ok(())
    }
}


pub(super) fn check_operands(
    world: &mut World<'_, '_>,
    instance: SemanticInstanceId,
    root: &SharedType,
    variant: Option<u32>,
    fields: &[ValueId],
    values: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let inputs = match (World::type_of(root)?, variant) {
        (Type::Record { fields, .. }, None) => fields.iter().map(|(_, ty)| ty).collect::<Vec<_>>(),
        (Type::Enum { variants, .. }, Some(ordinal)) => variants
            .get(ordinal as usize)
            .ok_or_else(|| bad("reified CT world changes its selected variant"))?
            .1.iter().collect(),
        _ => return Err(bad("reified CT world changes its nominal constructor form")),
    };
    if fields.len() != inputs.len() {
        return Err(bad("reified CT world changes its complete constructor arity"));
    }
    world.pay(fields.len())?;
    for (operand, ty) in fields.iter().zip(inputs) {
        let expected = world.zero(ty)?;
        let actual = world.get(values, *operand)?;
        world.same(instance, &actual, &expected, implicit)?;
    }
    Ok(())
}
