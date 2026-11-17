//! Explicit trusted-cache candidate DATA, never an Analyze/Verify capability.
//! Reconstructed bases cannot take the current-owner Complete shortcut.
use super::*;

#[derive(Clone)]
pub struct RawAnalysisRow {
    pub input: Arc<TypedFunction>,
    pub plan: Arc<FunctionPlan>,
    pub costs: [usize; 5],
}
#[derive(Clone)]
pub struct RawAnalysisBase {
    pub context: Vec<u8>,
    pub rows: Vec<RawAnalysisRow>,
    pub global_cost: usize,
}
struct ImportWork {
    bodies: usize,
    body_reuses: usize,
    plans: usize,
    plan_reuses: usize,
}

struct Bytes {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Bytes {
    fn put(&mut self, bytes: &[u8]) -> Option<()> {
        if self.bytes.len().checked_add(bytes.len())? > self.maximum {
            return None;
        }
        self.bytes.extend_from_slice(bytes);
        Some(())
    }
    fn word(&mut self, value: usize) -> Option<()> {
        self.put(&u64::try_from(value).ok()?.to_le_bytes())
    }
    fn text(&mut self, value: &str) -> Option<()> {
        self.word(value.len())?;
        self.put(value.as_bytes())
    }
    fn ty(&mut self, value: &Type) -> Option<()> {
        self.word(match value {
            Type::Unit => 0,
            Type::Bool => 1,
            Type::I8 => 2,
            Type::I16 => 3,
            Type::I32 => 4,
            Type::I64 => 5,
            Type::Isize => 6,
            Type::U8 => 7,
            Type::U16 => 8,
            Type::U32 => 9,
            Type::U64 => 10,
            Type::Usize => 11,
            Type::F32 => 12,
            Type::F64 => 13,
            _ => return None,
        })
    }
}

impl RawAnalysisBase {
    /// Every varying global/header input to the closed scalar product domain.
    /// Diagnostic spans remain current, and are compared exactly for ordinary
    /// foreign/unsafe bodies by the existing span_sensitive rule.
    pub fn context_for(program: &TypedProgram, limits: &Limits) -> Option<Vec<u8>> {
        let TypedModule {
            schema_version,
            module_id,
            source_fingerprint: _,
            target,
            functions,
            closures,
            resources,
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            unsupported,
        } = &program.module;
        let TypedProgram {
            module: _,
            interfaces: _,
            root_functions: _,
            task_interfaces,
        } = program;
        if !closures.is_empty()
            || !resources.is_empty()
            || !dyn_declarations.is_empty()
            || !dyn_interfaces.is_empty()
            || !dyn_vtables.is_empty()
            || !life_declarations.is_empty()
            || !life_instances.is_empty()
            || !unsupported.is_empty()
            || !task_interfaces.is_empty()
            || functions.is_empty()
            || functions.len() > limits.max_functions
        {
            return None;
        }
        let mut out = Bytes {
            bytes: Vec::new(),
            maximum: limits.max_source_bytes.min(16 * 1024 * 1024),
        };
        out.put(b"ZIL-RAW-ANALYZE-CONTEXT-1")?;
        out.word(*schema_version as usize)?;
        out.word(module_id.0 as usize)?;
        let zil_base::TargetProfile {
            id,
            triple,
            pointer_width,
            endian,
            backend,
            rule_version,
            checked_arithmetic,
        } = target;
        for value in [id, triple, backend, rule_version] {
            out.text(value)?;
        }
        out.word(*pointer_width as usize)?;
        out.word(match endian {
            zil_base::Endian::Little => 0,
            zil_base::Endian::Big => 1,
        })?;
        out.word(usize::from(*checked_arithmetic))?;
        let Limits {
            max_source_bytes,
            max_tokens,
            max_nodes,
            max_functions,
            max_blocks,
            max_instructions,
            max_locals,
            max_values,
            max_analysis_steps,
            max_ct_steps,
            max_call_depth,
        } = limits;
        for value in [
            max_source_bytes,
            max_tokens,
            max_nodes,
            max_functions,
            max_blocks,
            max_instructions,
            max_locals,
            max_values,
            max_analysis_steps,
            max_call_depth,
        ] {
            out.word(*value)?;
        }
        out.put(&max_ct_steps.to_le_bytes())?;
        out.word(functions.len())?;
        for (index, function) in functions.iter().enumerate() {
            let TypedFunction {
                storage_life,
                call_kind,
                reference_return_bound,
                callable_return_contract,
                callable_contract,
                protocol_contract,
                id,
                name,
                foreign,
                environment,
                parameters,
                return_type,
                result_mode,
                locals,
                values: _,
                blocks: _,
                entry: _,
                span,
            } = function;
            if id.0 as usize != index
                || *call_kind != CallKind::Direct
                || storage_life.is_some()
                || reference_return_bound.is_some()
                || callable_return_contract.is_some()
                || callable_contract.is_some()
                || protocol_contract.is_some()
                || environment.is_some()
                || parameters.len() > limits.max_locals
            {
                return None;
            }
            out.word(index)?;
            out.text(name)?;
            out.word(span.file.0 as usize)?;
            out.word(match result_mode {
                PassingMode::Copy => 0,
                PassingMode::Move => 1,
                PassingMode::Describe => 2,
                PassingMode::Create => 3,
                PassingMode::Give => 4,
                PassingMode::Borrow => 5,
                PassingMode::BorrowMut => 6,
                PassingMode::Infer => 7,
            })?;
            out.ty(return_type)?;
            out.word(parameters.len())?;
            for id in parameters {
                out.word(id.0 as usize)?;
                out.ty(&locals.get(id.0 as usize)?.ty)?;
            }
            out.word(usize::from(foreign.is_some()))?;
            if let Some(ForeignDeclaration {
                abi,
                symbol,
                noescape_parameters,
                static_callbacks,
                scoped_callback,
            }) = foreign
            {
                if scoped_callback.is_some() {
                    return None;
                }
                out.text(abi)?;
                out.text(symbol)?;
                out.word(usize::from(*static_callbacks))?;
                out.word(noescape_parameters.len())?;
                for index in noescape_parameters {
                    out.word(*index as usize)?;
                }
            }
        }
        Some(out.bytes)
    }

    /// Eligibility/equality only; neither result supplies a successful receipt.
    pub fn span_sensitive(function: &TypedFunction) -> Option<bool> {
        Some(closed(function)?.span_sensitive)
    }
    pub fn same_template(left: &TypedFunction, right: &TypedFunction) -> bool {
        match (closed(left), closed(right)) {
            (Some(a), Some(b)) if !a.span_sensitive && !b.span_sensitive => {
                let mut rebound = left.clone();
                rebound.id = right.id;
                rebound.name = right.name.clone();
                candidate_queries::semantic_input_eq(&rebound, right)
            }
            (Some(_), Some(_)) => left == right,
            _ => false,
        }
    }
    /// Raw equality components for request-local immutable-column memoization.
    /// Callers still bind each actual outer header and span-sensitive input.
    pub fn same_template_headers(left: &TypedFunction, right: &TypedFunction) -> bool {
        candidate_queries::semantic_header_eq(left, right)
    }
    pub fn same_template_columns(left: &TypedFunction, right: &TypedFunction) -> bool {
        candidate_queries::semantic_columns_eq(left, right)
    }

    fn epoch(&self, current: &Arc<TypedProgram>, limits: &Limits) -> Option<(Epoch, ImportWork)> {
        if self.context != Self::context_for(current, limits)?
            || self.rows.len() != current.module.functions.len()
            || self.rows.len() > limits.max_functions
            || self.global_cost > limits.max_analysis_steps
        {
            return None;
        }
        let mut previous = (**current).clone();
        let mut functions = PersistentVec::new();
        let mut plans = PersistentVec::new();
        let mut costs = Vec::with_capacity(self.rows.len());
        let mut total = self.global_cost;
        let mut counts = Total::default();
        let mut facts = BodyFacts::default();
        let mut shapes = HashMap::new();
        let mut plan_reuses = 0;
        for (index, row) in self.rows.iter().enumerate() {
            let input = row.input.as_ref();
            let function = current.module.functions.get(index)?;
            if input.id.0 as usize != index
                || row.plan.function != input.id
                || !projection_bounded(input, limits)
                || !projection_bounded(function, limits)
                || !signature_eq(input, function)
            {
                return None;
            }
            let (_, inventory) = facts.get(input)?;
            // plan_shape reads only these input lengths and the immutable
            // plan columns. Literal operands and diagnostic placement are not
            // its dependencies; they remain bound by the separate full input
            // comparison and per-function header/aggregate quota checks.
            let shape_key = (
                input.locals.len(),
                input.values.len(),
                input.blocks.len(),
                std::ptr::from_ref(&*row.plan.blocks) as usize,
                std::ptr::from_ref(&*row.plan.holds) as usize,
            );
            let shape = if let Some(shape) = shapes.get(&shape_key) {
                plan_reuses += 1;
                *shape
            } else {
                let shape = plan_shape(input, &row.plan);
                shapes.insert(shape_key, shape);
                shape
            };
            if !shape {
                return None;
            }
            let cost = row
                .costs
                .iter()
                .try_fold(0usize, |n, cost| n.checked_add(*cost))?;
            if row.costs[0].checked_add(row.costs[1])? == 0 || row.costs[2] == 0 {
                return None;
            }
            total = total.checked_add(cost)?;
            counts = counts.add(Total { cost, ..inventory })?;
            if total > limits.max_analysis_steps || !counts.counts_fit(limits) {
                return None;
            }
            functions.push_owner(row.input.clone());
            plans.push_owner(row.plan.clone());
            costs.push(row.costs);
        }
        previous.module.functions = functions;
        let candidate = Arc::new(CandidatePlan {
            schema_version: previous.module.schema_version,
            module_fingerprint: Fingerprint::ZERO,
            target_fingerprint: previous.module.target.fingerprint(),
            functions: plans,
            origins: vec![],
            responsibilities: vec![],
            unsupported: vec![],
        });
        let epoch = Epoch::new_with_facts(
            &Arc::new(previous),
            candidate,
            limits,
            Recording { costs, cost: total },
            &BTreeMap::new(),
            &mut facts,
        )?;
        Some((
            epoch,
            ImportWork {
                bodies: facts.built,
                body_reuses: facts.reused,
                plans: shapes.len(),
                plan_reuses,
            },
        ))
    }
}

fn plan_shape(function: &TypedFunction, plan: &FunctionPlan) -> bool {
    if plan.blocks.len() != function.blocks.len() || plan.holds.len() != function.values.len() {
        return false;
    }
    for (index, block) in plan.blocks.iter().enumerate() {
        if block.block.0 as usize != index {
            return false;
        }
        for locals in [&block.initialized_in, &block.initialized_out] {
            if locals.windows(2).any(|pair| pair[0] >= pair[1])
                || locals
                    .iter()
                    .any(|id| id.0 as usize >= function.locals.len())
            {
                return false;
            }
        }
    }
    plan.holds
        .iter()
        .enumerate()
        .all(|(index, hold)| hold.value.0 as usize == index && hold.roots.is_empty())
}

impl AnalysisSession {
    pub fn has_product_base(&self) -> bool {
        self.product.is_some()
    }

    /// Only a complete successful current producer can export raw accounting.
    /// Driver persistence is separately gated by full Verify/native success.
    pub fn raw_product_for(
        &self,
        program: &Arc<TypedProgram>,
        candidate: &Arc<CandidatePlan>,
    ) -> Option<RawAnalysisBase> {
        if !self.product_complete.as_ref()?.matches(program, candidate) {
            return None;
        }
        let epoch = self.product.as_ref()?;
        if !Arc::ptr_eq(&epoch.program, program)
            || !candidate.origins.is_empty()
            || !candidate.responsibilities.is_empty()
            || !candidate.unsupported.is_empty()
        {
            return None;
        }
        Some(RawAnalysisBase {
            context: RawAnalysisBase::context_for(program, &epoch.limits)?,
            global_cost: epoch.global_cost,
            rows: epoch
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    Some(RawAnalysisRow {
                        input: row.function.clone(),
                        plan: candidate.functions.owner(index)?.clone(),
                        costs: row.costs,
                    })
                })
                .collect::<Option<_>>()?,
        })
    }

    pub fn analyze_product_with_raw(
        &mut self,
        program: &Arc<TypedProgram>,
        limits: &Limits,
        raw: Option<&RawAnalysisBase>,
    ) -> CompileResult<Arc<CandidatePlan>> {
        // A decoder never sets Complete or replaces a live successful epoch.
        // The existing update performs exact body/span checks and reserves the
        // entire unchanged ledger before admitting/executing changed functions.
        if self.product.is_none()
            && let Some(raw) = raw
            && let Some((base, work)) = raw.epoch(program, limits)
            && let Some((next, mut stats, changes)) = base.update(program, limits)
        {
            let result = next.candidate.clone();
            self.product = Some(next);
            self.product_complete = Some(Arc::new(Complete {
                program: program.clone(),
                candidate: result.clone(),
                limits: limits.clone(),
            }));
            stats.raw_functions_restored = stats.product_function_queries_reused;
            stats.raw_body_facts_built = work.bodies;
            stats.raw_body_facts_reused = work.body_reuses;
            stats.raw_plan_shapes_checked = work.plans;
            stats.raw_plan_shapes_reused = work.plan_reuses;
            self.stats = stats;
            self.product_changes = changes;
            self.function_results = Arc::default();
            return Ok(result);
        }
        self.analyze_product(program, limits)
    }
}

#[cfg(test)]
#[path = "raw_product_tests.rs"]
mod tests;
