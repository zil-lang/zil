//! Rebind complete successful scalar products to current parser placement.
//! Declaration order and bytes (including attached attributes) are exact. No
//! insertion, deletion, internal edit, generated input or public digest admits
//! this path. Raw IDs and generated-name cursors consequently do not move.
use super::*;
use crate::generated_spans::Visitor;

pub(super) struct Placement {
    items: Vec<(Span, Span)>,
}
impl Placement {
    pub(super) fn between(old: &ParsedSource, current: &ParsedSource) -> Option<Self> {
        if !old.same_attribute_context(current)
            || old.source().module_id != current.source().module_id
            || old.source().file_id != current.source().file_id
            || old.source().path != current.source().path
        {
            return None;
        }
        let before = &old.parsed().module.items;
        let after = &current.parsed().module.items;
        if before.len() != after.len() {
            return None;
        }
        let region = |item: &s::Item| Span {
            start: item
                .attributes
                .iter()
                .fold(item.span.start, |start, a| start.min(a.span.start)),
            ..item.span
        };
        let mut items = Vec::with_capacity(before.len());
        for (old_item, item) in before.iter().zip(after) {
            let prior = region(old_item);
            let next = region(item);
            if prior.file != old.source().file_id
                || next.file != current.source().file_id
                || old.source().text.get(prior.start..prior.end)?
                    != current.source().text.get(next.start..next.end)?
                || old_item.span.start.checked_sub(prior.start)?
                    != item.span.start.checked_sub(next.start)?
                || items
                    .last()
                    .is_some_and(|(a, b): &(Span, Span)| a.end > prior.start || b.end > next.start)
            {
                return None;
            }
            items.push((prior, next));
        }
        // Use the actual lossless lexer tape, not a second approximation of
        // comments or a claim that matching AST items cover the complete file.
        // Any nontrivia token outside the retained declarations fails closed.
        for (parsed, side) in [(old, false), (current, true)] {
            let mut start = 0;
            for (before, after) in &items {
                let region = if side { *after } else { *before };
                if !trivia_gap(parsed, start, region.start) {
                    return None;
                }
                start = region.end;
            }
            if !trivia_gap(parsed, start, parsed.source().text.len()) {
                return None;
            }
        }
        Some(Self { items })
    }
    fn span(&self, span: Span) -> Option<Span> {
        let index = self
            .items
            .partition_point(|(old, _)| old.start <= span.start)
            .checked_sub(1)?;
        let (old, current) = self.items[index];
        if span.file != old.file || span.end < span.start || span.end > old.end {
            return None;
        }
        // An empty span on two adjacent declaration boundaries has no unique
        // owner if a new gap separates them. Let real production resolve it.
        if span.start == span.end && span.start == old.start && index > 0 {
            let (prior, prior_current) = self.items[index - 1];
            if prior.end == span.start && prior_current.end != current.start {
                return None;
            }
        }
        Some(Span {
            file: current.file,
            start: current
                .start
                .checked_add(span.start.checked_sub(old.start)?)?,
            end: current
                .start
                .checked_add(span.end.checked_sub(old.start)?)?,
        })
    }
    pub(super) fn imports(&self, imports: &[CoreImport]) -> Option<Vec<CoreImport>> {
        imports
            .iter()
            .map(|edge| {
                Some(CoreImport {
                    alias: edge.alias.clone(),
                    module: edge.module,
                    span: self.span(edge.span)?,
                })
            })
            .collect()
    }
    fn item(&self, item: &Arc<s::Item>) -> Option<Arc<s::Item>> {
        if self.span(item.span)? == item.span {
            return Some(item.clone());
        }
        let mut result = item.as_ref().clone();
        SyntaxPlacement(self).item(&mut result, 0).ok()?;
        Some(Arc::new(result))
    }
    /// All publication is tentative. A missing position, recipe or product
    /// abandons this candidate without altering the previous successful epoch.
    pub(super) fn rebind(
        &self,
        old: &Entry,
        current: &Entry,
        environment: &Arc<Environment>,
    ) -> Option<(Entry, u64, u64)> {
        let mut next = current.clone();
        let old_module = &old.parsed.parsed().module;
        let module = &current.parsed.parsed().module;
        let mut receipts = BTreeMap::new();
        // Module spans include edited gaps. Their successful demands are sums
        // of the same ordered declarations/imports; diagnostic positions are
        // supplied by current syntax when a replay cannot pay the demand.
        for (&(stage, start), &receipt) in old.receipts.iter() {
            let start = if matches!(stage, Stage::Preflight | Stage::Restore) {
                if start != old_module.span.start {
                    return None;
                }
                module.span.start
            } else {
                self.span(Span {
                    file: old_module.span.file,
                    start,
                    end: start,
                })?
                .start
            };
            if receipts.insert((stage, start), receipt).is_some() {
                return None;
            }
        }
        let mut normalized = BTreeMap::new();
        let mut headers = BTreeMap::new();
        let mut restored = BTreeMap::new();
        let mut syntax_copies = 0;
        for (ordinal, (before, item)) in old_module.items.iter().zip(&module.items).enumerate() {
            if matches!(item.kind, s::ItemKind::Import(_)) {
                continue;
            }
            // Reconstructed Normalize always starts from this request's real
            // parser input, allowance and Index, never from a restored body.
            let NormalizedCache::Deferred(recipe) = old.normalized.get(&before.span.start)? else {
                return None;
            };
            if recipe.ordinal != ordinal || recipe.span != before.span {
                return None;
            }
            normalized.insert(
                item.span.start,
                NormalizedCache::Deferred(Arc::new(NormalizedRecipe {
                    parsed: current.parsed.clone(),
                    ordinal,
                    span: item.span,
                    module: module.id,
                    environment: environment.clone(),
                    declaration: recipe.declaration.clone(),
                })),
            );
            if let Some(outcome) = old.scalar_headers.get(&before.span.start) {
                headers.insert(item.span.start, *outcome);
            }
            let prior = old.restored_items.get(&before.span.start)?;
            let rebound = self.item(&prior.item)?;
            syntax_copies += u64::from(!Arc::ptr_eq(&rebound, &prior.item));
            restored.insert(
                item.span.start,
                RestoredItem {
                    item: rebound,
                    before: prior.before.clone(),
                    after: prior.after.clone(),
                    added: self.imports(&prior.added)?.into(),
                },
            );
        }
        let old_shared = old.shared_restored.as_ref()?;
        let shared_items = module
            .items
            .iter()
            .filter(|item| !matches!(item.kind, s::ItemKind::Import(_)))
            .map(|item| restored.get(&item.span.start).map(|item| item.item.clone()))
            .collect::<Option<Vec<_>>>()?;
        let shared = Arc::new(modules::SharedScalarModule {
            id: module.id,
            span: module.span,
            items: shared_items.into(),
            imports: self.imports(&old_shared.imports)?,
        });
        let (lower_limits, lower) = old.lower.as_ref()?;
        let lower = self.lower(lower, current.parsed.source())?;
        let map_copies = lower.source_map.entries.len() as u64;
        next.receipts = Arc::new(receipts);
        next.normalized = Arc::new(normalized);
        next.scalar_headers = Arc::new(headers);
        next.restored_items = Arc::new(restored);
        next.restore_base = old.restore_base.clone();
        next.shared_restored = Some(shared);
        next.lower = Some((lower_limits.clone(), Arc::new(lower)));
        next.relocated_lower = true;
        // revision remains fresh: absolute output and SourceMap did change.
        Some((next, syntax_copies, map_copies))
    }
    fn lower(&self, old: &ExpandedModule, source: &SourceUnit) -> Option<ExpandedModule> {
        // Fail closed on products outside the scalar lowering domain. Do not
        // silently leave future catalogs or span-bearing metadata unbound.
        let CoreModule {
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            closure_life_contexts,
            schema_version: _,
            module_id,
            source_fingerprint: _,
            functions,
            aggregates,
            aggregate_templates,
            aggregate_applications,
            enums,
            unsupported,
        } = old.core.as_ref();
        if *module_id != source.module_id
            || !dyn_declarations.is_empty()
            || !dyn_interfaces.is_empty()
            || !dyn_vtables.is_empty()
            || !life_declarations.is_empty()
            || !life_instances.is_empty()
            || !closure_life_contexts.is_empty()
            || !aggregates.is_empty()
            || !aggregate_templates.is_empty()
            || !aggregate_applications.is_empty()
            || !enums.is_empty()
            || !unsupported.is_empty()
            || !old.generated_imports.is_empty()
            || !old.generated_exports.is_empty()
            || !old.imported_public_traits.is_empty()
            || !old.trait_method_names.is_empty()
            || !old.dyn_method_emissions.is_empty()
            || !old.closure_context_emissions.is_empty()
            || !old.callable_context_emissions.is_empty()
            || old.project_trait_work != 0
            || old.project_trait_nodes != 0
            || old.member_owner_work != 0
            || old.member_owner_nodes != 0
        {
            return None;
        }
        let mut core = old.core.as_ref().clone();
        for (index, function) in functions.iter().enumerate() {
            if self.span(function.span)? == function.span {
                continue;
            }
            let mut function = function.clone();
            self.function(&mut function)?;
            core.functions.set_owner(index, Arc::new(function));
        }
        core.source_fingerprint = source.fingerprint();
        let mut map = old.source_map.as_ref().clone();
        for entry in &mut map.entries {
            entry.origin = self.span(entry.origin)?;
            for span in &mut entry.related {
                *span = self.span(*span)?;
            }
            if let SourceTarget::Span(span) = &mut entry.target {
                *span = self.span(*span)?;
            }
        }
        let mut receipts = old.item_lower_receipts.as_ref().clone();
        for receipt in receipts.values_mut() {
            receipt.rebind_span(|span| self.span(span))?;
        }
        let mut result = old.clone();
        result.core = Arc::new(core);
        result.source_map = Arc::new(map);
        result.item_lower_receipts = Arc::new(receipts);
        result.reused_lower_items = 0;
        result.executed_lower_items = 0;
        result.copied_lower_map_entries = 0;
        Some(result)
    }
    fn function(&self, function: &mut CoreFunction) -> Option<()> {
        let CoreFunction {
            storage_life,
            call_kind: _,
            life,
            type_parameters,
            state_parameters,
            protocol_contract,
            callable_return,
            id: _,
            name: _,
            callable_parameters,
            foreign: _,
            parameters,
            return_type: _,
            result_mode: _,
            body,
            receiver,
            span,
        } = function;
        if storage_life.is_some()
            || life.is_some()
            || !type_parameters.is_empty()
            || !state_parameters.is_empty()
            || protocol_contract.is_some()
            || callable_return.is_some()
            || !callable_parameters.is_empty()
            || receiver.is_some()
        {
            return None;
        }
        *span = self.span(*span)?;
        for parameter in parameters {
            parameter.span = self.span(parameter.span)?;
        }
        self.block(body)
    }
    fn block(&self, block: &mut CoreBlock) -> Option<()> {
        block.span = self.span(block.span)?;
        for statement in &mut block.statements {
            statement.span = self.span(statement.span)?;
            match &mut statement.kind {
                CoreStatementKind::Binding { initializer, .. }
                | CoreStatementKind::Return(initializer) => {
                    if let Some(value) = initializer {
                        self.expr(value)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target)?;
                    self.expr(value)?;
                }
                CoreStatementKind::Expression(value) => self.expr(value)?,
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition)?;
                    self.block(then_body)?;
                    if let Some(body) = else_body {
                        self.block(body)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition)?;
                    self.block(body)?;
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body)?
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {}
                _ => return None,
            }
        }
        Some(())
    }
    fn expr(&self, expression: &mut CoreExpr) -> Option<()> {
        expression.span = self.span(expression.span)?;
        match &mut expression.kind {
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_) => {}
            CoreExprKind::Unary { operand, .. } => self.expr(operand)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.expr(left)?;
                self.expr(right)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(callee)?;
                for argument in arguments {
                    self.expr(argument)?;
                }
            }
            CoreExprKind::Member {
                object,
                owner: None,
                ..
            } => self.expr(object)?,
            CoreExprKind::Sequence { bindings, value } => {
                for (_, expression) in bindings {
                    self.expr(expression)?;
                }
                self.expr(value)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition)?;
                self.expr(then_value)?;
                self.expr(else_value)?;
            }
            _ => return None,
        }
        Some(())
    }
}
fn trivia_gap(parsed: &ParsedSource, start: usize, end: usize) -> bool {
    use zil_syntax::token::{Symbol, TokenKind};
    let tokens = &parsed.parsed().tokens;
    let first = tokens.partition_point(|token| token.span.end <= start);
    tokens[first..]
        .iter()
        .take_while(|token| token.span.start < end)
        .all(|token| {
            token.span.start >= start
                && token.span.end <= end
                && (token.is_trivia()
                    || matches!(
                        token.kind,
                        TokenKind::Newline | TokenKind::Symbol(Symbol::Semicolon)
                    ))
        })
}
struct SyntaxPlacement<'a>(&'a Placement);
impl Visitor for SyntaxPlacement<'_> {
    fn node(&mut self, span: &mut Span, _: usize) -> CompileResult<()> {
        *span = self.0.span(*span).ok_or_else(Vec::new)?;
        Ok(())
    }
    fn text(&mut self, _: &str, _: Span) -> CompileResult<()> {
        Ok(())
    }
    fn path_component(&mut self, _: Span, _: usize) -> CompileResult<()> {
        Ok(())
    }
}
