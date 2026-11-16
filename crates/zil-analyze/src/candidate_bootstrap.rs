//! Equal scalar evaluators in this request share genuinely completed stages.
//! A selector has no authority: complete current CFG equality is mandatory.
use super::*;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::Hasher;

#[derive(Clone)]
pub(super) struct InputHasher {
    input: FingerprintBuilder,
    body: DefaultHasher,
}
impl InputHasher {
    pub(super) fn new() -> Self {
        let mut input = FingerprintBuilder::new();
        "zil-analyze-scalar-input-v2".hash(&mut input);
        Self {
            input,
            body: DefaultHasher::new(),
        }
    }
    pub(super) fn finish_input(&self, function: &TypedFunction) -> (Fingerprint, u64) {
        // Put the per-function identity after the complete body so a current
        // equal-body projection can share the SHA state, never its final key.
        let mut input = self.input.clone();
        function.id.hash(&mut input);
        function.name.hash(&mut input);
        (input.digest(), self.body.finish())
    }
}
impl Hasher for InputHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.input.write(bytes);
        self.body.write(bytes);
    }
    fn finish(&self) -> u64 {
        self.body.finish()
    }
}

pub(super) struct Templates<'a> {
    buckets: HashMap<u64, Vec<&'a TypedFunction>>,
    remaining: usize,
    count: usize,
}
impl<'a> Templates<'a> {
    pub(super) fn new(work: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            remaining: work.min(1_000_000),
            count: 0,
        }
    }
    pub(super) fn len(&self) -> usize {
        self.count
    }
    pub(super) fn bind(
        &mut self,
        function: &'a TypedFunction,
        selector: u64,
    ) -> Option<FunctionId> {
        let locals = function.locals.iter().try_fold(0usize, |n, local| {
            n.checked_add(local.name.len().checked_add(1)?)
        })?;
        let cost = function.blocks.iter().try_fold(
            locals
                .checked_add(function.values.len())?
                .checked_add(function.parameters.len())?
                .checked_add(1)?,
            |n, block| n.checked_add(block.instructions.len().checked_add(1)?),
        )?;
        self.remaining = self.remaining.checked_sub(cost)?;
        let bucket = self.buckets.entry(selector).or_default();
        for previous in bucket.iter() {
            self.remaining = self.remaining.checked_sub(cost)?;
            if semantic_body_eq(previous, function) {
                return Some(previous.id);
            }
        }
        bucket.push(function);
        self.count += 1;
        None
    }
}

#[cfg(test)]
#[path = "candidate_bootstrap_tests.rs"]
mod tests;
