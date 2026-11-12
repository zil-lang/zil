//! Source relations are checked from executable definitions and the latest
//! reachable stores. Public positions are only upper bounds after this proof.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

type Path = Vec<ContractProjection>;
type Sources = BTreeSet<StoragePosition>;
#[derive(Clone)]
pub(crate) struct Leaf<'a> {
    pub position: StoragePosition,
    pub ty: &'a Type,
}

fn invalid(message: &str) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-SOURCE", None, message)
}
fn field<'a>(
    module: &'a TypedModule,
    ty: &'a Type,
    selector: ContractProjection,
) -> Option<&'a Type> {
    let ty = match ty {
        Type::BorrowedStorage { storage, .. } => storage.as_ref(),
        _ => ty,
    };
    match (ty, selector) {
        (Type::Closure(id), ContractProjection::Capture(index)) => module
            .closures
            .get(id.0 as usize)?
            .captures
            .get(index as usize),
        (Type::Tuple(fields), ContractProjection::Place(PlaceProjection::Field(index))) => {
            fields.get(index as usize)
        }
        (
            Type::Record { fields, .. } | Type::Package { fields, .. },
            ContractProjection::Place(PlaceProjection::Field(index)),
        ) => fields.get(index as usize).map(|(_, t)| t),
        (
            Type::Array { element, length },
            ContractProjection::Place(PlaceProjection::Field(index)),
        ) if index < *length => Some(element),
        (Type::Enum { variants, .. }, ContractProjection::Place(PlaceProjection::Variant(p))) => {
            variants.get(p.variant as usize)?.1.get(p.index as usize)
        }
        (
            Type::ErrorUnion { error, result },
            ContractProjection::Place(PlaceProjection::ErrorArm(is_error)),
        ) => Some(if is_error { error } else { result }),
        _ => None,
    }
}
fn bound_children(pending: usize, leaves: usize, children: usize, limits: &Limits) -> Check<()> {
    if pending.saturating_add(leaves).saturating_add(children) > limits.max_nodes {
        Err(limit("callable source leaf graph exceeds node bound"))
    } else {
        Ok(())
    }
}
pub(crate) fn capture_leaves<'a>(
    module: &'a TypedModule,
    closure: ClosureId,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<Vec<Leaf<'a>>> {
    let definition = closures::definition(module, closure, None)?;
    let mut leaves = Vec::new();
    let mut pending = Vec::new();
    bound_children(0, 0, definition.captures.len(), limits)?;
    budget.spend(definition.captures.len())?;
    for (index, ty) in definition.captures.iter().enumerate() {
        pending.push((StorageBase::Capture(index as u32), Vec::new(), ty));
    }
    while let Some((base, path, ty)) = pending.pop() {
        budget.spend(path.len().saturating_add(1))?;
        if path.len() > limits.max_call_depth
            || leaves.len().saturating_add(pending.len()) > limits.max_nodes
        {
            return Err(limit("callable source leaf graph exceeds bounds"));
        }
        let ty = match ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            _ => ty,
        };
        match ty {
            Type::Address { .. } | Type::Desc(_) => leaves.push(Leaf {
                position: StoragePosition { base, path },
                ty,
            }),
            Type::Closure(id) => {
                let child = closures::definition(module, *id, None)?;
                bound_children(pending.len(), leaves.len(), child.captures.len(), limits)?;
                budget.spend(
                    child
                        .captures
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (index, ty) in child.captures.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(ContractProjection::Capture(index as u32));
                    pending.push((base, child_path, ty));
                }
            }
            Type::Tuple(fields) => {
                bound_children(pending.len(), leaves.len(), fields.len(), limits)?;
                budget.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (i, ty) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(i as u32)));
                    pending.push((base, p, ty));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                bound_children(pending.len(), leaves.len(), fields.len(), limits)?;
                budget.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (i, (_, ty)) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(i as u32)));
                    pending.push((base, p, ty));
                }
            }
            Type::Array { element, length } => {
                bound_children(pending.len(), leaves.len(), *length as usize, limits)?;
                budget.spend((*length as usize).saturating_mul(path.len().saturating_add(2)))?;
                for i in 0..*length {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(i)));
                    pending.push((base, p, element));
                }
            }
            Type::Enum { variants, .. } => {
                for (v, (_, fields)) in variants.iter().enumerate() {
                    bound_children(pending.len(), leaves.len(), fields.len(), limits)?;
                    budget.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                    for (i, ty) in fields.iter().enumerate() {
                        let mut p = path.clone();
                        p.push(ContractProjection::Place(PlaceProjection::Variant(
                            VariantProjection {
                                variant: v as u32,
                                index: i as u32,
                            },
                        )));
                        pending.push((base, p, ty));
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                bound_children(pending.len(), leaves.len(), 2, limits)?;
                budget.spend(2usize.saturating_mul(path.len().saturating_add(2)))?;
                for (is_error, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::ErrorArm(
                        is_error,
                    )));
                    pending.push((base, p, ty));
                }
            }
            Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{retained_life,..} if !retained_life.ports.is_empty()) =>
            {
                return Err(unsupported(
                    "returned environments with retained Task ports require actual port transport",
                    None,
                ));
            }
            Type::OwnedDyn { interface } => {
                // An erased owning shell cannot hide an external source leaf.
                dynamic_dispatch::closed_owned(module, *interface, budget)?;
            }
            _ => {}
        }
    }
    budget.spend(leaves.len().saturating_mul(leaves.len().saturating_add(1)))?;
    leaves.sort_by(|a, b| a.position.cmp(&b.position));
    Ok(leaves)
}
fn equal(a: &Type, b: &Type, budget: &mut Budget) -> Check<bool> {
    let n = closures::type_layout_work(a, budget)?
        .saturating_add(closures::type_layout_work(b, budget)?);
    budget.spend(n)?;
    Ok(a == b)
}
fn compatible(from: &Type, to: &Type, budget: &mut Budget) -> Check<bool> {
    if equal(from, to, budget)? {
        return Ok(true);
    }
    match (from, to) {
        (
            Type::Address {
                mutable: fm,
                inner: a,
            },
            Type::Address {
                mutable: tm,
                inner: b,
            },
        ) if !*tm || *fm => equal(a, b, budget),
        _ => Ok(false),
    }
}
/// A physical source selector may select a stored pointer value or the address
/// of a field; their endpoint types keep those two operations distinct.
pub(crate) fn source_type<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    source: &StoragePosition,
    budget: &mut Budget,
) -> Check<(&'a Type, bool)> {
    let StorageBase::Parameter(index) = source.base else {
        return Err(invalid("callable source must name an actual parameter"));
    };
    let local = f
        .parameters
        .get(index as usize)
        .ok_or_else(|| invalid("callable source parameter is absent"))?;
    let mut ty = &f.locals[local.0 as usize].ty;
    let mut projected_address = false;
    for selector in &source.path {
        budget.spend(1)?;
        if let Type::Address { inner, .. } = ty {
            ty = inner;
            projected_address = true;
        }
        ty = field(module, ty, *selector).ok_or_else(|| {
            invalid("callable source projection does not match its actual input type")
        })?;
    }
    Ok((ty, projected_address))
}
pub(super) fn claim(
    module: &TypedModule,
    f: &TypedFunction,
    leaves: &[Leaf<'_>],
    budget: &mut Budget,
) -> Check<()> {
    let c = f
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| invalid("missing callable source claim"))?;
    let bindings = c
        .capture_sources
        .as_ref()
        .ok_or_else(|| invalid("callable sources are unknown"))?;
    for (i, binding) in bindings.iter().enumerate() {
        budget.spend(
            leaves
                .len()
                .saturating_add(binding.sources.len())
                .saturating_add(i + 1),
        )?;
        if bindings[..i].iter().any(|b| b.position == binding.position) {
            return Err(invalid("duplicate callable source destination"));
        }
        let leaf = leaves
            .iter()
            .find(|l| l.position == binding.position)
            .ok_or_else(|| {
                invalid("callable source destination is not an actual external capture leaf")
            })?;
        let mut seen = BTreeSet::new();
        for atom in &binding.sources {
            let source=crate::exact_storage_source(atom)?;
            budget.spend(
                source
                    .path
                    .len()
                    .saturating_add(seen.len())
                    .saturating_add(1),
            )?;
            if !seen.insert(source) {
                return Err(invalid("duplicate callable input source"));
            }
            let (ty, projected) = source_type(module, f, source, budget)?;
            if compatible(ty, leaf.ty, budget)? {
                continue;
            }
            if projected
                && let Type::Address { inner, .. } = leaf.ty
                && equal(ty, inner, budget)?
            {
                continue;
            }
            return Err(invalid(
                "callable source and capture leaf have incompatible endpoint types",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Query {
    Value(FunctionId, ValueId, Path),
    Local(FunctionId, LocalId, usize, usize, Path),
}
struct Trace<'a, 'b> {
    module: &'a TypedModule,
    graphs: &'a [FunctionGraph],
    definitions: &'a [Vec<Option<(usize, usize)>>],
    expected: &'a Type,
    limits: &'a Limits,
    budget: &'b mut Budget,
    active: BTreeSet<Query>,
    memo: BTreeMap<Query, Option<Sources>>,
    cycles: usize,
}
impl Trace<'_, '_> {
    fn query(&mut self, q: Query, depth: usize) -> Check<Option<Sources>> {
        let path = match &q {
            Query::Value(.., path) | Query::Local(.., path) => path,
        };
        self.budget.spend(
            self.active
                .len()
                .saturating_add(self.memo.len())
                .saturating_add(3)
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if depth > self.limits.max_call_depth {
            return Err(limit("callable source trace depth exceeded"));
        }
        if let Some(v) = self.memo.get(&q) {
            self.budget.spend(v.as_ref().map_or(0, |set| {
                set.iter().fold(0usize, |n, source| {
                    n.saturating_add(source.path.len().saturating_add(1))
                })
            }))?;
            return Ok(v.clone());
        }
        if !self.active.insert(q.clone()) {
            self.cycles = self.cycles.saturating_add(1);
            return Ok(Some(Sources::new()));
        }
        let cycles = self.cycles;
        let result = match &q {
            Query::Value(fid, value, path) => self.value(*fid, *value, path, depth + 1),
            Query::Local(fid, local, block, end, path) => {
                self.local(*fid, *local, *block, *end, path, depth + 1)
            }
        }?;
        self.active.remove(&q);
        self.budget.spend(result.as_ref().map_or(0, |set| {
            set.iter().fold(1usize, |n, source| {
                n.saturating_add(source.path.len().saturating_add(1))
            })
        }))?;
        if self.cycles == cycles {
            let width = match &q {
                Query::Value(.., path) | Query::Local(.., path) => path.len().saturating_add(1),
            };
            self.budget
                .spend(self.memo.len().saturating_add(1).saturating_mul(width))?;
            self.memo.insert(q, result.clone());
        }
        Ok(result)
    }
    fn join(&mut self, to: &mut Sources, from: Option<Sources>) -> Check<bool> {
        let Some(from) = from else { return Ok(false) };
        self.budget.spend(from.len().saturating_add(to.len()))?;
        let from_width = from.iter().fold(0usize, |n, source| {
            n.saturating_add(source.path.len().saturating_add(1))
        });
        let to_width = to.iter().fold(0usize, |n, source| {
            n.saturating_add(source.path.len().saturating_add(1))
        });
        self.budget.spend(
            from_width
                .saturating_mul(to.len().saturating_add(1))
                .saturating_add(to_width.saturating_mul(from.len())),
        )?;
        to.extend(from);
        Ok(true)
    }
    fn input(
        &mut self,
        f: &TypedFunction,
        local: LocalId,
        path: &[ContractProjection],
    ) -> Check<Option<Sources>> {
        self.budget.spend(
            path.len()
                .saturating_add(f.parameters.len())
                .saturating_add(1),
        )?;
        let Some(index) = f.parameters.iter().position(|p| *p == local) else {
            return Ok(None);
        };
        let source = StoragePosition {
            base: StorageBase::Parameter(index as u32),
            path: path.to_vec(),
        };
        let (ty, projected) = source_type(self.module, f, &source, self.budget)?;
        let mut valid = compatible(ty, self.expected, self.budget)?;
        if projected && let Type::Address { inner, .. } = self.expected {
            valid |= equal(ty, inner, self.budget)?;
        }
        if valid {
            Ok(Some(Sources::from([source])))
        } else {
            Ok(None)
        }
    }
    fn local(
        &mut self,
        fid: FunctionId,
        local: LocalId,
        b: usize,
        end: usize,
        path: &[ContractProjection],
        depth: usize,
    ) -> Check<Option<Sources>> {
        let f = &self.module.functions[fid.0 as usize];
        for i in (0..end).rev() {
            self.budget.spend(path.len().saturating_add(1))?;
            match &f.blocks[b].instructions[i].kind {
                InstructionKind::Store { local: l, value } if *l == local => {
                    return self.query(Query::Value(fid, *value, path.to_vec()), depth);
                }
                InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l)
                    if *l == local =>
                {
                    return Ok(None);
                }
                _ => {}
            }
        }
        if b == f.entry.0 as usize {
            return self.input(f, local, path);
        }
        let mut out = Sources::new();
        let mut count = 0;
        for p in &self.graphs[fid.0 as usize].predecessors[b] {
            if self.graphs[fid.0 as usize].reachable[*p] {
                count += 1;
                let n = f.blocks[*p].instructions.len();
                self.budget.spend(path.len().saturating_add(1))?;
                let part = self.query(Query::Local(fid, local, *p, n, path.to_vec()), depth)?;
                if !self.join(&mut out, part)? {
                    return Ok(None);
                }
            }
        }
        Ok((count > 0).then_some(out))
    }
    fn value(
        &mut self,
        fid: FunctionId,
        value: ValueId,
        path: &[ContractProjection],
        depth: usize,
    ) -> Check<Option<Sources>> {
        let f = &self.module.functions[fid.0 as usize];
        self.budget.spend(1)?;
        let Some((block, index)) = self.definitions[fid.0 as usize][value.0 as usize] else {
            return Ok(None);
        };
        let kind = &f.blocks[block].instructions[index].kind;
        self.budget.spend(path.len().saturating_add(2))?;
        match kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => self.query(
                Query::Local(fid, *local, block, index, path.to_vec()),
                depth,
            ),
            InstructionKind::MakeClosure { captures, .. } => {
                let Some((ContractProjection::Capture(capture), tail)) = path.split_first() else {
                    return Ok(None);
                };
                self.query(
                    Query::Value(fid, captures[*capture as usize], tail.to_vec()),
                    depth,
                )
            }
            InstructionKind::MakeAggregate { fields, .. }
            | InstructionKind::MakePackage { fields, .. } => {
                let Some((ContractProjection::Place(PlaceProjection::Field(field)), tail)) =
                    path.split_first()
                else {
                    return Ok(None);
                };
                self.query(
                    Query::Value(fid, fields[*field as usize], tail.to_vec()),
                    depth,
                )
            }
            InstructionKind::ExtractField { aggregate, index } => {
                let mut p = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
                p.extend_from_slice(path);
                self.query(Query::Value(fid, *aggregate, p), depth)
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value,
            } => {
                if matches!(path.first(),Some(ContractProjection::Place(PlaceProjection::Field(i)))if i==index)
                {
                    self.query(Query::Value(fid, *value, path[1..].to_vec()), depth)
                } else {
                    self.query(Query::Value(fid, *aggregate, path.to_vec()), depth)
                }
            }
            InstructionKind::ReborrowAddress { address, .. }
            | InstructionKind::DescribeAddress { address } => {
                self.query(Query::Value(fid, *address, path.to_vec()), depth)
            }
            InstructionKind::AddressField { address, index } => {
                let mut p = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
                p.extend_from_slice(path);
                self.query(Query::Value(fid, *address, p), depth)
            }
            InstructionKind::BorrowPlaceAddress {
                address,
                path: projection,
                ..
            } => {
                self.budget.spend(
                    projection
                        .len()
                        .saturating_add(path.len())
                        .saturating_add(1),
                )?;
                let mut p = projection
                    .iter()
                    .map(|p| ContractProjection::Place(*p))
                    .collect::<Vec<_>>();
                p.extend_from_slice(path);
                self.query(Query::Value(fid, *address, p), depth)
            }
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                let selected = path
                    .iter()
                    .try_fold(&f.locals[local.0 as usize].ty, |ty, p| {
                        field(self.module, ty, *p)
                    });
                let stored = if let Some(ty) = selected {
                    equal(ty, self.expected, self.budget)?
                } else {
                    false
                };
                if stored {
                    self.query(
                        Query::Local(fid, *local, block, index, path.to_vec()),
                        depth,
                    )
                } else {
                    Ok(None)
                }
            }
            InstructionKind::LoadStoredBorrow {
                storage,
                path: projection,
                ..
            } => {
                self.budget.spend(
                    projection
                        .len()
                        .saturating_add(path.len())
                        .saturating_add(1),
                )?;
                let mut p = projection
                    .iter()
                    .map(|p| ContractProjection::Place(*p))
                    .collect::<Vec<_>>();
                p.extend_from_slice(path);
                self.query(Query::Value(fid, *storage, p), depth)
            }
            InstructionKind::ReadAddress { address } => {
                self.query(Query::Value(fid, *address, path.to_vec()), depth)
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                if self.active.iter().any(|query| match query {
                    Query::Value(id, ..) | Query::Local(id, ..) => id == function,
                }) {
                    return Err(unsupported(
                        "recursive callable source substitution needs a finite proven source relation",
                        Some(f.span),
                    ));
                }
                let callee = &self.module.functions[function.0 as usize];
                let mut relative = Sources::new();
                let mut returns = 0;
                for (b, block) in callee.blocks.iter().enumerate() {
                    if self.graphs[function.0 as usize].reachable[b]
                        && let Terminator::Return(Some(value)) = block.terminator
                    {
                        returns += 1;
                        self.budget.spend(path.len().saturating_add(1))?;
                        let part =
                            self.query(Query::Value(*function, value, path.to_vec()), depth)?;
                        if !self.join(&mut relative, part)? {
                            return Ok(None);
                        }
                    }
                }
                if returns == 0 {
                    return Ok(None);
                }
                let mut out = Sources::new();
                for source in relative {
                    let StorageBase::Parameter(parameter) = source.base else {
                        return Ok(None);
                    };
                    let actual = arguments[parameter as usize];
                    let part = self.query(Query::Value(fid, actual, source.path), depth)?;
                    if !self.join(&mut out, part)? {
                        return Ok(None);
                    }
                }
                Ok(Some(out))
            }
            InstructionKind::StaticUtf8 { .. } => Ok(Some(Sources::new())),
            _ => Ok(None),
        }
    }
}
pub(super) fn check_bodies(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if budget.callable_source_functions.is_empty() {
        return Ok(());
    }
    budget.spend(budget.callable_source_functions.len())?;
    let functions = budget.callable_source_functions.clone();
    let mut selected = Vec::new();
    for id in functions {
        let f = &module.functions[id.0 as usize];
        if let Some(c) = &f.callable_return_contract {
            let leaves = capture_leaves(module, c.closure, limits, budget)?;
            if !leaves.is_empty() {
                selected.push((f, leaves));
            }
        }
    }
    if selected.is_empty() {
        return Ok(());
    }
    budget.spend(module.functions.len())?;
    let mut definitions = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        budget.spend(function.values.len().saturating_add(function.blocks.len()))?;
        let mut row = vec![None; function.values.len()];
        for (b, block) in function.blocks.iter().enumerate() {
            budget.spend(block.instructions.len())?;
            if graphs[function.id.0 as usize].reachable[b] {
                for (i, instruction) in block.instructions.iter().enumerate() {
                    if let Some(value) = instruction.result {
                        row[value.0 as usize] = Some((b, i));
                    }
                }
            }
        }
        definitions.push(row);
    }
    for (f, leaves) in selected {
        let c = f
            .callable_return_contract
            .as_ref()
            .expect("selected source function");
        claim(module, f, &leaves, budget)?;
        if budget.stored_borrows {
            budget.spend(1)?;
            budget.pending_callable_sources.push(f.id);
            continue;
        }
        for leaf in leaves {
            let StorageBase::Capture(index) = leaf.position.base else {
                return Err(invalid("external source destination is not a capture"));
            };
            budget.spend(leaf.position.path.len().saturating_add(1))?;
            let mut path = vec![ContractProjection::Capture(index)];
            path.extend_from_slice(&leaf.position.path);
            let mut trace = Trace {
                module,
                graphs,
                definitions: &definitions,
                expected: leaf.ty,
                limits,
                budget,
                active: BTreeSet::new(),
                memo: BTreeMap::new(),
                cycles: 0,
            };
            for (b, block) in f.blocks.iter().enumerate() {
                if graphs[f.id.0 as usize].reachable[b]
                    && let Terminator::Return(Some(value)) = block.terminator
                {
                    trace.budget.spend(path.len().saturating_add(1))?;
                    let actual=trace.query(Query::Value(f.id,value,path.clone()),0)?.ok_or_else(||invalid("returned environment leaf depends on local, absent or unknown storage"))?;
                    let declared = c
                        .capture_sources
                        .as_ref()
                        .and_then(|bs| bs.iter().find(|b| b.position == leaf.position));
                    trace.budget.spend(actual.len().saturating_mul(
                        declared.map_or(1, |b| b.sources.len().saturating_add(1)),
                    ))?;
                    for source in &actual {
                        let present = match declared {
                            Some(binding) => crate::exact_storage_sources_contain(&binding.sources,source)?,
                            None => false,
                        };
                        if present { continue; }
                        return Err(invalid(
                            "callable source claim omits an actual reachable input source",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn envelope(
    module: &TypedModule,
    f: &TypedFunction,
    id: ClosureId,
    leaves: &[Leaf<'_>],
    budget: &mut Budget,
) -> Check<()> {
    borrowed_envelope(module, f, id, leaves, false, budget)
}

pub(super) fn parameter_envelope(
    module: &TypedModule,
    f: &TypedFunction,
    id: ClosureId,
    leaves: &[Leaf<'_>],
    budget: &mut Budget,
) -> Check<()> {
    borrowed_envelope(module, f, id, leaves, true, budget)
}

fn borrowed_envelope(
    module: &TypedModule,
    f: &TypedFunction,
    id: ClosureId,
    leaves: &[Leaf<'_>],
    parameter: bool,
    budget: &mut Budget,
) -> Check<()> {
    let c = f
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| invalid("missing callable result contract"))?;
    let definition = closures::definition(module, id, Some(f.span))?;
    if f.foreign.is_some()
        || f.environment.is_some()
        || (!parameter && f.callable_contract.is_some())
        || f.result_mode != PassingMode::Infer
        || c.closure != id
        || if parameter {
            !closures::supports(definition.kind, c.kind)
        } else {
            c.kind != definition.kind
        }
        || c.lending_captures != definition.lending_captures
    {
        return Err(invalid(
            "borrowed callable factory boundary differs from its actual definition",
        ));
    }
    if !parameter {
        let mut input_closure = None;
        for local in &f.parameters {
            budget.spend(1)?;
            if let Type::Closure(input) = f.locals[local.0 as usize].ty
                && (input == id || input_closure.replace(input).is_some())
            {
                return Err(invalid(
                    "fresh callable factory requires one distinct grounded input environment",
                ));
            }
        }
    }
    let (mut hold, mut call, mut writes) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for leaf in leaves {
        budget.spend(1)?;
        let StorageBase::Capture(index) = leaf.position.base else {
            return Err(invalid("factory external leaf has invalid base"));
        };
        call.insert(index);
        match leaf.ty {
            Type::Address { mutable, .. } => {
                hold.insert(index);
                if *mutable {
                    writes.insert(index);
                }
            }
            Type::Desc(_) => {
                writes.insert(index);
            }
            _ => return Err(invalid("factory external leaf kind is not supported")),
        }
    }
    if c.hold != hold.into_iter().collect::<Vec<_>>()
        || c.call != call.into_iter().collect::<Vec<_>>()
        || c.writes != writes.into_iter().collect::<Vec<_>>()
    {
        return Err(invalid(
            "callable Hold/Call/write envelope omits actual external capture slots",
        ));
    }
    let mut cleanup = Vec::new();
    for (index, ty) in definition.captures.iter().enumerate() {
        budget.spend(1)?;
        match ty {
            ty if packages::capture_owner(ty) => {
                packages::check_capture_type(module, ty, Some(f.span), budget)?;
                cleanup.push(index as u32);
            }
            Type::Managed(_) | Type::Closure(_) => cleanup.push(index as u32),
            Type::OwnedDyn { interface } => {
                dynamic_dispatch::closed_owned(module, *interface, budget)?;
                cleanup.push(index as u32);
            }
            ty if budget.inline_store_root(ty)?
                && dynamic_store::inline_sum_capture(ty, budget)? =>
            {
                dynamic_store::check_owned_capture(module, ty, None, budget)?;
                cleanup.push(index as u32);
            }
            Type::Address { .. } | Type::Desc(_) | Type::BorrowedStorage { .. } => {}
            ty if ty.is_copy_value() => {}
            _ => {
                return Err(unsupported(
                    "borrowed callable result capture has no checked ownership protocol",
                    Some(f.span),
                ));
            }
        }
    }
    if c.cleanup != cleanup {
        return Err(invalid(
            "callable cleanup differs from its actual owned capture fields",
        ));
    }
    claim(module, f, leaves, budget)
}
