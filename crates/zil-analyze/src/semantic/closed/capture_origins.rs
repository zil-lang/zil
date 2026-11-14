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
#[derive(Clone, Copy)]
struct Definition {
    block: BlockId,
    instruction: usize,
}
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
    initialization: crate::Initialization,
    definitions: Vec<Option<Definition>>,
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
        let initialization = crate::initialize(function, b)?;
        if function.values.len() > b.helper_nodes {
            return Err(limit_error(
                "capture provenance SSA directory exceeds its node budget",
            ));
        }
        b.spend(function.values.len(), WORK)?;
        let mut definitions = vec![None; function.values.len()];
        for block in &function.blocks {
            b.spend(block.instructions.len().saturating_add(1), WORK)?;
            for (instruction, item) in block.instructions.iter().enumerate() {
                if let Some(value) = item.result {
                    let slot = definitions.get_mut(value.0 as usize).ok_or_else(|| {
                        fail("capture provenance SSA definition leaves its actual value directory")
                    })?;
                    if slot
                        .replace(Definition {
                            block: block.id,
                            instruction,
                        })
                        .is_some()
                    {
                        return Err(fail(
                            "capture provenance SSA value has several actual definitions",
                        ));
                    }
                }
            }
        }
        Ok(Self {
            check,
            function,
            initialization,
            definitions,
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
    fn capture(&self, index: u32, b: &mut Budget) -> CompileResult<()> {
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
                    return Ok((Some(*index), Vec::new()));
                }
                InstructionKind::EndPackageCapture { local: target, .. } if *target == local => {
                    return Err(fail(
                        "capture provenance crosses an ended owning Package view",
                    ));
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
                let instruction = &self.function.blocks[definition.block.0 as usize].instructions
                    [definition.instruction];
                match instruction.kind {
                    InstructionKind::ReadCapture { index }
                    | InstructionKind::TakeCapture { index }
                    | InstructionKind::DescribeCapture { index }
                    | InstructionKind::BorrowCapture { index, .. } => {
                        self.capture(index, b)?;
                        Ok((Some(index), Vec::new()))
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
