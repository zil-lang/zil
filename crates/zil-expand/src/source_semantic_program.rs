//! One request's original templates, actual Engine owners and Core emissions.
//! All rows remain untrusted program data for Types and the safety kernels.
use super::*;
use std::collections::BTreeMap;
use zil_base::NodeId;
const TEMPLATE: u32 = 1 << 31;
#[path = "source_semantic_scopes.rs"]
mod scopes;
struct FunctionTemplate {
    definition: SemanticDefinitionId,
    parameters: Vec<s::GenericParam>,
}
struct Instance {
    id: SemanticInstanceId,
    template: usize,
    actuals: Vec<SemanticActualRecipe>,
    from_call: bool,
    core: Option<(ModuleId, NodeId)>,
}
struct Child {
    id: SemanticInstanceId,
    definition: SemanticDefinitionId,
    parent: SemanticInstanceId,
    root: SemanticInstanceId,
    environment: Vec<SemanticActualRecipe>,
    core: Option<SemanticClosureTarget>,
}
#[derive(Default)]
pub(crate) struct Program {
    pub(crate) nominal_declarations: Vec<LifeNominalDeclaration>,
    dyn_templates: Vec<dyn_original::Template>,
    pub(super) original_nodes: Vec<DefinitionBodyNodes>,
    pub(crate) modules: Vec<(ModuleId, SemanticModule)>,
    templates: Vec<FunctionTemplate>,
    instances: Vec<Instance>,
    pub(crate) ledger: emission::Ledger,
    pub(crate) nodes: Vec<(ModuleId, crate::semantic_lower::SemanticNodeEmission)>,
    next_instance: u32,
    children: Vec<Child>,
    closure_events: BTreeMap<u32, usize>,
    lexical_scopes: BTreeMap<SemanticDefinitionId, SemanticInstanceId>,
    generated_targets: BTreeMap<ModuleId, BTreeMap<String, usize>>,
    call_requests: Vec<requests::Request>,
    scope_applications: Vec<scopes::Application>,
    scope_owners: BTreeMap<SemanticInstanceId, SemanticInstanceId>,
    scope_requests: Vec<requests::Request>,
}
impl Program {
    /// Borrow the original lexical names of this exact generation owner.
    /// Template and actual markers are different namespaces; neither selects
    /// the first function sharing an origin or a generated spelling.
    pub(crate) fn nominal_owner_names(
        &self,
        owner: s::SemanticContextMarker,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<&[SemanticOriginalNameBinding]> {
        let raw = owner
            .raw_index()
            .ok_or_else(|| error(span, "nominal generation owner is absent"))?;
        let template = if raw & TEMPLATE != 0 {
            (raw & !TEMPLATE) as usize
        } else {
            self.instances
                .get(raw as usize)
                .ok_or_else(|| error(span, "nominal generation instance is absent"))?
                .template
        };
        let definition = self
            .templates
            .get(template)
            .ok_or_else(|| error(span, "nominal generation template is absent"))?
            .definition;
        budget.identity_work(self.modules.len(), span)?;
        let module = self
            .modules
            .iter()
            .find(|(id, _)| *id == definition.module)
            .ok_or_else(|| error(span, "nominal generation module is absent"))?;
        budget.identity_work(module.1.definitions.len(), span)?;
        let original = module
            .1
            .definitions
            .iter()
            .find(|row| row.id == definition)
            .ok_or_else(|| error(span, "nominal generation original definition is absent"))?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &original.body else {
            return Err(error(
                span,
                "nominal generation owner lacks its original body",
            ));
        };
        Ok(names)
    }
    pub(crate) fn valid_function(&self, marker: s::SemanticContextMarker) -> bool {
        marker.raw_index().is_some_and(|raw| {
            if raw & TEMPLATE != 0 {
                self.templates.get((raw & !TEMPLATE) as usize).is_some()
            } else {
                self.instances.get(raw as usize).is_some()
            }
        })
    }
    pub(crate) fn valid_node(&self, marker: s::SemanticContextMarker) -> bool {
        self.ledger.is_template(marker)
    }
    pub(crate) fn ensure_function(
        &mut self,
        function: &mut s::Function,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let Some(raw) = function.semantic.raw_index() else {
            return Ok(());
        };
        if raw & TEMPLATE == 0 {
            return Ok(());
        }
        let index = (raw & !TEMPLATE) as usize;
        let template = self
            .templates
            .get(index)
            .ok_or_else(|| error(function.span, "function Source owner is absent"))?;
        let id = SemanticInstanceId {
            module: template.definition.module,
            index: self.next_instance,
        };
        let mut actuals = Vec::new();
        for (ordinal, formal) in template.parameters.iter().enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: ordinal as u32,
            };
            let value = match formal.kind {
                s::GenericParamKind::Lifetime => {
                    SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                }
                s::GenericParamKind::Type(_) => {
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                }
                s::GenericParamKind::Const(_) => {
                    return Err(unsupported(
                        formal.span,
                        "remaining Const owner requires actual specialization operands",
                    ));
                }
            };
            budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), formal.span)?;
            actuals.push(value);
        }
        self.ledger
            .original_owner(id, template.definition, budget, function.span)?;
        budget.lineage_bytes(std::mem::size_of::<Instance>(), function.span)?;
        let marker = u32::try_from(self.instances.len())
            .map_err(|_| error(function.span, "semantic function marker exhausted"))?;
        if marker >= TEMPLATE {
            return Err(error(
                function.span,
                "semantic function marker overlaps templates",
            ));
        }
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(function.span, "semantic instance namespace exhausted"))?;
        self.instances.push(Instance {
            id,
            template: index,
            actuals,
            from_call: false,
            core: None,
        });
        function.semantic = s::SemanticContextMarker::untrusted(marker);
        self.realize_body(function, budget)
    }
    pub(crate) fn specialize(
        &mut self,
        function: &mut s::Function,
        arguments: &BTreeMap<String, s::TypeRef>,
        module: ModuleId,
        generated: &str,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let Some(raw) = function.semantic.raw_index() else {
            return Ok(());
        };
        if raw & TEMPLATE == 0 {
            return Err(error(
                function.span,
                "actual function specialization reuses a completed owner",
            ));
        }
        let index = (raw & !TEMPLATE) as usize;
        let template = self
            .templates
            .get(index)
            .ok_or_else(|| error(function.span, "specialized Source template is absent"))?;
        let id = SemanticInstanceId {
            module: template.definition.module,
            index: self.next_instance,
        };
        // The caller's original body, not an erased specialization TypeRef,
        // determines all original mixed actuals, including phantom classes.
        for argument in arguments.values() {
            budget.ty(argument)?;
        }
        let actuals = Vec::new();
        self.ledger
            .original_owner(id, template.definition, budget, function.span)?;
        budget.lineage_bytes(std::mem::size_of::<Instance>(), function.span)?;
        let marker = u32::try_from(self.instances.len())
            .map_err(|_| error(function.span, "generic semantic marker namespace exhausted"))?;
        if marker >= TEMPLATE {
            return Err(error(
                function.span,
                "generic semantic marker overlaps template namespace",
            ));
        }
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(function.span, "semantic instance namespace exhausted"))?;
        self.instances.push(Instance {
            id,
            template: index,
            actuals,
            from_call: true,
            core: None,
        });
        budget.text(generated, function.span)?;
        budget.lineage_bytes(
            generated.len() + 12 * std::mem::size_of::<((ModuleId, String), usize)>(),
            function.span,
        )?;
        if self
            .generated_targets
            .entry(module)
            .or_default()
            .insert(generated.to_owned(), marker as usize)
            .is_some()
        {
            return Err(error(
                function.span,
                "actual generic emission symbol was registered twice",
            ));
        }
        function.semantic = s::SemanticContextMarker::untrusted(marker);
        self.realize_body(function, budget)
    }
    pub(crate) fn record_specialized_call(
        &mut self,
        marker: s::SemanticContextMarker,
        module: ModuleId,
        generated: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        let Some(raw) = marker.raw_index() else {
            return Ok(());
        };
        budget.text(generated, span)?;
        budget.identity_work(
            self.generated_targets.len().checked_ilog2().unwrap_or(0) as usize + 1,
            span,
        )?;
        budget.identity_work(
            self.generated_targets.get(&module).map_or(0, |targets| {
                targets.len().checked_ilog2().unwrap_or(0) as usize + 1
            }),
            span,
        )?;
        let Some(target) = self
            .generated_targets
            .get(&module)
            .and_then(|targets| targets.get(generated))
            .copied()
        else {
            return Ok(());
        };
        let (caller, call) = self.ledger.actual_call_site(raw).ok_or_else(|| {
            error(
                span,
                "generic call lacks its actual original operation token",
            )
        })?;
        let callee = self
            .instances
            .get(target)
            .ok_or_else(|| error(span, "generic call target emission owner is absent"))?
            .id;
        budget.identity_work(self.call_requests.len(), span)?;
        if let Some(previous) = self
            .call_requests
            .iter()
            .find(|request| request.caller == caller && request.call == call)
        {
            if previous.callee != callee {
                return Err(error(
                    span,
                    "one actual original call resolves to conflicting generic emissions",
                ));
            }
            return Ok(());
        }
        budget.lineage_bytes(std::mem::size_of::<requests::Request>(), span)?;
        self.call_requests.push(requests::Request {
            caller,
            call,
            callee,
        });
        Ok(())
    }
    fn realize_body(&mut self, function: &mut s::Function, budget: &mut Budget<'_>) -> R<()> {
        use crate::generated_spans::Visitor;
        let raw = function
            .semantic
            .raw_index()
            .ok_or_else(|| error(function.span, "actual function owner is absent"))?;
        let owner = self
            .instances
            .get(raw as usize)
            .ok_or_else(|| error(function.span, "actual function owner is out of range"))?
            .id;
        struct Realize<'a, 'b, 'l> {
            program: &'a mut Program,
            owner: SemanticInstanceId,
            root: SemanticInstanceId,
            budget: &'b mut Budget<'l>,
        }
        impl Visitor for Realize<'_, '_, '_> {
            fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
                self.budget.node(*span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.budget.text(text, span).map_err(|e| vec![e])
            }
            fn expression(&mut self, e: &mut s::Expr, depth: usize) -> CompileResult<()> {
                let original = e.semantic;
                let child = if matches!(e.kind, s::ExprKind::Closure { .. })
                    && self.program.ledger.is_template(original)
                {
                    Some(
                        self.program
                            .allocate_child(original, self.owner, self.root, self.budget, e.span)
                            .map_err(|e| vec![e])?,
                    )
                } else {
                    None
                };
                if self.program.ledger.is_template(e.semantic) {
                    e.semantic = self
                        .program
                        .ledger
                        .instantiate_template(e.semantic, self.owner, self.budget, e.span)
                        .map_err(|e| vec![e])?;
                }
                if let Some(child) = child {
                    let raw = e.semantic.raw_index().expect("actual source closure event");
                    self.program.closure_events.insert(raw, child);
                    let parent = self.owner;
                    self.owner = self.program.children[child].id;
                    let result = crate::generated_spans::walk_expression(self, e, depth);
                    self.owner = parent;
                    result
                } else {
                    crate::generated_spans::walk_expression(self, e, depth)
                }
            }
            fn statement(&mut self, s: &mut s::Stmt, depth: usize) -> CompileResult<()> {
                if self.program.ledger.is_template(s.semantic) {
                    s.semantic = self
                        .program
                        .ledger
                        .instantiate_template(s.semantic, self.owner, self.budget, s.span)
                        .map_err(|e| vec![e])?;
                }
                crate::generated_spans::walk_statement(self, s, depth)
            }
        }
        let mut pass = Realize {
            program: self,
            owner,
            root: owner,
            budget,
        };
        match &mut function.body {
            s::FunctionBody::Block(body) => pass.block(body, 0),
            s::FunctionBody::Expression(e) => pass.expression(e, 0),
            s::FunctionBody::Declaration => Ok(()),
        }
        .map_err(|mut e| e.remove(0))
    }
    fn allocate_child(
        &mut self,
        marker: s::SemanticContextMarker,
        parent: SemanticInstanceId,
        root: SemanticInstanceId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<usize> {
        let (owner, site) = self
            .ledger
            .template_site(marker)
            .ok_or_else(|| error(span, "closure literal has no original constructor event"))?;
        let module = self
            .modules
            .iter()
            .find(|(id, _)| *id == owner.module)
            .ok_or_else(|| error(span, "closure original module is absent"))?;
        let definition = module
            .1
            .definitions
            .iter()
            .find(|row| row.id == owner)
            .ok_or_else(|| error(span, "closure original parent is absent"))?;
        let sites = match &definition.body {
            SemanticDefinitionBody::Function { sites, .. }
            | SemanticDefinitionBody::FunctionWithBody { sites, .. } => sites,
            _ => return Err(error(span, "closure original parent is not a function")),
        };
        let original = sites
            .iter()
            .find(|row| row.id == site)
            .ok_or_else(|| error(span, "closure original site is absent"))?;
        let SemanticSiteRecipeKind::Closure {
            definition,
            environment,
        } = &original.kind
        else {
            return Err(error(
                span,
                "closure literal uses a different original operation",
            ));
        };
        for value in environment {
            prepay_recipe(value, budget, span, 0)?;
        }
        budget.lineage_bytes(
            std::mem::size_of::<Child>()
                + environment.len() * std::mem::size_of::<SemanticActualRecipe>(),
            span,
        )?;
        let id = SemanticInstanceId {
            module: owner.module,
            index: self.next_instance,
        };
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(span, "closure instance namespace exhausted"))?;
        self.ledger.original_owner(id, *definition, budget, span)?;
        let index = self.children.len();
        self.children.push(Child {
            id,
            definition: *definition,
            parent,
            root,
            environment: environment.clone(),
            core: None,
        });
        Ok(index)
    }
    /// A private trait alias really lowers this exact source body a second
    /// time. Mint a new owner, child forest and immutable-template productions
    /// before binding its distinct Core creator; ordinary duplicate binds stay
    /// errors. The caller supplies the first creator from the actual alias
    /// lowering event, never from a spelling or matching function shape.
    pub(crate) fn fork_function_owner(
        &mut self,
        marker: u32,
        module: ModuleId,
        source: NodeId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<(u32, BTreeMap<u32, u32>)> {
        let old = self
            .instances
            .get(marker as usize)
            .ok_or_else(|| error(span, "alias clone original function owner is absent"))?;
        if old.core != Some((module, source)) {
            return Err(error(
                span,
                "alias clone does not name its actual first Core creator",
            ));
        }
        // A generic body's specialization request also carries the actual
        // caller/Call identity. It cannot be recreated from a code alias.
        if old.from_call {
            return Err(unsupported(
                span,
                "generic code alias needs its actual independent specialization request",
            ));
        }
        let old_id = old.id;
        let template = old.template;
        for actual in &old.actuals {
            prepay_recipe(actual, budget, span, 0)?;
        }
        budget.lineage_bytes(
            old.actuals
                .len()
                .checked_mul(std::mem::size_of::<SemanticActualRecipe>())
                .ok_or_else(|| error(span, "alias actual clone allocation overflow"))?,
            span,
        )?;
        let actuals = old.actuals.clone();
        let definition = self.templates[template].definition;
        let id = SemanticInstanceId {
            module: definition.module,
            index: self.next_instance,
        };
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(span, "alias owner instance namespace exhausted"))?;
        let marker = u32::try_from(self.instances.len())
            .map_err(|_| error(span, "alias function marker namespace exhausted"))?;
        if marker >= TEMPLATE {
            return Err(error(
                span,
                "alias marker overlaps original template namespace",
            ));
        }
        self.ledger.original_owner(id, definition, budget, span)?;
        budget.lineage_push(
            &mut self.instances,
            Instance {
                id,
                template,
                actuals,
                from_call: false,
                core: None,
            },
            span,
        )?;
        if let Some(scope) = self.scope_owners.get(&old_id).copied() {
            budget.lineage_bytes(
                12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                span,
            )?;
            self.scope_owners.insert(id, scope);
        }
        let mut owners = BTreeMap::new();
        budget.lineage_bytes(
            12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
            span,
        )?;
        owners.insert(old_id, id);
        let mut children = BTreeMap::new();
        let old_children = self.children.len();
        for index in 0..old_children {
            budget.node(span, 0)?;
            let old = &self.children[index];
            if old.root != old_id {
                continue;
            }
            if old.core.is_some() {
                return Err(error(
                    span,
                    "alias clone attempted after child emission consumption",
                ));
            }
            let parent = *owners
                .get(&old.parent)
                .ok_or_else(|| error(span, "alias child has no actual cloned lexical parent"))?;
            let child = SemanticInstanceId {
                module: old.definition.module,
                index: self.next_instance,
            };
            self.next_instance = self
                .next_instance
                .checked_add(1)
                .ok_or_else(|| error(span, "alias child namespace exhausted"))?;
            for actual in &old.environment {
                prepay_recipe(actual, budget, span, 0)?;
            }
            budget.lineage_bytes(
                old.environment
                    .len()
                    .checked_mul(std::mem::size_of::<SemanticActualRecipe>())
                    .ok_or_else(|| error(span, "alias environment clone allocation overflow"))?,
                span,
            )?;
            let environment = old.environment.clone();
            let definition = old.definition;
            let source_id = old.id;
            self.ledger
                .original_owner(child, definition, budget, span)?;
            let new_index = self.children.len();
            budget.lineage_push(
                &mut self.children,
                Child {
                    id: child,
                    definition,
                    parent,
                    root: id,
                    environment,
                    core: None,
                },
                span,
            )?;
            budget.lineage_bytes(
                12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                span,
            )?;
            owners.insert(source_id, child);
            budget.lineage_bytes(12 * std::mem::size_of::<(usize, usize)>(), span)?;
            children.insert(index, new_index);
        }
        let original_requests = self.call_requests.len();
        for index in 0..original_requests {
            budget.node(span, 0)?;
            let request = self.call_requests[index];
            if let Some(caller) = owners.get(&request.caller) {
                // The actual duplicated body still names this same concrete
                // helper. Only its caller/child owner has been reminted.
                budget.lineage_push(
                    &mut self.call_requests,
                    requests::Request {
                        caller: *caller,
                        call: request.call,
                        callee: request.callee,
                    },
                    span,
                )?;
            }
        }
        let events = self.ledger.fork_templates(&owners, budget, span)?;
        let mut closure_events = Vec::new();
        for (old, new) in &events {
            budget.node(span, 0)?;
            if let Some(index) = self.closure_events.get(old) {
                let child = *children.get(index).ok_or_else(|| {
                    error(
                        span,
                        "alias Closure production lacks its actual cloned child",
                    )
                })?;
                budget.lineage_push(&mut closure_events, (*new, child), span)?;
            }
        }
        for (event, child) in closure_events {
            budget.lineage_bytes(12 * std::mem::size_of::<(u32, usize)>(), span)?;
            if self.closure_events.insert(event, child).is_some() {
                return Err(error(span, "alias Closure event was registered twice"));
            }
        }
        Ok((marker, events))
    }
    pub(crate) fn bind_function(
        &mut self,
        marker: u32,
        module: ModuleId,
        node: NodeId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        let instance = self
            .instances
            .get_mut(marker as usize)
            .ok_or_else(|| error(span, "actual emitted function owner is absent"))?;
        if let Some((previous_module, previous)) = instance.core {
            return Err(error(
                span,
                &format!(
                    "Source function owner {marker} was emitted twice: original module {} Core {}, new module {} Core {}",
                    previous_module.0, previous.0, module.0, node.0
                ),
            ));
        }
        let definition = self.templates[instance.template].definition;
        self.ledger
            .bind_creator(instance.id, definition, node, budget, span)?;
        instance.core = Some((module, node));
        for child in &self.children {
            if child.root == instance.id {
                self.ledger
                    .bind_creator(child.id, child.definition, node, budget, span)?;
            }
        }
        Ok(())
    }
    pub(crate) fn consume_nodes(
        &mut self,
        outputs: &[(ProgramModule, zil_base::SourceMap)],
        budget: &mut Budget<'_>,
    ) -> R<()> {
        let mut inventory = nodes::Inventory::new(
            self.nodes
                .iter()
                .map(|(module, row)| (*module, row.creator, row.node, row.statement)),
            budget,
        )?;
        for (module, _) in outputs {
            for function in &module.core.functions {
                inventory.block(
                    module.core.module_id,
                    function.id,
                    &function.body,
                    budget,
                    0,
                )?;
            }
        }
        for (module, emission) in std::mem::take(&mut self.nodes) {
            let owner = self.ledger.actual_owner(emission.marker).ok_or_else(|| {
                error(
                    Span::default(),
                    "actual semantic node has no unconsumed Source owner",
                )
            })?;
            let node = inventory
                .nodes
                .get(&(module, emission.creator, emission.node, emission.statement))
                .and_then(|node| *node)
                .ok_or_else(|| {
                    error(
                        Span::default(),
                        &format!("actual semantic emission does not locate its minted Core node: module {} creator {} node {} statement {} token {}",module.0,emission.creator.0,emission.node.0,emission.statement,emission.marker),
                    )
                })?;
            let site = match node {
                nodes::Node::Expression(expression) => {
                    let role = self.ledger.actual_role(emission.marker).ok_or_else(|| {
                        error(expression.span, "actual semantic expression has no role")
                    })?;
                    self.ledger.expression(
                        s::SemanticContextMarker::untrusted(emission.marker),
                        owner,
                        emission.creator,
                        expression,
                        0,
                        role,
                        budget,
                    )?
                }
                nodes::Node::Statement(statement) => self.ledger.statement(
                    s::SemanticContextMarker::untrusted(emission.marker),
                    owner,
                    emission.creator,
                    statement,
                    0,
                    budget,
                )?,
            };
            if let SemanticSiteAction::Closure { closure, .. } = &site.action {
                let index = self
                    .closure_events
                    .get(&emission.marker)
                    .copied()
                    .ok_or_else(|| {
                        error(
                            Span::default(),
                            "closure mint has no actual original child owner",
                        )
                    })?;
                let child = self
                    .children
                    .get_mut(index)
                    .ok_or_else(|| error(Span::default(), "closure child owner is absent"))?;
                if child.parent != owner || child.core.is_some() {
                    return Err(error(
                        Span::default(),
                        "closure mint reuses or changes its actual lexical parent",
                    ));
                }
                child.core = Some(closure.clone());
            }
            budget.lineage_bytes(std::mem::size_of::<SemanticSite>(), Span::default())?;
            let target = self
                .modules
                .iter_mut()
                .find(|(id, _)| *id == owner.module)
                .ok_or_else(|| {
                    error(
                        Span::default(),
                        "actual semantic node owner module is absent",
                    )
                })?;
            target.1.sites.push(site);
        }
        Ok(())
    }
    fn closed_lexical_scope(
        &mut self,
        definition: SemanticDefinitionId,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<SemanticInstanceId> {
        let span = Span::default();
        budget.node(span, depth)?;
        budget.identity_work(
            self.lexical_scopes.len().checked_ilog2().unwrap_or(0) as usize + 1,
            span,
        )?;
        if let Some(id) = self.lexical_scopes.get(&definition) {
            return Ok(*id);
        }
        let original = self
            .modules
            .iter()
            .flat_map(|(_, module)| &module.definitions)
            .find(|row| row.id == definition)
            .ok_or_else(|| error(span, "actual lexical scope original declaration is absent"))?;
        if !original.parameters.is_empty() || !original.environment.is_empty() {
            return Err(unsupported(
                span,
                &format!(
                    "generic lexical scope requires the actual complete call and receiver substitution: definition {}:{}",
                    definition.module.0, definition.index
                ),
            ));
        }
        let parent = original.parent;
        let self_type = match &original.body {
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::TraitImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
            ) => {
                prepay_recipe(self_type, budget, span, 0)?;
                self_type.clone()
            }
            SemanticDefinitionBody::Nominal { .. } => SemanticTypeRecipe::Nominal {
                definition,
                arguments: Vec::new(),
            },
            _ => {
                return Err(unsupported(
                    span,
                    "abstract lexical Self requires its actual complete implementation substitution",
                ));
            }
        };
        let parent = parent
            .map(|parent| self.closed_lexical_scope(parent, budget, depth + 1))
            .transpose()?;
        let id = SemanticInstanceId {
            module: definition.module,
            index: self.next_instance,
        };
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(span, "lexical scope instance namespace exhausted"))?;
        budget.lineage_bytes(
            std::mem::size_of::<SemanticInstance>()
                + 12 * std::mem::size_of::<(SemanticDefinitionId, SemanticInstanceId)>(),
            span,
        )?;
        let row = SemanticInstance {
            id,
            definition,
            target: SemanticInstanceTarget::Scope,
            lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
            parent,
            actuals: Vec::new(),
            header_embedding: Vec::new(),
            environment: None,
            self_type: Some(SemanticTypeValue::Core {
                scope: id,
                source: SemanticCoreTypeSource::OriginalRecipe(Box::new(self_type)),
            }),
        };
        let module = self
            .modules
            .iter_mut()
            .find(|(module, _)| *module == id.module)
            .ok_or_else(|| error(span, "actual lexical scope module is absent"))?;
        module.1.instances.push(row);
        self.lexical_scopes.insert(definition, id);
        Ok(id)
    }
    pub(crate) fn publish(
        &mut self,
        cores: &mut [(&mut CoreModule, &mut zil_base::SourceMap)],
        budget: &mut Budget<'_>,
    ) -> R<()> {
        self.ledger.finish(budget, Span::default())?;
        // Complete non-generic lexical scopes are data derived from their
        // actual original declarations. Generic scopes need actual caller
        // substitutions and are never silently represented by empty actuals.
        let mut parents = BTreeMap::new();
        for instance in &self.instances {
            let definition = self.templates[instance.template].definition;
            let row = self
                .modules
                .iter()
                .flat_map(|(_, module)| &module.definitions)
                .find(|row| row.id == definition)
                .ok_or_else(|| error(Span::default(), "function original scope is absent"))?;
            if let Some(parent) = row.parent {
                budget.lineage_bytes(
                    12 * std::mem::size_of::<(SemanticInstanceId, SemanticDefinitionId)>(),
                    Span::default(),
                )?;
                parents.insert(instance.id, parent);
            }
        }
        self.publish_impl_scopes(budget)?;
        let mut resolved_parents = BTreeMap::new();
        for (instance, definition) in parents {
            let parent = if let Some(scope) = self.scope_owners.get(&instance) {
                *scope
            } else {
                self.closed_lexical_scope(definition, budget, 0)?
            };
            budget.lineage_bytes(
                12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                Span::default(),
            )?;
            resolved_parents.insert(instance, parent);
        }
        for instance in &self.instances {
            let (module, function) = instance.core.ok_or_else(|| {
                error(
                    Span::default(),
                    "Source instance has no actual Core function target",
                )
            })?;
            let definition = self.templates[instance.template].definition;
            let data = self
                .modules
                .iter_mut()
                .find(|(id, _)| *id == instance.id.module)
                .ok_or_else(|| error(Span::default(), "semantic owner module is absent"))?;
            let original = data
                .1
                .definitions
                .iter()
                .find(|row| row.id == definition)
                .ok_or_else(|| error(Span::default(), "semantic original definition is absent"))?;
            let parent = resolved_parents.get(&instance.id).copied();
            if original.parent.is_some() != parent.is_some() {
                return Err(error(
                    Span::default(),
                    "method original lexical parent is unresolved",
                ));
            }
            let mut actuals = Vec::new();
            for actual in &instance.actuals {
                prepay_recipe(actual, budget, Span::default(), 0)?;
                budget.lineage_bytes(
                    std::mem::size_of::<SemanticActual>()
                        + std::mem::size_of::<SemanticActualRecipe>(),
                    Span::default(),
                )?;
                actuals.push(SemanticActual::CoreRecipe {
                    scope: instance.id,
                    recipe: Box::new(actual.clone()),
                });
            }
            budget.lineage_bytes(std::mem::size_of::<SemanticInstance>(), Span::default())?;
            data.1.instances.push(SemanticInstance {
                id: instance.id,
                definition,
                target: SemanticInstanceTarget::Function(SemanticFunctionTarget::Core {
                    module,
                    function,
                }),
                lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
                parent,
                actuals,
                header_embedding: Vec::new(),
                environment: None,
                self_type: parent.map(|parent| SemanticTypeValue::Core {
                    scope: parent,
                    source: SemanticCoreTypeSource::OriginalRecipe(Box::new(
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(0),
                        },
                    )),
                }),
            });
        }
        for child in &self.children {
            let closure = child.core.clone().ok_or_else(|| {
                error(
                    Span::default(),
                    "original closure has no actual Core constructor",
                )
            })?;
            let SemanticClosureTarget::Core {
                module,
                creator,
                expression,
                occurrence,
            } = closure
            else {
                return Err(error(Span::default(), "Source closure target is not Core"));
            };
            let mut actuals = Vec::new();
            for value in &child.environment {
                prepay_recipe(value, budget, Span::default(), 0)?;
                budget.lineage_bytes(
                    std::mem::size_of::<SemanticActual>()
                        + std::mem::size_of::<SemanticActualRecipe>(),
                    Span::default(),
                )?;
                actuals.push(SemanticActual::CoreRecipe {
                    scope: child.parent,
                    recipe: Box::new(value.clone()),
                });
            }
            let target = self
                .modules
                .iter_mut()
                .find(|(id, _)| *id == child.id.module)
                .ok_or_else(|| error(Span::default(), "closure semantic module is absent"))?;
            budget.lineage_bytes(std::mem::size_of::<SemanticInstance>(), Span::default())?;
            target.1.instances.push(SemanticInstance {
                id: child.id,
                definition: child.definition,
                target: SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                    module,
                    creator,
                    expression,
                    occurrence,
                }),
                lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
                parent: Some(child.parent),
                actuals: Vec::new(),
                header_embedding: Vec::new(),
                environment: Some(SemanticEnvironmentInstance {
                    closure: SemanticClosureTarget::Core {
                        module,
                        creator,
                        expression,
                        occurrence,
                    },
                    actuals,
                }),
                self_type: None,
            });
        }
        let mut pending = Vec::new();
        for instance in &self.instances {
            if instance.from_call {
                budget.lineage_bytes(
                    std::mem::size_of::<(SemanticInstanceId, usize)>(),
                    Span::default(),
                )?;
                pending.push((
                    instance.id,
                    self.templates[instance.template].parameters.len(),
                ));
            }
        }
        for application in &self.scope_applications {
            budget.lineage_push(
                &mut pending,
                (application.id, application.parameters),
                Span::default(),
            )?;
        }
        let mut requests = Vec::new();
        for request in self.call_requests.iter().chain(&self.scope_requests) {
            budget.lineage_push(&mut requests, *request, Span::default())?;
        }
        requests::expand(
            &mut self.modules,
            &requests,
            &pending,
            &mut self.next_instance,
            budget,
        )?;
        for declaration in &self.nominal_declarations {
            if !declaration.id.compiler {
                continue;
            }
            let mut present = false;
            for (core, _) in cores.iter() {
                budget.identity_work(core.life_declarations.len(), Span::default())?;
                for prior in core
                    .life_declarations
                    .iter()
                    .filter(|row| row.id == declaration.id)
                {
                    if prior != declaration {
                        return Err(error(
                            Span::default(),
                            "compiler nominal inventory conflicts with its actual template",
                        ));
                    }
                    present = true;
                }
            }
            if !present {
                let (core, _) = cores.first_mut().ok_or_else(|| {
                    error(
                        Span::default(),
                        "compiler catalogue has no actual output module",
                    )
                })?;
                crate::semantic_nominal_lower::pay_declaration(
                    declaration,
                    budget,
                    Span::default(),
                )?;
                budget.lineage_push(
                    &mut core.life_declarations,
                    declaration.clone(),
                    Span::default(),
                )?;
            }
        }
        for (core, _) in cores.iter_mut() {
            let index = self
                .modules
                .iter()
                .position(|(module, _)| *module == core.module_id)
                .ok_or_else(|| {
                    error(
                        Span::default(),
                        "Core output has no complete original semantic module",
                    )
                })?;
            for declaration in &self.nominal_declarations {
                budget.node(Span::default(), 0)?;
                if declaration.id.module != core.module_id {
                    continue;
                }
                budget.identity_work(core.life_declarations.len(), Span::default())?;
                if let Some(prior) = core
                    .life_declarations
                    .iter()
                    .find(|row| row.id == declaration.id)
                {
                    if prior != declaration {
                        return Err(error(
                            Span::default(),
                            "original nominal catalogue conflicts with actual emission",
                        ));
                    }
                } else {
                    crate::semantic_nominal_lower::pay_declaration(
                        declaration,
                        budget,
                        Span::default(),
                    )?;
                    budget.lineage_push(
                        &mut core.life_declarations,
                        declaration.clone(),
                        Span::default(),
                    )?;
                }
            }
            let data = &mut self.modules[index].1;
            core.semantic = Some(SemanticModule {
                definitions: std::mem::take(&mut data.definitions),
                instances: std::mem::take(&mut data.instances),
                sites: std::mem::take(&mut data.sites),
                dyn_scopes: std::mem::take(&mut data.dyn_scopes),
            });
        }
        dyn_original::publish(&mut self.dyn_templates, cores, budget)?;
        Ok(())
    }
}

fn item_mut<'a>(module: &'a mut s::Module, path: &[u32]) -> Option<&'a mut s::Item> {
    let (root, tail) = path.split_first()?;
    let mut item = module.items.get_mut(*root as usize)?;
    for index in tail {
        let rows = match &mut item.kind {
            s::ItemKind::Aggregate(a) => &mut a.methods,
            s::ItemKind::Trait(t) => &mut t.members,
            s::ItemKind::Impl(i) => &mut i.members,
            _ => return None,
        };
        item = rows.get_mut(*index as usize)?;
    }
    Some(item)
}
pub(crate) fn prepare(
    syntax: &[&s::Module],
    origins: &[ModuleDeclarationOrigins],
    imports: &[Vec<CoreImport>],
    remaining: &mut Limits,
) -> CompileResult<(Vec<s::Module>, Program)> {
    let inventory = collect_original_semantics(syntax, origins, imports, remaining)?;
    let limits = remaining.clone();
    let mut budget = Budget::new(&limits);
    let result = (|| -> R<_> {
        let span = syntax.first().map_or(Span::default(), |module| module.span);
        let mut modules = budget.retained_copy(span, |budget| {
            // This is an actual retained AST clone. Borrowed normalization
            // lookups after it must not keep reserving hypothetical copies.
            for module in syntax {
                for item in &module.items {
                    budget.item(item)?;
                }
            }
            budget.lineage_bytes(
                syntax
                    .len()
                    .checked_mul(std::mem::size_of::<s::Module>())
                    .ok_or_else(|| error(span, "original module clone allocation overflow"))?,
                span,
            )?;
            Ok(syntax
                .iter()
                .map(|module| (*module).clone())
                .collect::<Vec<_>>())
        })?;
        let mut nominal_declarations =
            crate::semantic_nominal_lower::originals(&inventory.originals, &mut budget)?;
        for declaration in inventory.compiler_declarations {
            budget.lineage_push(&mut nominal_declarations, declaration, span)?;
        }
        let mut program = Program {
            nominal_declarations,
            modules: inventory.modules,
            original_nodes: inventory.body_nodes,
            ..Default::default()
        };
        // All functions borrow the same immutable import-edge inventory. Do
        // not allocate and retain a new copy for every function normalization.
        let mut mappings = Vec::new();
        for (module, edges) in syntax.iter().zip(imports) {
            budget.lineage_push(&mut mappings, (module.id, edges.as_slice()), span)?;
        }
        for row in &inventory.originals.declarations {
            let Some(original) = row.function() else {
                continue;
            };
            let target = modules
                .iter_mut()
                .find(|m| m.id == row.module)
                .ok_or_else(|| error(row.item.span, "original function module is absent"))?;
            let item = item_mut(target, &row.path)
                .ok_or_else(|| error(row.item.span, "actual cloned original function is absent"))?;
            let function = match &mut item.kind {
                s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => f,
                _ => {
                    return Err(error(
                        row.item.span,
                        "original function clone changed declaration kind",
                    ));
                }
            };
            let marker = u32::try_from(program.templates.len())
                .map_err(|_| error(function.span, "original function namespace exhausted"))?;
            if marker >= TEMPLATE - 1 {
                return Err(error(
                    function.span,
                    "original function namespace overlaps absent marker",
                ));
            }
            let parameters = budget.retained_copy(function.span, |budget| {
                // FunctionTemplate retains only mixed formals, not a second
                // copy of the complete source function and its owning body.
                budget.generic_parameters_snapshot(&original.generics)?;
                budget.lineage_bytes(
                    original
                        .generics
                        .len()
                        .checked_mul(std::mem::size_of::<s::GenericParam>())
                        .ok_or_else(|| {
                            error(function.span, "original generic clone allocation overflow")
                        })?,
                    function.span,
                )?;
                Ok(original.generics.clone())
            })?;
            budget.lineage_push(
                &mut program.templates,
                FunctionTemplate {
                    definition: SemanticDefinitionId {
                        module: row.module,
                        index: row.declaration.0,
                    },
                    parameters,
                },
                function.span,
            )?;
            function.semantic = s::SemanticContextMarker::untrusted(marker | TEMPLATE);
            let owner = inventory
                .originals
                .declarations
                .iter()
                .position(|candidate| {
                    candidate.module == row.module && candidate.declaration == row.declaration
                })
                .ok_or_else(|| error(function.span, "original function owner lookup failed"))?;
            let mut normalizer = Builder {
                inventory: &inventory.originals,
                compiler_nominals: &inventory.compiler_nominals,
                imports: &mappings,
                budget: &mut budget,
                owner,
                lexical_offset: 0,
                implicit: Vec::new(),
                bound_lives: Vec::new(),
                invocations: Vec::new(),
                closures: Vec::new(),
                next_body_nodes: Vec::new(),
                opaque_result: None,
                body_nodes: Vec::new(),
            };
            plan::function(&mut program, &mut normalizer, function, &inventory.closures)?;
        }
        effects::propose(&mut program.modules, &inventory.originals, &mut budget)?;
        let mut normalizer = Builder {
            inventory: &inventory.originals,
            compiler_nominals: &inventory.compiler_nominals,
            imports: &mappings,
            budget: &mut budget,
            owner: 0,
            lexical_offset: 0,
            implicit: Vec::new(),
            bound_lives: Vec::new(),
            invocations: Vec::new(),
            closures: Vec::new(),
            next_body_nodes: Vec::new(),
            opaque_result: None,
            body_nodes: Vec::new(),
        };
        program.dyn_templates = dyn_original::collect(&mut normalizer, &program.modules)?;
        budget.drop_lineage_vec(mappings, span)?;
        Ok((modules, program))
    })();
    *remaining = budget.remaining_limits();
    result.map_err(|e| vec![e])
}
