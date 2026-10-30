//! Source #if scheduling over one immutable ConfigReady snapshot. Selection
//! only produces syntax; every selected program follows the ordinary pipeline.
mod budget;
mod context;
mod generation;
mod hygiene;
mod materialize;
mod type_values;

use budget::Budget;
use context::Context;
use std::collections::BTreeSet;
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, NodeId, Phase, SCHEMA_VERSION,
    SourceUnit, Span, TargetProfile,
};
use zil_expand::ct_selection::{self as syntax, SyntaxCost, Visitor};
use zil_ir::*;
use zil_syntax::{ParseResult, ast as s};

#[derive(Clone, Debug)]
pub(super) struct Dependency {
    pub module: ModuleId,
    pub function: String,
    pub declaration: Span,
    pub source: Fingerprint,
}
#[derive(Clone, Debug)]
pub(super) struct Trace {
    pub module: ModuleId,
    pub condition: Span,
    pub selected_then: bool,
    pub dependencies: Vec<Dependency>,
    pub steps: u64,
    pub nodes: usize,
}
#[derive(Clone, Debug)]
pub(super) struct Receipt {
    pub usage: zil_ct::SourceEvaluationUsage,
    pub trace: Vec<Trace>,
    generations: Vec<generation::ForTrace>,
}
impl Default for Receipt {
    fn default() -> Self {
        Self {
            usage: zil_ct::SourceEvaluationUsage { steps: 0, nodes: 0 },
            trace: Vec::new(),
            generations: Vec::new(),
        }
    }
}
impl Receipt {
    pub fn to_json(&self) -> String {
        use zil_base::json_string;
        fn span(s: Span) -> String {
            format!(
                "{{\"file\":{},\"start\":{},\"end\":{}}}",
                s.file.0, s.start, s.end
            )
        }
        let selections = self.trace.iter().map(|t| {
            let dependencies = t.dependencies.iter().map(|d| format!("{{\"module\":{},\"function\":{},\"declaration\":{},\"source\":{}}}", d.module.0, json_string(&d.function), span(d.declaration), json_string(&d.source.to_hex()))).collect::<Vec<_>>().join(",");
            format!("{{\"module\":{},\"condition\":{},\"selected_then\":{},\"steps\":{},\"nodes\":{},\"dependencies\":[{}]}}", t.module.0, span(t.condition), t.selected_then, t.steps, t.nodes, dependencies)
        }).collect::<Vec<_>>().join(",");
        let ordinary = format!(
            "{{\"profile\":\"ConfigReady-scalar219\",\"steps\":{},\"nodes\":{},\"selections\":[{}]}}",
            self.usage.steps, self.usage.nodes, selections
        );
        if self.generations.is_empty() {
            return ordinary;
        }
        let generations = self.generations.iter().map(|t| {
            let dependencies = t.dependencies.iter().map(|d| format!("{{\"module\":{},\"function\":{},\"declaration\":{},\"source\":{}}}", d.module.0, json_string(&d.function), span(d.declaration), json_string(&d.source.to_hex()))).collect::<Vec<_>>().join(",");
            format!("{{\"module\":{},\"iterable\":{},\"binding\":{},\"iterations\":{},\"steps\":{},\"nodes\":{},\"dependencies\":[{}]}}", t.module.0, span(t.iterable), json_string(&t.binding), t.iterations, t.steps, t.nodes, dependencies)
        }).collect::<Vec<_>>().join(",");
        format!(
            "{},\"generations\":[{}]}}",
            &ordinary[..ordinary.len() - 1],
            generations
        )
    }
    pub fn remaining(&self, limits: &Limits) -> CompileResult<Limits> {
        let mut remaining = limits.clone();
        remaining.max_ct_steps = remaining
            .max_ct_steps
            .checked_sub(self.usage.steps)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "cached selection CT step budget exceeded",
                    None,
                )]
            })?;
        remaining.max_nodes = remaining
            .max_nodes
            .checked_sub(self.usage.nodes)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "cached selection node budget exceeded",
                    None,
                )]
            })?;
        Ok(remaining)
    }
}
pub(super) struct Selected {
    pub modules: Vec<s::Module>,
    pub receipt: Receipt,
}

/// The parser already bounded this lexical tape. Inspecting tokens avoids new
/// AST copies/CT fees for old sources, and ignores spelling inside comments or
/// literals. A syntax-island token can conservatively request the bounded pass.
pub(super) fn needed(source: &SourceUnit, parsed: &ParseResult) -> bool {
    use zil_syntax::token::{Symbol, TokenKind};
    let mut hash = false;
    for token in &parsed.tokens {
        if token.is_trivia() {
            continue;
        }
        if hash
            && token.kind == TokenKind::Identifier
            && matches!(token.text(&source.text), "if" | "for")
        {
            return true;
        }
        hash = token.kind == TokenKind::Symbol(Symbol::Hash);
    }
    false
}

pub(super) fn select(
    sources: &[SourceUnit],
    modules: &[&s::Module],
    imports: &[Vec<CoreImport>],
    root: ModuleId,
    fingerprint: Fingerprint,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<Selected> {
    if sources.len() != modules.len() || modules.len() != imports.len() {
        return Err(vec![Diagnostic::error(
            Phase::Ct,
            "E-CT-SELECTION-INPUT",
            None,
            "selection source/module/import identities disagree",
        )]);
    }
    let mut budget = Budget::new(limits);
    // Both syntax branches were parsed. Bound even the discarded borrowed trees
    // before cloning raw syntax; this does not resolve their names or types.
    for module in modules {
        let cost = syntax::module_cost(module, &budget.remaining)?;
        budget.copy(cost, module.span)?;
        budget.copy(cost, module.span)?;
    }
    let context = Context::freeze(sources, modules, imports, root, fingerprint, &mut budget)?;
    let mut output: Vec<_> = modules.iter().map(|module| (*module).clone()).collect();
    let mut trace = Vec::new();
    let mut generations = Vec::new();
    for module in &mut output {
        let mut materializer = materialize::Materializer {
            module: module.id,
            context: &context,
            budget: &mut budget,
            target,
            trace: &mut trace,
            locals: Vec::new(),
            iterations: Vec::new(),
            for_trace: &mut generations,
            next_generation: 0,
        };
        materializer.items(&mut module.items, 0, false)?;
    }
    Ok(Selected {
        modules: output,
        receipt: Receipt {
            usage: budget.usage(),
            trace,
            generations,
        },
    })
}

fn phase_error(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-PHASE-DEPENDENCY",
        Some(span),
        message,
    )]
}
fn forbidden(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-FORBIDDEN",
        Some(span),
        message,
    )]
}

impl materialize::Materializer<'_, '_> {
    fn choose(&mut self, expression: &s::Expr) -> CompileResult<bool> {
        let dependencies =
            self.context
                .dependencies(self.module, expression, &self.locals, self.budget)?;
        let cost = syntax::expression_cost(expression, &self.budget.remaining)?;
        self.budget.copy(cost, expression.span)?;
        // Prepay the complete closed declaration snapshot for the raw compiler's
        // header/lookup/body copies. It may use a strict subset, never more than
        // this bounded snapshot. This fee is cumulative across all conditions.
        self.budget.copy(self.context.cost, expression.span)?;
        self.budget.spend(8, 32, expression.span)?;
        let raw = syntax::expression(self.module, expression, &self.budget.remaining)?;
        let span = expression.span;
        let query = CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: self.module,
            source_fingerprint: self.context.program.source_fingerprint,
            functions: vec![CoreFunction {
                id: NodeId(u32::MAX - 8),
                name: "$selection219".into(),
                span,
                parameters: Vec::new(),
                return_type: Type::Bool,
                receiver: None,
                result_mode: PassingMode::Infer,
                foreign: None,
                callable_return: None,
                type_parameters: Vec::new(),
                state_parameters: Vec::new(),
                callable_parameters: Vec::new(),
                body: CoreBlock {
                    span,
                    statements: vec![CoreStatement {
                        id: NodeId(u32::MAX - 7),
                        span,
                        kind: CoreStatementKind::Return(Some(CoreExpr {
                            id: NodeId(u32::MAX - 6),
                            span,
                            kind: CoreExprKind::CompileTime(Box::new(raw)),
                        })),
                    }],
                },
            }],
            aggregates: Vec::new(),
            enums: Vec::new(),
            unsupported: Vec::new(),
        };
        let compiler = super::ct_program::Compiler {
            program: &self.context.program,
            module: self.module,
            target: self.target,
        };
        let context =
            zil_ct::SourceProgramContext::new(&self.context.program, &self.budget.remaining)?;
        let (evaluated, usage) = zil_ct::evaluate_source_constants_with_compiler(
            &query,
            self.target,
            &self.budget.remaining,
            Some(&context),
            &compiler,
        )?;
        self.budget.execute(usage, span)?;
        let [function] = evaluated.functions.as_slice() else {
            return Err(forbidden(
                span,
                "selection query produced an invalid function count",
            ));
        };
        let [statement] = function.body.statements.as_slice() else {
            return Err(forbidden(
                span,
                "selection query produced an invalid statement count",
            ));
        };
        let CoreStatementKind::Return(Some(CoreExpr {
            kind: CoreExprKind::Bool(selected),
            ..
        })) = &statement.kind
        else {
            return Err(vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-SELECTION-TYPE",
                Some(span),
                "#if condition must evaluate to bool",
            )]);
        };
        self.budget.node(span, 0)?;
        self.trace.push(Trace {
            module: self.module,
            condition: span,
            selected_then: *selected,
            dependencies,
            steps: usage.steps,
            nodes: usage.nodes,
        });
        Ok(*selected)
    }
}
