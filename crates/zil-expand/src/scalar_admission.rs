//! Facts from the real closed scalar classifier, retained with its immutable
//! parser owner. No raw AST, fingerprint or caller-provided boolean admits this
//! path. Only the most recent project can enable fused production.
use super::*;
use std::sync::Mutex;

pub(crate) fn item_shape(item: &s::Item) -> bool {
    let valid = match &item.kind {
        s::ItemKind::Import(import) => !import.syntax,
        s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
            function.generics.is_empty()
                && function.where_clause.is_empty()
                && function.contract.is_empty()
        }
        _ => false,
    };
    valid
        && item.attributes.iter().all(|a| {
            a.path.as_slice() == ["test"] && a.arguments.is_empty() && a.string_value.is_none()
        })
}

pub(crate) fn expression_shape(expression: &s::Expr) -> bool {
    match &expression.kind {
        s::ExprKind::Member {
            member: s::MemberName::Name(name),
            ..
        } if matches!(name.as_str(), "Poll" | "TaskCreateError" | "TaskPollState") => false,
        s::ExprKind::Literal(_)
        | s::ExprKind::Group(_)
        | s::ExprKind::Unary { .. }
        | s::ExprKind::Binary { .. }
        | s::ExprKind::Pipeline { .. }
        | s::ExprKind::Member { .. }
        | s::ExprKind::Call { .. }
        | s::ExprKind::If { .. } => true,
        s::ExprKind::Name(name) => !matches!(
            name.as_str(),
            "Option" | "Task" | "Poll" | "task_create" | "TaskCreateError" | "TaskPollState"
        ),
        _ => false,
    }
}

pub(crate) fn statement_shape(statement: &s::Stmt) -> bool {
    match &statement.kind {
        s::StmtKind::Binding { pattern, .. } => {
            matches!(
                pattern.kind,
                s::PatternKind::Name(_) | s::PatternKind::Wildcard
            )
        }
        s::StmtKind::Return(_)
        | s::StmtKind::Break
        | s::StmtKind::Continue
        | s::StmtKind::If {
            condition: s::Condition::Expression(_),
            ..
        }
        | s::StmtKind::While {
            condition: s::Condition::Expression(_),
            ..
        }
        | s::StmtKind::Loop(_)
        | s::StmtKind::Unsafe(_)
        | s::StmtKind::Assign { .. }
        | s::StmtKind::Expression(_) => true,
        _ => false,
    }
}

#[derive(Clone)]
struct Classified {
    parsed: ParsedSource,
    nodes: u64,
    depth_limit: usize,
}

#[derive(Clone, Default)]
struct State {
    modules: BTreeMap<ModuleId, Classified>,
    retained_bytes: usize,
    project: Option<(Vec<ParsedSource>, String)>,
}

#[derive(Default)]
pub(super) struct AdmissionCache(Mutex<State>);
impl Clone for AdmissionCache {
    fn clone(&self) -> Self {
        // Checkpoints cannot consume each other's request-local admission.
        Self(Mutex::new(self.lock().clone()))
    }
}
impl AdmissionCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    pub(super) fn classified(&self, parsed: &ParsedSource, limits: &Limits) -> Option<u64> {
        let state = self.lock();
        let fact = state.modules.get(&parsed.source().module_id)?;
        (fact.parsed.same_revision(parsed)
            && fact.nodes <= limits.max_nodes as u64
            && fact.depth_limit <= limits.max_call_depth
            && parsed.source().text.len() <= limits.max_source_bytes
            && parsed.parsed().fits_limits(limits))
        .then_some(fact.nodes)
    }
    pub(super) fn remember(&self, parsed: &ParsedSource, limits: &Limits, nodes: u64) {
        let mut state = self.lock();
        if let Some(old) = state.modules.remove(&parsed.source().module_id) {
            state.retained_bytes -= retained_bytes(&old.parsed);
        }
        // Failed/declined materializer requests may never call Expand. Bound
        // retained owners by the request's existing source allowance as well.
        let bytes = retained_bytes(parsed);
        if bytes > limits.max_source_bytes {
            return;
        }
        if state.retained_bytes > limits.max_source_bytes - bytes {
            state.modules.clear();
            state.retained_bytes = 0;
            state.project = None;
        }
        state.retained_bytes += bytes;
        state.modules.insert(
            parsed.source().module_id,
            Classified {
                parsed: parsed.clone(),
                nodes,
                depth_limit: limits.max_call_depth,
            },
        );
    }
    pub(super) fn take_project(&self, parsed: &[ParsedSource], context_key: &str) -> bool {
        let Some((owners, key)) = self.lock().project.take() else {
            return false;
        };
        // A no-rewrite materializer may conservatively mark lineage unavailable.
        // This admission binds actual original parser owners, not output rows;
        // it does not turn that marker into nominal lineage authority.
        // Reuse the context key the actual producer already constructed; do
        // not add another namespace formatting walk at consumption.
        let same_context = key == context_key
            || key.strip_prefix("closed:").is_some_and(|original| {
                context_key.strip_prefix("closed-unavailable:") == Some(original)
            });
        same_context
            && owners.len() == parsed.len()
            && owners.iter().zip(parsed).all(|(a, b)| a.same_revision(b))
    }
    pub(super) fn finish_request(&self) {
        let mut state = self.lock();
        state.modules.clear();
        state.retained_bytes = 0;
        state.project = None;
    }
}

fn retained_bytes(parsed: &ParsedSource) -> usize {
    parsed
        .source()
        .text
        .len()
        .saturating_add(parsed.source().path.len())
        .saturating_add(std::mem::size_of::<Classified>())
}

fn raw_capture(
    parsed: &[ParsedSource],
    context: source_context::SourceIdentityContext<'_>,
    limits: &Limits,
) -> CompileResult<(
    Option<Vec<source_context::ModuleDeclarationOrigins>>,
    Limits,
)> {
    let sources = parsed.iter().map(ParsedSource::source).collect::<Vec<_>>();
    let modules = parsed
        .iter()
        .map(|p| &p.parsed().module)
        .collect::<Vec<_>>();
    source_context::capture_original(&sources, &modules, context, limits)
}

fn original_context_key(context: source_context::SourceIdentityContext<'_>) -> Option<String> {
    use source_context::{DeclarationPlacement, SourceIdentityContext};
    match context {
        SourceIdentityContext::ClosedSnapshot {
            root,
            aliases,
            declarations: DeclarationPlacement::Original,
        } => Some(format!("closed:{root:?}:{aliases:?}")),
        SourceIdentityContext::Explicit {
            package,
            modules,
            declarations: DeclarationPlacement::Original,
        } => Some(format!("explicit:{package:?}:{modules:?}")),
        _ => None,
    }
}

impl ExpandSession {
    /// Capture on actual parser-owned scalar input after `parsed_scalar` has
    /// completed its real full classification. The classifier excludes every
    /// Discovery trigger: nominal/life/dyn types, generic factories, CT emit,
    /// meta expressions and task-result workers. Thus there is no second tree
    /// discovery here. Current module/source binding still executes and pays
    /// one node per module. Missing facts take the unchanged raw capture path.
    ///
    /// The exact owners and original context also enable the following
    /// `expand_parsed` request to fuse Distribute into Preflight + Normalize.
    /// Raw capture and expansion without this admission retain their old fees.
    /// The fused outer ledger pays current module/namespace binding, Preflight,
    /// Normalize (including output formation) and Restore. There is no removed
    /// Discovery/Distribute demand to replay. The separate plain ledger, Lower
    /// and SourceMap validation keep their actual checks and charges. Default
    /// limits are unchanged; this opt-in producer has new N/N-1 boundaries.
    pub fn capture_original_parsed(
        &self,
        parsed: &[ParsedSource],
        context: source_context::SourceIdentityContext<'_>,
        limits: &Limits,
    ) -> CompileResult<(
        Option<Vec<source_context::ModuleDeclarationOrigins>>,
        Limits,
    )> {
        self.admission.lock().project = None;
        let eligible = matches!(
            context,
            source_context::SourceIdentityContext::ClosedSnapshot {
                declarations: source_context::DeclarationPlacement::Original,
                ..
            } | source_context::SourceIdentityContext::Explicit {
                declarations: source_context::DeclarationPlacement::Original,
                ..
            }
        ) && !parsed.is_empty()
            && parsed
                .iter()
                .all(|input| self.admission.classified(input, limits).is_some());
        if !eligible {
            return raw_capture(parsed, context, limits);
        }
        let mut budget = generics::Budget::new(limits);
        let mut files = BTreeSet::new();
        let mut modules = BTreeSet::new();
        for input in parsed {
            let source = input.source();
            let module = &input.parsed().module;
            budget.node(module.span, 0).map_err(|error| vec![error])?;
            if source.module_id != module.id
                || !module.span.is_valid_for(source)
                || !files.insert(source.file_id)
                || !modules.insert(module.id)
            {
                // Enter the complete real discovery path. Binding work already
                // executed above stays paid; a declined optimization must not
                // reset this request's allowance or refund preliminary work.
                return raw_capture(parsed, context, &budget.remaining_limits());
            }
        }
        // Bind the actual original namespace even though no lineage rows are
        // needed. Prepay its real key copying/formatting before allocation;
        // this is metadata work, not a fictional discovery-tree receipt.
        let span = parsed[0].parsed().module.span;
        let bind = (|| {
            match context {
                source_context::SourceIdentityContext::ClosedSnapshot { aliases, .. } => {
                    for path in aliases.keys() {
                        budget.node(span, 0)?;
                        for name in path {
                            budget.text(name, span)?;
                        }
                    }
                }
                source_context::SourceIdentityContext::Explicit {
                    package, modules, ..
                } => {
                    budget.text(package, span)?;
                    for path in modules.values() {
                        budget.node(span, 0)?;
                        for name in path {
                            budget.text(name, span)?;
                        }
                    }
                }
            }
            Ok(())
        })();
        bind.map_err(|error| vec![error])?;
        let key = original_context_key(context).expect("original scalar context");
        let mut state = self.admission.lock();
        state.modules.retain(|id, _| modules.contains(id));
        state.retained_bytes = state
            .modules
            .values()
            .map(|fact| retained_bytes(&fact.parsed))
            .sum();
        state.project = Some((parsed.to_vec(), key));
        Ok((None, budget.remaining_limits()))
    }
}
