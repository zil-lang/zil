//! Same-request raw projection sharing. No analysis outcome is cached here:
//! all candidate stages, admission, global obligations and fees stay with their
//! original consumers. A selector only locates an exhaustively equal input.
use super::*;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::Hasher;

struct Input<'a> {
    function: &'a TypedFunction,
    hash: bootstrap::InputHasher,
}

pub(super) struct Inputs<'a> {
    entries: Vec<Input<'a>>,
    owners: HashMap<usize, Vec<usize>>,
    selectors: HashMap<u64, Vec<usize>>,
    remaining: usize,
}

impl<'a> Inputs<'a> {
    pub(super) fn new(work: usize) -> Self {
        Self {
            entries: Vec::new(),
            owners: HashMap::new(),
            selectors: HashMap::new(),
            remaining: work.min(1_000_000),
        }
    }

    fn matches(&mut self, index: usize, function: &TypedFunction) -> bool {
        let old = self.entries[index].function;
        // Fuel bounds optional matching, not the ordinary projection or fees.
        let cost = if old.locals.ptr_eq(&function.locals)
            && old.values.ptr_eq(&function.values)
            && old.blocks.ptr_eq(&function.blocks)
        {
            function.parameters.len().saturating_add(1)
        } else {
            function.blocks.iter().fold(
                function
                    .locals
                    .len()
                    .saturating_add(function.values.len())
                    .saturating_add(function.parameters.len())
                    .saturating_add(1),
                |n, b| n.saturating_add(b.instructions.len()).saturating_add(1),
            )
        };
        let Some(left) = self.remaining.checked_sub(cost) else {
            self.remaining = 0;
            return false;
        };
        self.remaining = left;
        semantic_body_eq(old, function)
    }

    pub(super) fn input(
        &mut self,
        function: &'a TypedFunction,
    ) -> Option<((Fingerprint, Vec<FunctionId>, u64), bool)> {
        if self.remaining == 0 {
            let mut hash = bootstrap::InputHasher::new();
            let callees = scalar_projection(function, &mut hash)?;
            let (input, selector) = hash.finish_input(function);
            return Some(((input, callees, selector), false));
        }
        let owner = function.blocks.as_ptr() as usize;
        // The held immutable owner plus complete header/local/value comparison
        // avoids even rewalking the decoder's shared instruction payload.
        let owner_candidates = self.owners.get(&owner).cloned().unwrap_or_default();
        for index in owner_candidates {
            if self.entries[index].function.blocks.ptr_eq(&function.blocks)
                && self.matches(index, function)
            {
                let (input, selector) = self.entries[index].hash.finish_input(function);
                return Some(((input, Vec::new(), selector), true));
            }
            if self.remaining == 0 {
                break;
            }
        }
        let mut selector = DefaultHasher::new();
        let callees = scalar_projection(function, &mut selector)?;
        let selector = selector.finish();
        // Calls never share projection admission with another outer function.
        // Their signatures/dependencies still run through the existing path.
        if callees.is_empty() {
            let candidates = self.selectors.get(&selector).cloned().unwrap_or_default();
            for index in candidates {
                if self.matches(index, function) {
                    self.owners.entry(owner).or_default().push(index);
                    let (input, selector) = self.entries[index].hash.finish_input(function);
                    return Some(((input, Vec::new(), selector), true));
                }
                if self.remaining == 0 {
                    break;
                }
            }
        }
        let mut hash = bootstrap::InputHasher::new();
        let actual = scalar_projection(function, &mut hash)?;
        let (input, body_selector) = hash.finish_input(function);
        if actual.is_empty() && self.remaining > 0 && self.entries.len() < 16_384 {
            let index = self.entries.len();
            self.entries.push(Input { function, hash });
            self.owners.entry(owner).or_default().push(index);
            self.selectors.entry(selector).or_default().push(index);
        }
        Some(((input, actual, body_selector), false))
    }
}

#[derive(Default)]
pub(super) struct Outputs<'a> {
    bodies: HashMap<(usize, usize), Vec<(&'a FunctionPlan, FingerprintBuilder)>>,
    pub(super) reuses: usize,
}
impl<'a> Outputs<'a> {
    pub(super) fn fingerprint(&mut self, result: &'a FunctionPlan) -> Fingerprint {
        let key = (
            result.blocks.as_ptr() as usize,
            result.holds.as_ptr() as usize,
        );
        // Only successful completed candidate rows reach this publication path.
        // Require actual shared owners; outer FunctionId is always rebound.
        let found = self
            .bodies
            .get(&key)
            .and_then(|rows| {
                rows.iter().find(|(old, _)| {
                    old.blocks.ptr_eq(&result.blocks) && old.holds.ptr_eq(&result.holds)
                })
            })
            .map(|(_, hash)| hash.clone());
        let mut hash = if let Some(hash) = found {
            self.reuses += 1;
            hash
        } else {
            let hash = output_projection(result);
            if self.bodies.len() < 16_384 {
                self.bodies
                    .entry(key)
                    .or_default()
                    .push((result, hash.clone()));
            }
            hash
        };
        result.function.hash(&mut hash);
        hash.digest()
    }
}

#[cfg(test)]
#[path = "candidate_projection_tests.rs"]
mod tests;
