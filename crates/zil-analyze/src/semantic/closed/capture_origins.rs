//! Finite actual whole-capture provenance. A loop is a graph edge, not another
//! Rust stack frame. This query returns a physical capture ordinal only; class
//! images, initialized owners, loans and cleanup are checked by their kernels.
use super::*;
use std::collections::VecDeque;
fn limit_error(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Query {
    Value(ValueId),
    Local(LocalId, BlockId, usize),
}
type Definition = crate::SsaDefinition;
struct Node {
    query: Query,
    seed: Option<u32>,
    dependencies: Vec<usize>,
    users: Vec<usize>,
    origin: Option<u32>,
}
struct Graph<'c, 'a, 'i, 'f> {
    check: &'c Check<'a, 'i>,
    function: &'f TypedFunction,
    initialization: std::sync::Arc<crate::Initialization>,
    definitions: std::sync::Arc<Vec<Option<crate::SsaDefinition>>>,
    producer: Option<ValueId>,
    producers: Option<BTreeSet<ValueId>>,
    probe: bool,
    expected_capture: Option<u32>,
    untouched_whole: bool,
    scalar_mirror: Option<(LocalId, u32)>,
    indices: BTreeMap<Query, usize>,
    nodes: Vec<Node>,
}
impl<'c, 'a, 'i, 'f> Graph<'c, 'a, 'i, 'f> {
    fn new(
        check: &'c Check<'a, 'i>,
        function: &'f TypedFunction,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        // This independent finite pass checks every reachable initialization
        // edge. In particular a missing origin cannot hide behind a loop's
        // optimistic initial local set or behind a dead predecessor.
        let initialization = crate::shared_initialization(function, b)?;
        if function.values.len() > b.helper_nodes {
            return Err(limit_error(
                "capture provenance SSA directory exceeds its node budget",
            ));
        }
        let definitions = crate::shared_definitions(function, b)?;
        Ok(Self {
            check,
            function,
            initialization,
            definitions,
            producer: None,
            producers: None,
            probe: false,
            expected_capture: None,
            untouched_whole: false,
            scalar_mirror: None,
            indices: BTreeMap::new(),
            nodes: Vec::new(),
        })
    }
    fn intern(&mut self, query: Query, b: &mut Budget) -> CompileResult<usize> {
        b.lookup(self.indices.len(), WORK)?;
        if let Some(index) = self.indices.get(&query) {
            return Ok(*index);
        }
        if self.nodes.len() >= b.helper_nodes {
            return Err(limit_error(
                "capture provenance graph exceeds its complete node budget",
            ));
        }
        let index = self.nodes.len();
        self.check.push_value(
            &mut self.nodes,
            Node {
                query,
                seed: None,
                dependencies: Vec::new(),
                users: Vec::new(),
                origin: None,
            },
            b,
        )?;
        b.lookup(self.indices.len(), WORK)?;
        self.indices.insert(query, index);
        Ok(index)
    }
    fn select_spill(&mut self, local: LocalId, capture: u32, b: &mut Budget) -> CompileResult<()> {
        self.capture(capture, b)?;
        let definition = self.function.environment.and_then(|id| self.check.module.closures.get(id.0 as usize))
            .ok_or_else(|| fail("spilled capture has no creator environment"))?;
        let field = definition.captures.get(capture as usize)
            .ok_or_else(|| fail("spilled capture has no physical field"))?;
        if definition.kind != CallableKind::Fn && field.is_b0_scalar()
            && self.function.locals.get(local.0 as usize).is_some_and(|row| row.id == local && row.ty == *field)
        {
            self.scalar_mirror = Some((local, capture));
        }
        Ok(())
    }

    fn capture_seed(&self, index: u32) -> u32 {
        if self.probe { u32::from(self.expected_capture != Some(index)) } else { index }
    }

    fn capture(&self, index: u32, b: &mut Budget) -> CompileResult<()> {
        if self.producer.is_some() && !self.probe {
            return Err(fail("original expression transport selects another actual environment source"));
        }
        let environment = self
            .function
            .environment
            .ok_or_else(|| fail("actual capture origin has no physical closure environment"))?;
        b.spend(self.check.module.closures.len().saturating_add(1), WORK)?;
        let closure = self
            .check
            .module
            .closures
            .get(environment.0 as usize)
            .filter(|row| row.id == environment && row.function == self.function.id)
            .ok_or_else(|| {
                fail("actual capture origin changes its exact closure entry identity")
            })?;
        if index as usize >= closure.captures.len() {
            return Err(fail(
                "actual capture origin selects no physical environment slot",
            ));
        }
        Ok(())
    }
    fn definition(&self, value: ValueId, b: &mut Budget) -> CompileResult<Definition> {
        b.spend(1, WORK)?;
        self.definitions
            .get(value.0 as usize)
            .copied()
            .flatten()
            .ok_or_else(|| {
                fail("capture provenance selects an SSA value without an actual definition")
            })
    }
    fn available(&self, value: ValueId, at: Definition, b: &mut Budget) -> CompileResult<()> {
        let source = self.definition(value, b)?;
        if !self
            .initialization
            .reachable
            .get(source.block.0 as usize)
            .copied()
            .unwrap_or(false)
        {
            return Err(fail("capture provenance SSA definition is unreachable"));
        }
        if source.block == at.block {
            if source.instruction >= at.instruction {
                return Err(fail(
                    "capture provenance reads an SSA source before its actual definition",
                ));
            }
            return Ok(());
        }
        // A real entry-to-use path avoiding the defining block disproves
        // dominance. No public type or alleged class image substitutes for it.
        b.spend(self.function.blocks.len(), WORK)?;
        let mut seen = vec![false; self.function.blocks.len()];
        let mut pending = Vec::new();
        self.check
            .push_value(&mut pending, self.function.entry, b)?;
        while let Some(block) = pending.pop() {
            b.spend(1, WORK)?;
            if block == source.block {
                continue;
            }
            let visited = seen
                .get_mut(block.0 as usize)
                .ok_or_else(|| fail("capture provenance dominance leaves the actual CFG"))?;
            if *visited {
                continue;
            }
            *visited = true;
            if block == at.block {
                return Err(fail(
                    "capture provenance SSA source does not dominate its actual use",
                ));
            }
            let actual = &self.function.blocks[block.0 as usize];
            match actual.terminator {
                Terminator::Jump(next) => self.check.push_value(&mut pending, next, b)?,
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.check.push_value(&mut pending, then_block, b)?;
                    self.check.push_value(&mut pending, else_block, b)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn one(&self, query: Query, b: &mut Budget) -> CompileResult<Vec<Query>> {
        let mut queries = Vec::new();
        self.check.push_value(&mut queries, query, b)?;
        Ok(queries)
    }
    fn value_dependency(
        &self,
        value: ValueId,
        at: Definition,
        b: &mut Budget,
    ) -> CompileResult<Query> {
        self.available(value, at, b)?;
        Ok(Query::Value(value))
    }
    fn local(
        &self,
        local: LocalId,
        block: BlockId,
        end: usize,
        b: &mut Budget,
    ) -> CompileResult<(Option<u32>, Vec<Query>)> {
        let local_type = &self
            .function
            .locals
            .get(local.0 as usize)
            .filter(|row| row.id == local)
            .ok_or_else(|| fail("capture provenance selects no canonical actual local"))?
            .ty;
        let actual = self
            .function
            .blocks
            .get(block.0 as usize)
            .filter(|row| row.id == block)
            .ok_or_else(|| fail("capture provenance selects no actual basic block"))?;
        if end > actual.instructions.len() {
            return Err(fail(
                "capture provenance local boundary exceeds its exact instruction sequence",
            ));
        }
        if !self
            .initialization
            .reachable
            .get(block.0 as usize)
            .copied()
            .unwrap_or(false)
        {
            return Err(fail(
                "capture provenance selects an unreachable local boundary",
            ));
        }
        for (instruction, item) in actual.instructions.iter().enumerate().take(end).rev() {
            b.spend(1, WORK)?;
            match &item.kind {
                InstructionKind::BindPackageCapture {
                    local: target,
                    index,
                } if *target == local => {
                    self.capture(*index, b)?;
                    return Ok((Some(self.capture_seed(*index)), Vec::new()));
                }
                InstructionKind::EndPackageCapture { local: target, .. } if *target == local => {
                    return Err(fail(
                        "capture provenance crosses an ended owning Package view",
                    ));
                }
                InstructionKind::WriteCapture { index, value }
                    if self.scalar_mirror == Some((local, *index)) => {
                    if instruction.checked_sub(1).and_then(|i| actual.instructions.get(i))
                        .is_none_or(|i| !matches!(i.kind, InstructionKind::Store { local: dest, value: rhs }
                            if dest == local && rhs == *value))
                    {
                        return Err(fail("captured scalar write and local mirror disagree"));
                    }
                }
                InstructionKind::Store {
                    local: target,
                    value,
                } if *target == local => {
                    let ty =
                        self.function.values.get(value.0 as usize).ok_or_else(|| {
                            fail("capture provenance store has no actual SSA type")
                        })?;
                    crate::dyn_metadata::ty(ty, b)?;
                    crate::dyn_metadata::ty(local_type, b)?;
                    if ty != local_type {
                        return Err(fail(
                            "capture provenance whole store changes the complete local type",
                        ));
                    }
                    // A synchronized scalar assignment preserves the binding, even
                    // when the new data is an arithmetic result. Keep scanning
                    // for the original capture on every incoming path.
                    if let Some((spill, capture)) = self.scalar_mirror
                        && spill == local && instruction + 1 < end
                        && matches!(actual.instructions[instruction + 1].kind,
                            InstructionKind::WriteCapture { index, value: rhs }
                            if index == capture && rhs == *value)
                    {
                        self.available(*value, Definition { block, instruction }, b)?;
                        continue;
                    }
                    let dependency =
                        self.value_dependency(*value, Definition { block, instruction }, b)?;
                    return Ok((None, self.one(dependency, b)?));
                }
                InstructionKind::UnpackTuple {
                    local: source,
                    fields,
                }
                | InstructionKind::UnpackVariant {
                    local: source,
                    fields,
                    ..
                } => {
                    b.spend(fields.len().saturating_add(1), WORK)?;
                    if *source == local {
                        return Err(fail(
                            "capture provenance crosses a consumed whole aggregate activation",
                        ));
                    }
                    if fields.contains(&local) {
                        return Err(unknown(
                            "projected capture provenance requires its complete original field and tag source relation",
                        ));
                    }
                }
                InstructionKind::DropDynOwned { local: target } if *target == local => {
                    return Err(fail(
                        "capture provenance crosses a retired owning Dyn activation",
                    ));
                }
                kind => match crate::incremental::Operation::of(kind) {
                    // An intermediate whole-owner spill must retain the exact
                    // occupant. A projected replacement or a new writable view
                    // cannot be justified by the earlier whole Store alone.
                    crate::incremental::Operation::Read(target)
                        if self.untouched_whole && target == local &&
                            !matches!(kind, InstructionKind::PlaceTagLocal { .. }
                                | InstructionKind::VariantTagLocal { .. }
                                | InstructionKind::ReadVariantLocal { .. }
                                | InstructionKind::ReadPackageField { .. }
                                | InstructionKind::BorrowLocal { .. }) =>
                    {
                        return Ok((Some(1), Vec::new()));
                    }
                    crate::incremental::Operation::Take(target)
                    | crate::incremental::Operation::Kill(target)
                        if target == local =>
                    {
                        return Err(fail(
                            "capture provenance crosses a vacant or retired local activation",
                        ));
                    }
                    crate::incremental::Operation::Store(target) if target == local => {
                        return Err(unknown(
                            "capture provenance local definition needs its complete whole-value transfer",
                        ));
                    }
                    _ => {}
                },
            }
        }
        // The entry edge exists even when the entry block has back edges. A
        // function parameter is not a physical capture merely because a loop
        // later overwrites its same-numbered local with a captured value.
        if block == self.function.entry {
            b.spend(self.function.parameters.len().saturating_add(1), WORK)?;
            if self.probe && self.function.parameters.contains(&local) {
                return Ok((Some(1), Vec::new()));
            }
            return Err(fail(
                "capture provenance entry local has no actual environment origin",
            ));
        }
        let mut dependencies = Vec::new();
        for predecessor in &self.initialization.predecessors[block.0 as usize] {
            b.spend(1, WORK)?;
            if !self.initialization.reachable[*predecessor] {
                continue;
            }
            let previous = &self.function.blocks[*predecessor];
            self.check.push_value(
                &mut dependencies,
                Query::Local(local, previous.id, previous.instructions.len()),
                b,
            )?;
        }
        if dependencies.is_empty() {
            return Err(fail(
                "capture provenance local has no reachable initialized predecessor origin",
            ));
        }
        Ok((None, dependencies))
    }
    fn expand(&self, query: Query, b: &mut Budget) -> CompileResult<(Option<u32>, Vec<Query>)> {
        match query {
            Query::Local(local, block, end) => self.local(local, block, end, b),
            Query::Value(value) => {
                let definition = self.definition(value, b)?;
                if !self.initialization.reachable[definition.block.0 as usize] {
                    return Err(fail("capture provenance SSA value is unreachable"));
                }
                let selected = if let Some(producers) = &self.producers {
                    b.lookup(producers.len(), WORK)?;
                    producers.contains(&value)
                } else { self.producer == Some(value) };
                if selected { return Ok((Some(0), Vec::new())); }
                let instruction = &self.function.blocks[definition.block.0 as usize].instructions
                    [definition.instruction];
                match instruction.kind {
                    InstructionKind::ReadCapture { index }
                    | InstructionKind::TakeCapture { index }
                    | InstructionKind::DescribeCapture { index }
                    | InstructionKind::BorrowCapture { index, .. } => {
                        self.capture(index, b)?;
                        Ok((Some(self.capture_seed(index)), Vec::new()))
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::DescribeAddress { address } => Ok((
                        None,
                        self.one(self.value_dependency(address, definition, b)?, b)?,
                    )),
                    InstructionKind::Load(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::DescribeLocal { local } => Ok((
                        None,
                        self.one(
                            Query::Local(local, definition.block, definition.instruction),
                            b,
                        )?,
                    )),
                    _ if self.probe => Ok((Some(1),Vec::new())),
                    _ => {
                        let mut errors = unknown(
                            "whole capture provenance requires the exact actual capture/local transport operation",
                        );
                        errors[0].notes.push(format!(
                            "actual function {} SSA {}",
                            self.function.id.0, value.0
                        ));
                        Err(errors)
                    }
                }
            }
        }
    }
    fn solve(mut self, root: Query, b: &mut Budget) -> CompileResult<u32> {
        let root = self.intern(root, b)?;
        let mut cursor = 0;
        while cursor < self.nodes.len() {
            b.spend(1, WORK)?;
            let (seed, dependencies) = self.expand(self.nodes[cursor].query, b)?;
            self.nodes[cursor].seed = seed;
            self.nodes[cursor].origin = seed;
            for dependency in dependencies {
                let dependency = self.intern(dependency, b)?;
                b.spend(
                    self.nodes[cursor].dependencies.len().saturating_add(1),
                    WORK,
                )?;
                if self.nodes[cursor].dependencies.contains(&dependency) {
                    continue;
                }
                self.check
                    .push_value(&mut self.nodes[cursor].dependencies, dependency, b)?;
                self.check
                    .push_value(&mut self.nodes[dependency].users, cursor, b)?;
            }
            cursor += 1;
        }
        let mut pending = VecDeque::new();
        for (index, node) in self.nodes.iter().enumerate() {
            b.spend(1, WORK)?;
            if node.seed.is_some() {
                b.spend(1, WORK)?;
                pending.push_back(index);
            }
        }
        while let Some(index) = pending.pop_front() {
            b.spend(1, WORK)?;
            let origin = self.nodes[index]
                .origin
                .ok_or_else(|| fail("capture provenance worklist loses its grounded source"))?;
            let users = self.nodes[index].users.len();
            for offset in 0..users {
                b.spend(1, WORK)?;
                let user = self.nodes[index].users[offset];
                match self.nodes[user].origin {
                    Some(previous) if previous != origin && self.probe => {
                        if previous!=1 {
                            self.nodes[user].origin=Some(1);
                            b.spend(1,WORK)?;
                            pending.push_back(user);
                        }
                    }
                    Some(previous) if previous != origin => {
                        return Err(unknown(
                            "capture provenance has several actual physical environment origins",
                        ));
                    }
                    Some(_) => {}
                    None => {
                        self.nodes[user].origin = Some(origin);
                        b.spend(1, WORK)?;
                        pending.push_back(user);
                    }
                }
            }
        }
        // Reject every ungrounded reachable dependency, even when another
        // branch gave the requested root an apparent answer. A seed-free SCC
        // is Unknown, never an empty source or an arbitrary first capture.
        for node in &self.nodes {
            b.spend(1, WORK)?;
            if node.origin.is_none() {
                if self.probe {return Ok(1);}
                return Err(unknown(
                    "capture provenance contains an ungrounded actual CFG source cycle",
                ));
            }
        }
        self.nodes[root]
            .origin
            .ok_or_else(|| fail("capture provenance has no complete actual environment origin"))
    }
}
impl Check<'_, '_> {
    /// Select a genuine original operation before interpreting its value.
    /// False is a mismatching/ungrounded candidate, never a successful proof.
    pub(super) fn operation_operand_matches(
        &self,function:&TypedFunction,value:ValueId,producer:ValueId,b:&mut Budget,
    )->CompileResult<bool> {
        let mut graph=Graph::new(self,function,b)?;
        graph.producer=Some(producer);
        graph.probe=true;
        Ok(graph.solve(Query::Value(value),b)?==0)
    }
    pub(super) fn local_holds_operand_at(
        &self,
        function: &TypedFunction,
        local: LocalId,
        block: BlockId,
        index: usize,
        producer: ValueId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let mut graph = Graph::new(self, function, b)?;
        graph.producer = Some(producer);
        graph.probe = true;
        graph.untouched_whole = true;
        Ok(graph.solve(Query::Local(local, block, index), b)? == 0)
    }

    pub(super) fn operation_operands_cover(&self, function: &TypedFunction,
        value: ValueId, producers: &BTreeSet<ValueId>, b: &mut Budget) -> CompileResult<bool> {
        let mut graph = Graph::new(self, function, b)?;
        b.spend(producers.len().saturating_add(1), WORK)?;
        graph.producers = Some(producers.clone());
        graph.probe = true;
        Ok(graph.solve(Query::Value(value), b)? == 0)
    }
    /// Prove one exact prepared owner from the actual original local transfer.
    /// Equal types and classes do not select an originating move.
    pub(super) fn prepared_owner_source(
        &self,
        function: &TypedFunction,
        original: LocalId,
        actual: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
    ) -> CompileResult<(BlockId, usize)> {
        operations::initialized_local_before(function, at, actual, b)?;
        if original == actual {
            return Ok(at);
        }
        let expected = &function.locals[actual.0 as usize].ty;
        let initialization = crate::shared_initialization(function, b)?;
        let mut source = None;
        for block in &function.blocks {
            b.spend(1, WORK)?;
            if !initialization.reachable[block.id.0 as usize] {
                continue;
            }
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                if !matches!(instruction.kind, InstructionKind::TakeLocal { local } if local == original)
                {
                    continue;
                }
                let producer = instruction
                    .result
                    .ok_or_else(|| fail("receiver preparation omits its actual moved SSA"))?;
                if function.values.get(producer.0 as usize) != Some(expected)
                    || !self.operation_local_matches(function, actual, at, producer, b)?
                {
                    continue;
                }
                let point = (block.id, index);
                operations::initialized_local_before(function, point, original, b)?;
                if source.replace(point).is_some() {
                    return Err(fail(
                        "receiver preparation has several complete original owner occurrences",
                    ));
                }
            }
        }
        source.ok_or_else(|| fail("receiver preparation has no complete original owning transfer"))
    }

    fn operation_local_matches(
        &self,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        producer: ValueId,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let mut graph = Graph::new(self, function, b)?;
        graph.producer = Some(producer);
        graph.probe = true;
        Ok(graph.solve(Query::Local(local, at.0, at.1), b)? == 0)
    }

    /// The same complete transport graph, grounded at one actual emitted SSA
    /// instead of a physical capture ordinal. All incoming paths must retain
    /// this exact producer; equal types/classes grant no source identity.
    pub(super) fn original_producer_value(
        &self, function: &TypedFunction, value: ValueId, producer: ValueId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut graph = Graph::new(self, function, b)?;
        graph.producer = Some(producer);
        graph.solve(Query::Value(value), b)?;
        Ok(())
    }

    pub(super) fn original_producer_local(
        &self, function: &TypedFunction, local: LocalId, at: (BlockId, usize),
        producer: ValueId, b: &mut Budget,
    ) -> CompileResult<()> {
        let mut graph = Graph::new(self, function, b)?;
        graph.producer = Some(producer);
        graph.solve(Query::Local(local, at.0, at.1), b)?;
        Ok(())
    }
    /// Reconstruct whether every reaching path to this creator operand starts at
    /// the selected physical field. Other candidate fields can have independent
    /// origins. A synchronized scalar spill preserves its binding across writes.
    pub(super) fn capture_operand_matches(
        &self, function: &TypedFunction, local: LocalId, operand: ValueId,
        expected: u32, b: &mut Budget,
    ) -> CompileResult<bool> {
        let mut graph = Graph::new(self, function, b)?;
        graph.probe = true;
        graph.expected_capture = Some(expected);
        graph.select_spill(local, expected, b)?;
        Ok(graph.solve(Query::Value(operand), b)? == 0)
    }

    pub(super) fn spilled_capture_name(
        &self, function: &TypedFunction, local: LocalId, value: ValueId,
        capture: u32, b: &mut Budget,
    ) -> CompileResult<u32> {
        let mut graph = Graph::new(self, function, b)?;
        graph.select_spill(local, capture, b)?;
        graph.solve(Query::Value(value), b)
    }
    pub(super) fn capture_origin_value(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(limit_error(
                "capture provenance query enters beyond its structural depth budget",
            ));
        }
        Graph::new(self, function, b)?.solve(Query::Value(value), b)
    }
    pub(super) fn capture_origin_local(
        &self,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<u32> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(limit_error(
                "capture provenance query enters beyond its structural depth budget",
            ));
        }
        Graph::new(self, function, b)?.solve(Query::Local(local, at.0, at.1), b)
    }
}
