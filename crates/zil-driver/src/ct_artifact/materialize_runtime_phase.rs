//! The runtime specialization callback retains the original CT environment.
//! Every query selects immutable source expressions; specialized AST is only
//! the receiving syntax, and never enters the source authority catalog.
use super::*;
use compiler::source_sites::{Selection, SourceNode, SourceSite, SourceTreeId};
use std::cell::RefCell;
use zil_expand::runtime_ct::{RuntimeCt, RuntimeDefinition, RuntimeInstance};

fn synchronize(environment: &mut Environment<'_>, remaining: &Limits) -> CompileResult<()> {
    let previous = environment.remaining_limits_for_input();
    environment.budget.reserve_analysis_work(
        previous
            .max_analysis_steps
            .saturating_sub(remaining.max_analysis_steps) as u64,
    )?;
    environment
        .budget
        .reserve_generated_nodes(previous.max_nodes.saturating_sub(remaining.max_nodes) as u64)?;
    environment.budget.reserve_temporary_bytes(
        previous
            .max_source_bytes
            .saturating_sub(remaining.max_source_bytes) as u64,
    )
}

struct Scan<'a> {
    budget: &'a mut zil_ct::CtExecutionBudget,
    max_depth: usize,
    next: u32,
    selected: Option<u32>,
    address: Option<usize>,
    roots: Vec<usize>,
    roots_only: bool,
}
impl zil_expand::ct_selection::Visitor for Scan<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.max_depth {
            return Err(category("runtime CT source depth exhausted", span));
        }
        self.budget.reserve_analysis_work(1)
    }
    fn text(&mut self, value: &str, _: Span) -> CompileResult<()> {
        self.budget.reserve_analysis_work(value.len() as u64)
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        self.node(expression.span, depth)?;
        let ordinal = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| category("runtime CT source ordinal exhausted", expression.span))?;
        if self.selected == Some(ordinal) {
            self.address = Some(expression as *const s::Expr as usize);
        }
        if let s::ExprKind::MetaEval(inner) = &expression.kind {
            self.budget
                .reserve_temporary_bytes(std::mem::size_of::<usize>() as u64)?;
            self.roots.push(inner.as_ref() as *const s::Expr as usize);
            if self.roots_only {
                return Ok(());
            }
        }
        zil_expand::ct_selection::walk_expression(self, expression, depth)
    }
}
pub(super) fn deferred(
    function: &s::Function,
    budget: &mut zil_ct::CtExecutionBudget,
    max_depth: usize,
) -> CompileResult<bool> {
    if !function
        .generics
        .iter()
        .any(|p| !matches!(p.kind, s::GenericParamKind::Lifetime))
    {
        return Ok(false);
    }
    let mut scan = Scan {
        budget,
        max_depth,
        next: 0,
        selected: None,
        address: None,
        roots: Vec::new(),
        roots_only: true,
    };
    match &function.body {
        s::FunctionBody::Block(body) => {
            zil_expand::ct_selection::Visitor::block(&mut scan, body, 0)?
        }
        s::FunctionBody::Expression(expression) => {
            zil_expand::ct_selection::Visitor::expression(&mut scan, expression, 0)?
        }
        s::FunctionBody::Declaration => {}
    }
    Ok(!scan.roots.is_empty())
}

pub(crate) struct RuntimePhase<'a> {
    environment: RefCell<Environment<'a>>,
    sources: &'a [SourceUnit],
    parsed: &'a [Arc<ParseResult>],
    paths: &'a BTreeMap<Vec<String>, ModuleId>,
}
impl<'a> Materialized<'a> {
    pub(crate) fn take_runtime_phase(
        &mut self,
        sources: &'a [SourceUnit],
        parsed: &'a [Arc<ParseResult>],
        paths: &'a BTreeMap<Vec<String>, ModuleId>,
    ) -> Option<RuntimePhase<'a>> {
        self.runtime_environment
            .take()
            .map(|environment| RuntimePhase {
                environment: RefCell::new(environment),
                sources,
                parsed,
                paths,
            })
    }
    pub(crate) fn finish_runtime_phase(&mut self, phase: RuntimePhase<'a>) -> CompileResult<()> {
        let environment = phase.environment.into_inner();
        self.usage = environment.budget.usage().clone();
        self.usage.analysis_work = self
            .usage
            .analysis_work
            .checked_add(self.capture_usage.0)
            .ok_or_else(|| category("runtime CT capture work receipt overflow", Span::default()))?;
        self.usage.generated_nodes = self
            .usage
            .generated_nodes
            .checked_add(self.capture_usage.1)
            .ok_or_else(|| category("runtime CT capture node receipt overflow", Span::default()))?;
        self.lineage_remaining = Some(environment.remaining_limits_for_input());
        self.runtime_environment = Some(environment);
        Ok(())
    }
}

#[derive(Clone)]
enum FrozenSource {
    Original(Arc<ParseResult>),
    Generated(Arc<zil_expand::source_context::GeneratedSourceTree>),
}
impl FrozenSource {
    fn module(&self) -> &s::Module {
        match self {
            Self::Original(parsed) => &parsed.module,
            Self::Generated(tree) => &tree.module,
        }
    }
}
struct Declaration {
    source: FrozenSource,
    tree: SourceTreeId,
    local: zil_base::NodeId,
    symbol: CtSymbolId,
}
impl RuntimePhase<'_> {
    fn declaration(
        &self,
        definition: &RuntimeDefinition,
        environment: &mut Environment<'_>,
    ) -> CompileResult<Declaration> {
        if definition.origin.closure_sites
            != zil_expand::source_context::ClosureSiteLineage::Original
        {
            return Err(category(
                "runtime CT transformed declaration requires retained source call correspondence",
                Span::default(),
            ));
        }
        if definition.member.is_some() {
            return Err(category(
                "runtime CT member specialization needs its actual member ledger",
                Span::default(),
            ));
        }
        let (source, tree, local, generation_path) = if let Some(generated) =
            &definition.origin.generated
        {
            environment
                .budget
                .reserve_analysis_work(environment.generated_catalog.trees.len() as u64 + 1)?;
            let row = environment
                .generated_catalog
                .trees
                .iter()
                .position(|tree| Arc::ptr_eq(tree, &generated.tree))
                .ok_or_else(|| {
                    category(
                        "runtime CT generated tree is outside the actual catalog",
                        Span::default(),
                    )
                })?;
            (
                FrozenSource::Generated(generated.tree.clone()),
                SourceTreeId::Generated(row as u32),
                zil_base::NodeId(generated.root),
                generated.tree.generation_fingerprint(),
            )
        } else {
            environment
                .budget
                .reserve_analysis_work(self.parsed.len() as u64 + 1)?;
            let parsed = self
                .parsed
                .iter()
                .find(|parsed| parsed.module.id == definition.module)
                .ok_or_else(|| category("runtime CT original module is absent", Span::default()))?;
            (
                FrozenSource::Original(parsed.clone()),
                SourceTreeId::Original,
                definition.origin.declaration,
                None,
            )
        };
        if source.module().id != definition.module {
            return Err(category(
                "runtime CT source module differs",
                source.module().span,
            ));
        }
        Ok(Declaration {
            source,
            tree,
            local,
            symbol: CtSymbolId {
                module: definition.module,
                declaration: definition.origin.declaration,
                generation_path,
            },
        })
    }
}
impl Declaration {
    fn runtime_call(
        &self,
        ordinal: u32,
        environment: &mut Environment<'_>,
    ) -> CompileResult<SourceSite> {
        let item = self.item(environment)?;
        let mut address = None;
        zil_expand::runtime_ct::observe_explicit_calls(
            item,
            environment.remaining_limits_for_input().max_call_depth,
            |event| match event {
                zil_expand::runtime_ct::RuntimeCallEvent::Work(work) => {
                    environment.budget.reserve_analysis_work(work as u64)
                }
                zil_expand::runtime_ct::RuntimeCallEvent::Call {
                    expression,
                    ordinal: selected,
                } => {
                    if selected == ordinal {
                        address = Some(expression as *const s::Expr as usize);
                    }
                    Ok(())
                }
            },
        )?;
        self.site(
            address.ok_or_else(|| {
                category(
                    "runtime CT actual explicit source call is absent",
                    item.span,
                )
            })?,
            environment,
        )
    }

    fn item<'a>(&'a self, environment: &mut Environment<'_>) -> CompileResult<&'a s::Item> {
        compiler::source_scope::original_declaration(
            self.source.module(),
            self.local,
            environment.remaining_limits_for_input().max_call_depth,
            |work| environment.budget.reserve_analysis_work(work as u64),
        )?
        .map(|original| original.item)
        .ok_or_else(|| {
            category(
                "runtime CT original declaration is absent",
                self.source.module().span,
            )
        })
    }
    fn site(&self, address: usize, environment: &mut Environment<'_>) -> CompileResult<SourceSite> {
        compiler::source_sites::select_in_tree(
            self.source.module(),
            self.tree,
            Selection::Expr(address),
            environment.remaining_limits_for_input().max_call_depth,
            |work| environment.budget.reserve_analysis_work(work as u64),
        )?
        .map(|node| node.site)
        .ok_or_else(|| {
            category(
                "runtime CT expression is outside its frozen tree",
                self.source.module().span,
            )
        })
    }
}

pub(super) struct RuntimeBinding<'a> {
    instance: &'a RuntimeInstance,
    definition: Declaration,
    caller: CtSymbolId,
    call_site: SourceSite,
    roots: Vec<SourceSite>,
    cursor: usize,
}

impl RuntimeCt for RuntimePhase<'_> {
    fn definition_expression(
        &self,
        definition: &RuntimeDefinition,
        ordinal: u32,
        remaining: &mut Limits,
    ) -> CompileResult<s::TypeRef> {
        let mut environment = self.environment.borrow_mut();
        synchronize(&mut environment, remaining)?;
        let declaration = self.declaration(definition, &mut environment)?;
        let item = declaration.item(&mut environment)?;
        let mut scan = Scan {
            max_depth: remaining.max_call_depth,
            budget: &mut environment.budget,
            next: 0,
            selected: None,
            address: None,
            roots: Vec::new(),
            roots_only: false,
        };
        zil_expand::ct_selection::Visitor::item(&mut scan, item, 0)?;
        let address = *scan.roots.get(ordinal as usize).ok_or_else(|| {
            category(
                "symbolic runtime CT meta-expression ordinal is absent",
                item.span,
            )
        })?;
        let site = declaration.site(address, &mut environment)?;
        let result = environment.check_runtime_definition_expression(&declaration.symbol, site)?;
        *remaining = environment.remaining_limits_for_input();
        Ok(result)
    }
    fn instantiated(
        &self,
        instance: &RuntimeInstance,
        _: &s::Item,
        output: &mut s::Item,
        remaining: &mut Limits,
    ) -> CompileResult<()> {
        if instance.call.parent_instance.is_some() {
            return Err(category(
                "runtime CT nested specialization requires an authenticated parent instance",
                output.span,
            ));
        }
        let mut environment = self.environment.borrow_mut();
        synchronize(&mut environment, remaining)?;
        let declaration = self.declaration(&instance.definition, &mut environment)?;
        let caller = self.declaration(&instance.call.caller, &mut environment)?;
        let call_site =
            caller.runtime_call(instance.call.explicit_call_ordinal, &mut environment)?;
        let original = declaration.item(&mut environment)?;
        let mut scan = Scan {
            max_depth: remaining.max_call_depth,
            budget: &mut environment.budget,
            next: 0,
            selected: None,
            address: None,
            roots: Vec::new(),
            roots_only: true,
        };
        zil_expand::ct_selection::Visitor::item(&mut scan, original, 0)?;
        let mut roots = Vec::new();
        for address in scan.roots {
            environment
                .budget
                .reserve_temporary_bytes(std::mem::size_of::<SourceSite>() as u64)?;
            roots.push(declaration.site(address, &mut environment)?);
        }
        let owner = declaration.symbol.clone();
        let binding = RuntimeBinding {
            instance,
            definition: declaration,
            caller: caller.symbol,
            call_site,
            roots,
            cursor: 0,
        };
        let mut materializer = Materializer::runtime(
            &mut environment,
            self.sources,
            self.parsed,
            self.paths,
            binding,
            remaining.max_call_depth,
        );
        let s::ItemKind::Function(function) = &mut output.kind else {
            return Err(category(
                "runtime CT instance is not a function",
                output.span,
            ));
        };
        materializer
            .runtime_scopes
            .push(function.params.iter().map(|p| p.name.clone()).collect());
        match &mut function.body {
            s::FunctionBody::Block(block) => {
                materializer.block(instance.definition.module, block, &owner, 0)?
            }
            s::FunctionBody::Expression(expression) => {
                materializer.expr(instance.definition.module, expression, &owner, 0)?
            }
            s::FunctionBody::Declaration => {
                return Err(category("runtime CT instance has no body", output.span));
            }
        }
        let binding = materializer
            .runtime_binding
            .as_ref()
            .expect("runtime callback retains its binding");
        if binding.cursor != binding.roots.len() {
            return Err(category(
                "runtime CT source roots and materialized roots differ",
                output.span,
            ));
        }
        *remaining = materializer.environment.remaining_limits_for_input();
        Ok(())
    }
}

impl<'a, 'b> Materializer<'a, 'b> {
    fn runtime(
        environment: &'a mut Environment<'b>,
        sources: &'b [SourceUnit],
        parsed: &'b [Arc<ParseResult>],
        paths: &'b BTreeMap<Vec<String>, ModuleId>,
        binding: RuntimeBinding<'a>,
        max_depth: usize,
    ) -> Self {
        Self {
            runtime_binding: Some(binding),
            runtime_deferred: false,
            environment,
            sources,
            parsed,
            registry: None,
            paths,
            previews: Vec::new(),
            required_imports: Vec::new(),
            runtime_scopes: Vec::new(),
            data_source_scopes: Vec::new(),
            max_depth,
            track_declarations: false,
            declaration_lineage_preserved: true,
            member_retentions: Vec::new(),
            generated_trees: Vec::new(),
            current_generated_tree: None,
            pending_generated_tree: None,
            current_emitter: None,
            item_path: Vec::new(),
            next_generated_declaration: BTreeMap::new(),
            source_namespaces: BTreeMap::new(),
            canonical_source_sites: BTreeMap::new(),
            indexed_source_trees: BTreeSet::new(),
            bound_source_sites: Default::default(),
            generated_context: false,
            closure_site_revision: 0,
            changed_closure_sites: BTreeSet::new(),
            self_constant_sources: Vec::new(),
            self_constant_impl_sources: Vec::new(),
            self_constant_impl_outputs: Vec::new(),
        }
    }

    pub(super) fn runtime_expression_data(
        &mut self,
        span: Span,
    ) -> CompileResult<super::super::execute::EvaluatedData> {
        struct Copy<'a>(&'a mut zil_ct::CtExecutionBudget, usize);
        impl zil_expand::ct_selection::Visitor for Copy<'_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                if depth > self.1 {
                    return Err(category("runtime CT actual clone depth exhausted", span));
                }
                self.0.reserve_analysis_work(1)?;
                self.0.reserve_temporary_bytes(256)
            }
            fn text(&mut self, text: &str, _: Span) -> CompileResult<()> {
                self.0.reserve_analysis_work(text.len() as u64)?;
                self.0.reserve_temporary_bytes(text.len() as u64)
            }
        }
        let binding = self
            .runtime_binding
            .as_mut()
            .expect("runtime context dispatch");
        let site = *binding.roots.get(binding.cursor).ok_or_else(|| {
            category(
                "runtime CT instance introduced an unauthenticated expression root",
                span,
            )
        })?;
        binding.cursor += 1;
        self.environment
            .budget
            .reserve_temporary_bytes(std::mem::size_of::<
                compiler::runtime_context::RuntimeEntrySource,
            >() as u64)?;
        for argument in &binding.instance.arguments {
            self.environment
                .budget
                .reserve_temporary_bytes(
                    std::mem::size_of::<zil_expand::runtime_ct::RuntimeArgument>() as u64,
                )?;
            let mut copy = Copy(&mut self.environment.budget, self.max_depth);
            match argument {
                zil_expand::runtime_ct::RuntimeArgument::Type(ty) => {
                    zil_expand::ct_selection::Visitor::ty(&mut copy, ty, 0)?
                }
                zil_expand::runtime_ct::RuntimeArgument::Const(expression) => {
                    zil_expand::ct_selection::Visitor::expression(&mut copy, expression, 0)?
                }
                zil_expand::runtime_ct::RuntimeArgument::LifeDefinition { .. } => {}
            }
        }
        let descriptor = compiler::runtime_context::RuntimeEntrySource {
            definition: binding.definition.symbol.clone(),
            caller: binding.caller.clone(),
            call_site: binding.call_site,
            expression_site: site,
            arguments: binding.instance.arguments.clone(),
        };
        let source = binding.definition.source.clone();
        let located = compiler::source_sites::select_in_tree(
            source.module(),
            site.tree,
            Selection::Site(site),
            self.max_depth,
            |work| self.environment.budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| category("runtime CT original expression root is absent", span))?;
        let SourceNode::Expr(expression) = located.node else {
            return Err(category(
                "runtime CT original root is not an expression",
                span,
            ));
        };
        self.reject_runtime_capture(expression)?;
        let input = self.environment.empty_input(span)?;
        let entry = Entry::Expression {
            module: descriptor.definition.module,
            expression,
            definition: descriptor.definition.clone(),
            lexical_owner: Some(descriptor.definition.clone()),
            expected: None,
            parameters: &[],
        };
        self.environment
            .evaluate_runtime_data_at_source(entry, &input, span, &descriptor)
    }
}
