//! A frozen query's source call has no runtime instruction. Its actual
//! transcribed data must form a fresh complete producer tree at its own use.
use super::*;

pub(in crate::semantic) fn serializable(
    ty: &Type,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "reified compile-time data exceeds its finite type depth",
            None,
        )]);
    }
    if ty.is_b0_scalar() {
        return Ok(());
    }
    match ty {
        Type::Array { element, .. } => serializable(element, b, depth + 1),
        Type::Tuple(fields) => {
            for field in fields {
                serializable(field, b, depth + 1)?;
            }
            Ok(())
        }
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                serializable(field, b, depth + 1)?;
            }
            Ok(())
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                b.spend(1, WORK)?;
                for field in fields {
                    serializable(field, b, depth + 1)?;
                }
            }
            Ok(())
        }
        _ => Err(unknown("reified compile-time result needs a complete serializable data type")),
    }
}

pub(super) fn product(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(_) | Type::Record { .. } | Type::Array { .. } | Type::Enum { .. })
}

impl Check<'_, '_> {
    pub(super) fn ct_data_recipe(
        &self,
        instance: &SemanticInstance,
        expected: &SemanticTypeRecipe,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(node),
        } = expected else { return Ok(false); };
        let index = self.inventory.original_index(instance.definition, b)?;
        Ok(matches!(index.get_expr(*node, b)?.kind, CoreExprKind::CompileTime(_)))
    }

    pub(super) fn ct_data_value(
        &self,
        function: &TypedFunction,
        at: (BlockId, usize, SemanticSiteEdge),
        value: ValueId,
        original: &value::Value,
        b: &mut Budget,
    ) -> CompileResult<usize> {
        if at.2 != SemanticSiteEdge::Before || !empty(&original.life) || !original.classes.is_empty() {
            return Err(fail("reified compile-time data changes its original class-free boundary"));
        }
        serializable(&original.ty, b, 0)?;
        self.zero_data(&original.ty, b, 0)?;
        self.ct_data_tree(function, at.0, at.1, value, &original.ty, b, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn ct_data_tree(
        &self,
        function: &TypedFunction,
        block: BlockId,
        after: usize,
        value: ValueId,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<usize> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(vec![zil_base::Diagnostic::limit(
                zil_base::Phase::Analyze,
                "reified compile-time data exceeds its finite producer depth",
                None,
            )]);
        }
        let (producer_block, ordinal, instruction) = self.defining(function, value, b)?;
        if producer_block != block
            || ordinal.checked_add(1) != Some(after)
            || !geometry::same_type(
                function.values.get(value.0 as usize)
                    .ok_or_else(|| fail("reified compile-time producer has no complete actual type"))?,
                ty,
                b,
            )?
        {
            return Err(fail("reified compile-time data changes its exact contiguous producer"));
        }
        let (fields, expected): (&[ValueId], Vec<&Type>) = match (ty, &instruction.kind) {
            (Type::Tuple(types), InstructionKind::MakeAggregate { fields }) => {
                b.spend(types.len(), WORK)?;
                (fields, types.iter().collect())
            }
            (Type::Record { fields: types, .. }, InstructionKind::MakeAggregate { fields }) => {
                b.spend(types.len(), WORK)?;
                (fields, types.iter().map(|(_, ty)| ty).collect())
            }
            (Type::Array { element, length }, InstructionKind::MakeAggregate { fields })
                if fields.len() == *length as usize =>
            {
                b.spend(fields.len(), WORK)?;
                (fields, vec![element.as_ref(); fields.len()])
            }
            (Type::Enum { variants, .. }, InstructionKind::MakeVariant { variant, fields }) => {
                let (_, types) = variants.get(*variant as usize)
                    .ok_or_else(|| fail("reified compile-time data selects another enum variant"))?;
                b.spend(types.len(), WORK)?;
                (fields, types.iter().collect())
            }
            (ty, InstructionKind::Constant(constant)) if ty.is_b0_scalar() && constant.ty() == *ty => {
                return Ok(ordinal);
            }
            _ => return Err(fail("reified compile-time data changes its complete structural producer")),
        };
        if fields.len() != expected.len() {
            return Err(fail("reified compile-time data changes its complete inline arity"));
        }
        let mut before = ordinal;
        for (&field, ty) in fields.iter().zip(expected).rev() {
            // Each preceding sibling ends before the entire current subtree.
            // Reuse would place a producer at two distinct boundaries.
            before = self.ct_data_tree(function, block, before, field, ty, b, depth + 1)?;
        }
        Ok(before)
    }
}
