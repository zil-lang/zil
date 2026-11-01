//! Bounded source recipes for the one-StoreElement Product profile. This only
//! transports original declarations and applications; Types checks them again.
use super::*;
use zil_base::{
    ExpansionRole, FileId, ModuleId, NodeId, SourceMap, SourceMapEntry, SourceMapId, SourceTarget,
};
use zil_ir::*;

#[derive(Default)]
pub(crate) struct Products {
    templates: Vec<(Aggregate, Span)>,
    applications: Vec<Application>,
    pub(super) allowance: Option<Limits>,
}
#[derive(Clone)]
pub(super) struct Scope {
    pub(super) owner: String,
    pub(super) span: Span,
    pub(super) ordinary: Option<String>,
    pub(super) callable: Option<String>,
    pub(super) pair: Option<[String; 2]>,
}
struct Application {
    aggregate: String,
    template: String,
    arguments: Vec<TypeRef>,
    scope: Option<Scope>,
}
pub(super) fn parameter(aggregate: &Aggregate) -> Option<&GenericParam> {
    let [parameter] = aggregate.generics.as_slice() else {
        return None;
    };
    let GenericParamKind::Type(bounds) = &parameter.kind else {
        return None;
    };
    (aggregate.kind == zil_syntax::ast::AggregateKind::Package
        && bounds.len() == 1
        && stores::is_bound(&bounds[0]))
    .then_some(parameter)
}
impl Engine<'_> {
    pub(super) fn collect_products(&mut self, module: &Module) -> R<()> {
        for item in &module.items {
            let ItemKind::Aggregate(a) = &item.kind else {
                continue;
            };
            if parameter(a).is_none() {
                continue;
            }
            self.budget.item(item)?;
            self.products.templates.push((a.clone(), item.span));
        }
        Ok(())
    }
    pub(super) fn product_scope(&mut self, arguments: &[TypeRef], span: Span) -> R<Option<Scope>> {
        if self.products.templates.is_empty() {
            return Ok(None);
        }
        let mut dependent = false;
        for argument in arguments {
            self.budget.ty(argument)?;
            for name in self
                .store_parameter
                .iter()
                .chain(self.callable_name.iter())
                .chain(self.callable_pair.iter().flatten())
            {
                dependent |= callable::refers(argument, name);
            }
        }
        if !dependent {
            return Ok(None);
        }
        if let Some(scope) = &self.product_owner {
            self.budget.text(&scope.owner, span)?;
            for name in scope
                .ordinary
                .iter()
                .chain(scope.callable.iter())
                .chain(scope.pair.iter().flatten())
            {
                self.budget.text(name, span)?;
            }
        }
        self.product_owner
            .clone()
            .map(Some)
            .ok_or_else(|| error(span, "symbolic Product application lost its lexical owner"))
    }
    pub(super) fn record_product(
        &mut self,
        template: &str,
        aggregate: &str,
        arguments: Vec<TypeRef>,
        scope: Option<Scope>,
        span: Span,
    ) -> R<()> {
        self.budget.text(template, span)?;
        self.budget.text(aggregate, span)?;
        for argument in &arguments {
            self.budget.ty(argument)?;
        }
        self.products.applications.push(Application {
            aggregate: aggregate.into(),
            template: template.into(),
            arguments,
            scope,
        });
        Ok(())
    }
    pub(super) fn product_aggregate_scope(&mut self, name: &str, span: Span) -> R<Option<Scope>> {
        if self.products.is_empty() {
            return Ok(None);
        }
        for a in &self.products.applications {
            self.budget.node(span, 0)?;
            self.budget.text(&a.aggregate, span)?;
            if a.aggregate == name {
                if let Some(scope) = &a.scope {
                    self.budget.text(&scope.owner, span)?;
                    for name in scope
                        .ordinary
                        .iter()
                        .chain(scope.callable.iter())
                        .chain(scope.pair.iter().flatten())
                    {
                        self.budget.text(name, span)?;
                    }
                }
                return Ok(a.scope.clone());
            }
        }
        Ok(None)
    }
}
impl Products {
    pub(super) fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
    pub(crate) fn restore_owners(
        &mut self,
        functions: &BTreeMap<String, (ModuleId, String, bool)>,
    ) {
        for a in &mut self.applications {
            if let Some(scope) = &mut a.scope
                && let Some((_, original, _)) = functions.get(&scope.owner)
            {
                scope.owner.clone_from(original);
            }
        }
    }
    pub(crate) fn publish(
        &self,
        cores: &mut [(&mut CoreModule, &mut SourceMap)],
        sources: &[zil_base::SourceUnit],
        limits: &Limits,
    ) -> CompileResult<()> {
        if self.templates.is_empty() {
            return Ok(());
        }
        let allowance = self.allowance.as_ref().unwrap_or(limits);
        let mut budget = Budget::new(allowance);
        self.publish_inner(cores, sources, &mut budget)
            .map_err(|e| vec![e])
    }
    fn publish_inner(
        &self,
        cores: &mut [(&mut CoreModule, &mut SourceMap)],
        sources: &[zil_base::SourceUnit],
        budget: &mut Budget<'_>,
    ) -> R<()> {
        for source in sources {
            budget.node(Span::default(), 0)?;
            budget.text(&source.path, Span::default())?;
        }
        for (core, map) in cores.iter() {
            for _ in &map.entries {
                budget.node(Span::default(), 0)?;
            }
            for a in &core.aggregates {
                budget.node(a.span, 0)?;
                budget.text(&a.name, a.span)?;
            }
            for f in &core.functions {
                budget.node(f.span, 0)?;
                budget.text(&f.name, f.span)?;
            }
        }
        let owners: BTreeMap<FileId, ModuleId> =
            sources.iter().map(|s| (s.file_id, s.module_id)).collect();
        let mut families = BTreeMap::new();
        let mut next = BTreeMap::new();
        for (core, map) in cores.iter() {
            let value = map
                .entries
                .iter()
                .filter_map(|e| match e.target {
                    SourceTarget::Node(id) => Some(id.0),
                    _ => None,
                })
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| error(Span::default(), "Product recipe identity overflow"))?;
            next.insert(core.module_id, value);
        }
        for (a, span) in &self.templates {
            budget.node(*span, 0)?;
            budget.text(&a.name, *span)?;
            let module = *owners
                .get(&span.file)
                .ok_or_else(|| error(*span, "Product family lost its source module"))?;
            let index = next
                .get_mut(&module)
                .ok_or_else(|| error(*span, "Product family module is absent"))?;
            let id = NodeId(*index);
            *index = index
                .checked_add(1)
                .ok_or_else(|| error(*span, "Product recipe identity overflow"))?;
            families.insert(a.name.clone(), (module, id));
        }
        let mut templates = Vec::new();
        for (a, span) in &self.templates {
            let formal = parameter(a).expect("collected bounded Product");
            budget.node(*span, 0)?;
            budget.text(&a.name, *span)?;
            budget.text(&formal.name, *span)?;
            let mut fields = Vec::new();
            for f in &a.fields {
                budget.node(f.span, 0)?;
                budget.text(&f.name, f.span)?;
                let ty = recipe(&f.ty, &formal.name, &families, budget, 0)?;
                fields.push(CoreAggregateFieldRecipe {
                    name: f.name.clone(),
                    public: f.public,
                    manages: f.manages,
                    ty,
                    span: f.span,
                });
            }
            let (module, id) = families[&a.name];
            templates.push((
                module,
                CoreAggregateTemplate {
                    id,
                    name: a.name.clone(),
                    parameter: CoreTypeParameter {
                        index: 0,
                        name: formal.name.clone(),
                        bound: CoreTypeBound::StoreElement,
                    },
                    fields,
                    span: *span,
                },
            ));
        }
        let mut applications = Vec::new();
        for a in &self.applications {
            for (core, _) in cores.iter() {
                for d in &core.aggregates {
                    budget.node(d.span, 0)?;
                    budget.text(&d.name, d.span)?;
                }
                if a.scope.is_some() {
                    for f in &core.functions {
                        budget.node(f.span, 0)?;
                        budget.text(&f.name, f.span)?;
                    }
                }
            }
            let (module, aggregate) = cores
                .iter()
                .find_map(|(c, _)| {
                    c.aggregates
                        .iter()
                        .find(|d| d.name == a.aggregate)
                        .map(|d| (c.module_id, d.id))
                })
                .ok_or_else(|| {
                    error(
                        Span::default(),
                        "Product application lost generated declaration",
                    )
                })?;
            let (template_module, template) = families[&a.template];
            let owner = a
                .scope
                .as_ref()
                .map(|s| {
                    cores
                        .iter()
                        .find_map(|(c, _)| {
                            c.functions
                                .iter()
                                .find(|f| f.name == s.owner && f.span == s.span)
                                .map(|f| CoreAggregateOwner {
                                    module: c.module_id,
                                    function: f.id,
                                })
                        })
                        .ok_or_else(|| error(s.span, "Product application owner is absent"))
                })
                .transpose()?;
            let mut arguments = Vec::new();
            for t in &a.arguments {
                budget.ty(t)?;
                let ordinary = a.scope.as_ref().and_then(|s| s.ordinary.as_deref());
                arguments.push(
                    if ordinary.is_some_and(|name| callable::parameter_mode(t, name) == Some(None))
                    {
                        CoreAggregateArgument::Parameter { index: 0 }
                    } else {
                        CoreAggregateArgument::Concrete(super::super::type_ref(t))
                    },
                );
            }
            applications.push((
                module,
                CoreAggregateApplication {
                    aggregate,
                    template_module,
                    template,
                    owner,
                    arguments,
                },
            ));
        }
        for (module, t) in templates {
            let (core, map) = cores
                .iter_mut()
                .find(|(c, _)| c.module_id == module)
                .expect("checked module");
            budget.node(t.span, 0)?;
            map.entries.push(SourceMapEntry {
                id: SourceMapId(map.entries.len() as u32),
                target: SourceTarget::Node(t.id),
                origin: t.span,
                role: ExpansionRole::Source,
                related: Vec::new(),
                parent: None,
            });
            core.aggregate_templates.push(t);
        }
        for (module, a) in applications {
            cores
                .iter_mut()
                .find(|(c, _)| c.module_id == module)
                .expect("checked module")
                .0
                .aggregate_applications
                .push(a);
        }
        Ok(())
    }
}
fn recipe(
    ty: &TypeRef,
    formal: &str,
    families: &BTreeMap<String, (ModuleId, NodeId)>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<CoreAggregateTypeRecipe> {
    budget.node(ty.span, depth)?;
    if callable::parameter_mode(ty, formal) == Some(None) {
        return Ok(CoreAggregateTypeRecipe::Parameter { index: 0 });
    }
    if let Some(element) = stores::element(ty) {
        return Ok(CoreAggregateTypeRecipe::Store {
            element: Box::new(recipe(element, formal, families, budget, depth + 1)?),
        });
    }
    if let TypeKind::Named { path, args } = &grouped(ty).kind
        && path.len() == 1
        && let Some((module, template)) = families.get(&path[0])
    {
        let mut arguments = Vec::new();
        for arg in args {
            let GenericArg::Type(t) = arg else {
                return Err(unsupported(ty.span, "constant Product argument"));
            };
            arguments.push(recipe(t, formal, families, budget, depth + 1)?);
        }
        return Ok(CoreAggregateTypeRecipe::Apply {
            module: *module,
            template: *template,
            arguments,
        });
    }
    if callable::refers(ty, formal) {
        return Err(unsupported(
            ty.span,
            "Product parameter requires a direct field, Store or finite child application",
        ));
    }
    budget.ty(ty)?;
    Ok(CoreAggregateTypeRecipe::Concrete(super::super::type_ref(
        ty,
    )))
}
