//! Checker-owned, bounded, process-local successful body inputs. This object
//! cannot be decoded from cache bytes or supply a Builder schedule.
use super::*;
use std::hash::{Hash, Hasher};
const MAX_RETAINED: usize = 16 * 1024 * 1024;
// Count the complete derived structural encoding without producing bytes.
// The per-write allowance conservatively covers enum/collection allocation.
struct RetainedSize(usize);
impl Hasher for RetainedSize {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = self.0.saturating_add(bytes.len()).saturating_add(64);
    }
    fn finish(&self) -> u64 {
        self.0 as u64
    }
}

#[derive(Clone)]
enum SymbolInput {
    Function(Signature),
    Global,
}
impl SymbolInput {
    fn from(item: &Item) -> Option<Self> {
        match item {
            Item::Function(f) => Some(Self::Function(f.signature.clone())),
            Item::Global(_) => Some(Self::Global),
            Item::NamedType(..) => None,
        }
    }
    fn matches(&self, item: &Item) -> bool {
        match (self, item) {
            (Self::Function(old), Item::Function(current)) => old.same_abi(&current.signature),
            (Self::Global, Item::Global(_)) => true,
            _ => false,
        }
    }
}
struct Checked {
    function: Function,
    symbols: Vec<(String, SymbolInput)>,
}

/// Reuse successful machine body checks while all actual inputs remain equal.
/// Whole-module names, declarations, globals, function headers and their error
/// order are checked on every invocation. Function bodies retain strong owners;
/// COW edits miss. Named layouts use the ordinary traversal. Every observed
/// symbol binding must match, including complete callee signatures. No field is caller-writable and
/// there is no serialized import or constructor for a successful body.
#[derive(Default)]
pub struct PreflightSession {
    triple: String,
    bodies: HashMap<usize, Vec<Checked>>,
    cost: usize,
    checked: usize,
    reused: usize,
}
impl PreflightSession {
    pub fn validate(&mut self, module: &Module) -> std::result::Result<(), crate::Error> {
        let (triple, items) = inputs(module);
        validate_inputs_cached::<false>(triple, items, Some(self))
            .map(|_| ())
            .map_err(|message| crate::Error {
                kind: crate::ErrorKind::Verify,
                message,
            })
    }
    /// Actual successful body checks and body reuses, excluding header checks.
    pub fn body_counts(&self) -> (usize, usize) {
        (self.checked, self.reused)
    }

    pub(super) fn prepare(&mut self, triple: &str, world: &World<'_>) -> bool {
        // Named layouts keep the complete ordinary traversal. In particular,
        // never clone an as-yet-unvalidated recursive raw layout into a cache.
        if !world.named.is_empty() {
            self.bodies.clear();
            self.cost = 0;
            return false;
        }
        if self.triple != triple {
            self.bodies.clear();
            self.cost = 0;
            self.triple = triple.to_owned();
        }
        true
    }
    pub(super) fn reuses(&mut self, function: &Function, world: &World<'_>) -> bool {
        let Some(blocks) = &function.blocks else {
            return false;
        };
        let key = std::ptr::from_ref(&**blocks) as usize;
        let found = self.bodies.get(&key).is_some_and(|entries| {
            entries.iter().any(|old| {
                old.function.signature == function.signature
                    && old
                        .function
                        .blocks
                        .as_ref()
                        .is_some_and(|b| b.ptr_eq(blocks))
                    && old.symbols.iter().all(|(name, input)| {
                        world
                            .symbols
                            .get(name.as_str())
                            .is_some_and(|item| input.matches(item))
                    })
            })
        });
        self.reused += usize::from(found);
        found
    }
    pub(super) fn capture<'a>(
        &mut self,
        function: &'a Function,
        world: &World<'a>,
    ) -> Option<PendingBody<'a>> {
        self.checked += 1;
        let reads = world.reads.borrow();
        let reads = reads.as_ref()?;
        // Borrow only. Even a later declaration may still be malformed; no
        // recursive type/body copying happens until the WHOLE module succeeds.
        Some(PendingBody {
            function,
            symbols: reads.iter().copied().collect(),
        })
    }
    pub(super) fn commit(&mut self, pending: Vec<PendingBody<'_>>, world: &World<'_>) {
        for pending in pending {
            if self.bodies.len() >= 4096 {
                break;
            }
            let mut size = RetainedSize(0);
            pending.function.hash(&mut size);
            for name in &pending.symbols {
                name.hash(&mut size);
                if let Some(Item::Function(f)) = world.symbols.get(name) {
                    f.signature.hash(&mut size);
                }
            }
            let cost = size.0;
            if self.cost.checked_add(cost).is_none_or(|n| n > MAX_RETAINED) {
                continue;
            }
            let key = std::ptr::from_ref(&**pending.function.blocks.as_ref().unwrap()) as usize;
            let bucket = self.bodies.entry(key).or_default();
            if bucket.len() >= 16 {
                continue;
            }
            let symbols = pending
                .symbols
                .into_iter()
                .map(|name| {
                    Some((
                        name.to_owned(),
                        SymbolInput::from(world.symbols.get(name)?)?,
                    ))
                })
                .collect::<Option<Vec<_>>>();
            let Some(symbols) = symbols else { continue };
            self.cost += cost;
            bucket.push(Checked {
                function: pending.function.clone(),
                symbols,
            });
        }
    }
}
pub(super) struct PendingBody<'a> {
    function: &'a Function,
    symbols: Vec<&'a str>,
}
impl PendingBody<'_> {
    pub(super) fn cost(&self) -> usize {
        self.symbols.len().saturating_add(1)
    }
}

#[cfg(test)]
#[path = "preflight_session_tests.rs"]
mod tests;
