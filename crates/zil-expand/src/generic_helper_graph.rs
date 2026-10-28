//! One finite helper dependency check after the existing symbolic body pass.
//! Edges are recorded only after lexical resolution recognizes a direct helper
//! call; shadowing and a coincidentally equal spelling do not supply edges.
use super::*;

#[derive(Default)]
pub(super) struct Dependencies {
    nodes: BTreeMap<String, (Span, Vec<String>)>,
}

fn charge(count: usize, span: Span, budget: &mut Budget<'_>) -> R<()> {
    for _ in 0..count {
        budget.node(span, 0)?;
    }
    Ok(())
}

fn lookup(name: &str, count: usize, span: Span, budget: &mut Budget<'_>) -> R<()> {
    for _ in 0..count.checked_ilog2().map_or(1, |n| n as usize + 2) {
        budget.node(span, 0)?;
        budget.text(name, span)?;
    }
    Ok(())
}

impl Dependencies {
    pub(super) fn register(&mut self, name: &str, span: Span, budget: &mut Budget<'_>) -> R<()> {
        lookup(name, self.nodes.len(), span, budget)?;
        if !self.nodes.contains_key(name) {
            charge(2, span, budget)?;
            budget.text(name, span)?;
            self.nodes.insert(name.to_owned(), (span, Vec::new()));
        }
        Ok(())
    }

    pub(super) fn edge(
        &mut self,
        caller: &str,
        callee: &str,
        span: Span,
        budget: &mut Budget<'_>,
    ) -> R<()> {
        self.register(callee, span, budget)?;
        lookup(caller, self.nodes.len(), span, budget)?;
        charge(1, span, budget)?;
        budget.text(callee, span)?;
        self.nodes
            .get_mut(caller)
            .expect("caller registered before its body is checked")
            .1
            .push(callee.to_owned());
        Ok(())
    }

    pub(super) fn check(self, budget: &mut Budget<'_>) -> R<()> {
        let Some((_, (span, _))) = self.nodes.first_key_value() else {
            return Ok(());
        };
        let span = *span;
        let count = self.nodes.len();
        charge(count.saturating_mul(4), span, budget)?;
        let mut indices = BTreeMap::new();
        for (index, name) in self.nodes.keys().enumerate() {
            lookup(name, indices.len(), span, budget)?;
            indices.insert(name.as_str(), index);
        }
        let mut degree = vec![0usize; count];
        let mut users = vec![Vec::new(); count];
        for (caller, (_, edges)) in self.nodes.values().enumerate() {
            for callee in edges {
                lookup(callee, count, span, budget)?;
                charge(2, span, budget)?;
                degree[caller] += 1;
                users[indices[callee.as_str()]].push(caller);
            }
        }
        let mut ready: VecDeque<_> = degree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree == 0).then_some(index))
            .collect();
        let mut visited = 0usize;
        while let Some(callee) = ready.pop_front() {
            charge(users[callee].len().saturating_add(1), span, budget)?;
            visited += 1;
            for &caller in &users[callee] {
                degree[caller] -= 1;
                if degree[caller] == 0 {
                    ready.push_back(caller);
                }
            }
        }
        if visited != count {
            return Err(unsupported(
                span,
                "recursive callable helper dependency requires an explicit finite contract",
            ));
        }
        Ok(())
    }
}
