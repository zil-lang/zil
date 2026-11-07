//! Borrowed schema traversal only. This bounds transport/copying, not nominal,
//! binder, ownership or execution admission; Types checks those independently.
use zil_base::{CompileResult, Diagnostic, Limits, Phase, Span};
use zil_ir::*;
fn quota(span: Option<Span>) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "Product metadata node/depth/work budget exhausted",
        span,
    )]
}
struct Walk<'a> {
    limits: &'a Limits,
    nodes: usize,
    work: usize,
}
impl Walk<'_> {
    fn node(&mut self, depth: usize, span: Option<Span>) -> CompileResult<()> {
        if depth > self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            return Err(quota(span));
        }
        self.work = self.work.checked_sub(1).ok_or_else(|| quota(span))?;
        self.nodes += 1;
        Ok(())
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.work = self
            .work
            .checked_sub(text.len())
            .ok_or_else(|| quota(Some(span)))?;
        Ok(())
    }
    fn ty(&mut self, ty: &Type, span: Span, depth: usize) -> CompileResult<()> {
        super::source::hint_type(ty, depth, &mut self.nodes, &mut self.work, self.limits)
            .map(|_| ())
            .map_err(|mut ds| {
                for d in &mut ds {
                    d.span = Some(span)
                }
                ds
            })
    }
    fn recipe(&mut self, root: &CoreAggregateTypeRecipe, span: Span) -> CompileResult<()> {
        let mut pending = vec![(root, 0usize)];
        while let Some((recipe, depth)) = pending.pop() {
            self.node(depth, Some(span))?;
            match recipe {
                CoreAggregateTypeRecipe::Concrete(ty) => self.ty(ty, span, depth + 1)?,
                CoreAggregateTypeRecipe::Parameter { .. } => {}
                CoreAggregateTypeRecipe::Task {
                    result,
                    ports,
                    binder_count,
                    retained,
                } => {
                    self.work = self
                        .work
                        .checked_sub(ports.len().saturating_add(*binder_count as usize))
                        .ok_or_else(|| quota(Some(span)))?;
                    self.work = self
                        .work
                        .checked_sub(retained.len())
                        .ok_or_else(|| quota(Some(span)))?;
                    for row in retained {
                        if row.path.len() > self.limits.max_call_depth {
                            return Err(quota(Some(span)));
                        }
                        for _ in &row.path {
                            self.node(depth + 1, Some(span))?;
                        }
                    }
                    pending.push((result, depth + 1));
                    for port in ports {
                        pending.push((&port.pointee, depth + 1));
                    }
                }
                CoreAggregateTypeRecipe::AsyncRegistration {
                    environment: element,
                }
                | CoreAggregateTypeRecipe::Registration {
                    environment: element,
                }
                | CoreAggregateTypeRecipe::Store { element } => pending.push((element, depth + 1)),
                CoreAggregateTypeRecipe::Apply { arguments, .. } => {
                    if arguments.len() > self.limits.max_locals {
                        return Err(quota(Some(span)));
                    }
                    self.work = self
                        .work
                        .checked_sub(arguments.len())
                        .ok_or_else(|| quota(Some(span)))?;
                    pending.extend(arguments.iter().map(|arg| (arg, depth + 1)));
                }
            }
        }
        Ok(())
    }
}
/// Return exact newly charged (nodes, work) without copying raw metadata.
/// Empty tables preserve the old path's logical fees.
pub fn preflight_aggregate_metadata(
    core: &CoreModule,
    limits: &Limits,
) -> CompileResult<(usize, usize)> {
    if core.aggregate_templates.is_empty() && core.aggregate_applications.is_empty() {
        return Ok((0, 0));
    }
    if core.aggregate_templates.len() > limits.max_nodes
        || core.aggregate_applications.len() > limits.max_nodes
    {
        return Err(quota(None));
    }
    let mut w = Walk {
        limits,
        nodes: 0,
        work: limits.max_analysis_steps,
    };
    for template in &core.aggregate_templates {
        w.node(0, Some(template.span))?;
        w.text(&template.name, template.span)?;
        w.node(0, Some(template.span))?;
        for (index, parameter) in template.parameters.iter().enumerate() {
            if index != 0 {
                w.node(0, Some(template.span))?;
            }
            w.text(&parameter.name, template.span)?;
        }
        if template.fields.len() > limits.max_locals {
            return Err(quota(Some(template.span)));
        }
        for field in &template.fields {
            w.node(0, Some(field.span))?;
            w.text(&field.name, field.span)?;
            w.recipe(&field.ty, field.span)?;
        }
    }
    for app in &core.aggregate_applications {
        w.node(0, None)?;
        if app.owner.is_some() {
            w.node(0, None)?
        }
        if app.arguments.len() > limits.max_locals {
            return Err(quota(None));
        }
        for arg in &app.arguments {
            w.node(0, None)?;
            if let CoreAggregateArgument::Concrete(ty) = arg {
                w.ty(ty, Span::default(), 1)?
            }
        }
    }
    Ok((w.nodes, limits.max_analysis_steps - w.work))
}

/// Bound every Core metadata copy. This is transport cost only, never a source
/// identity, class equation, capture or CT execution certificate.
pub fn preflight_core_metadata(
    core: &CoreModule,
    limits: &Limits,
) -> CompileResult<(usize, usize)> {
    let (nodes, work) = preflight_aggregate_metadata(core, limits)?;
    let mut w = Walk {
        limits,
        nodes,
        work: limits.max_analysis_steps - work,
    };
    fn path(w: &mut Walk<'_>, length: usize) -> CompileResult<()> {
        if length > w.limits.max_call_depth {
            return Err(quota(None));
        }
        for _ in 0..length {
            w.node(1, None)?;
        }
        Ok(())
    }
    fn position(w: &mut Walk<'_>, value: &CoreClosureHeaderPosition) -> CompileResult<()> {
        match value {
            CoreClosureHeaderPosition::Borrow(position) => path(w, position.path.len()),
            CoreClosureHeaderPosition::Class(position) => path(w, position.path.len()),
        }
    }
    for context in &core.closure_life_contexts {
        w.node(0, None)?;
        w.text(&context.identity.package, Span::default())?;
        for part in context
            .identity
            .module_path
            .iter()
            .chain(&context.identity.declaration_path)
        {
            w.node(1, None)?;
            w.text(part, Span::default())?;
        }
        path(&mut w, context.identity.generation_path.len())?;
        for parameter in &context.parameters {
            w.node(1, None)?;
            if !matches!(parameter, LifeParameterKind::Lifetime) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "closure context requires the finite life-only Definition profile",
                    None,
                )]);
            }
        }
        for binding in &context.signature.bindings {
            w.node(1, None)?;
            path(&mut w, binding.position.path.len())?;
        }
        for binding in &context.signature.classes {
            w.node(1, None)?;
            path(&mut w, binding.position.path.len())?;
        }
        for _ in &context.named_fixed {
            w.node(1, None)?;
        }
        for input in &context.inputs {
            w.node(1, None)?;
            position(&mut w, &input.position)?;
        }
    }
    Ok((w.nodes, limits.max_analysis_steps - w.work))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, ModuleId, NodeId, SCHEMA_VERSION, TargetProfile};
    #[test]
    fn raw_closure_context_copy_is_bounded_before_ct_snapshot() {
        let mut module = core();
        module.closure_life_contexts.push(CoreClosureLifeContext {
            creator: NodeId(0),
            expression: NodeId(1),
            declaration: LifeDeclarationId {
                module: ModuleId(7),
                declaration: NodeId(0),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "x".repeat(4096),
                module_path: vec!["m".into()],
                declaration_path: vec!["f".into()],
                generation_path: vec![],
            },
            parameters: vec![LifeParameterKind::Lifetime],
            occurrence: 0,
            signature: StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: vec![],
                classes: vec![],
            },
            named_fixed: vec![],
            inputs: vec![],
        });
        let limits = Limits::default();
        let (nodes, work) = preflight_core_metadata(&module, &limits).unwrap();
        assert!(work >= 4096 && nodes > 0);
        assert!(
            preflight_core_metadata(
                &module,
                &Limits {
                    max_analysis_steps: work - 1,
                    ..limits.clone()
                }
            )
            .is_err()
        );
        preflight_core_metadata(
            &module,
            &Limits {
                max_analysis_steps: work,
                max_nodes: nodes,
                ..limits.clone()
            },
        )
        .unwrap();
        let error = crate::evaluate_source_constants(
            &module,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps: 64,
                ..limits.clone()
            },
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .all(|error| error.category == zil_base::ResultCategory::ResourceLimit)
        );
        module.closure_life_contexts[0].identity.generation_path =
            vec![0; limits.max_call_depth + 1];
        assert!(preflight_core_metadata(&module, &limits).is_err());
        module.closure_life_contexts.clear();
        assert_eq!(preflight_core_metadata(&module, &limits).unwrap(), (5, 14));
    }

    fn core() -> CoreModule {
        CoreModule {
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            closure_life_contexts: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(7),
            source_fingerprint: Fingerprint::ZERO,
            functions: vec![],
            aggregates: vec![],
            enums: vec![],
            unsupported: vec![],
            aggregate_applications: vec![],
            aggregate_templates: vec![CoreAggregateTemplate {
                id: NodeId(23),
                name: "Bag".into(),
                parameters: vec![CoreTypeParameter {
                    index: 0,
                    name: "T".into(),
                    bound: CoreTypeBound::StoreElement,
                }],
                fields: vec![CoreAggregateFieldRecipe {
                    name: "items".into(),
                    public: false,
                    manages: true,
                    ty: CoreAggregateTypeRecipe::Store {
                        element: Box::new(CoreAggregateTypeRecipe::Parameter { index: 0 }),
                    },
                    span: Span::default(),
                }],
                span: Span::default(),
            }],
        }
    }
    #[test]
    fn product_metadata_is_prepaid_at_exact_node_work_and_depth_boundaries() {
        let core = core();
        let limits = Limits::default();
        let (nodes, work) = preflight_aggregate_metadata(&core, &limits).unwrap();
        let exact = Limits {
            max_nodes: nodes,
            max_analysis_steps: work,
            max_call_depth: 1,
            ..limits
        };
        assert_eq!(
            preflight_aggregate_metadata(&core, &exact).unwrap(),
            (nodes, work)
        );
        for limits in [
            Limits {
                max_nodes: nodes - 1,
                ..exact.clone()
            },
            Limits {
                max_analysis_steps: work - 1,
                ..exact.clone()
            },
            Limits {
                max_call_depth: 0,
                ..exact
            },
        ] {
            assert_eq!(
                preflight_aggregate_metadata(&core, &limits).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    #[test]
    fn product_runtime_normalization_preserves_recipes_and_identity_scans_them() {
        let mut core = core();
        let limits = Limits::default();
        assert_eq!(
            crate::evaluate_source_constants(&core, &TargetProfile::default(), &limits).unwrap(),
            core
        );
        assert_eq!(
            crate::materialize::first_free_id(&core, &limits).unwrap(),
            24
        );
        core.aggregate_templates[0].id = NodeId(u32::MAX);
        assert!(crate::materialize::first_free_id(&core, &limits).is_err());
    }
}
