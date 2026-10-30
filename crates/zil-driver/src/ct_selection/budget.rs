use super::*;

pub(super) struct Budget {
    pub remaining: Limits,
    initial_nodes: usize,
    initial_steps: u64,
}
impl Budget {
    pub fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.clone(),
            initial_nodes: limits.max_nodes,
            initial_steps: limits.max_ct_steps,
        }
    }
    pub fn spend(&mut self, nodes: usize, work: usize, span: Span) -> CompileResult<()> {
        self.remaining.max_nodes = self
            .remaining
            .max_nodes
            .checked_sub(nodes)
            .ok_or_else(|| limit(span, "selection node/copy budget exceeded"))?;
        self.remaining.max_analysis_steps = self
            .remaining
            .max_analysis_steps
            .checked_sub(work)
            .ok_or_else(|| limit(span, "selection work/copy budget exceeded"))?;
        Ok(())
    }
    pub fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.remaining.max_call_depth {
            return Err(limit(span, "selection nesting budget exceeded"));
        }
        self.spend(1, 1, span)
    }
    pub fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.spend(0, text.len(), span)
    }
    pub fn copy(&mut self, cost: SyntaxCost, span: Span) -> CompileResult<()> {
        self.spend(cost.nodes, cost.work, span)
    }
    pub fn execute(
        &mut self,
        usage: zil_ct::SourceEvaluationUsage,
        span: Span,
    ) -> CompileResult<()> {
        self.remaining.max_ct_steps = self
            .remaining
            .max_ct_steps
            .checked_sub(usage.steps)
            .ok_or_else(|| limit(span, "selection CT step budget exceeded"))?;
        self.spend(usage.nodes, 0, span)
    }
    pub fn usage(&self) -> zil_ct::SourceEvaluationUsage {
        zil_ct::SourceEvaluationUsage {
            steps: self.initial_steps - self.remaining.max_ct_steps,
            nodes: self.initial_nodes - self.remaining.max_nodes,
        }
    }
}
fn limit(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, message, Some(span))]
}
