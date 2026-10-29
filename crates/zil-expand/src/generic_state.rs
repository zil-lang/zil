//! Preserve the ordinary data identity of a mixed owning State independently
//! of the lexical spelling used for its symbolic callable parameters.
use super::*;

impl Engine<'_> {
    pub(super) fn state_family(
        &mut self,
        item: &Item,
        arguments: &[TypeRef],
        keys: &[String],
        span: Span,
    ) -> R<Option<usize>> {
        if self.callable_pair.is_some() {
            return self.dual_state_family(item, arguments, keys, span);
        }
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
    fn dual_state_family(
        &mut self,
        item: &Item,
        arguments: &[TypeRef],
        keys: &[String],
        span: Span,
    ) -> R<Option<usize>> {
        let ItemKind::Aggregate(aggregate) = &item.kind else {
            return Ok(None);
        };
        if aggregate.kind != AggregateKind::Package || !(1..=3).contains(&arguments.len()) {
            return Ok(None);
        }
        // A one-slot Item<F> has no ordinary data argument. Preserve the old
        // single-root family spelling when it occurs inside an F/G scope.
        if arguments.len() == 1 {
            return Ok(None);
        }
        let names = self
            .callable_pair
            .as_ref()
            .expect("new two-root lexical scope");
        let mut callback_slots = [false; 3];
        for (index, argument) in arguments.iter().enumerate() {
            self.budget.ty(argument)?;
            for name in names {
                if callable::parameter_mode(argument, name) == Some(None) {
                    callback_slots[index] = true;
                } else if callable::refers(argument, name) {
                    return Err(unsupported(
                        span,
                        "two-root State requires direct callable type slots",
                    ));
                }
            }
        }
        if !callback_slots.iter().any(|slot| *slot) {
            return Ok(None);
        }
        let mut callbacks = 0usize;
        for field in &aggregate.fields {
            self.budget.ty(&field.ty)?;
            for (index, parameter) in aggregate.generics.iter().enumerate() {
                if !callback_slots[index] {
                    continue;
                }
                if callable::parameter_mode(&field.ty, &parameter.name) == Some(None) {
                    callbacks += 1;
                } else if callable::refers(&field.ty, &parameter.name) {
                    return Err(unsupported(
                        span,
                        "two-root State requires direct callable fields",
                    ));
                }
            }
        }
        if !(1..=2).contains(&callbacks) {
            return Err(unsupported(
                span,
                "owning State requires one or two direct callable fields",
            ));
        }
        // The family masks aggregate argument slots, not function binder order.
        // Field selectors carry the mapping; concrete Types substitution uses
        // the family's stable field declaration order for its callback vector.
        self.budget.node(span, 0)?;
        self.budget.text(&aggregate.name, span)?;
        for (index, key) in keys.iter().enumerate() {
            self.budget.node(span, 0)?;
            if !callback_slots[index] {
                self.budget.text(key, span)?;
            }
        }
        let key = (
            aggregate.name.clone(),
            keys.iter()
                .enumerate()
                .map(|(index, key)| (!callback_slots[index]).then(|| key.clone()))
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
