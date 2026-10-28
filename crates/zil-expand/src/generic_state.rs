//! Preserve the ordinary data identity of a mixed owning State independently
//! of the lexical spelling used for its one symbolic callable parameter.
use super::*;

impl Engine<'_> {
    pub(super) fn state_family(
        &mut self,
        item: &Item,
        arguments: &[TypeRef],
        keys: &[String],
        span: Span,
    ) -> R<Option<usize>> {
        let (ItemKind::Aggregate(aggregate), Some(binder)) = (&item.kind, &self.callable_name)
        else {
            return Ok(None);
        };
        if aggregate.kind != AggregateKind::Package || arguments.len() != 2 {
            return Ok(None);
        }
        let mut callable_index = None;
        for (index, argument) in arguments.iter().enumerate() {
            self.budget.ty(argument)?;
            if callable::parameter_mode(argument, binder) == Some(None) {
                if callable_index.replace(index).is_some() {
                    return Err(unsupported(
                        span,
                        "mixed owning State has exactly one callable type slot",
                    ));
                }
            } else if callable::refers(argument, binder) {
                return Err(unsupported(
                    span,
                    "mixed owning State requires a direct callable type slot",
                ));
            }
        }
        let Some(callable_index) = callable_index else {
            return Ok(None);
        };
        let variable = &aggregate.generics[callable_index].name;
        let mut fields = 0usize;
        for field in &aggregate.fields {
            self.budget.ty(&field.ty)?;
            if callable::parameter_mode(&field.ty, variable) == Some(None) {
                fields += 1;
            } else if callable::refers(&field.ty, variable) {
                return Err(unsupported(
                    span,
                    "mixed owning State requires a direct callable field",
                ));
            }
        }
        if !(1..=2).contains(&fields) {
            return Err(unsupported(
                span,
                "owning State requires one or two direct fields of its single callable type",
            ));
        }
        // Charge the second key copy before allocating it. The original key
        // already paid complete type traversal, including nested nominal args.
        self.budget.node(span, 0)?;
        self.budget.text(&aggregate.name, span)?;
        for (index, key) in keys.iter().enumerate() {
            self.budget.node(span, 0)?;
            if index != callable_index {
                self.budget.text(key, span)?;
            }
        }
        let key = (
            aggregate.name.clone(),
            keys.iter()
                .enumerate()
                .map(|(index, key)| (index != callable_index).then(|| key.clone()))
                .collect(),
        );
        if let Some(family) = self.state_families.get(&key) {
            return Ok(Some(*family));
        }
        let family = self.state_families.len();
        self.state_families.insert(key, family);
        Ok(Some(family))
    }
}
