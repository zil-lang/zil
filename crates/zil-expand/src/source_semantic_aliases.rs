//! Transparent aliases use declaration scope and complete caller actuals.
use super::*;
pub(super) struct AliasFrame {
    pub(super) row: usize,
    pub(super) bound_depth: usize,
    pub(super) arguments: Vec<SemanticActualRecipe>,
}
impl Builder<'_, '_, '_, '_> {
    pub(super) fn alias_actual(
        &mut self,
        name: &str,
        span: Span,
    ) -> R<Option<SemanticActualRecipe>> {
        let Some(frame) = self.alias_frames.iter().rev().find(|f| f.row == self.owner) else {
            return Ok(None);
        };
        let mut ordinal = None;
        for (i, p) in self.inventory.declarations[self.owner]
            .parameters()
            .iter()
            .enumerate()
        {
            self.budget.text(&p.name, span)?;
            self.budget.text(name, span)?;
            if p.name == name {
                ordinal = Some(i);
                break;
            }
        }
        let Some(i) = ordinal else { return Ok(None) };
        let value = frame
            .arguments
            .get(i)
            .ok_or_else(|| error(span, "alias formal lacks its complete mixed actual"))?;
        let shift = self
            .bound_lives
            .len()
            .checked_sub(frame.bound_depth)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| error(span, "alias quantifier depth differs from caller"))?;
        prepay_recipe(value, self.budget, span, 0)?;
        prepay_recipe_work(value, self.budget, span, 0)?;
        let mut value = value.clone();
        // Preflight pays every node before this structural capture-avoidance walk.
        shift_actual(&mut value, shift, 0, span)?;
        Ok(Some(value))
    }
    pub(super) fn alias_type(
        &mut self,
        row: usize,
        arguments: Vec<SemanticActualRecipe>,
        path: &mut Vec<u32>,
        depth: usize,
        payload: bool,
    ) -> R<SemanticTypeRecipe> {
        let inventory = self.inventory;
        let s::ItemKind::TypeAlias(alias) = &inventory.declarations[row].item.kind else {
            return Err(error(Span::default(), "alias target is not an alias"));
        };
        let target = alias.value.as_ref().ok_or_else(|| {
            error(
                inventory.declarations[row].item.span,
                "type alias has no target",
            )
        })?;
        self.budget
            .identity_work(self.alias_frames.len(), target.span)?;
        if self.alias_frames.iter().any(|f| f.row == row) {
            return Err(error(target.span, "recursive transparent type alias"));
        }
        // Keep the caller's coordinate depth, hiding its lexical spellings.
        let mut hidden = Vec::new();
        for _ in &self.bound_lives {
            self.budget
                .lineage_push(&mut hidden, Vec::new(), target.span)?;
        }
        let frame = AliasFrame {
            row,
            arguments,
            bound_depth: self.bound_lives.len(),
        };
        self.budget
            .lineage_push(&mut self.alias_frames, frame, target.span)?;
        let (caller, offset) = (self.owner, self.lexical_offset);
        self.owner = row;
        self.lexical_offset = 0;
        let bounds = std::mem::replace(&mut self.bound_lives, hidden);
        let result = self.ty(target, path, depth + 1, payload);
        self.bound_lives = bounds;
        self.owner = caller;
        self.lexical_offset = offset;
        self.alias_frames.pop();
        result
    }
}
fn shift_life(v: &mut SemanticLifeRecipe, n: u32, p: u32, s: Span) -> R<()> {
    if let SemanticLifeRecipe::Bound { depth, .. } = v
        && *depth >= p
    {
        *depth = depth
            .checked_add(n)
            .ok_or_else(|| error(s, "alias bound lifetime overflow"))?
    }
    Ok(())
}
fn shift_actual(v: &mut SemanticActualRecipe, n: u32, p: u32, s: Span) -> R<()> {
    match v {
        SemanticActualRecipe::Life(v) => shift_life(v, n, p, s),
        SemanticActualRecipe::Type(v) => shift_type(v, n, p, s),
        SemanticActualRecipe::Const { carrier, value } => {
            shift_type(carrier, n, p, s)?;
            shift_const(value, n, p, s)
        }
        SemanticActualRecipe::Infer => Err(error(s, "alias actual cannot remain inferred")),
    }
}
fn shift_app(v: &mut SemanticTraitRecipe, n: u32, p: u32, s: Span) -> R<()> {
    for v in &mut v.arguments {
        shift_actual(v, n, p, s)?
    }
    Ok(())
}
fn shift_const(v: &mut SemanticConstRecipe, n: u32, p: u32, s: Span) -> R<()> {
    match v {
        SemanticConstRecipe::Unary {
            operand_type,
            value,
            ..
        } => {
            shift_type(operand_type, n, p, s)?;
            shift_const(value, n, p, s)?
        }
        SemanticConstRecipe::Binary {
            operand_type,
            left,
            right,
            ..
        } => {
            shift_type(operand_type, n, p, s)?;
            shift_const(left, n, p, s)?;
            shift_const(right, n, p, s)?
        }
        SemanticConstRecipe::Convert {
            source_type,
            target_type,
            value,
        } => {
            shift_type(source_type, n, p, s)?;
            shift_type(target_type, n, p, s)?;
            shift_const(value, n, p, s)?
        }
        SemanticConstRecipe::AssociatedConst {
            receiver,
            application,
            ..
        } => {
            shift_type(receiver, n, p, s)?;
            shift_app(application, n, p, s)?
        }
        SemanticConstRecipe::Value(_)
        | SemanticConstRecipe::IntegerLiteral(_)
        | SemanticConstRecipe::FloatLiteral(_)
        | SemanticConstRecipe::Formal(_)
        | SemanticConstRecipe::Environment { .. } => {}
    }
    Ok(())
}
fn shift_classes(v: &mut SemanticClassSequenceRecipe, n: u32, p: u32, s: Span) -> R<()> {
    for v in &mut v.parts {
        match v {
            SemanticClassSequencePart::One(v) => shift_life(v, n, p, s)?,
            SemanticClassSequencePart::AllClasses { ty, .. } => shift_type(ty, n, p, s)?,
        }
    }
    Ok(())
}
fn shift_retained(v: &mut SemanticRetainedPart, n: u32, p: u32, s: Span) -> R<()> {
    match v {
        SemanticRetainedPart::One { parent, .. } => shift_life(parent, n, p, s),
        SemanticRetainedPart::All { parents, .. } => shift_classes(parents, n, p, s),
    }
}
fn shift_nominal(v: &mut SemanticNominalPart, n: u32, p: u32, s: Span) -> R<()> {
    match v {
        SemanticNominalPart::One { class, .. } => shift_life(class, n, p, s),
        SemanticNominalPart::FromType { ty, classes, .. } => {
            shift_type(ty, n, p, s)?;
            shift_classes(classes, n, p, s)
        }
    }
}
fn shift_shape(v: &mut SemanticRetainedShapeRecipe, n: u32, p: u32, s: Span) -> R<()> {
    shift_classes(&mut v.domain, n, p, s)?;
    for v in &mut v.ports {
        match v {
            SemanticPortPart::One { leaf, class, .. } => {
                shift_type(leaf, n, p, s)?;
                shift_life(class, n, p, s)?
            }
            SemanticPortPart::FromType { ty, classes } => {
                shift_type(ty, n, p, s)?;
                shift_classes(classes, n, p, s)?
            }
        }
    }
    for v in &mut v.retained {
        shift_retained(v, n, p, s)?
    }
    for v in &mut v.nominal {
        shift_nominal(v, n, p, s)?
    }
    Ok(())
}
fn shift_type(v: &mut SemanticTypeRecipe, n: u32, p: u32, s: Span) -> R<()> {
    match v {
        SemanticTypeRecipe::Exact(v) => {
            for v in &mut v.classes {
                shift_life(v, n, p, s)?
            }
        }
        SemanticTypeRecipe::Nominal { arguments, .. }
        | SemanticTypeRecipe::Opaque { arguments, .. }
        | SemanticTypeRecipe::Closure { arguments, .. } => {
            for v in arguments {
                shift_actual(v, n, p, s)?
            }
            if let SemanticTypeRecipe::Opaque { hidden, .. } = v {
                shift_type(hidden, n, p, s)?
            }
            if let SemanticTypeRecipe::Closure { environment, .. } = v {
                for v in environment {
                    shift_actual(v, n, p, s)?
                }
            }
        }
        SemanticTypeRecipe::AssociatedType {
            receiver,
            application,
            ..
        } => {
            shift_type(receiver, n, p, s)?;
            shift_app(application, n, p, s)?
        }
        SemanticTypeRecipe::Tuple(v) | SemanticTypeRecipe::AnonymousSum(v) => {
            for v in v {
                shift_type(v, n, p, s)?
            }
        }
        SemanticTypeRecipe::Array { element, length } => {
            shift_type(element, n, p, s)?;
            shift_const(length, n, p, s)?
        }
        SemanticTypeRecipe::Address {
            lifetime, inner, ..
        }
        | SemanticTypeRecipe::Desc { lifetime, inner } => {
            if let SemanticOuterLifeRecipe::Class(v) = lifetime {
                shift_life(v, n, p, s)?
            }
            shift_type(inner, n, p, s)?
        }
        SemanticTypeRecipe::Slice(v)
        | SemanticTypeRecipe::Managed(v)
        | SemanticTypeRecipe::OwnedStore(v)
        | SemanticTypeRecipe::Mutex(v)
        | SemanticTypeRecipe::Registration(v)
        | SemanticTypeRecipe::AsyncRegistration(v) => shift_type(v, n, p, s)?,
        SemanticTypeRecipe::MutexGuard { payload, retained } => {
            shift_type(payload, n, p, s)?;
            shift_shape(retained, n, p, s)?
        }
        SemanticTypeRecipe::Task { result, retained } => {
            shift_type(result, n, p, s)?;
            shift_shape(retained, n, p, s)?
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => {
            shift_type(error, n, p, s)?;
            shift_type(result, n, p, s)?
        }
        SemanticTypeRecipe::Qualified { storage, life } => {
            shift_type(storage, n, p, s)?;
            shift_classes(&mut life.domain, n, p, s)?;
            for v in &mut life.slots {
                shift_life(&mut v.class, n, p, s)?
            }
            for v in &mut life.retained {
                shift_retained(v, n, p, s)?
            }
            for v in &mut life.nominal {
                shift_nominal(v, n, p, s)?
            }
        }
        SemanticTypeRecipe::Code { signature, .. } => {
            let p = p
                .checked_add(u32::from(signature.bound_lifetimes != 0))
                .ok_or_else(|| error(s, "alias signature binder overflow"))?;
            for v in &mut signature.parameters {
                shift_type(&mut v.ty, n, p, s)?
            }
            shift_type(&mut signature.result, n, p, s)?
        }
        SemanticTypeRecipe::Dyn { interface, .. } => {
            shift_app(&mut interface.application, n, p, s)?;
            match &mut interface.context {
                SemanticDynContextRecipe::Closed => {}
                SemanticDynContextRecipe::Type(v) => shift_type(v, n, p, s)?,
                SemanticDynContextRecipe::Shape(v) => shift_shape(v, n, p, s)?,
            }
            for v in &mut interface.associated {
                shift_app(&mut v.application, n, p, s)?;
                match &mut v.value {
                    SemanticAssociatedRecipe::Type(v) => shift_type(v, n, p, s)?,
                    SemanticAssociatedRecipe::Const { carrier, value } => {
                        shift_type(carrier, n, p, s)?;
                        shift_const(value, n, p, s)?
                    }
                }
            }
        }
        SemanticTypeRecipe::Atom(_)
        | SemanticTypeRecipe::Formal(_)
        | SemanticTypeRecipe::Environment { .. }
        | SemanticTypeRecipe::SelfType { .. }
        | SemanticTypeRecipe::UnresolvedNamed(_)
        | SemanticTypeRecipe::Unsupported(_)
        | SemanticTypeRecipe::OriginalType { .. } => {}
    }
    Ok(())
}
