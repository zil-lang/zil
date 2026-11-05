//! Ordinary source AST attribution to raw CT-only CFG. This module executes no
//! source operation and produces no verified object or materialized syntax.
#[path = "compiler_builtins.rs"]
mod builtins;
#[path = "compiler_cfg.rs"]
mod cfg;
#[path = "compiler_domain.rs"]
mod domain;
#[path = "compiler_expr.rs"]
mod expr;
#[path = "compiler_generated.rs"]
mod generated;
#[path = "compiler_generics.rs"]
mod generics;
#[path = "compiler_host_context.rs"]
mod host_context;
#[path = "compiler_members.rs"]
mod members;
#[path = "compiler_mixed_generics.rs"]
mod mixed_generics;
#[path = "compiler_quote.rs"]
mod quote;
#[path = "compiler_record_relocation.rs"]
mod record_relocation;
#[path = "compiler_runtime_context.rs"]
pub(super) mod runtime_context;
#[path = "compiler_selection.rs"]
mod selection;
#[path = "compiler_source_scope.rs"]
pub(super) mod source_scope;
#[path = "compiler_source_sites.rs"]
mod source_site_events;
#[path = "source_sites.rs"]
pub(super) mod source_sites;
#[path = "compiler_symbolic_calls.rs"]
mod symbolic_calls;
#[path = "compiler_symbolic_nominal.rs"]
mod symbolic_nominal;
#[path = "compiler_types.rs"]
mod types;
use std::collections::{BTreeMap, VecDeque};
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, SourceUnit, Span};
use zil_ir::ct_artifact::*;
use zil_ir::{BinaryOp, Constant, UnaryOp};
use zil_syntax::ast as s;

pub(super) fn constant_type(value: &Constant) -> CtTy {
    expr::scalar_constant(value)
}

pub(super) fn scalar_type(name: &str) -> Option<CtScalarTy> {
    types::scalar(name)
}

/// Producer-only lexical lowering; a Type value carries no borrow permission.
pub(super) fn source_relation(
    names: &[&str],
    parameters: &[zil_syntax::ast::TypeRef],
    result: &zil_syntax::ast::TypeRef,
) -> zil_ir::FunctionLifeContract {
    use zil_syntax::ast::TypeKind;
    let tag = |mut ty: &zil_syntax::ast::TypeRef| {
        while let TypeKind::Group(inner) = &ty.kind {
            ty = inner;
        }
        match &ty.kind {
            TypeKind::Reference {
                lifetime: Some(name),
                ..
            } => names.iter().rposition(|n| *n == name).map(|i| i as u32),
            _ => None,
        }
    };
    let mut life = zil_ir::FunctionLifeContract {
        binder_count: 0,
        parameter_lives: parameters.iter().map(tag).collect(),
        result_life: tag(result),
    };
    let mut remap = vec![None; names.len()];
    for label in life
        .parameter_lives
        .iter_mut()
        .chain(std::iter::once(&mut life.result_life))
    {
        if let Some(old) = *label {
            let dense = *remap[old as usize].get_or_insert_with(|| {
                let n = life.binder_count;
                life.binder_count += 1;
                n
            });
            *label = Some(dense);
        }
    }
    life
}

type R<T> = CompileResult<T>;
fn error(span: Span, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-SOURCE",
        Some(span),
        message,
    )]
}

fn category_error(span: Span, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-CATEGORY",
        Some(span),
        message,
    )]
}
fn forbidden(span: Span, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-FORBIDDEN",
        Some(span),
        message,
    )]
}

#[derive(Clone, Copy)]
pub(crate) struct SourceModule<'a> {
    pub source: &'a SourceUnit,
    pub syntax: &'a s::Module,
    pub imports: &'a [SourceImport],
    /// Actual declaration spans resolved by the immutable project source index.
    pub symbols: &'a [(Span, CtSymbolId)],
}
#[derive(Clone, Debug)]
pub(crate) struct SourceImport {
    pub alias: String,
    pub target: ImportTarget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportTarget {
    Module(ModuleId),
    Meta,
    Syntax,
}
pub(crate) enum Entry<'a> {
    Function(CtSymbolId),
    // The standalone compiler test imports this file without the driver path
    // that constructs expression entries.
    #[allow(dead_code)]
    Expression {
        module: ModuleId,
        expression: &'a s::Expr,
        definition: CtSymbolId,
        /// Actual original lexical function. Generated declarations retain
        /// their naming owner above but have no authentic binder ledger yet.
        lexical_owner: Option<CtSymbolId>,
        expected: Option<CtTy>,
        parameters: &'a [(String, CtTy)],
    },
}
pub(crate) struct CompileRequest<'a> {
    pub target_pointer_width: u16,
    pub modules: &'a [SourceModule<'a>],
    pub source_fingerprint: Fingerprint,
    pub entry: Entry<'a>,
    pub input_tokens: &'a CtInputTable,
    pub snapshot: &'a CtSnapshotRequest,
    pub limits: &'a Limits,
}

struct Budget<'a> {
    limits: &'a Limits,
    work: usize,
    nodes: usize,
    bytes: usize,
    functions: usize,
    values: usize,
    locals: usize,
    blocks: usize,
    instructions: usize,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            work: limits.max_analysis_steps,
            nodes: limits.max_nodes,
            bytes: limits.max_source_bytes,
            functions: limits.max_functions,
            values: limits.max_values,
            locals: limits.max_locals,
            blocks: limits.max_blocks,
            instructions: limits.max_instructions,
        }
    }
    fn take(remaining: &mut usize, n: usize, span: Span, what: &str) -> R<()> {
        *remaining = remaining
            .checked_sub(n)
            .ok_or_else(|| vec![Diagnostic::limit(Phase::Ct, what, Some(span))])?;
        Ok(())
    }
    fn work(&mut self, n: usize, span: Span) -> R<()> {
        Self::take(
            &mut self.work,
            n,
            span,
            "CT source attribution work exhausted",
        )
    }
    fn node(&mut self, span: Span) -> R<()> {
        self.work(1, span)?;
        Self::take(
            &mut self.nodes,
            1,
            span,
            "CT source generated-node budget exhausted",
        )
    }
    fn text(&mut self, text: &str, span: Span) -> R<()> {
        self.work(text.len(), span)?;
        Self::take(
            &mut self.bytes,
            text.len(),
            span,
            "CT source temporary-byte budget exhausted",
        )
    }
    fn depth(&mut self, depth: usize, span: Span) -> R<()> {
        self.work(1, span)?;
        if depth > self.limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT source nesting budget exhausted",
                Some(span),
            )]);
        }
        Ok(())
    }
    fn ty(&mut self, ty: &CtTy, depth: usize, span: Span) -> R<()> {
        self.depth(depth, span)?;
        match ty {
            CtTy::Tuple(fields) | CtTy::AnonymousSum(fields) => {
                for field in fields {
                    self.ty(field, depth + 1, span)?;
                }
            }
            CtTy::Array { element, .. } | CtTy::List(element) => {
                self.ty(element, depth + 1, span)?
            }
            _ => {}
        }
        Ok(())
    }
    fn copy_ty(&mut self, ty: &CtTy, depth: usize, span: Span) -> R<()> {
        self.depth(depth, span)?;
        self.node(span)?;
        Self::take(
            &mut self.bytes,
            std::mem::size_of::<CtTy>(),
            span,
            "CT source temporary type bytes exhausted",
        )?;
        match ty {
            CtTy::Tuple(fields) | CtTy::AnonymousSum(fields) => {
                for field in fields {
                    self.copy_ty(field, depth + 1, span)?;
                }
            }
            CtTy::Array { element, .. } | CtTy::List(element) => {
                self.copy_ty(element, depth + 1, span)?
            }
            _ => {}
        }
        Ok(())
    }
    fn declared(&self) -> CtDeclaredBudget {
        CtDeclaredBudget {
            max_steps: self.limits.max_ct_steps,
            max_call_depth: self.limits.max_call_depth.min(u32::MAX as usize) as u32,
            max_live_cells: self.limits.max_values as u64,
            max_temporary_bytes: self.bytes as u64,
            max_generated_nodes: self.nodes as u64,
            max_instances: self.functions as u64,
            max_analysis_work: self.work as u64,
        }
    }
}
#[derive(Clone, Copy)]
struct Declaration<'a> {
    module: usize,
    item: &'a s::Item,
    function: &'a s::Function,
}
enum PendingTypes {
    Data(Vec<CtTy>),
    Metadata(Vec<CtTypeRef>),
    Mixed(Vec<CtScopeActual>, Option<CtAmbientScopeId>),
}
/// Private fresh-compiler facts. Neither raw artifacts nor caches can supply
/// this request-local event ledger to invocation authority.
#[derive(Clone, Debug, Default)]
pub(crate) struct SourceCompilation {
    pub bindings: Vec<SourceLifeBinding>,
    pub calls: Vec<SourceInstanceCall>,
    pub scopes: Vec<CtAmbientLifeScope>,
    pub relocations: Vec<SourceRecordRelocation>,
    pub type_sites: Vec<SourceTypeSite>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceRecordRelocation {
    pub caller: CtFunctionId,
    pub block: CtBlockId,
    pub input: CtValueId,
    pub source: CtRecordSchemaId,
    pub target: CtRecordSchemaId,
    pub output: CtValueId,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceTypeSite {
    pub caller: CtFunctionId,
    pub value: CtValueId,
    pub site: source_sites::SourceSite,
}
#[derive(Clone, Debug)]
pub(crate) struct SourceInstanceCall {
    pub caller: CtFunctionId,
    pub caller_definition: CtSymbolId,
    pub caller_scope: CtAmbientLifeScope,
    pub caller_scope_id: CtAmbientScopeId,
    pub callee: CtSymbolId,
    pub instance: CtFunctionId,
    pub actuals: Vec<CtScopeActual>,
    pub enclosing: Option<CtAmbientScopeId>,
    pub site: source_sites::SourceSite,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceLifeBinding {
    pub enumeration: bool,
    pub schema: u32,
    pub caller: Option<CtFunctionId>,
    pub definition: CtSymbolId,
    /// Class -> original generic parameter ordinal, including intervening kinds.
    pub parameters: Vec<u32>,
    /// Complete root class references when the classes cross lexical owners.
    pub sources: Option<Vec<CtAmbientLifeReference>>,
    pub site: Option<source_sites::SourceSite>,
}
struct SourceLifeScope {
    tree: source_sites::SourceTreeId,
    enclosing: Option<CtAmbientScopeId>,
    definition: CtSymbolId,
    span: Span,
    binders: Vec<(String, u32)>,
    instance: CtScopeInstance,
}
struct Compiler<'a> {
    active_host: Option<CtAmbientScopeId>,
    modules: &'a [SourceModule<'a>],
    declarations: Vec<Declaration<'a>>,
    member_hosts: Vec<members::MemberLocation<'a>>,
    selected: BTreeMap<usize, CtFunctionId>,
    generic_selected: Vec<(usize, Vec<CtTy>, CtFunctionId)>,
    generic_metadata_selected: Vec<(usize, Vec<CtTypeRef>, CtFunctionId)>,
    mixed_selected: Vec<(
        usize,
        Vec<CtScopeActual>,
        Option<CtAmbientScopeId>,
        CtFunctionId,
    )>,
    pending: VecDeque<(usize, PendingTypes, CtFunctionId)>,
    bindings: Vec<(String, CtTy)>,
    metadata_bindings: Vec<(String, CtTypeRef)>,
    scalar_bindings: Vec<(String, Constant)>,
    source_life_scope: Option<SourceLifeScope>,
    current_source_function: Option<CtFunctionId>,
    ambient_life_scopes: Vec<CtAmbientLifeScope>,
    source_life_bindings: Vec<SourceLifeBinding>,
    source_instance_calls: Vec<SourceInstanceCall>,
    source_type_sites: Vec<SourceTypeSite>,
    current_source_expr: Option<(usize, usize)>,
    current_source_type: Option<(usize, usize)>,
    current_nominal_site: Option<source_sites::SourceSite>,
    metadata_ambient: Vec<CtAmbientClassBinding>,
    generated_catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    entry_source_site: Option<source_sites::SourceSite>,
    entry_source_expression: Option<&'a s::Expr>,
    record_relocations: Vec<SourceRecordRelocation>,
    active_copy_metadata: Vec<CtTypeRef>,
    symbolic: Vec<CtTy>,
    abstract_scalars: Vec<(String, symbolic_nominal::ScalarArgument)>,
    abstract_lives: Vec<(String, CtSymbolId, u32)>,
    abstract_nominals: Vec<symbolic_nominal::Nominal>,
    active_abstract_nominals: Vec<CtRecordSchemaId>,
    checking_generics: bool,
    checked_generics: Vec<usize>,
    classification_binders: Vec<String>,
    functions: Vec<Option<CtFunction>>,
    constants: Vec<CtConstant>,
    records: Vec<CtRecordSchema>,
    enums: Vec<CtEnumSchema>,
    record_sources: Vec<(usize, &'a s::Aggregate)>,
    record_arguments: Vec<Vec<CtTy>>,
    enum_sources: Vec<(usize, &'a s::EnumDecl)>,
    templates: Vec<CtTemplate>,
    active_aliases: Vec<(usize, Span)>,
    budget: Budget<'a>,
}
impl<'a> Compiler<'a> {
    // The same source alias can be reached through type syntax and CT expressions.
    // Track the actual declaration, rather than a textual name or recursion depth.
    fn enter_alias(&mut self, module: usize, declaration: Span, use_span: Span) -> R<()> {
        self.budget.work(1, use_span)?;
        for &(active_module, active_span) in &self.active_aliases {
            self.budget.work(1, use_span)?;
            if active_module == module && active_span == declaration {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-PHASE-CYCLE",
                    Some(use_span),
                    "CT source type alias depends on its own unfinished attribution",
                )]);
            }
        }
        self.budget.work(1, use_span)?;
        self.active_aliases.push((module, declaration));
        Ok(())
    }
    fn unindexed(modules: &'a [SourceModule<'a>], limits: &'a Limits) -> Self {
        Self {
            modules,
            declarations: Vec::new(),
            member_hosts: Vec::new(),
            active_host: None,
            selected: BTreeMap::new(),
            generic_selected: Vec::new(),
            generic_metadata_selected: Vec::new(),
            mixed_selected: Vec::new(),
            pending: VecDeque::new(),
            bindings: Vec::new(),
            metadata_bindings: Vec::new(),
            scalar_bindings: Vec::new(),
            source_life_scope: None,
            current_source_function: None,
            ambient_life_scopes: Vec::new(),
            source_life_bindings: Vec::new(),
            source_instance_calls: Vec::new(),
            source_type_sites: Vec::new(),
            current_source_expr: None,
            current_source_type: None,
            current_nominal_site: None,
            metadata_ambient: Vec::new(),
            generated_catalog: None,
            entry_source_site: None,
            entry_source_expression: None,
            record_relocations: Vec::new(),
            active_copy_metadata: Vec::new(),
            symbolic: Vec::new(),
            abstract_scalars: Vec::new(),
            abstract_lives: Vec::new(),
            abstract_nominals: Vec::new(),
            active_abstract_nominals: Vec::new(),
            checking_generics: false,
            checked_generics: Vec::new(),
            classification_binders: Vec::new(),
            functions: Vec::new(),
            constants: Vec::new(),
            records: Vec::new(),
            enums: Vec::new(),
            record_sources: Vec::new(),
            record_arguments: Vec::new(),
            enum_sources: Vec::new(),
            templates: Vec::new(),
            active_aliases: Vec::new(),
            budget: Budget::new(limits),
        }
    }
    fn indexed(modules: &'a [SourceModule<'a>], limits: &'a Limits) -> R<Self> {
        let mut compiler = Self::unindexed(modules, limits);
        compiler.index()?;
        Ok(compiler)
    }
    fn index(&mut self) -> R<()> {
        let modules = self.modules;
        for (module, input) in modules.iter().enumerate() {
            self.budget.work(1, input.syntax.span)?;
            if input.source.module_id != input.syntax.id
                || !input.syntax.span.is_valid_for(input.source)
            {
                return Err(error(
                    input.syntax.span,
                    "CT syntax does not belong to its actual source unit",
                ));
            }
            for import in input.imports {
                self.budget.text(&import.alias, input.syntax.span)?;
                let mut found = 0;
                for item in &input.syntax.items {
                    self.budget.work(1, item.span)?;
                    match &item.kind {
                        s::ItemKind::Import(actual) if !actual.syntax => {
                            let alias = actual
                                .alias
                                .as_deref()
                                .or_else(|| actual.path.last().map(String::as_str));
                            self.budget
                                .work(import.alias.len().saturating_add(1), item.span)?;
                            if alias == Some(import.alias.as_str()) {
                                let valid = match import.target {
                                    ImportTarget::Meta => {
                                        actual.path.iter().map(String::as_str).eq(["std", "meta"])
                                    }
                                    ImportTarget::Syntax => {
                                        actual.path.iter().map(String::as_str).eq(["std", "syntax"])
                                    }
                                    ImportTarget::Module(_) => true,
                                };
                                if !valid {
                                    return Err(error(
                                        item.span,
                                        "CT standard module identity differs from its actual source import",
                                    ));
                                }
                                found += 1;
                            }
                        }
                        s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. }
                            if function.name == import.alias =>
                        {
                            return Err(error(
                                item.span,
                                "CT import alias conflicts with an actual ordinary function binding",
                            ));
                        }
                        s::ItemKind::Const(value) if value.name == import.alias => {
                            return Err(error(
                                item.span,
                                "CT import alias conflicts with an actual value binding",
                            ));
                        }
                        _ => {}
                    }
                }
                if found != 1 {
                    return Err(error(
                        input.syntax.span,
                        "CT import must have one actual ordinary source import binding",
                    ));
                }
                if let ImportTarget::Module(id) = import.target {
                    self.module_index(id, input.syntax.span)?;
                }
            }
            for item in &input.syntax.items {
                self.budget.work(1, item.span)?;
                if let s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } =
                    &item.kind
                {
                    self.budget.node(item.span)?;
                    self.declarations.push(Declaration {
                        module,
                        item,
                        function,
                    });
                }
                self.index_members(module, item)?;
            }
        }
        Ok(())
    }

    fn item_symbol(&mut self, module: usize, item: &s::Item) -> R<CtSymbolId> {
        if let Some(symbol) = self.generated_item_symbol(module, item)? {
            return Ok(symbol);
        }
        self.budget.work(
            self.modules[module]
                .syntax
                .items
                .len()
                .saturating_add(self.modules[module].symbols.len()),
            item.span,
        )?;
        let top = self.modules[module]
            .syntax
            .items
            .iter()
            .position(|actual| std::ptr::eq(actual, item));
        let declaration = if let Some(ordinal) = top {
            zil_base::NodeId(
                u32::try_from(ordinal)
                    .map_err(|_| error(item.span, "CT original declaration ordinal overflow"))?,
            )
        } else {
            source_scope::original_item(
                self.modules[module].syntax,
                item,
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, item.span),
            )?
            .ok_or_else(|| error(item.span, "CT declaration is not an actual original item"))?
            .declaration
        };
        let symbol = CtSymbolId {
            module: self.modules[module].source.module_id,
            declaration,
            generation_path: None,
        };
        if !self.modules[module]
            .symbols
            .iter()
            .any(|(_, actual)| actual == &symbol)
        {
            return Err(error(
                item.span,
                "CT declaration is absent from frozen inventory",
            ));
        }
        Ok(symbol)
    }
    fn module_index(&mut self, id: ModuleId, span: Span) -> R<usize> {
        for (index, module) in self.modules.iter().enumerate() {
            self.budget.work(1, span)?;
            if module.syntax.id == id {
                return Ok(index);
            }
        }
        Err(error(
            span,
            "CT source module is absent from the frozen project",
        ))
    }
    fn resolve_path(&mut self, current: usize, path: &[String], span: Span) -> R<(usize, String)> {
        if path.len() == 1 {
            self.budget.text(&path[0], span)?;
            return Ok((current, path[0].clone()));
        }
        if path.len() != 2 {
            return Err(error(
                span,
                "CT names require an actual local declaration or imported module member",
            ));
        }
        for import in self.modules[current].imports {
            self.budget.work(1, span)?;
            if import.alias == path[0] {
                if let ImportTarget::Module(id) = import.target {
                    let module = self.module_index(id, span)?;
                    self.budget.text(&path[1], span)?;
                    return Ok((module, path[1].clone()));
                }
                return Err(error(
                    span,
                    "CT standard module member is not an ordinary declaration",
                ));
            }
        }
        Err(error(span, "CT source import is unresolved"))
    }
    fn function(&mut self, current: usize, path: &[String], span: Span) -> R<usize> {
        if let Some(index) = self.member_function(current, path, span)? {
            return Ok(index);
        }
        let (module, name) = self.resolve_path(current, path, span)?;
        let mut found = None;
        self.budget.work(
            self.member_hosts
                .len()
                .saturating_mul(self.declarations.len()),
            span,
        )?;
        for (index, declaration) in self.declarations.iter().enumerate() {
            self.budget.work(1, span)?;
            if declaration.module == module
                && declaration.function.name == name
                && !self
                    .member_hosts
                    .iter()
                    .any(|member| member.declaration == index)
            {
                if found.is_some() {
                    return Err(error(span, "duplicate CT function declaration"));
                }
                if module != current && !declaration.item.public {
                    return Err(error(span, "CT function is private to its defining module"));
                }
                found = Some(index);
            }
        }
        found.ok_or_else(|| error(span, "unknown CT function"))
    }
    fn enqueue(&mut self, declaration: usize) -> R<CtFunctionId> {
        let span = self.declarations[declaration].function.span;
        self.budget.work(
            self.selected
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            span,
        )?;
        if let Some(id) = self.selected.get(&declaration) {
            return Ok(*id);
        }
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.functions,
            1,
            span,
            "CT source function-instance budget exhausted",
        )?;
        let id = CtFunctionId(self.functions.len() as u32);
        self.functions.push(None);
        self.budget.work(
            self.selected
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            span,
        )?;
        self.selected.insert(declaration, id);
        self.budget.node(span)?;
        self.pending
            .push_back((declaration, PendingTypes::Data(Vec::new()), id));
        Ok(id)
    }
    fn header(&mut self, declaration: usize) -> R<(Vec<CtTy>, CtTy)> {
        self.header_parameters(declaration, Vec::new())
    }
    fn header_parameters(
        &mut self,
        declaration: usize,
        mut parameters: Vec<CtTy>,
    ) -> R<(Vec<CtTy>, CtTy)> {
        let d = self.declarations[declaration];
        if d.function.generics.is_empty() && !d.function.where_clause.is_empty() {
            return Err(error(
                d.function.span,
                "CT function must be a concrete ordinary function instance",
            ));
        }
        if matches!(d.item.kind, s::ItemKind::Extern { .. }) {
            return Err(error(d.item.span, "foreign functions cannot execute in CT"));
        }
        for parameter in &d.function.params {
            self.budget.node(parameter.span)?;
            if parameter.give {
                return Err(error(
                    parameter.span,
                    "CT-only function has no resource or give parameter",
                ));
            }
            parameters.push(self.ty(d.module, &parameter.ty, 0)?);
        }
        let result = match &d.function.return_type {
            Some(ty) => self.ty(d.module, ty, 0)?,
            None => CtTy::Scalar(CtScalarTy::Unit),
        };
        Ok((parameters, result))
    }
}

pub(crate) fn compile(request: CompileRequest<'_>) -> R<CtArtifactProgram> {
    compile_scoped(request).map(|(program, _)| program)
}
pub(crate) fn compile_scoped(
    request: CompileRequest<'_>,
) -> R<(CtArtifactProgram, SourceCompilation)> {
    compile_scoped_at_source(request, None, None)
}
pub(crate) fn compile_scoped_with_catalog<'a>(
    request: CompileRequest<'a>,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
) -> R<(CtArtifactProgram, SourceCompilation)> {
    compile_scoped_at_source(request, catalog, None)
}
pub(crate) fn compile_scoped_at_source<'a>(
    request: CompileRequest<'a>,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    source_site: Option<source_sites::SourceSite>,
) -> R<(CtArtifactProgram, SourceCompilation)> {
    compile_scoped_context(request, catalog, source_site, None)
}
pub(crate) fn compile_scoped_runtime_at_source<'a>(
    request: CompileRequest<'a>,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    source: &runtime_context::RuntimeEntrySource,
) -> R<(CtArtifactProgram, SourceCompilation)> {
    compile_scoped_context(request, catalog, Some(source.expression_site), Some(source))
}
fn compile_scoped_context<'a>(
    request: CompileRequest<'a>,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
    source_site: Option<source_sites::SourceSite>,
    runtime_source: Option<&runtime_context::RuntimeEntrySource>,
) -> R<(CtArtifactProgram, SourceCompilation)> {
    let mut compiler = Compiler::indexed(request.modules, request.limits)?;
    compiler.generated_catalog = catalog;
    compiler.index_generated()?;
    compiler.entry_source_site = source_site;
    compiler.entry_source_expression = match &request.entry {
        Entry::Expression { expression, .. } => Some(*expression),
        _ => None,
    };
    compiler.check_generic_definitions()?;
    // An uncalled CT-only definition is still checked; runtime-only functions
    // are compiled only when actually referenced by the pure CT call graph.
    for index in 0..compiler.declarations.len() {
        let d = compiler.declarations[index];
        if d.function.generics.is_empty()
            && !compiler.has_generic_host(index)?
            && compiler.ct_only_function(d.module, d.function)?
        {
            compiler.enqueue(index)?;
        }
    }
    let entry = match request.entry {
        Entry::Function(symbol) => {
            let mut found = None;
            for index in 0..compiler.declarations.len() {
                let d = compiler.declarations[index];
                if compiler.item_symbol(d.module, d.item)? == symbol {
                    found = Some(index);
                    break;
                }
            }
            compiler.enqueue(found.ok_or_else(|| {
                error(
                    Span::new(zil_base::FileId(0), 0, 0),
                    "CT entry symbol is not an actual source function",
                )
            })?)?
        }
        Entry::Expression {
            module,
            expression,
            definition,
            lexical_owner,
            expected,
            parameters,
        } => {
            let module = compiler.module_index(module, expression.span)?;
            compiler.source_life_scope = match lexical_owner {
                Some(owner) => compiler.source_life_scope(module, &owner, expression.span)?,
                None => None,
            };
            if let Some(source) = runtime_source {
                if source.definition != definition {
                    return Err(error(
                        expression.span,
                        "runtime CT recipe owner differs from its original entry",
                    ));
                }
                compiler.bind_runtime_entry(module, expression, source)?;
            }
            compiler.budget.node(expression.span)?;
            Budget::take(
                &mut compiler.budget.functions,
                1,
                expression.span,
                "CT query instance budget exhausted",
            )?;
            let id = CtFunctionId(compiler.functions.len() as u32);
            compiler.functions.push(None);
            compiler.current_source_function = Some(id);
            let body = cfg::expression(
                &mut compiler,
                id,
                module,
                definition,
                expression,
                expected,
                parameters,
            )?;
            compiler.functions[id.0 as usize] = Some(body);
            id
        }
    };
    if runtime_source.is_some() {
        compiler.bindings.clear();
        compiler.metadata_bindings.clear();
        compiler.scalar_bindings.clear();
    }
    while let Some((declaration, arguments, id)) = compiler.pending.pop_front() {
        compiler.budget.work(
            compiler
                .selected
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            compiler.declarations[declaration].item.span,
        )?;
        // A pending body owns its own lexical scope. It cannot inherit the
        // expression caller's life binders, even when diagnostic spans coincide.
        compiler.source_life_scope = None;
        compiler.active_host = None;
        match arguments {
            PendingTypes::Data(arguments) => {
                compiler.bindings = compiler.instance_bindings(declaration, &arguments)?
            }
            PendingTypes::Metadata(arguments) => {
                compiler.metadata_bindings =
                    compiler.metadata_instance_bindings(declaration, &arguments)?
            }
            PendingTypes::Mixed(arguments, enclosing) => {
                compiler.active_host = enclosing;
                compiler.bind_mixed_instance(declaration, &arguments)?;
            }
        }
        let body = cfg::function(&mut compiler, id, declaration)?;
        compiler.bindings.clear();
        compiler.metadata_bindings.clear();
        compiler.scalar_bindings.clear();
        compiler.functions[id.0 as usize] = Some(body);
    }
    // Account for the actual bounded source-view copies before allocating them.
    for token in &request.input_tokens.tokens {
        compiler.budget.node(token.span)?;
        compiler.budget.text(&token.text, token.span)?;
    }
    compiler.budget.work(
        request.snapshot.requested_queries.len(),
        request
            .modules
            .first()
            .map_or(Span::new(zil_base::FileId(0), 0, 0), |m| m.syntax.span),
    )?;
    let scopes = if compiler.source_instance_calls.is_empty() {
        Vec::new()
    } else {
        compiler
            .budget
            .work(compiler.ambient_life_scopes.len(), Span::default())?;
        Budget::take(
            &mut compiler.budget.bytes,
            compiler
                .ambient_life_scopes
                .len()
                .saturating_mul(std::mem::size_of::<CtAmbientLifeScope>()),
            Span::default(),
            "CT private caller scope ledger bytes exhausted",
        )?;
        for scope in &compiler.ambient_life_scopes {
            if let CtScopeInstance::Instantiated(actuals) = &scope.instance {
                Compiler::actual_cost(&mut compiler.budget, actuals, Span::default())?;
            }
        }
        compiler.ambient_life_scopes.clone()
    };
    let retain_scopes = runtime_source.is_some() || compiler.has_ambient_use()?;
    let source_compilation = SourceCompilation {
        bindings: std::mem::take(&mut compiler.source_life_bindings),
        calls: std::mem::take(&mut compiler.source_instance_calls),
        scopes,
        relocations: std::mem::take(&mut compiler.record_relocations),
        type_sites: std::mem::take(&mut compiler.source_type_sites),
    };
    Ok((
        CtArtifactProgram {
            schema_version: CT_ARTIFACT_SCHEMA_VERSION,
            ambient_life_scopes: if retain_scopes {
                compiler.ambient_life_scopes
            } else {
                Vec::new()
            },
            target_pointer_width: request.target_pointer_width,
            source_fingerprint: request.source_fingerprint,
            functions: compiler
                .functions
                .into_iter()
                .map(|f| f.expect("queued CT source body completed"))
                .collect(),
            entry,
            constants: compiler.constants,
            record_schemas: compiler.records,
            enum_schemas: compiler.enums,
            templates: compiler.templates,
            input_tokens: request.input_tokens.clone(),
            snapshot: request.snapshot.clone(),
            budget: compiler.budget.declared(),
        },
        source_compilation,
    ))
}

/// The raw program retains the shared remaining ledger across classification.
pub(crate) fn ct_only_definitions(program: &mut CtArtifactProgram) -> R<Vec<CtSymbolId>> {
    domain::definitions(program)
}

/// Find an actual CT-only definition for definition-only checking. The caller
/// compiles/verifies it without running the VM, so no invented source entry is
/// needed. This inspection charges the same request remainder before returning.
pub(crate) fn definitions_entry(
    modules: &[SourceModule<'_>],
    limits: &mut Limits,
) -> R<(Option<CtSymbolId>, Vec<CtSymbolId>)> {
    definitions_entry_with_catalog(modules, limits, None)
}
pub(crate) fn definitions_entry_with_catalog<'a>(
    modules: &'a [SourceModule<'a>],
    limits: &mut Limits,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
) -> R<(Option<CtSymbolId>, Vec<CtSymbolId>)> {
    let initial_nodes = limits.max_nodes;
    let initial_bytes = limits.max_source_bytes;
    let mut compiler = Compiler::indexed(modules, limits)?;
    compiler.generated_catalog = catalog;
    compiler.index_generated()?;
    let indexed_nodes = compiler.budget.nodes;
    let indexed_bytes = compiler.budget.bytes;
    let definitions = compiler.check_generic_definitions();
    // Return spent work even when symbolic definition checking fails.
    if let Err(errors) = definitions {
        let remaining = compiler.budget.declared();
        drop(compiler);
        limits.max_analysis_steps = remaining.max_analysis_work as usize;
        limits.max_nodes = initial_nodes - (indexed_nodes - remaining.max_generated_nodes as usize);
        limits.max_source_bytes =
            initial_bytes - (indexed_bytes - remaining.max_temporary_bytes as usize);
        limits.max_functions = remaining.max_instances as usize;
        return Err(errors);
    }
    let definitions = definitions?;
    let mut entry = None;
    for index in 0..compiler.declarations.len() {
        let declaration = compiler.declarations[index];
        if declaration.function.generics.is_empty()
            && !compiler.has_generic_host(index)?
            && compiler.ct_only_function(declaration.module, declaration.function)?
        {
            entry = Some(compiler.item_symbol(declaration.module, declaration.item)?);
            break;
        }
    }
    let remaining = compiler.budget.declared();
    drop(compiler);
    limits.max_analysis_steps = remaining.max_analysis_work as usize;
    limits.max_nodes = initial_nodes - (indexed_nodes - remaining.max_generated_nodes as usize);
    limits.max_source_bytes =
        initial_bytes - (indexed_bytes - remaining.max_temporary_bytes as usize);
    limits.max_functions = remaining.max_instances as usize;
    Ok((entry, definitions))
}

/// Select the CT value domain before either producer runs. No errors are used
/// as a fallback signal. The caller debits the changed remainder even on Err.
pub(crate) fn uses_ct_domain(
    modules: &[SourceModule<'_>],
    module: ModuleId,
    expression: &s::Expr,
    parameters: &[(String, CtTy)],
    limits: &mut Limits,
) -> R<bool> {
    selection::uses(modules, module, expression, parameters, limits)
}
pub(crate) fn uses_ct_domain_with_catalog<'a>(
    modules: &'a [SourceModule<'a>],
    module: ModuleId,
    expression: &s::Expr,
    parameters: &[(String, CtTy)],
    limits: &mut Limits,
    catalog: Option<&'a zil_expand::source_context::GeneratedSourceCatalog>,
) -> R<bool> {
    selection::uses_with_catalog(modules, module, expression, parameters, limits, catalog)
}

#[path = "compiler_metadata.rs"]
pub(super) mod metadata;

#[path = "compiler_data.rs"]
mod data;

#[path = "compiler_type_value.rs"]
mod type_value;

#[path = "compiler_generic_metadata.rs"]
mod generic_metadata;

#[path = "compiler_scalar_arguments.rs"]
mod scalar_arguments;

#[path = "compiler_metadata_copy.rs"]
mod metadata_copy;

#[path = "compiler_scoped_metadata.rs"]
mod scoped_metadata;
