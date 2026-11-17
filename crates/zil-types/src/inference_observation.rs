//! Facts collected by the current bounded closure visitor. These are local to
//! one inference request, never decoded receipts or authority to check types.
use super::*;

#[derive(Default)]
pub(crate) struct BodyObservation {
    visits: usize,
    bindings: usize,
    max_depth: usize,
    complex: bool,
    reserved: bool,
    factory: bool,
    projection: Option<projection::Projection>,
    shape: Option<scalar_shapes::ShapeObservation>,
}
impl BodyObservation {
    pub fn with_projection(origin: Span) -> Self {
        Self {
            projection: Some(projection::Projection::new(origin)),
            shape: Some(scalar_shapes::ShapeObservation::default()),
            ..Self::default()
        }
    }
    pub fn take_input(&mut self) -> Option<BodyInput> {
        self.projection.take()?.finish()
    }
    pub fn shape_fee(&self, function: &CoreFunction, limits: &Limits) -> Option<usize> {
        self.shape.as_ref()?.finish(function, limits)
    }
    fn visit(&mut self, depth: usize) {
        self.max_depth = self.max_depth.max(depth);
        if let Some(visits) = self.visits.checked_add(1) {
            self.visits = visits;
        } else {
            self.complex = true;
        }
    }
    pub fn block(&mut self, block: &CoreBlock, depth: usize) {
        self.visit(depth);
        if let Some(shape) = &mut self.shape {
            shape.block(depth);
        }
        if let Some(projection) = &mut self.projection {
            projection.block(block);
        }
    }
    pub fn statement(&mut self, statement: &CoreStatement, depth: usize) {
        self.visit(depth);
        if let Some(shape) = &mut self.shape {
            shape.statement(statement, depth);
        }
        if let Some(projection) = &mut self.projection {
            projection.statement(statement);
        }
        match &statement.kind {
            CoreStatementKind::Binding { name, .. } => {
                self.reserved |= !names::plain_binding_name(name);
                if let Some(bindings) = name
                    .len()
                    .checked_add(1)
                    .and_then(|fee| self.bindings.checked_add(fee))
                {
                    self.bindings = bindings;
                } else {
                    self.complex = true;
                }
            }
            CoreStatementKind::Assign { .. }
            | CoreStatementKind::Expression(_)
            | CoreStatementKind::Return(_)
            | CoreStatementKind::If { .. }
            | CoreStatementKind::While { .. }
            | CoreStatementKind::Block(_)
            | CoreStatementKind::Unsafe(_)
            | CoreStatementKind::Defer(_)
            | CoreStatementKind::Break
            | CoreStatementKind::Continue => {}
            // Pattern/scope depth differs between the original visitors. Keep
            // their existing full paths rather than approximate those charges.
            _ => self.complex = true,
        }
    }
    pub fn expression(
        &mut self,
        expression: &CoreExpr,
        depth: usize,
        factories: &BTreeSet<String>,
    ) {
        self.visit(depth);
        if let Some(shape) = &mut self.shape {
            shape.expression(expression, depth);
        }
        if let Some(projection) = &mut self.projection {
            projection.expression(expression);
        }
        match &expression.kind {
            CoreExprKind::Call { callee, .. } => {
                // A superset of actual factory dependencies: shadowing, aliases
                // and missing exports can only remove these candidates. This
                // permits a negative result without rebuilding lexical scopes.
                let name = match &callee.kind {
                    CoreExprKind::Name(name) => Some(name),
                    CoreExprKind::Member { member, .. } => Some(member),
                    _ => None,
                };
                self.factory |= name.is_some_and(|name| factories.contains(name));
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::Unary { .. }
            | CoreExprKind::CompileTime(_)
            | CoreExprKind::Try(_)
            | CoreExprKind::IntegerConvert { .. }
            | CoreExprKind::NumericConvert { .. }
            | CoreExprKind::Ascribed { .. }
            | CoreExprKind::Binary { .. }
            | CoreExprKind::Member { .. }
            | CoreExprKind::If { .. } => {}
            _ => self.complex = true,
        }
    }
    pub fn names_fee(&self, limits: &Limits) -> Option<usize> {
        (!self.complex && !self.reserved && self.max_depth <= limits.max_call_depth)
            .then_some(self.visits)
    }
    pub fn dependencies_fee(
        &self,
        function: &CoreFunction,
        has_closure: bool,
        limits: &Limits,
    ) -> Option<usize> {
        if self.complex
            || self.factory
            || has_closure
            || self.max_depth > limits.max_call_depth
            || !function.type_parameters.is_empty()
            || !function.state_parameters.is_empty()
        {
            return None;
        }
        let mut fee = self
            .visits
            .checked_add(self.bindings)?
            .checked_add(function.parameters.len())?;
        for parameter in &function.parameters {
            fee = fee.checked_add(parameter.name.len())?;
        }
        Some(fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use zil_base::{FileId, Fingerprint, ModuleId, SourceUnit};
    use zil_syntax::{ParsedSource, attributes::AttributeContext};

    fn raw(texts: &[&str]) -> CoreProgram {
        let modules = texts
            .iter()
            .enumerate()
            .map(|(id, text)| {
                let source = Arc::new(SourceUnit::new(
                    FileId(id as u32),
                    ModuleId(id as u32),
                    format!("m{id}.zil"),
                    *text,
                ));
                let parsed =
                    ParsedSource::parse(source, &Limits::default(), &AttributeContext::default());
                assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
                let core = zil_expand::expand(parsed.source(), &parsed.parsed().module).unwrap();
                ProgramModule {
                    exports: core.functions.iter().map(|f| f.id).collect(),
                    core: Arc::new(core),
                    imports: vec![],
                }
            })
            .collect();
        CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules,
        }
    }

    fn compare(program: &CoreProgram, limits: &Limits) {
        assert_eq!(
            infer_program_visiting(program, limits, None, true).map(Cow::into_owned),
            infer_program_visiting(program, limits, None, false).map(Cow::into_owned),
            "work={} nodes={} depth={}",
            limits.max_analysis_steps,
            limits.max_nodes,
            limits.max_call_depth,
        );
    }

    const BRANCHES: &str = "fn first(x:i32)->i32 {let y:i32=x+1; if x<10 {y=x+2;} else {y=x*3;} while false {y=x+4;} defer {let z=x+5;} return y;} fn second(x:i32)->i32 {return first(x)+missing(x);}";

    #[test]
    fn current_observation_fees_equal_both_complete_original_visitors() {
        let program = raw(&[BRANCHES]);
        let limits = Limits::default();
        for function in &program.modules[0].core.functions {
            let mut budget = Budget::new(&limits);
            budget.observation = Some(BodyObservation::default());
            assert!(!contains_closure(&function.body, 0, &mut budget).unwrap());
            let body = budget.observation.take().unwrap();
            assert_eq!(
                body.names_fee(&limits),
                Some(names::check_function(function, &limits).unwrap())
            );
            let mut remaining = limits.max_analysis_steps;
            assert!(
                retained::dependencies(function, &BTreeMap::new(), None, &mut remaining, &limits)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                body.dependencies_fee(function, false, &limits),
                Some(limits.max_analysis_steps - remaining)
            );
        }
        // Names has its own aggregate node allowance; type visits do not count
        // toward it. Compare every prefix, including the failing current span.
        let total: usize = program.modules[0]
            .core
            .functions
            .iter()
            .map(|f| names::check_function(f, &limits).unwrap())
            .sum();
        let mut observations = Vec::new();
        for function in &program.modules[0].core.functions {
            let mut budget = Budget::new(&limits);
            budget.observation = Some(BodyObservation::default());
            contains_closure(&function.body, 0, &mut budget).unwrap();
            observations.push(budget.observation.take());
        }
        for depth in 0..=10 {
            let quota = Limits {
                max_call_depth: depth,
                ..limits.clone()
            };
            assert_eq!(
                names::check_modules_observed(
                    program.modules.iter().map(|m| m.core.as_ref()),
                    &quota,
                    None,
                    &observations
                ),
                names::check_modules(program.modules.iter().map(|m| m.core.as_ref()), &quota),
            );
        }
        for nodes in 0..=total + 1 {
            let quota = Limits {
                max_nodes: nodes,
                ..limits.clone()
            };
            assert_eq!(
                names::check_modules_observed(
                    program.modules.iter().map(|m| m.core.as_ref()),
                    &quota,
                    None,
                    &observations
                ),
                names::check_modules(program.modules.iter().map(|m| m.core.as_ref()), &quota),
            );
        }
    }

    #[test]
    fn complete_raw_pipeline_preserves_work_node_depth_and_first_error_prefixes() {
        let defaults = Limits::default();
        for source in [
            BRANCHES,
            "fn first(x:i32)->i32 {if false {let i32=x;} return x;} fn second(x:i32)->i32 {let self=x;return x;}",
            "fn first(x:i32)->i32 {let f=(y)=>y+1;return f(x);} fn second(x:i32)->i32 {return x;}",
        ] {
            let program = raw(&[source]);
            compare(&program, &defaults);
            for depth in 0..=10 {
                compare(
                    &program,
                    &Limits {
                        max_call_depth: depth,
                        ..defaults.clone()
                    },
                );
            }
            for work in [true, false] {
                // Find the first success using only the original visitors. A
                // semantic failure stays a failure at the default upper bound.
                let mut low = 0;
                let mut high = if work {
                    defaults.max_analysis_steps
                } else {
                    defaults.max_nodes
                };
                while low < high {
                    let mid = low + (high - low) / 2;
                    let mut quota = defaults.clone();
                    if work {
                        quota.max_analysis_steps = mid;
                    } else {
                        quota.max_nodes = mid;
                    }
                    if infer_program_visiting(&program, &quota, None, false).is_ok() {
                        high = mid;
                    } else {
                        low = mid + 1;
                    }
                }
                for value in [
                    0,
                    1,
                    2,
                    3,
                    low.saturating_sub(1),
                    low,
                    low.saturating_add(1),
                ] {
                    let mut quota = defaults.clone();
                    if work {
                        quota.max_analysis_steps = value;
                    } else {
                        quota.max_nodes = value;
                    }
                    compare(&program, &quota);
                }
            }
        }
    }

    #[test]
    fn later_module_factory_aliases_shadowing_and_misses_use_current_headers() {
        let limits = Limits::default();
        for call in ["dep.factory(x)", "dep.missing(x)", "missing(x)"] {
            for shadow in ["", "let dep=x;"] {
                let caller = format!("fn caller(x:i32)->i32 {{{shadow}return {call};}}");
                let mut program = raw(&[&caller, "fn factory(x:i32)->i32 {return x;}"]);
                program.modules[0].imports.push(CoreImport {
                    alias: "dep".into(),
                    module: ModuleId(1),
                    span: Span::default(),
                });
                // The membership table must include a factory declared after
                // this caller, regardless of alias spelling or lexical scopes.
                Arc::make_mut(&mut program.modules[1].core).functions[0].callable_return =
                    Some(CoreCallableReturn {
                        kind: CallableKind::Fn,
                        parameters: vec![Type::I32],
                        result: Type::I32,
                    });
                compare(&program, &limits);
                for work in 0..=256 {
                    compare(
                        &program,
                        &Limits {
                            max_analysis_steps: work,
                            ..limits.clone()
                        },
                    );
                }
            }
        }
        let program = raw(&["fn caller(x:i32)->i32 {return factory(x);}"]);
        let function = &program.modules[0].core.functions[0];
        let mut budget = Budget::new(&limits);
        budget.factory_names.insert("factory".into());
        budget.observation = Some(BodyObservation::default());
        contains_closure(&function.body, 0, &mut budget).unwrap();
        assert!(
            budget
                .observation
                .take()
                .unwrap()
                .dependencies_fee(function, false, &limits)
                .is_none()
        );
    }
}
