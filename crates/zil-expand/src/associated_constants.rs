//! Request-local locations for verified CT associated equations. A plan is
//! untrusted source bookkeeping, never an impl witness or a serialized fact.
use zil_base::{CompileResult, Limits};
use zil_syntax::ast::{Module, TypeRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Source {
    pub item: usize,
    pub member: usize,
}
#[derive(Debug)]
pub struct Equation {
    pub source: Source,
    pub trait_item: usize,
    pub receiving: TypeRef,
    /// Actual declaration member ordinals, not names or constant-only indices.
    pub dependencies: Vec<usize>,
}
#[derive(Debug)]
pub struct Instance {
    pub item: usize,
    pub trait_item: usize,
    /// One source for every constant, in declaration member order.
    pub effective: Vec<Source>,
    pub default_checks: Vec<Source>,
}
#[derive(Debug)]
pub struct Plan {
    pub definitions: Vec<Equation>,
    pub instances: Vec<Instance>,
    pub remaining_limits: Limits,
}

/// Locations refer exclusively to this immutable module's complete item/member
/// vectors. The caller must retain it until all jobs finish, then update a clone.
pub fn prepare(module: &Module, target_pointer_width: u16, limits: &Limits) -> CompileResult<Plan> {
    crate::generics::constant_plan::prepare(module, target_pointer_width, limits)
}
