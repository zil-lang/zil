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
#[path = "compiler_quote.rs"]
mod quote;
#[path = "compiler_selection.rs"]
mod selection;
#[path = "compiler_types.rs"]
mod types;
#[path = "compiler_generics.rs"]
mod generics;
use std::collections::{BTreeMap, VecDeque};
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, SourceUnit, Span};
use zil_ir::ct_artifact::*;
use zil_ir::{BinaryOp, Constant, UnaryOp};
use zil_syntax::ast as s;

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
        expected: Option<CtTy>,
        parameters: &'a [(String, CtTy)],
    },
}
pub(crate) struct CompileRequest<'a> {
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
            CtTy::Tuple(fields) => {
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
struct Compiler<'a> {
    modules: &'a [SourceModule<'a>],
    declarations: Vec<Declaration<'a>>,
    selected: BTreeMap<usize, CtFunctionId>,
    generic_selected: Vec<(usize, Vec<CtTy>, CtFunctionId)>,
    pending: VecDeque<(usize, Vec<CtTy>, CtFunctionId)>,
    bindings: Vec<(String, CtTy)>,
    symbolic: Vec<CtTy>,
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
            selected: BTreeMap::new(),
            generic_selected: Vec::new(),
            pending: VecDeque::new(),
            bindings: Vec::new(),
            symbolic: Vec::new(),
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
        for (module, input) in self.modules.iter().enumerate() {
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
            }
        }
        Ok(())
    }

    fn symbol(&mut self, module: usize, span: Span) -> R<CtSymbolId> {
        for (actual, symbol) in self.modules[module].symbols {
            self.budget.work(1, span)?;
            if *actual == span {
                if symbol.module != self.modules[module].syntax.id {
                    return Err(error(
                        span,
                        "declaration identity belongs to a different source module",
                    ));
                }
                return Ok(symbol.clone());
            }
        }
        Err(error(
            span,
            "source declaration has no frozen symbol identity",
        ))
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
        let (module, name) = self.resolve_path(current, path, span)?;
        let mut found = None;
        for (index, declaration) in self.declarations.iter().enumerate() {
            self.budget.work(1, span)?;
            if declaration.module == module && declaration.function.name == name {
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
        self.pending.push_back((declaration, Vec::new(), id));
        Ok(id)
    }
    fn header(&mut self, declaration: usize) -> R<(Vec<CtTy>, CtTy)> {
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
        let mut parameters = Vec::new();
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
    let mut compiler = Compiler::indexed(request.modules, request.limits)?;
    compiler.check_generic_definitions()?;
    // An uncalled CT-only definition is still checked; runtime-only functions
    // are compiled only when actually referenced by the pure CT call graph.
    for index in 0..compiler.declarations.len() {
        let d = compiler.declarations[index];
        if d.function.generics.is_empty() && compiler.ct_only_function(d.module, d.function)? {
            compiler.enqueue(index)?;
        }
    }
    let entry = match request.entry {
        Entry::Function(symbol) => {
            let mut found = None;
            for index in 0..compiler.declarations.len() {
                let d = compiler.declarations[index];
                if compiler.symbol(d.module, d.item.span)? == symbol {
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
            expected,
            parameters,
        } => {
            let module = compiler.module_index(module, expression.span)?;
            compiler.budget.node(expression.span)?;
            Budget::take(
                &mut compiler.budget.functions,
                1,
                expression.span,
                "CT query instance budget exhausted",
            )?;
            let id = CtFunctionId(compiler.functions.len() as u32);
            compiler.functions.push(None);
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
    while let Some((declaration, arguments, id)) = compiler.pending.pop_front() {
        compiler.budget.work(
            compiler
                .selected
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            compiler.declarations[declaration].item.span,
        )?;
        compiler.bindings = compiler.instance_bindings(declaration, &arguments)?;
        let body = cfg::function(&mut compiler, id, declaration)?;
        compiler.bindings.clear();
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
    Ok(CtArtifactProgram {
        schema_version: CT_ARTIFACT_SCHEMA_VERSION,
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
    })
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
    let mut compiler = Compiler::indexed(modules, limits)?;
    let definitions = compiler.check_generic_definitions();
    // Return spent work even when symbolic definition checking fails.
    if let Err(errors) = definitions {
        let remaining = compiler.budget.declared();
        drop(compiler);
        limits.max_analysis_steps = remaining.max_analysis_work as usize;
        limits.max_nodes = remaining.max_generated_nodes as usize;
        limits.max_source_bytes = remaining.max_temporary_bytes as usize;
        limits.max_functions = remaining.max_instances as usize;
        return Err(errors);
    }
    let definitions = definitions?;
    let mut entry = None;
    for index in 0..compiler.declarations.len() {
        let declaration = compiler.declarations[index];
        if declaration.function.generics.is_empty()
            && compiler.ct_only_function(declaration.module, declaration.function)? {
            entry = Some(compiler.symbol(declaration.module, declaration.item.span)?);
            break;
        }
    }
    let remaining = compiler.budget.declared();
    drop(compiler);
    limits.max_analysis_steps = remaining.max_analysis_work as usize;
    limits.max_nodes = remaining.max_generated_nodes as usize;
    limits.max_source_bytes = remaining.max_temporary_bytes as usize;
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
