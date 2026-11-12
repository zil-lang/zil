//! Finite actual upper sources for dynamic selection. This is neither a meet
//! nor a substitute for stored targets, initialization, or loan permissions.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;

type Site = (usize, usize);
#[derive(Clone, Copy)]
struct Boundary {
    site: Site,
    actual: usize,
    expected: usize,
}
#[derive(Clone, Copy)]
enum Consumer {
    Copy,
    Observe,
    Bound,
    Forbidden,
}
struct CallRequirements {
    outputs: Vec<usize>,
    inputs: Vec<usize>,
}
struct Use {
    site: Site,
    port: usize,
    consumer: Consumer,
}

pub(super) struct Flow {
    edges: Vec<(usize, usize)>,
    seeds: Vec<usize>,
    selections: Vec<Vec<usize>>,
    call_requirements: Vec<CallRequirements>,
    affected: Vec<bool>,
    uses: Vec<Use>,
    boundaries: RefCell<Vec<Boundary>>,
    site: Cell<Site>,
}

fn work(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "dynamic Array actual upper relation")
}

impl Flow {
    fn new(count: usize, b: &mut Budget) -> CompileResult<Self> {
        work(b, count.saturating_add(1))?;
        Ok(Self {
            edges: Vec::new(),
            seeds: Vec::new(),
            selections: Vec::new(),
            call_requirements: Vec::new(),
            affected: vec![false; count],
            uses: Vec::new(),
            boundaries: RefCell::new(Vec::new()),
            site: Cell::new((usize::MAX, 0)),
        })
    }
    fn edge(&mut self, to: usize, from: usize, b: &mut Budget) -> CompileResult<()> {
        work(b, self.edges.len().saturating_mul(2).saturating_add(1))?;
        if to >= self.affected.len() || from >= self.affected.len() {
            return Err(bad("upper transport port is outside actual variables"));
        }
        if !self.edges.contains(&(to, from)) {
            self.edges.push((to, from));
        }
        Ok(())
    }
    fn seed(&mut self, port: usize, b: &mut Budget) -> CompileResult<()> {
        work(b, self.seeds.len().saturating_mul(2).saturating_add(1))?;
        if port >= self.affected.len() {
            return Err(bad("upper source is outside actual variables"));
        }
        if !self.seeds.contains(&port) {
            self.seeds.push(port);
        }
        Ok(())
    }
    fn seed_ports(&mut self, p: &Ports, b: &mut Budget) -> CompileResult<()> {
        work(
            b,
            p.classes
                .len()
                .saturating_add(p.array_elements.len())
                .saturating_add(1),
        )?;
        if let Some(port) = p.outer {
            self.seed(port, b)?;
        }
        for port in p.classes.iter().chain(&p.array_elements) {
            self.seed(*port, b)?;
        }
        Ok(())
    }
    fn copy_ports(&mut self, to: &Ports, from: &Ports, b: &mut Budget) -> CompileResult<()> {
        self.copy_elements(to, from, b)?;
        work(
            b,
            to.classes
                .len()
                .saturating_add(from.classes.len())
                .saturating_add(1),
        )?;
        if to.classes.len() != from.classes.len() {
            return Err(bad("upper transport has different actual class arity"));
        }
        if let (Some(to), Some(from)) = (to.outer, from.outer) {
            self.edge(to, from, b)?;
        }
        for (to, from) in to.classes.iter().zip(&from.classes) {
            self.edge(*to, *from, b)?;
        }
        Ok(())
    }
    fn copy_elements(&mut self, to: &Ports, from: &Ports, b: &mut Budget) -> CompileResult<()> {
        work(
            b,
            to.array_elements
                .len()
                .saturating_add(from.array_elements.len()),
        )?;
        if to.array_elements.len() != from.array_elements.len() {
            return Err(bad("upper transport loses an actual Array ordinal"));
        }
        for (to, from) in to.array_elements.iter().zip(&from.array_elements) {
            self.edge(*to, *from, b)?;
        }
        Ok(())
    }
    fn close_affected(&mut self, b: &mut Budget) -> CompileResult<()> {
        let mut changed = true;
        while changed {
            changed = false;
            work(b, self.edges.len().saturating_add(1))?;
            for (to, from) in &self.edges {
                if self.affected[*from] && !self.affected[*to] {
                    self.affected[*to] = true;
                    changed = true;
                }
            }
        }
        Ok(())
    }
    pub(super) fn enter(&self, block: usize, instruction: usize) {
        self.site.set((block, instruction));
    }
    pub(super) fn affected(&self, port: usize) -> bool {
        self.affected.get(port).copied().unwrap_or(false)
    }
    pub(super) fn transport(
        &self,
        to: usize,
        from: usize,
        e: &mut Equations,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.affected(to) || self.affected(from) {
            work(b, self.edges.len().saturating_add(1))?;
            if to != from && !self.edges.contains(&(to, from)) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "dynamic upper transport has no actual typed edge",
                    None,
                )]);
            }
            Ok(())
        } else {
            e.equal(to, from, b)
        }
    }
    pub(super) fn bind(
        &self,
        actual: usize,
        expected: usize,
        e: &mut Equations,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.affected(actual) || self.affected(expected) {
            let mut rows = self.boundaries.borrow_mut();
            work(b, rows.len().saturating_add(1))?;
            rows.push(Boundary {
                site: self.site.get(),
                actual,
                expected,
            });
            Ok(())
        } else {
            e.equal(actual, expected, b)
        }
    }
    fn domains(&self, e: &Equations, b: &mut Budget) -> CompileResult<Vec<BTreeSet<usize>>> {
        work(b, self.affected.len().saturating_add(self.seeds.len()))?;
        let mut domains = vec![BTreeSet::new(); self.affected.len()];
        for port in &self.seeds {
            let source = e.root(*port, b)?;
            work(b, 1)?;
            domains[*port].insert(source);
        }
        let mut changed = true;
        while changed {
            changed = false;
            work(b, self.edges.len().saturating_add(1))?;
            for (to, from) in &self.edges {
                work(b, domains[*from].len())?;
                let source = domains[*from].clone();
                for source in source {
                    let lookup = domains[*to]
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2);
                    work(b, lookup.saturating_add(1))?;
                    changed |= domains[*to].insert(source);
                }
            }
        }
        // A seed cannot discharge another Store/copy incoming dependency.
        work(b, self.affected.len())?;
        let mut required = self.affected.clone();
        {
            let boundaries = self.boundaries.borrow();
            work(b, boundaries.len())?;
            for row in boundaries.iter() {
                if self.affected(row.actual) || self.affected(row.expected) {
                    let actual = required
                        .get_mut(row.actual)
                        .ok_or_else(|| bad("identity actual upper is outside the typed graph"))?;
                    *actual = true;
                }
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            work(b, self.edges.len().saturating_add(1))?;
            for (to, from) in &self.edges {
                if required[*to] && !required[*from] {
                    required[*from] = true;
                    changed = true;
                }
            }
            work(b, self.call_requirements.len())?;
            for call in &self.call_requirements {
                work(b, call.outputs.len().saturating_add(call.inputs.len()))?;
                if call.outputs.iter().any(|port| required[*port]) {
                    for input in &call.inputs {
                        if !required[*input] {
                            required[*input] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        work(b, required.len())?;
        for (port, required) in required.iter().enumerate() {
            if *required && domains[port].is_empty() {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "dynamic upper dependency has no complete actual source",
                    None,
                )]);
            }
        }
        work(b, self.selections.len())?;
        for selection in &self.selections {
            work(b, selection.len())?;
            for input in selection {
                if domains[*input].is_empty() {
                    return Err(bad(
                        "dynamic Array selection has an absent actual upper candidate",
                    ));
                }
            }
        }
        Ok(domains)
    }
    pub(super) fn checkpoint(&self, e: &mut Equations, b: &mut Budget) -> CompileResult<()> {
        let domains = self.domains(e, b)?;
        let boundaries = self.boundaries.borrow();
        work(b, boundaries.len())?;
        for boundary in boundaries.iter() {
            if !self.affected(boundary.actual) && !self.affected(boundary.expected) {
                continue;
            }
            if self.affected(boundary.actual) {
                let sources = &domains[boundary.actual];
                work(b, sources.len().saturating_add(1))?;
                if sources.is_empty() {
                    return Err(bad("dynamic identity binding has no actual upper source"));
                }
                for source in sources {
                    e.equal(*source, boundary.expected, b)?;
                }
                e.equal(boundary.actual, boundary.expected, b)?;
            } else {
                e.equal(boundary.actual, boundary.expected, b)?;
            }
        }
        Ok(())
    }
    pub(super) fn finish(&self, e: &mut Equations, b: &mut Budget) -> CompileResult<()> {
        self.checkpoint(e, b)?;
        let domains = self.domains(e, b)?;
        let boundaries = self.boundaries.borrow();
        work(b, self.uses.len())?;
        for usage in &self.uses {
            if !self.affected(usage.port) {
                continue;
            }
            work(b, 1)?;
            if domains[usage.port].is_empty() {
                return Err(bad("dynamic outer use has no actual upper source"));
            }
            match usage.consumer {
                Consumer::Copy | Consumer::Observe => {}
                Consumer::Bound => {
                    work(b, boundaries.len())?;
                    if !boundaries
                        .iter()
                        .any(|row| row.site == usage.site && row.actual == usage.port)
                    {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "dynamic upper consumer lacks a classified actual identity binding",
                            None,
                        )]);
                    }
                }
                Consumer::Forbidden => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "this consumer does not yet preserve dynamic upper alternatives",
                        None,
                    )]);
                }
            }
        }
        Ok(())
    }
}

fn value(values: &[Ports], id: ValueId) -> CompileResult<&Ports> {
    values
        .get(id.0 as usize)
        .ok_or_else(|| bad("actual upper operand is absent"))
}
fn port(p: &Ports) -> CompileResult<usize> {
    p.outer
        .ok_or_else(|| bad("actual upper source is not an Address"))
}
fn source_slot(
    ty: &Type,
    p: &Ports,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Option<usize>> {
    if path.is_empty() {
        return Ok(p.outer);
    }
    if !p.array_elements.is_empty() {
        let [PlaceProjection::Field(i)] = path else {
            return Ok(None);
        };
        return Ok(p.array_elements.get(*i as usize).copied());
    }
    let Some(s) = shape(ty) else { return Ok(None) };
    work(
        b,
        s.slots.len().saturating_mul(path.len().saturating_add(1)),
    )?;
    Ok(s.slots
        .iter()
        .find(|s| s.path == path)
        .and_then(|s| p.classes.get(s.binder as usize))
        .copied())
}

impl Flow {
    fn boundary(
        &mut self,
        site: Site,
        actual: usize,
        expected: usize,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let rows = self.boundaries.get_mut();
        work(b, rows.len().saturating_add(1))?;
        rows.push(Boundary {
            site,
            actual,
            expected,
        });
        Ok(())
    }
    fn use_ports(
        &mut self,
        site: Site,
        p: &Ports,
        consumer: Consumer,
        b: &mut Budget,
    ) -> CompileResult<()> {
        work(
            b,
            p.classes
                .len()
                .saturating_add(p.array_elements.len())
                .saturating_add(1),
        )?;
        for upper in p.outer.iter().chain(&p.classes).chain(&p.array_elements) {
            work(b, self.uses.len().saturating_add(1))?;
            self.uses.push(Use {
                site,
                port: *upper,
                consumer,
            });
        }
        Ok(())
    }
    fn require_call_inputs(
        &mut self,
        out: &Ports,
        arguments: &[ValueId],
        values: &[Ports],
        b: &mut Budget,
    ) -> CompileResult<()> {
        let output_count = out
            .classes
            .len()
            .saturating_add(out.array_elements.len())
            .saturating_add(usize::from(out.outer.is_some()));
        work(b, output_count.saturating_add(arguments.len()))?;
        let outputs = out
            .outer
            .iter()
            .chain(&out.classes)
            .chain(&out.array_elements)
            .copied()
            .collect();
        let mut count = 0usize;
        for argument in arguments {
            let p = value(values, *argument)?;
            count = count
                .saturating_add(p.classes.len())
                .saturating_add(p.array_elements.len())
                .saturating_add(usize::from(p.outer.is_some()));
        }
        work(b, count.saturating_add(arguments.len()))?;
        let mut inputs = Vec::with_capacity(count);
        for argument in arguments {
            let p = value(values, *argument)?;
            inputs.extend(
                p.outer
                    .iter()
                    .chain(&p.classes)
                    .chain(&p.array_elements)
                    .copied(),
            );
        }
        work(b, self.call_requirements.len().saturating_add(1))?;
        self.call_requirements
            .push(CallRequirements { outputs, inputs });
        Ok(())
    }
    fn projection(
        &mut self,
        parent_ty: &Type,
        parent: &Ports,
        child: &Ports,
        step: LifeTypeProjection,
        construction: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let (physical, prefix) = if let Type::Address { inner, .. } = parent_ty {
            (inner.as_ref(), true)
        } else {
            (parent_ty, false)
        };
        work(b, child.keys.len())?;
        for (index, (path, class)) in child.keys.iter().enumerate() {
            work(b, path.len().saturating_add(2))?;
            let tail = if path.first() == Some(&LifeTypeProjection::Pointee) {
                &path[1..]
            } else {
                path.as_slice()
            };
            let mut full = vec![step];
            full.extend_from_slice(tail);
            let parent_class = if let Type::BorrowedStorage { life, .. } = physical {
                work(
                    b,
                    life.retained
                        .len()
                        .saturating_mul(full.len().saturating_add(1)),
                )?;
                let Some(row) = life
                    .retained
                    .iter()
                    .find(|r| r.path == full && r.class == *class)
                else {
                    continue;
                };
                work(b, parent.keys.len().saturating_mul(2))?;
                let wanted_prefix: &[LifeTypeProjection] = if prefix {
                    &[LifeTypeProjection::Pointee]
                } else {
                    &[]
                };
                parent
                    .keys
                    .iter()
                    .position(|(p, c)| p.as_slice() == wanted_prefix && *c == row.binder)
            } else {
                work(
                    b,
                    full.len()
                        .saturating_add(usize::from(prefix))
                        .saturating_add(1),
                )?;
                let mut wanted = Vec::new();
                if prefix {
                    wanted.push(LifeTypeProjection::Pointee);
                }
                wanted.extend_from_slice(&full);
                work(
                    b,
                    parent
                        .keys
                        .len()
                        .saturating_mul(wanted.len().saturating_add(1)),
                )?;
                parent
                    .keys
                    .iter()
                    .position(|(p, c)| p == &wanted && c == class)
            };
            if let Some(parent_class) = parent_class {
                let (to, from) = if construction {
                    (parent.classes[parent_class], child.classes[index])
                } else {
                    (child.classes[index], parent.classes[parent_class])
                };
                self.edge(to, from, b)?;
            }
        }
        Ok(())
    }
    pub(super) fn build(
        module: &TypedModule,
        f: &TypedFunction,
        reachable: &[bool],
        ports: (&[Ports], &[Ports], &[Ports]),
        count: usize,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        let (values, locals, captures) = ports;
        let mut flow = Self::new(count, b)?;
        work(b, f.parameters.len().saturating_add(captures.len()))?;
        for parameter in &f.parameters {
            flow.seed_ports(
                locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| bad("upper parameter local is absent"))?,
                b,
            )?;
        }
        for capture in captures {
            flow.seed_ports(capture, b)?;
        }
        for (bi, block) in f.blocks.iter().enumerate() {
            work(b, 1)?;
            for (ii, i) in block.instructions.iter().enumerate() {
                work(b, 1)?;
                let site = (bi, ii);
                let out = i.result.map(|id| value(values, id)).transpose()?;
                let mut consumer = Consumer::Forbidden;
                match &i.kind {
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        if let Some(out) = out {
                            flow.copy_ports(out, &locals[local.0 as usize], b)?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::Store { local, value: id } => {
                        flow.copy_ports(&locals[local.0 as usize], value(values, *id)?, b)?;
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        if let Some(out) = out {
                            if let Some(outer) = out.outer {
                                flow.seed(outer, b)?;
                            }
                            let source = &locals[local.0 as usize];
                            flow.copy_elements(out, source, b)?;
                            work(b, out.classes.len().saturating_add(source.classes.len()))?;
                            for (a, z) in out.classes.iter().zip(&source.classes) {
                                flow.edge(*a, *z, b)?;
                            }
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::ReadCapture { index } => {
                        if let (Some(out), Some(source)) = (out, captures.get(*index as usize)) {
                            flow.copy_ports(out, source, b)?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::ReborrowAddress { address, .. } => {
                        if let Some(out) = out {
                            flow.copy_ports(out, value(values, *address)?, b)?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::AddressField { address, index } => {
                        if let Some(out) = out {
                            let source = value(values, *address)?;
                            if let (Some(to), Some(from)) = (out.outer, source.outer) {
                                flow.edge(to, from, b)?;
                            }
                            work(b, out.array_elements.len())?;
                            for (ordinal, to) in out.array_elements.iter().enumerate() {
                                let path = [
                                    PlaceProjection::Field(*index),
                                    PlaceProjection::Field(ordinal as u32),
                                ];
                                if let Some(from) =
                                    source_slot(&f.values[address.0 as usize], source, &path, b)?
                                {
                                    flow.edge(*to, from, b)?;
                                }
                            }
                            flow.projection(
                                &f.values[address.0 as usize],
                                source,
                                out,
                                LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                                false,
                                b,
                            )?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        if let (Some(out), Some(from)) = (out, value(values, *address)?.outer)
                            && let Some(to) = out.outer
                        {
                            flow.edge(to, from, b)?;
                        }
                        consumer = if out.is_some_and(|p| p.outer.is_some()) {
                            Consumer::Copy
                        } else {
                            Consumer::Forbidden
                        };
                    }
                    InstructionKind::ReadAddress { address } => {
                        if let Some(out) = out {
                            let source = value(values, *address)?;
                            flow.copy_elements(out, source, b)?;
                            work(b, out.classes.len().saturating_add(source.classes.len()))?;
                            for (a, z) in out.classes.iter().zip(&source.classes) {
                                flow.edge(*a, *z, b)?;
                            }
                            if let Some(id) = i.result {
                                b.type_work(
                                    &f.values[id.0 as usize],
                                    false,
                                    "dynamic closed Copy observation",
                                )?;
                                if closed_copy(&f.values[id.0 as usize], b)? {
                                    consumer = Consumer::Observe;
                                } else if super::super::stored_flow::shared_array_value(
                                    &f.values[id.0 as usize],
                                ) {
                                    consumer = Consumer::Copy;
                                }
                            }
                        }
                    }
                    InstructionKind::LoadStoredBorrow { storage, path, .. } => {
                        if let Some(out) = out
                            && let (Some(to), Some(from)) = (
                                out.outer,
                                source_slot(
                                    &f.values[storage.0 as usize],
                                    value(values, *storage)?,
                                    path,
                                    b,
                                )?,
                            )
                        {
                            flow.edge(to, from, b)?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::StoreStoredBorrow {
                        storage,
                        path,
                        value: id,
                    } => {
                        if let Some(expected) = source_slot(
                            &f.values[storage.0 as usize],
                            value(values, *storage)?,
                            path,
                            b,
                        )? {
                            flow.boundary(site, port(value(values, *id)?)?, expected, b)?;
                        }
                        consumer = Consumer::Bound;
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakePackage { fields } => {
                        if let Some(out) = out {
                            if !out.array_elements.is_empty() {
                                if out.array_elements.len() != fields.len() {
                                    return Err(bad(
                                        "dynamic Array constructor lacks a physical element",
                                    ));
                                }
                                work(b, fields.len())?;
                                for (to, id) in out.array_elements.iter().zip(fields) {
                                    flow.edge(*to, port(value(values, *id)?)?, b)?;
                                }
                                consumer = Consumer::Copy;
                            } else {
                                if let Some(ty) = i.result.map(|id| &f.values[id.0 as usize]) {
                                    if let Some(shape) = shape(ty) {
                                        work(b, shape.slots.len())?;
                                        for slot in &shape.slots {
                                            let Some((PlaceProjection::Field(field), tail)) =
                                                slot.path.split_first()
                                            else {
                                                continue;
                                            };
                                            let Some(id) = fields.get(*field as usize) else {
                                                continue;
                                            };
                                            let source = value(values, *id)?;
                                            if let (Some(actual), Some(expected)) = (
                                                source_slot(
                                                    &f.values[id.0 as usize],
                                                    source,
                                                    tail,
                                                    b,
                                                )?,
                                                out.classes.get(slot.binder as usize).copied(),
                                            ) {
                                                flow.edge(expected, actual, b)?;
                                                flow.boundary(site, actual, expected, b)?;
                                            }
                                        }
                                    }
                                    work(b, fields.len())?;
                                    for (index, id) in fields.iter().enumerate() {
                                        flow.projection(
                                            ty,
                                            out,
                                            value(values, *id)?,
                                            LifeTypeProjection::Place(PlaceProjection::Field(
                                                index as u32,
                                            )),
                                            true,
                                            b,
                                        )?;
                                    }
                                }
                                consumer = Consumer::Bound;
                            }
                        }
                    }
                    InstructionKind::ExtractIndex { aggregate, .. }
                        if super::super::stored_flow::shared_array_value(
                            &f.values[aggregate.0 as usize],
                        ) =>
                    {
                        let out =
                            out.ok_or_else(|| bad("dynamic extraction has no upper result"))?;
                        let to = port(out)?;
                        let source = value(values, *aggregate)?;
                        let Type::Array { length, .. } = &f.values[aggregate.0 as usize] else {
                            unreachable!()
                        };
                        if source.array_elements.len() != *length as usize {
                            return Err(bad("dynamic upper selection lacks an actual ordinal"));
                        }
                        work(
                            b,
                            source
                                .array_elements
                                .len()
                                .saturating_add(flow.selections.len())
                                .saturating_add(1),
                        )?;
                        flow.selections.push(source.array_elements.clone());
                        flow.affected[to] = true;
                        for from in &source.array_elements {
                            flow.edge(to, *from, b)?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::ExtractField { aggregate, index } => {
                        if let Some(out) = out {
                            let source = value(values, *aggregate)?;
                            if let Some(to) = out.outer
                                && let Some(from) = source_slot(
                                    &f.values[aggregate.0 as usize],
                                    source,
                                    &[PlaceProjection::Field(*index)],
                                    b,
                                )?
                            {
                                flow.edge(to, from, b)?;
                            }
                            work(b, out.array_elements.len())?;
                            for (ordinal, to) in out.array_elements.iter().enumerate() {
                                let path = [
                                    PlaceProjection::Field(*index),
                                    PlaceProjection::Field(ordinal as u32),
                                ];
                                if let Some(from) =
                                    source_slot(&f.values[aggregate.0 as usize], source, &path, b)?
                                {
                                    flow.edge(*to, from, b)?;
                                }
                            }
                            flow.projection(
                                &f.values[aggregate.0 as usize],
                                source,
                                out,
                                LifeTypeProjection::Place(PlaceProjection::Field(*index)),
                                false,
                                b,
                            )?;
                        }
                        consumer = Consumer::Copy;
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let callee = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| bad("dynamic observer callee is absent"))?;
                        if callee.storage_life.is_some() {
                            if let Some(out) = out {
                                flow.require_call_inputs(out, arguments, values, b)?;
                                flow.seed_ports(out, b)?;
                            }
                            consumer = Consumer::Bound;
                        } else if observer(callee, b)? {
                            consumer = Consumer::Observe;
                        }
                    }
                    InstructionKind::MakeClosure { .. } => {
                        consumer = Consumer::Bound;
                    }
                    _ => {}
                }
                for input in crate::resources::uses(&i.kind) {
                    work(b, 1)?;
                    let input_consumer = if matches!(i.kind, InstructionKind::StoreStoredBorrow { storage, .. } if storage == input)
                    {
                        Consumer::Copy
                    } else {
                        consumer
                    };
                    flow.use_ports(site, value(values, input)?, input_consumer, b)?;
                }
            }
            if reachable.get(bi) == Some(&true)
                && let Terminator::Return(Some(id)) = block.terminator
            {
                let site = (usize::MAX, bi);
                let out = value(values, id)?;
                flow.use_ports(site, out, Consumer::Bound, b)?;
                if f.storage_life.is_none()
                    && let Some(bound) = &f.reference_return_bound
                {
                    work(b, bound.len().saturating_add(1))?;
                    if let (Some(actual), Some(expected)) = (
                        out.outer,
                        bound
                            .first()
                            .and_then(|p| f.parameters.get(*p as usize))
                            .and_then(|p| locals.get(p.0 as usize))
                            .and_then(|p| p.outer),
                    ) {
                        flow.boundary(site, actual, expected, b)?;
                    }
                }
            }
        }
        flow.close_affected(b)?;
        Ok(flow)
    }
}

fn observer(f: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.storage_life.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || !f.protocol_contracts.is_empty()
        || f.reference_return_bound.is_some()
    {
        return Ok(false);
    }
    b.type_work(&f.return_type, false, "dynamic observer actual result")?;
    if !scalar(&f.return_type) {
        return Ok(false);
    }
    work(b, f.parameters.len())?;
    for parameter in &f.parameters {
        let ty = &f.locals[parameter.0 as usize].ty;
        b.type_work(ty, false, "dynamic observer actual parameter")?;
        match ty {
            Type::Address {
                mutable: false,
                inner,
            } => {
                if !closed_copy(inner, b)? {
                    return Ok(false);
                }
            }
            ty if scalar(ty) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn closed_copy(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    work(b, 1)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        work(b, 1)?;
        match ty {
            Type::Tuple(fields) => {
                work(b, fields.len())?;
                todo.extend(fields);
            }
            Type::Record { fields, .. } => {
                work(b, fields.len())?;
                todo.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Array { element, .. } => {
                work(b, 1)?;
                todo.push(element);
            }
            Type::Enum { variants, .. } => {
                work(b, variants.len())?;
                for (_, fields) in variants {
                    work(b, fields.len())?;
                    todo.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                work(b, 2)?;
                todo.extend([error.as_ref(), result.as_ref()]);
            }
            ty if scalar(ty) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn copy_elements(
    e: &mut Equations,
    a: &Ports,
    z: &Ports,
    flow: Option<&Flow>,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(flow) = flow else {
        return e.copy_elements(a, z, b);
    };
    if a.array_elements.is_empty() && z.array_elements.is_empty() {
        return Ok(());
    }
    b.spend(
        a.array_elements
            .len()
            .saturating_add(z.array_elements.len()),
        "array address upper propagation",
    )?;
    if a.array_elements.len() != z.array_elements.len() {
        return Err(bad("array address upper arity differs"));
    }
    for (a, z) in a.array_elements.iter().zip(&z.array_elements) {
        flow.transport(*a, *z, e, b)?;
    }
    Ok(())
}
pub(super) fn copy(
    e: &mut Equations,
    a: &Ports,
    z: &Ports,
    flow: Option<&Flow>,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(flow) = flow else {
        return e.copy(a, z, b);
    };
    copy_elements(e, a, z, Some(flow), b)?;
    b.spend(
        a.classes
            .len()
            .saturating_add(z.classes.len())
            .saturating_add(1),
        "class port propagation",
    )?;
    if a.classes.len() != z.classes.len() {
        return Err(bad("class port arity differs from actual value"));
    }
    if let (Some(a), Some(z)) = (a.outer, z.outer) {
        flow.transport(a, z, e, b)?;
    }
    for (a, z) in a.classes.iter().zip(&z.classes) {
        e.equal(*a, *z, b)?;
    }
    Ok(())
}
pub(super) fn transport(
    e: &mut Equations,
    to: usize,
    from: usize,
    flow: Option<&Flow>,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(flow) = flow {
        flow.transport(to, from, e, b)
    } else {
        e.equal(to, from, b)
    }
}
pub(super) fn binding(
    e: &mut Equations,
    actual: usize,
    expected: usize,
    flow: Option<&Flow>,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(flow) = flow {
        flow.bind(actual, expected, e, b)
    } else {
        e.equal(actual, expected, b)
    }
}
