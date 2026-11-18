//! Bind nominal data materialized by a CT query to its original result type
//! and to the actual consecutive producers at that query's use boundary.
use super::*;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reified_data(
        &self,
        module: &TypedModule,
        instance: &SemanticInstance,
        definition: &SemanticDefinition,
        function: &TypedFunction,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<BTreeMap<ValueId, bool>> {
        let original = self.original.borrow();
        let original = original
            .as_ref()
            .ok_or_else(|| bad("CT data lacks checked original bodies"))?;
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &definition.body else {
            return Err(bad("CT data owner lacks its complete original body"));
        };
        let mut producers = BTreeMap::new();
        let mut queries = BTreeSet::new();
        for &(ordinal, node) in original.compile_time_uses(definition.id, b)? {
            b.spend(1)?;
            let recipe = sites
                .get(ordinal)
                .ok_or_else(|| bad("CT use lacks its original recipe"))?;
            let observed = self.site_index.recipe(
                module,
                instance.id,
                recipe.id,
                sites::Action::TypeUse,
                b,
            )?;
            let [site] = observed.as_slice() else {
                if observed.is_empty() {
                    // A nested stage may have no runtime materialization.
                    continue;
                }
                return Err(bad("one CT expression has multiple actual TypeUse rows"));
            };
            let SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Value(root),
                ..
            } = site.action
            else {
                return Err(bad("CT result TypeUse must select an actual value"));
            };
            let ty = function
                .values
                .get(root.0 as usize)
                .ok_or_else(|| bad("CT result TypeUse selects an absent actual value"))?;
            if !matches!(
                ty,
                Type::Tuple(_) | Type::Array { .. } | Type::Record { .. } | Type::Enum { .. }
            ) {
                // Scalar TypeUse retains its ordinary actual value check.
                continue;
            }
            let SemanticSiteRecipeKind::TypeUse {
                expected:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Expression(expression),
                    },
            } = &recipe.kind
            else {
                return Err(bad(
                    "CT data use lacks its exact original result type recipe",
                ));
            };
            b.spend(tree_lookup(queries.len()))?;
            if *expression != node || !queries.insert(node) {
                return Err(bad(
                    "CT data use changes or repeats its original expression",
                ));
            }
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge: SemanticSiteEdge::Before,
            } = site.at
            else {
                return Err(bad("CT data use lacks its actual materialization boundary"));
            };
            let expected =
                self.site_expected(module, instance, site, environment, external, active, b)?;
            dynamic_metadata::ty(ty, b)?;
            if expected.ty != *ty
                || expected.life != empty_life()
                || !expected.classes.is_empty()
                || !expected.origins.is_empty()
                || !expected.statics.is_empty()
            {
                return Err(bad(
                    "materialized CT data changes the original whole type or domain",
                ));
            }
            serializable(ty, b)?;
            self.reified_producers(function, block, instruction, (root, ty), &mut producers, b)?;
        }
        Ok(producers)
    }

    fn reified_producers(
        &self,
        function: &TypedFunction,
        block: BlockId,
        mut boundary: u32,
        root: (ValueId, &Type),
        producers: &mut BTreeMap<ValueId, bool>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        let mut pending = vec![root];
        // Reification emits each child before its parent, in field order.
        // Walking backward rejects aliases, gaps, swaps and reused producers.
        while let Some((value, expected)) = pending.pop() {
            let (actual_block, at, op) = self.site_index.producers.get(function, value, b)?;
            if actual_block != block || at.checked_add(1) != Some(boundary) {
                return Err(bad(
                    "CT data producer is outside its exact materialization sequence",
                ));
            }
            boundary = at;
            let actual = &function.values[value.0 as usize];
            dynamic_metadata::ty(actual, b)?;
            dynamic_metadata::ty(expected, b)?;
            if actual != expected {
                return Err(bad("CT data producer changes an exact original child type"));
            }
            let nominal = matches!(expected, Type::Record { .. } | Type::Enum { .. });
            b.spend(tree_lookup(producers.len()))?;
            if producers.insert(value, nominal).is_some() {
                return Err(bad(
                    "one actual producer belongs to multiple CT result trees",
                ));
            }
            match (expected, &op.kind) {
                (Type::Tuple(types), InstructionKind::MakeAggregate { fields }) => {
                    children(&mut pending, fields, types.iter(), b)?;
                }
                (Type::Array { element, length }, InstructionKind::MakeAggregate { fields }) => {
                    children(
                        &mut pending,
                        fields,
                        std::iter::repeat_n(element.as_ref(), *length as usize),
                        b,
                    )?;
                }
                (Type::Record { fields: types, .. }, InstructionKind::MakeAggregate { fields }) => {
                    children(&mut pending, fields, types.iter().map(|(_, ty)| ty), b)?;
                }
                (Type::Enum { variants, .. }, InstructionKind::MakeVariant { variant, fields }) => {
                    let (_, types) = variants.get(*variant as usize).ok_or_else(|| {
                        bad("CT data selects a variant outside its original result type")
                    })?;
                    children(&mut pending, fields, types.iter(), b)?;
                }
                (ty, InstructionKind::Constant(constant)) if constant.ty() == *ty => {}
                _ => return Err(bad("CT data changes its finite serialized producer form")),
            }
        }
        Ok(())
    }

    pub(super) fn reified_variant(
        &self,
        module: &TypedModule,
        function: &TypedFunction,
        instruction: &Instruction,
        fields: &[ValueId],
        values: &[Option<CheckedType>],
        b: &mut Budget,
    ) -> Check<CheckedType> {
        crate::check_instruction(module, function, instruction, b)?;
        b.spend(fields.len())?;
        for field in fields {
            let operand = value_at(values, *field)?;
            if operand.life != empty_life()
                || !operand.classes.is_empty()
                || !operand.origins.is_empty()
                || !operand.statics.is_empty()
            {
                return Err(bad(
                    "CT enum materialization introduces an operand source domain",
                ));
            }
        }
        let result = instruction
            .result
            .ok_or_else(|| bad("CT enum materialization lacks its actual result"))?;
        self.closed_value(module, &function.values[result.0 as usize], b)
    }
}

fn tree_lookup(n: usize) -> usize {
    n.checked_ilog2().map_or(1, |n| n as usize + 2)
}

pub(super) fn nominal_producer(
    values: &BTreeMap<ValueId, bool>,
    instruction: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let Some(result) = instruction.result.filter(|_| !values.is_empty()) else {
        return Ok(false);
    };
    b.spend(tree_lookup(values.len()))?;
    Ok(values.get(&result) == Some(&true))
}

fn children<'ty>(
    pending: &mut Vec<(ValueId, &'ty Type)>,
    fields: &[ValueId],
    types: impl ExactSizeIterator<Item = &'ty Type>,
    b: &mut Budget,
) -> Check<()> {
    if fields.len() != types.len() {
        return Err(bad(
            "CT data materialization changes the original field arity",
        ));
    }
    b.spend(
        fields
            .len()
            .saturating_add(if pending.capacity() - pending.len() < fields.len() {
                pending.len()
            } else {
                0
            }),
    )?;
    pending.extend(fields.iter().copied().zip(types));
    Ok(())
}

fn serializable(ty: &Type, b: &mut Budget) -> Check<()> {
    // Check the complete type, including the unselected alternatives of a
    // sum and the element of an empty array.
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::Unit | Type::Bool => {}
            ty if ty.is_integer() || ty.is_float() => {}
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            _ => {
                return Err(bad(
                    "CT data has a nonserializable type in its complete result",
                ));
            }
        }
    }
    Ok(())
}
