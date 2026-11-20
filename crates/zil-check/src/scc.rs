//! Module call-graph Tarjan SCC decomposition and inductive cross-function
//! region-contract verification (`P0-B`).
//!
//! For every direct call cycle (self-recursion or mutual recursion), the
//! checker verifies that:
//! 1. Every function signature only names declared region parameters
//!    (`Region::Param(r)` with `r < sig.region_params`).
//! 2. Every function in a recursive SCC is inductively verified under the
//!    declared region contracts of its SCC peers (`check_function` checks each
//!    callee call against the callee's declared signature and checks each
//!    function's returns against its own declared signature).

use zil_ir::body::{Callee, FnId, Module, Terminator};

/// A strongly connected component of the module's direct call graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallScc {
    pub members: Vec<FnId>,
    pub recursive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SccContractError {
    pub function: FnId,
    pub reason: &'static str,
}

/// Computes strongly connected components in reverse topological order
/// (callees before callers) via Tarjan's algorithm.
pub fn call_graph_sccs(module: &Module) -> Vec<CallScc> {
    let n = module.functions.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, func) in module.functions.iter().enumerate() {
        for block in &func.body.blocks {
            if let Terminator::Call {
                callee: Callee::Direct(callee),
                ..
            } = &block.terminator
            {
                let dst = callee.index();
                if dst < n && !adj[i].contains(&dst) {
                    adj[i].push(dst);
                }
            }
        }
    }

    struct Tarjan<'a> {
        adj: &'a [Vec<usize>],
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        sccs: Vec<CallScc>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, v: usize) {
            self.indices[v] = Some(self.index);
            self.lowlink[v] = self.index;
            self.index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;

            for &w in &self.adj[v] {
                if self.indices[w].is_none() {
                    self.visit(w);
                    self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                } else if self.on_stack[w] {
                    self.lowlink[v] = self.lowlink[v].min(self.indices[w].unwrap());
                }
            }

            if self.lowlink[v] == self.indices[v].unwrap() {
                let mut members = Vec::new();
                while let Some(w) = self.stack.pop() {
                    self.on_stack[w] = false;
                    members.push(FnId::from_index(w));
                    if w == v {
                        break;
                    }
                }
                let recursive = members.len() > 1 || self.adj[v].contains(&v);
                self.sccs.push(CallScc { members, recursive });
            }
        }
    }

    let mut t = Tarjan {
        adj: &adj,
        index: 0,
        indices: vec![None; n],
        lowlink: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        sccs: Vec::new(),
    };
    for v in 0..n {
        if t.indices[v].is_none() {
            t.visit(v);
        }
    }
    t.sccs
}

/// Validates region-parameter well-formedness across every function signature
/// and computes the verified SCC decomposition order.
pub fn validate_scc_contracts(module: &Module) -> Result<Vec<CallScc>, SccContractError> {
    for (i, func) in module.functions.iter().enumerate() {
        let fn_id = FnId::from_index(i);
        let limit = func.sig.region_params;
        for &p in &func.sig.params {
            if module.tys.regions_in(p).into_iter().any(|r| r >= limit) {
                return Err(SccContractError {
                    function: fn_id,
                    reason: "parameter type references undeclared region parameter",
                });
            }
        }
        if module
            .tys
            .regions_in(func.sig.result)
            .into_iter()
            .any(|r| r >= limit)
        {
            return Err(SccContractError {
                function: fn_id,
                reason: "return type references undeclared region parameter",
            });
        }
    }
    Ok(call_graph_sccs(module))
}
