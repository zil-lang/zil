//! Ordinary Sum admission is distinct from capture/array/StoreElement profiles.
use super::*;
pub(super) struct Budget<'a> {
    limits: &'a zil_base::Limits,
    nodes: usize,
    work: usize,
}
impl<'a> Budget<'a> {
    pub(super) fn new(limits: &'a zil_base::Limits) -> Self {
        Self {
            limits,
            nodes: 0,
            work: 0,
        }
    }
    fn pay(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        self.nodes = self.nodes.checked_add(1).ok_or_else(limit)?;
        let names = match ty {
            Type::Package { name, fields } | Type::Record { name, fields } => name
                .len()
                .saturating_add(fields.iter().map(|(n, _)| n.len()).sum()),
            Type::Enum { name, variants } => name
                .len()
                .saturating_add(variants.iter().map(|(n, _)| n.len()).sum()),
            _ => 0,
        };
        self.work = self
            .work
            .checked_add(names.saturating_add(1))
            .ok_or_else(limit)?;
        if depth > self.limits.max_call_depth
            || self.nodes > self.limits.max_nodes
            || self.work > self.limits.max_analysis_steps
        {
            return Err(limit());
        }
        Ok(())
    }
    fn copy_work(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        match ty {
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    self.pay(t, depth)?;
                    self.copy_work(t, depth + 1)?;
                }
            }
            Type::Tuple(fields) => {
                for t in fields {
                    self.pay(t, depth)?;
                    self.copy_work(t, depth + 1)?;
                }
            }
            Type::Managed(element)
            | Type::Desc(element)
            | Type::OwnedStore { element }
            | Type::Array { element, .. } => {
                self.pay(element, depth)?;
                self.copy_work(element, depth + 1)?;
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        self.pay(t, depth)?;
                        self.copy_work(t, depth + 1)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.pay(error, depth)?;
                self.copy_work(error, depth + 1)?;
                self.pay(result, depth)?;
                self.copy_work(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn admits(
        &mut self,
        ty: &Type,
        resources: &[zil_ir::ResourceDefinition],
        depth: usize,
    ) -> CompileResult<bool> {
        self.pay(ty, depth)?;
        match ty {
            Type::Managed(payload) => {
                for _ in resources {
                    self.pay(ty, depth)?;
                    self.copy_work(payload, depth + 1)?;
                }
                Ok(resource_type(ty, resources))
            }
            Type::Package { fields, .. } => {
                for (_, t) in fields {
                    if !self.admits(t, resources, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        if !self.admits(t, resources, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::ErrorUnion { error, result } => Ok(self.admits(error, resources, depth + 1)?
                && self.admits(result, resources, depth + 1)?),
            _ => {
                self.copy_work(ty, depth + 1)?;
                Ok(ty.is_copy_value())
            }
        }
    }
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Lower,
        "ordinary Sum layout admission budget exceeded",
        None,
    )]
}

#[cfg(test)]
mod impl243_tests {
    use super::*;
    #[test]
    fn impl243_lower_uses_one_actual_request_budget_and_fresh_boundary() {
        let ty = Type::Enum {
            name: "Choice".into(),
            variants: vec![
                ("None".into(), vec![]),
                (
                    "Some".into(),
                    vec![Type::Managed(Box::new(Type::ByteBuffer))],
                ),
            ],
        };
        let resources = vec![zil_ir::ResourceDefinition {
            name: zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }];
        let mut limits = zil_base::Limits::default();
        let needed = {
            let mut budget = Budget::new(&limits);
            assert!(budget.admits(&ty, &resources, 0).unwrap());
            budget.work
        };
        limits.max_analysis_steps = needed;
        assert!(Budget::new(&limits).admits(&ty, &resources, 0).unwrap());
        limits.max_analysis_steps = needed - 1;
        assert_eq!(
            Budget::new(&limits).admits(&ty, &resources, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        limits.max_analysis_steps = needed;
        assert!(Budget::new(&limits).admits(&ty, &resources, 0).unwrap());
        // The same phase may not restart its remaining count for a second type.
        let mut shared = Budget::new(&limits);
        assert!(shared.admits(&ty, &resources, 0).unwrap());
        assert_eq!(
            shared.admits(&ty, &resources, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}
