//! Successful scalar materialization receipts, bound to actual parser owners.
//! No stored result admits CT execution or replaces independent verification.
use super::*;
use zil_ct::CtExecutionBudget;
use zil_syntax::ParsedSource;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScalarMaterializationStats {
    pub executed_modules: usize,
    pub reused_modules: usize,
    pub cloned_modules: usize,
    pub receipt_budget_misses: usize,
}
impl ScalarMaterializationStats {
    pub fn add(&mut self, other: Self) {
        self.executed_modules = self.executed_modules.saturating_add(other.executed_modules);
        self.reused_modules = self.reused_modules.saturating_add(other.reused_modules);
        self.cloned_modules = self.cloned_modules.saturating_add(other.cloned_modules);
        self.receipt_budget_misses = self
            .receipt_budget_misses
            .saturating_add(other.receipt_budget_misses);
    }
    pub fn to_json(&self) -> String {
        format!(
            "{{\"executed_modules\":{},\"reused_modules\":{},\"cloned_modules\":{},\"receipt_budget_misses\":{}}}",
            self.executed_modules,
            self.reused_modules,
            self.cloned_modules,
            self.receipt_budget_misses
        )
    }
}

#[derive(Clone)]
struct Entry {
    parsed: ParsedSource,
    max_depth: usize,
    path_capacity: usize,
    resulting_path_capacity: usize,
    receipt: Receipt,
}

#[derive(Clone, Copy)]
struct Receipt {
    work: u64,
    starting_live_bytes: u64,
    peak_live_bytes: u64,
    ending_live_bytes: u64,
}
impl Receipt {
    fn fits(&self, budget: &CtExecutionBudget) -> bool {
        budget.live_temporary_bytes() == self.starting_live_bytes
            && budget.remaining().max_analysis_work >= self.work
            && budget.remaining().max_temporary_bytes >= self.peak_live_bytes
            && self.peak_live_bytes >= self.starting_live_bytes
            && self.peak_live_bytes >= self.ending_live_bytes
    }

    /// Guards establish that every original operation succeeds. If they do not
    /// hold, the caller executes the original walk with its unchanged ledger.
    fn replay(&self, budget: &mut CtExecutionBudget) -> CompileResult<()> {
        budget.reserve_analysis_work(self.work)?;
        budget.reserve_temporary_bytes(self.peak_live_bytes - self.starting_live_bytes)?;
        budget.release_temporary_bytes(self.peak_live_bytes - self.ending_live_bytes);
        Ok(())
    }
}

/// This result can only be constructed after current admission of the exact
/// original parsed modules. Its private fields prevent forging a no-rewrite
/// result from cached public AST/Core data.
pub(crate) struct ScalarMaterialization {
    usage: CtUsageReceipt,
    remaining: Limits,
}
impl ScalarMaterialization {
    pub(crate) fn usage(&self) -> &CtUsageReceipt {
        &self.usage
    }
    pub(crate) fn remaining(&self) -> &Limits {
        &self.remaining
    }
}

/// Retain only the most recent successful project epoch. Failed and ineligible
/// requests do not publish partial replacement entries. Cloning is a bounded
/// checkpoint of shared immutable receipts and parsed owners.
#[derive(Clone, Default)]
pub(crate) struct ScalarMaterializationSession {
    entries: BTreeMap<ModuleId, Arc<Entry>>,
    stats: ScalarMaterializationStats,
}
impl ScalarMaterializationSession {
    pub(crate) fn checkpoint(&self) -> Self {
        self.clone()
    }
    pub(crate) fn last_stats(&self) -> ScalarMaterializationStats {
        self.stats
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn materialize(
        &mut self,
        producer: &zil_expand::ExpandSession,
        inputs: &[ParsedSource],
        sources: &[SourceUnit],
        parsed: &[Arc<ParseResult>],
        paths: &BTreeMap<Vec<String>, ModuleId>,
        scoped: Option<&crate::ScopedImports>,
        imports: &[Vec<CoreImport>],
        root: ModuleId,
        fingerprint: Fingerprint,
        target: &TargetProfile,
        limits: &Limits,
    ) -> CompileResult<Option<ScalarMaterialization>> {
        self.stats = ScalarMaterializationStats::default();
        if inputs.is_empty()
            || inputs.len() != sources.len()
            || sources.len() != parsed.len()
            || parsed.len() != imports.len()
            || !inputs
                .iter()
                .zip(sources)
                .zip(parsed)
                .all(|((input, source), parsed)| {
                    input.source() == source
                        && Arc::ptr_eq(&input.parsed_shared(), parsed)
                        && producer.parsed_scalar(input, limits)
                })
            || !retains_runtime_graph(parsed, imports, root)
        {
            return Ok(None);
        }
        let modules = parsed
            .iter()
            .map(|parsed| &parsed.module)
            .collect::<Vec<_>>();
        // Keep the original capture, frozen shape/index construction and CT
        // definition admission current. They have independent budget/error
        // boundaries and are not proved by a no-rewrite classifier.
        let (origins, capture_remaining) = zil_expand::source_context::capture_original(
            sources,
            &modules,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root,
                aliases: paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            limits,
        )?;
        if origins.is_some() {
            return Ok(None);
        }
        let mut environment = Environment::new_scoped(
            sources,
            parsed,
            paths,
            scoped,
            imports,
            root,
            fingerprint,
            target,
            &capture_remaining,
        )?;
        environment.bind_source_origins(None)?;
        environment.classify_definitions()?;
        if !environment.ct_only.is_empty() || environment.has_static_host_uses() {
            return Ok(None);
        }
        let mut materializer = Materializer {
            runtime_binding: None,
            runtime_deferred: false,
            environment: &mut environment,
            sources,
            parsed,
            registry: None,
            paths,
            previews: Vec::new(),
            required_imports: Vec::new(),
            runtime_scopes: Vec::new(),
            data_source_scopes: Vec::new(),
            max_depth: limits.max_call_depth,
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
            bound_source_sites: Default::default(),
            generated_context: false,
            closure_site_revision: 0,
            changed_closure_sites: BTreeSet::new(),
            self_constant_sources: Vec::new(),
            self_constant_impl_sources: Vec::new(),
            self_constant_impl_outputs: Vec::new(),
        };
        let mut next = BTreeMap::new();
        for input in inputs {
            let module = &input.parsed().module;
            let path_capacity = materializer.item_path.capacity();
            let prior = self.entries.get(&module.id).filter(|entry| {
                entry.parsed.same_revision(input)
                    && entry.max_depth == limits.max_call_depth
                    && entry.path_capacity == path_capacity
            });
            if let Some(entry) = prior {
                if entry.receipt.fits(&materializer.environment.budget) {
                    entry.receipt.replay(&mut materializer.environment.budget)?;
                    materializer
                        .item_path
                        .reserve_exact(entry.resulting_path_capacity);
                    self.stats.reused_modules += 1;
                    next.insert(module.id, entry.clone());
                    continue;
                }
                self.stats.receipt_budget_misses += 1;
            }
            self.stats.executed_modules += 1;
            self.stats.cloned_modules += 1;
            let receipt = execute_module(&mut materializer, module)?;
            let Some(receipt) = receipt else {
                return Ok(None);
            };
            next.insert(
                module.id,
                Arc::new(Entry {
                    parsed: input.clone(),
                    max_depth: limits.max_call_depth,
                    path_capacity,
                    resulting_path_capacity: materializer.item_path.capacity(),
                    receipt,
                }),
            );
        }
        // Scalar admission excludes every source of these effects. Check the
        // actual current execution too before publishing an unchanged result.
        if materializer.runtime_deferred
            || !materializer.previews.is_empty()
            || !materializer.required_imports.is_empty()
            || !materializer.self_constant_sources.is_empty()
            || !materializer.declaration_lineage_preserved
            || materializer.closure_site_revision != 0
        {
            return Ok(None);
        }
        let remaining = materializer.environment.remaining_limits_for_input();
        let mut usage = materializer.environment.budget.usage().clone();
        usage.analysis_work = usage
            .analysis_work
            .checked_add((limits.max_analysis_steps - capture_remaining.max_analysis_steps) as u64)
            .ok_or_else(|| category("declaration capture work count overflow", Span::default()))?;
        usage.generated_nodes = usage
            .generated_nodes
            .checked_add((limits.max_nodes - capture_remaining.max_nodes) as u64)
            .ok_or_else(|| category("declaration capture node count overflow", Span::default()))?;
        self.entries = next;
        Ok(Some(ScalarMaterialization { usage, remaining }))
    }
}

/// A successful module runs with the exact old allowance/live state, but a
/// fresh usage counter. This records its own peak through the public ledger
/// API even when an earlier module had a larger global high-water mark.
fn execute_module(
    materializer: &mut Materializer<'_, '_>,
    original: &s::Module,
) -> CompileResult<Option<Receipt>> {
    let starting_live_bytes = materializer.environment.budget.live_temporary_bytes();
    let mut segment = CtExecutionBudget::new(materializer.environment.budget.remaining());
    segment.reserve_temporary_bytes(starting_live_bytes)?;
    let prior = std::mem::replace(&mut materializer.environment.budget, segment);
    let mut output = original.clone();
    let result: CompileResult<()> = (|| {
        materializer.bind_source_items(
            original,
            &output.items,
            compiler::source_sites::SourceTreeId::Original,
        )?;
        materializer.self_constants(&mut output)?;
        materializer.items(output.id, &mut output.items, None, 0, false)?;
        materializer.release_bound_source_sites();
        Ok(())
    })();
    let segment = std::mem::replace(&mut materializer.environment.budget, prior);
    result?;
    let usage = segment.usage();
    if usage.steps != 0
        || usage.generated_nodes != 0
        || usage.instances != 0
        || usage.peak_call_depth != 0
        || usage.peak_live_cells != 0
        || !usage.queries.is_empty()
    {
        return Ok(None);
    }
    let receipt = Receipt {
        work: usage.analysis_work,
        starting_live_bytes,
        peak_live_bytes: usage.peak_temporary_bytes,
        ending_live_bytes: segment.live_temporary_bytes(),
    };
    debug_assert_eq!(output, *original);
    debug_assert!(receipt.fits(&materializer.environment.budget));
    receipt.replay(&mut materializer.environment.budget)?;
    Ok(Some(receipt))
}

fn retains_runtime_graph(
    parsed: &[Arc<ParseResult>],
    imports: &[Vec<CoreImport>],
    root: ModuleId,
) -> bool {
    let mut indices = BTreeMap::new();
    for (index, parsed) in parsed.iter().enumerate() {
        if indices.insert(parsed.module.id, index).is_some()
            || !parsed
                .module
                .items
                .iter()
                .any(|item| !matches!(item.kind, s::ItemKind::Import(_)))
            || parsed.module.items.iter().any(|item| {
                matches!(&item.kind,
                s::ItemKind::Import(import) if import.syntax || matches!(import.path.as_slice(),
                    [a, b] if a == "std" && (b == "meta" || b == "syntax")))
            })
        {
            return false;
        }
    }
    let mut visited = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(module) = pending.pop() {
        if !visited.insert(module) {
            continue;
        }
        let Some(index) = indices.get(&module) else {
            return false;
        };
        pending.extend(imports[*index].iter().map(|import| import.module));
    }
    visited.len() == parsed.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::FileId;

    struct Fixture {
        inputs: Vec<ParsedSource>,
        sources: Vec<SourceUnit>,
        parsed: Vec<Arc<ParseResult>>,
        imports: Vec<Vec<CoreImport>>,
        paths: BTreeMap<Vec<String>, ModuleId>,
    }
    impl Fixture {
        fn new(root_body: &str) -> Self {
            let texts = [
                format!("import left\nimport right\nfn main()->i32{{{root_body}}}"),
                "pub fn emit(x:i32)->i32{if x<7{return x+3;}else{return x+4;}}".into(),
                "pub fn emit(x:i32)->i32{if x<7{return x+5;}else{return x+6;}}".into(),
            ];
            let inputs = texts
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    ParsedSource::parse(
                        Arc::new(SourceUnit::new(
                            FileId(index as u32),
                            ModuleId(index as u32),
                            format!("module{index}.zil"),
                            text,
                        )),
                        &Limits::default(),
                        &Default::default(),
                    )
                })
                .collect::<Vec<_>>();
            Self::from_inputs(inputs)
        }
        fn from_inputs(inputs: Vec<ParsedSource>) -> Self {
            let sources = inputs.iter().map(|input| input.source().clone()).collect();
            let parsed = inputs
                .iter()
                .map(ParsedSource::parsed_shared)
                .collect::<Vec<_>>();
            let edges = parsed[0]
                .module
                .items
                .iter()
                .filter_map(|item| {
                    let s::ItemKind::Import(import) = &item.kind else {
                        return None;
                    };
                    let alias = import.path[0].clone();
                    Some(CoreImport {
                        module: ModuleId(if alias == "left" { 1 } else { 2 }),
                        alias,
                        span: item.span,
                    })
                })
                .collect();
            Self {
                inputs,
                sources,
                parsed,
                imports: vec![edges, vec![], vec![]],
                paths: BTreeMap::from([
                    (vec!["left".into()], ModuleId(1)),
                    (vec!["right".into()], ModuleId(2)),
                ]),
            }
        }
        fn run(
            &self,
            session: &mut ScalarMaterializationSession,
            limits: &Limits,
        ) -> CompileResult<Option<ScalarMaterialization>> {
            session.materialize(
                &zil_expand::ExpandSession::default(),
                &self.inputs,
                &self.sources,
                &self.parsed,
                &self.paths,
                None,
                &self.imports,
                ModuleId(0),
                Fingerprint::of(self.sources[0].text.as_bytes()),
                &TargetProfile::default(),
                limits,
            )
        }
        fn full(&self, limits: &Limits) -> CompileResult<Materialized<'_>> {
            // The returned environment is absent for this scalar subset; these
            // stable target bytes must outlive the result's inferred lifetime.
            static TARGET: std::sync::OnceLock<TargetProfile> = std::sync::OnceLock::new();
            materialize_scoped(
                &self.sources,
                &self.parsed,
                &self
                    .parsed
                    .iter()
                    .map(|parsed| &parsed.module)
                    .collect::<Vec<_>>(),
                &self.paths,
                None,
                &self.imports,
                ModuleId(0),
                Fingerprint::of(self.sources[0].text.as_bytes()),
                TARGET.get_or_init(TargetProfile::default),
                limits,
                None,
                true,
            )
        }
    }

    #[test]
    fn unchanged_modules_skip_clone_and_walk_after_earlier_work_changes() {
        let original = Fixture::new("return left.emit(1)+right.emit(2);");
        let mut session = ScalarMaterializationSession::default();
        let limits = Limits::default();
        let cold = original.run(&mut session, &limits).unwrap().unwrap();
        let full = original.full(&limits).unwrap();
        assert_eq!(cold.usage(), &full.usage);
        assert_eq!(Some(cold.remaining()), full.lineage_remaining.as_ref());
        assert_eq!(session.last_stats().cloned_modules, 3);
        let warm = original.run(&mut session, &limits).unwrap().unwrap();
        assert_eq!(warm.usage(), &full.usage);
        assert_eq!(session.last_stats().reused_modules, 3);
        assert_eq!(session.last_stats().cloned_modules, 0);

        let changed = Fixture::new("let x:i32=1;let y:i32=x+2;return left.emit(y)+right.emit(x);");
        let edited = Fixture::from_inputs(vec![
            changed.inputs[0].clone(),
            original.inputs[1].clone(),
            original.inputs[2].clone(),
        ]);
        let incremental = edited.run(&mut session, &limits).unwrap().unwrap();
        let full = edited.full(&limits).unwrap();
        assert_eq!(incremental.usage(), &full.usage);
        assert_eq!(
            Some(incremental.remaining()),
            full.lineage_remaining.as_ref()
        );
        assert_eq!(session.last_stats().reused_modules, 2);
        assert_eq!(session.last_stats().cloned_modules, 1);
        assert_ne!(
            incremental.usage().analysis_work,
            cold.usage().analysis_work
        );
    }

    #[test]
    fn current_budget_failure_matches_original_and_does_not_replace_epoch() {
        let fixture = Fixture::new("return left.emit(1)+right.emit(2);");
        let mut session = ScalarMaterializationSession::default();
        let generous = Limits::default();
        fixture.run(&mut session, &generous).unwrap().unwrap();
        let checkpoint = session.checkpoint();
        let mut low = 0;
        let mut high = generous.max_analysis_steps;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..generous.clone()
            };
            if fixture.full(&limits).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0);
        let exact = Limits {
            max_analysis_steps: low,
            ..generous.clone()
        };
        let success = fixture.run(&mut session, &exact).unwrap().unwrap();
        assert_eq!(success.usage(), &fixture.full(&exact).unwrap().usage);
        assert_eq!(session.last_stats().reused_modules, 3);
        let insufficient = Limits {
            max_analysis_steps: low - 1,
            ..generous
        };
        let expected = fixture.full(&insufficient).err().expect("N-1 must reject");
        let actual = fixture
            .run(&mut session, &insufficient)
            .err()
            .expect("N-1 must reject");
        assert_eq!(actual, expected);
        assert_eq!(session.last_stats().receipt_budget_misses, 1);
        assert_eq!(session.last_stats().cloned_modules, 1);
        assert!(
            session
                .entries
                .iter()
                .all(|(module, entry)| Arc::ptr_eq(entry, &checkpoint.entries[module]))
        );
        let restored = fixture.run(&mut session, &exact).unwrap().unwrap();
        assert_eq!(restored.usage(), success.usage());
        assert_eq!(session.last_stats().reused_modules, 3);
    }

    #[test]
    fn temporary_peak_boundary_replays_admission_and_original_failure() {
        let fixture = Fixture::new("return left.emit(1)+right.emit(2);");
        let generous = Limits::default();
        let mut session = ScalarMaterializationSession::default();
        fixture.run(&mut session, &generous).unwrap().unwrap();
        let mut low = 0;
        let mut high = generous.max_source_bytes;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_source_bytes: middle,
                ..generous.clone()
            };
            if fixture.full(&limits).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0);
        let exact = Limits {
            max_source_bytes: low,
            ..generous.clone()
        };
        let admitted = fixture.run(&mut session, &exact).unwrap().unwrap();
        assert_eq!(admitted.usage(), &fixture.full(&exact).unwrap().usage);
        assert_eq!(session.last_stats().reused_modules, 3);
        let insufficient = Limits {
            max_source_bytes: low - 1,
            ..generous
        };
        let expected = fixture.full(&insufficient).err().expect("N-1 must reject");
        let actual = fixture
            .run(&mut session, &insufficient)
            .err()
            .expect("N-1 must reject");
        assert_eq!(actual, expected);
        assert_eq!(session.last_stats().receipt_budget_misses, 1);
        assert_eq!(session.last_stats().cloned_modules, 1);
    }

    #[test]
    fn changed_depth_domain_cannot_reuse_a_more_generous_admission() {
        let fixture = Fixture::new("return left.emit(1)+right.emit(2);");
        let generous = Limits::default();
        let mut session = ScalarMaterializationSession::default();
        fixture.run(&mut session, &generous).unwrap().unwrap();
        let mut low = 0;
        let mut high = generous.max_call_depth;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_call_depth: middle,
                ..generous.clone()
            };
            if fixture.full(&limits).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0);
        let exact = Limits {
            max_call_depth: low,
            ..generous.clone()
        };
        let admitted = fixture.run(&mut session, &exact).unwrap().unwrap();
        assert_eq!(admitted.usage(), &fixture.full(&exact).unwrap().usage);
        assert_eq!(session.last_stats().reused_modules, 0);
        fixture.run(&mut session, &exact).unwrap().unwrap();
        assert_eq!(session.last_stats().reused_modules, 3);
        let insufficient = Limits {
            max_call_depth: low - 1,
            ..generous
        };
        let expected = fixture.full(&insufficient).err().expect("N-1 must reject");
        let actual = match fixture.run(&mut session, &insufficient) {
            Err(errors) => errors,
            Ok(None) => fixture
                .full(&insufficient)
                .err()
                .expect("fallback must reject"),
            Ok(Some(_)) => panic!("old depth admission must not survive N-1"),
        };
        assert_eq!(actual, expected);
        assert_eq!(session.last_stats().reused_modules, 0);
    }

    #[test]
    fn public_parse_substitution_and_pruned_modules_cannot_claim_unchanged_output() {
        let mut fixture = Fixture::new("return left.emit(1)+right.emit(2);");
        let mut session = ScalarMaterializationSession::default();
        fixture
            .run(&mut session, &Limits::default())
            .unwrap()
            .unwrap();
        let mut substituted = fixture.parsed.clone();
        Arc::make_mut(&mut substituted[1]).module.items.clear();
        std::mem::swap(&mut substituted, &mut fixture.parsed);
        assert!(
            fixture
                .run(&mut session, &Limits::default())
                .unwrap()
                .is_none()
        );
        std::mem::swap(&mut substituted, &mut fixture.parsed);
        fixture.imports[0].pop();
        assert!(
            fixture
                .run(&mut session, &Limits::default())
                .unwrap()
                .is_none()
        );
        assert_eq!(session.entries.len(), 3);
        let ct = Fixture::new("return #(left.emit(1))+right.emit(2);");
        let ct = Fixture::from_inputs(vec![
            ct.inputs[0].clone(),
            fixture.inputs[1].clone(),
            fixture.inputs[2].clone(),
        ]);
        assert!(ct.run(&mut session, &Limits::default()).unwrap().is_none());
        assert_eq!(session.entries.len(), 3);
    }
}
