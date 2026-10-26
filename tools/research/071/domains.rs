//! A may-location prepass. It does not decide legality or enumerate states.
//! Original event operands still reach the independent candidate/oracle interpreters.
use crate::{
    budget::{R, Work},
    extract::{self, Layout},
};
use std::collections::{BTreeMap, VecDeque};
use zil_ir::{InstructionKind, PassingMode, TypedFunction, TypedModule};

#[derive(Debug)]
pub struct Domains {
    /// Owner indices, in original canonical order, for every token.
    pub owners: Vec<Vec<usize>>,
    pub raw_edges: usize,
}

pub fn compute(m: &TypedModule, f: &TypedFunction, l: &Layout, w: &mut Work) -> R<Domains> {
    w.charge(l.owners.iter().map(|s| s.len() + 2).sum())?;
    let index = l
        .owners
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let mut graph = vec![Vec::new(); l.owners.len()];
    let mut edges = 0;
    let mut edge = |src: String, dst: String, w: &mut Work| -> R<()> {
        w.charge(src.len() + dst.len() + 4)?;
        if let (Some(&a), Some(&b)) = (index.get(src.as_str()), index.get(dst.as_str())) {
            graph[a].push(b);
            edges += 1;
        }
        Ok(())
    };
    for block in &f.blocks {
        for ins in &block.instructions {
            w.charge(1)?;
            let dst = ins.result.map(|v| format!("V{}", v.0));
            match &ins.kind {
                InstructionKind::TakeLocal { local } | InstructionKind::Load(local) => {
                    if let Some(dst) = dst {
                        edge(format!("L{}", local.0), dst, w)?;
                    }
                }
                InstructionKind::Store { local, value } => {
                    edge(format!("V{}", value.0), format!("L{}", local.0), w)?
                }
                InstructionKind::TakeCapture { index } => {
                    if let Some(dst) = dst {
                        edge(format!("C{index}"), dst, w)?;
                    }
                }
                InstructionKind::MakeClosure { captures, .. } => {
                    if let Some(dst) = dst {
                        for value in captures {
                            // Count and visit every occurrence. No claim that
                            // the transport graph validates duplicate capture.
                            edge(format!("V{}", value.0), dst.clone(), w)?;
                        }
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } if m.functions[function.0 as usize].result_mode == PassingMode::Give => {
                    if let Some(dst) = dst {
                        for value in arguments {
                            w.charge(1)?;
                            if extract::owned(m, &f.values[value.0 as usize]) {
                                edge(format!("V{}", value.0), dst.clone(), w)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut domains = Vec::new();
    for token in &l.tokens {
        w.charge(token.len() + l.owners.len() + 2)?;
        let origin = if let Some(number) = token.strip_prefix('P') {
            let i = number
                .parse::<usize>()
                .map_err(|_| "InternalError: token parameter name")?;
            let local = f
                .parameters
                .get(i)
                .ok_or("InternalError: parameter token index")?;
            format!("L{}", local.0)
        } else {
            token.clone()
        };
        let start = *index
            .get(origin.as_str())
            .ok_or("InternalError: token origin absent")?;
        let mut seen = vec![false; l.owners.len()];
        seen[start] = true;
        let mut todo = VecDeque::from([start]);
        while let Some(owner) = todo.pop_front() {
            w.charge(1 + graph[owner].len())?;
            for &next in &graph[owner] {
                if !seen[next] {
                    seen[next] = true;
                    todo.push_back(next);
                }
            }
        }
        domains.push(
            seen.into_iter()
                .enumerate()
                .filter_map(|(i, yes)| yes.then_some(i))
                .collect(),
        );
    }
    // Include temporary graph/map release in the abstract budget too.
    w.charge(edges + l.owners.iter().map(|s| s.len() + 2).sum::<usize>())?;
    Ok(Domains {
        owners: domains,
        raw_edges: edges,
    })
}
