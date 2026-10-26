//! Basic-block worklist ablation with the same bitset representation as the cut.
//! This is the relevant scheduler comparison, not a measured production replacement.
use crate::budget::Work;
use crate::graph::{Graph, MAX_WORK, R, Row};
use std::collections::VecDeque;
use zil_ir::{Terminator, TypedFunction};

pub fn solve(g: &Graph, f: &TypedFunction) -> R<(Vec<Row>, usize)> {
    let n = f.blocks.len();
    let words = g.slots.len().div_ceil(64);
    let mut work = Work::new(MAX_WORK, "block solver");
    work.charge(
        n.saturating_mul(words.saturating_mul(2).saturating_add(8))
            .saturating_add(g.nodes.len().saturating_mul(words.saturating_add(1))),
    )?;
    if words == 0 {
        return Ok((vec![vec![]; g.nodes.len()], work.used()));
    }
    let mut succ = vec![vec![]; n];
    let mut pred = vec![vec![]; n];
    for (b, block) in f.blocks.iter().enumerate() {
        succ[b] = match block.terminator {
            Terminator::Jump(t) => vec![t.0 as usize],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block.0 as usize, else_block.0 as usize],
            _ => vec![],
        };
        for &to in &succ[b] {
            pred[to].push(b);
        }
    }
    let mut entry = vec![vec![0; words]; n];
    let mut end = entry.clone();
    let mut queued = (0..n).map(|b| g.reachable[g.starts[b]]).collect::<Vec<_>>();
    let mut todo = (0..n).rev().filter(|&b| queued[b]).collect::<VecDeque<_>>();
    let transfer = |p: usize, row: &mut Row| {
        for &s in &g.nodes[p].kill {
            row[s / 64] &= !(1u64 << (s % 64));
        }
        for &s in &g.nodes[p].uses {
            row[s / 64] |= 1u64 << (s % 64);
        }
    };
    while let Some(b) = todo.pop_front() {
        work.charge(
            words
                .saturating_mul(succ[b].len().saturating_add(4))
                .saturating_add(1),
        )?;
        queued[b] = false;
        let mut live = vec![0; words];
        for &to in &succ[b] {
            for (x, y) in live.iter_mut().zip(&entry[to]) {
                *x |= *y;
            }
        }
        let terminal = g.starts[b] + f.blocks[b].instructions.len();
        work.charge(
            g.nodes[terminal]
                .uses
                .len()
                .saturating_add(g.nodes[terminal].kill.len()),
        )?;
        transfer(terminal, &mut live);
        end[b] = live.clone();
        for p in (g.starts[b]..terminal).rev() {
            work.charge(
                g.nodes[p]
                    .uses
                    .len()
                    .saturating_add(g.nodes[p].kill.len())
                    .saturating_add(1),
            )?;
            transfer(p, &mut live);
        }
        if live != entry[b] {
            entry[b] = live;
            work.charge(pred[b].len())?;
            for &from in &pred[b] {
                if g.reachable[g.starts[from]] && !queued[from] {
                    queued[from] = true;
                    todo.push_back(from);
                }
            }
        }
    }
    let mut rows = vec![vec![0; words]; g.nodes.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        if !g.reachable[g.starts[b]] {
            continue;
        }
        work.charge(words.saturating_mul(2))?;
        let mut row = end[b].clone();
        let terminal = g.starts[b] + block.instructions.len();
        rows[terminal] = row.clone();
        for p in (g.starts[b]..terminal).rev() {
            work.charge(
                words
                    .saturating_add(g.nodes[p].uses.len())
                    .saturating_add(g.nodes[p].kill.len())
                    .saturating_add(1),
            )?;
            transfer(p, &mut row);
            rows[p] = row.clone();
        }
    }
    Ok((rows, work.used()))
}
