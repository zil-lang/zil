//! Occurrence-scoped graph normalization. Owner IDs are semantic backreferences.
//! Each occurrence gets a fresh token; no child is interned by a relative key.
use super::*;
use zil_ir::{StorageBinder, StorageLifeShape};

#[derive(Clone, Copy)]
struct Owner {
    source: CtTypeNodeId,
    token: usize,
    fixed: bool,
    count: u32,
}
struct Pending {
    node: CtTypeNode,
    owner: Option<usize>,
    references: Vec<(usize, usize)>,
}
struct Built {
    nodes: Vec<Pending>,
    key: Vec<u8>,
}
enum Task {
    Enter(CtTypeNodeId, u32),
    Exit(CtTypeNodeId, Option<usize>, usize),
}
struct Work<'a> {
    budget: &'a mut CtExecutionBudget,
    charged: u64,
}
impl Work<'_> {
    fn pay(&mut self, work: u64, bytes: u64) -> CompileResult<()> {
        self.budget.reserve_analysis_work(work)?;
        self.budget.reserve_temporary_bytes(bytes)?;
        self.charged = add(self.charged, bytes)?;
        Ok(())
    }
    fn key_bytes(&mut self, key: &mut Vec<u8>, bytes: &[u8]) -> CompileResult<()> {
        self.pay(bytes.len() as u64, mul(bytes.len() as u64, 2)?.max(16))?;
        key.extend_from_slice(bytes);
        Ok(())
    }
    fn number(&mut self, key: &mut Vec<u8>, n: u64) -> CompileResult<()> {
        self.key_bytes(key, &n.to_be_bytes())
    }
    fn symbol(&mut self, key: &mut Vec<u8>, symbol: &CtSymbolId) -> CompileResult<()> {
        self.number(key, u64::from(symbol.module.0))?;
        self.number(key, u64::from(symbol.declaration.0))?;
        self.key_bytes(key, &[u8::from(symbol.generation_path.is_some())])?;
        if let Some(path) = symbol.generation_path {
            self.key_bytes(key, &path.0)?;
        }
        Ok(())
    }
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Constant) -> CompileResult<()> {
        // The legacy scalar encoding is shared data, not a proof oracle.
        let mut key = Key {
            out: Vec::new(),
            budget: self.budget,
        };
        key.scalar(value)?;
        let bytes = key.out;
        self.charged = add(self.charged, bytes.len() as u64)?;
        self.key_bytes(out, &bytes)
    }
    fn reference(
        &mut self,
        key: &mut Vec<u8>,
        value: CtLifetimeArgument,
        scopes: &[Owner],
    ) -> CompileResult<usize> {
        self.pay(scopes.len() as u64 + 1, 0)?;
        let (depth, owner) = scopes
            .iter()
            .rev()
            .enumerate()
            .find(|(_, owner)| owner.source == value.owner)
            .ok_or_else(|| bad("CT lifetime owner is outside its actual enclosing scope"))?;
        let (fixed, ordinal) = match value.binder {
            StorageBinder::Fixed(n) => (true, n),
            StorageBinder::Invocation(n) => (false, n),
        };
        if fixed != owner.fixed || ordinal >= owner.count {
            return Err(bad("CT lifetime owner domain or ordinal does not match"));
        }
        self.number(key, depth as u64)?;
        self.key_bytes(key, &[u8::from(fixed)])?;
        self.number(key, u64::from(ordinal))?;
        Ok(owner.token)
    }
    fn path(
        &mut self,
        out: &mut Vec<u8>,
        path: &[zil_ir::LifeTypeProjection],
    ) -> CompileResult<()> {
        use zil_ir::LifeTypeProjection as P;
        self.number(out, path.len() as u64)?;
        for step in path {
            let (tag, n) = match step {
                P::Place(zil_ir::PlaceProjection::Field(n)) => (0, u64::from(*n)),
                P::Place(zil_ir::PlaceProjection::Variant(v)) => {
                    self.key_bytes(out, &[1])?;
                    self.number(out, u64::from(v.variant))?;
                    self.number(out, u64::from(v.index))?;
                    continue;
                }
                P::Place(zil_ir::PlaceProjection::ErrorArm(v)) => (2, u64::from(*v)),
                P::TypeArgument(n) => (3, u64::from(*n)),
                P::Port(n) => (4, u64::from(*n)),
                P::AssociatedType(n) => (5, u64::from(*n)),
                P::Pointee => (6, 0),
                P::Element => (7, 0),
                P::Parameter(n) => (8, u64::from(*n)),
                P::Result => (9, 0),
            };
            self.key_bytes(out, &[tag])?;
            self.number(out, n)?;
        }
        Ok(())
    }
    fn life(
        &mut self,
        key: &mut Vec<u8>,
        life: &zil_ir::FunctionLifeContract,
    ) -> CompileResult<()> {
        self.number(key, u64::from(life.binder_count))?;
        self.number(key, life.parameter_lives.len() as u64)?;
        for value in &life.parameter_lives {
            self.key_bytes(key, &[u8::from(value.is_some())])?;
            if let Some(n) = value {
                self.number(key, u64::from(*n))?;
            }
        }
        self.key_bytes(key, &[u8::from(life.result_life.is_some())])?;
        if let Some(n) = life.result_life {
            self.number(key, u64::from(n))?;
        }
        Ok(())
    }
    fn shape_copy(&mut self, shape: &StorageLifeShape) -> CompileResult<u64> {
        let count = add(
            add(shape.slots.len() as u64, shape.retained.len() as u64)?,
            shape.nominal_arguments.len() as u64,
        )?;
        self.pay(count, 0)?;
        let mut bytes = mul(count, 64)?;
        for length in shape
            .slots
            .iter()
            .map(|r| r.path.len())
            .chain(shape.retained.iter().map(|r| r.path.len()))
            .chain(shape.nominal_arguments.iter().map(|r| r.path.len()))
        {
            self.pay(length as u64, 0)?;
            bytes = add(bytes, mul(length as u64, 16)?)?;
        }
        self.pay(bytes, bytes)?;
        Ok(bytes)
    }
    fn storage(&mut self, key: &mut Vec<u8>, shape: &StorageLifeShape) -> CompileResult<()> {
        self.number(key, u64::from(shape.binder_count))?;
        self.number(key, shape.slots.len() as u64)?;
        for row in &shape.slots {
            self.number(key, row.path.len() as u64)?;
            for step in &row.path {
                self.path(key, &[zil_ir::LifeTypeProjection::Place(*step)])?;
            }
            self.number(key, u64::from(row.binder))?;
            self.key_bytes(key, &[row.mode as u8])?;
        }
        self.number(key, shape.retained.len() as u64)?;
        for row in &shape.retained {
            self.path(key, &row.path)?;
            self.number(key, u64::from(row.class))?;
            self.number(key, u64::from(row.binder))?;
        }
        self.number(key, shape.nominal_arguments.len() as u64)?;
        for row in &shape.nominal_arguments {
            self.path(key, &row.path)?;
            self.number(key, u64::from(row.declaration.module.0))?;
            self.number(key, u64::from(row.declaration.declaration.0))?;
            self.key_bytes(key, &[u8::from(row.declaration.compiler)])?;
            self.number(key, u64::from(row.argument))?;
            self.number(key, u64::from(row.binder))?;
        }
        Ok(())
    }
}

fn physical_remap(node: &mut CtTypeNode, offset: u32) -> CompileResult<()> {
    let map = |id: &mut CtTypeNodeId| -> CompileResult<()> {
        id.0 = id.0.checked_add(offset).ok_or_else(bound)?;
        Ok(())
    };
    match node {
        CtTypeNode::BorrowedStorage { storage, .. } => map(storage)?,
        CtTypeNode::Nominal { arguments, .. } => {
            for arg in arguments {
                if let CtTypeArgument::Type(id) = arg {
                    map(id)?;
                }
            }
        }
        CtTypeNode::Tuple(ids) | CtTypeNode::AnonymousSum(ids) => {
            for id in ids {
                map(id)?;
            }
        }
        CtTypeNode::FunctionPointer { parameters, result }
        | CtTypeNode::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | CtTypeNode::ForeignCallback { parameters, result } => {
            for id in parameters {
                map(id)?;
            }
            map(result)?;
        }
        CtTypeNode::Array { element, .. }
        | CtTypeNode::Address { inner: element, .. }
        | CtTypeNode::Slice(element)
        | CtTypeNode::Managed(element)
        | CtTypeNode::Desc(element)
        | CtTypeNode::OwnedStore(element) => map(element)?,
        CtTypeNode::ErrorUnion { error, result } => {
            map(error)?;
            map(result)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn canonical(
    reference: &CtTypeRef,
    budget: &mut CtExecutionBudget,
    keep_key: bool,
) -> CompileResult<(CtTypeRef, Vec<u8>)> {
    let mut work = Work { budget, charged: 0 };
    let result = run(reference, &mut work, keep_key);
    work.budget.release_temporary_bytes(work.charged);
    result
}
fn run(
    reference: &CtTypeRef,
    work: &mut Work<'_>,
    keep_key: bool,
) -> CompileResult<(CtTypeRef, Vec<u8>)> {
    if reference.root.0 as usize >= reference.nodes.len() {
        return Err(bad("CT scoped type root is missing"));
    }
    // Physical order/cycles are checked before occurrences. Owner references may
    // point forward and are deliberately excluded from this edge relation.
    for (index, node) in reference.nodes.iter().enumerate() {
        let edges = match node {
            CtTypeNode::Nominal { arguments, .. } => arguments.len(),
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
            CtTypeNode::FunctionPointer { parameters, .. }
            | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
            | CtTypeNode::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
            _ => 2,
        };
        work.pay(edges as u64 + 1, edges as u64 * 8)?;
        if children(node).iter().any(|id| id.0 as usize >= index) {
            return Err(bad("CT scoped graph has a physical forward edge or cycle"));
        }
    }
    work.pay(1, 256)?;
    let mut tasks = vec![Task::Enter(reference.root, 1)];
    let mut outputs: Vec<Built> = Vec::new();
    let mut scopes: Vec<Owner> = Vec::new();
    let mut tokens = 0usize;
    let mut ambient_tokens: Vec<(usize, u32, CtAmbientLifeReference)> = Vec::new();
    work.pay(reference.ambient_classes.len() as u64, 0)?;
    for row in &reference.ambient_classes {
        if !matches!(reference.nodes.get(row.owner.0 as usize),Some(CtTypeNode::BorrowedStorage{life,..}) if row.class<life.binder_count)
        {
            return Err(bad("ambient class mapping has no actual Fixed owner/class"));
        }
    }
    while let Some(task) = tasks.pop() {
        work.pay(1, 0)?;
        match task {
            Task::Enter(id, depth) => {
                if depth > work.budget.remaining().max_call_depth {
                    return Err(bound());
                }
                work.budget.reserve_generated_nodes(1)?;
                let node = &reference.nodes[id.0 as usize];
                let owner = match node {
                    CtTypeNode::BorrowedStorage { storage, life } => {
                        phantom_shape(reference, id, *storage, life, work)?;
                        Some((true, life.binder_count))
                    }
                    CtTypeNode::BorrowedFunctionPointer {
                        life, parameters, ..
                    } => {
                        flat_life(life, parameters.len(), work.budget)?;
                        Some((false, life.invocation_binder_count))
                    }
                    _ => None,
                };
                work.pay(1, 128)?;
                let token = owner.map(|(fixed, count)| {
                    let token = tokens;
                    tokens += 1;
                    scopes.push(Owner {
                        source: id,
                        token,
                        fixed,
                        count,
                    });
                    token
                });
                if let CtTypeNode::BorrowedStorage { life, .. } = node {
                    work.pay(reference.ambient_classes.len() as u64, 0)?;
                    let rows = reference
                        .ambient_classes
                        .iter()
                        .filter(|row| row.owner == id)
                        .count();
                    if rows != life.binder_count as usize {
                        return Err(bad("free Fixed classes require complete ambient mappings"));
                    }
                    work.pay(
                        (rows as u64).saturating_mul(reference.ambient_classes.len() as u64),
                        rows as u64
                            * std::mem::size_of::<(usize, u32, CtAmbientLifeReference)>() as u64,
                    )?;
                    for class in 0..life.binder_count {
                        let mut matching = reference
                            .ambient_classes
                            .iter()
                            .filter(|row| row.owner == id && row.class == class);
                        let row = matching
                            .next()
                            .ok_or_else(|| bad("ambient class mapping is missing"))?;
                        if matching.next().is_some() {
                            return Err(bad("ambient class mapping is duplicated"));
                        }
                        ambient_tokens.push((token.expect("Fixed owner token"), class, row.source));
                    }
                }
                let width = edge_width(node);
                work.pay(width as u64, width as u64 * 128)?;
                let mut ids = children(node);
                if matches!(node, CtTypeNode::AnonymousSum(_)) {
                    let mut flat = Vec::new();
                    while let Some(child) = ids.pop() {
                        work.pay(1, 8)?;
                        if let CtTypeNode::AnonymousSum(nested) = &reference.nodes[child.0 as usize]
                        {
                            work.pay(nested.len() as u64, nested.len() as u64 * 8)?;
                            ids.extend(nested.iter().copied());
                        } else {
                            flat.push(child);
                        }
                    }
                    ids = flat;
                }
                tasks.push(Task::Exit(id, token, ids.len()));
                for child in ids.into_iter().rev() {
                    tasks.push(Task::Enter(child, depth + 1));
                }
            }
            Task::Exit(id, token, count) => {
                let node = &reference.nodes[id.0 as usize];
                let split = outputs.len().checked_sub(count).ok_or_else(bound)?;
                let mut parts = outputs.split_off(split);
                if matches!(node, CtTypeNode::AnonymousSum(_)) {
                    if count < 2 {
                        return Err(bad("anonymous sum needs two members"));
                    }
                    let bytes = parts
                        .iter()
                        .try_fold(0u64, |n, p| add(n, p.key.len() as u64))?;
                    work.pay(mul(bytes, add(count as u64, 1)?)?, 0)?;
                    parts.sort_by(|a, b| a.key.cmp(&b.key));
                    if parts.windows(2).any(|p| p[0].key == p[1].key) {
                        return Err(bad("anonymous sum repeats a complete scoped member"));
                    }
                }
                let mut key = Vec::new();
                let tag = match node {
                    CtTypeNode::Primitive(_) => 0,
                    CtTypeNode::Builtin(_) => 1,
                    CtTypeNode::Nominal { .. } => 2,
                    CtTypeNode::Tuple(_) => 3,
                    CtTypeNode::Array { .. } => 4,
                    CtTypeNode::FunctionPointer { .. } => 5,
                    CtTypeNode::Address { .. } => 6,
                    CtTypeNode::Slice(_) => 7,
                    CtTypeNode::Managed(_) => 8,
                    CtTypeNode::Desc(_) => 9,
                    CtTypeNode::OwnedStore(_) => 10,
                    CtTypeNode::ForeignCallback { .. } => 11,
                    CtTypeNode::Opaque { .. } => 12,
                    CtTypeNode::ErrorUnion { .. } => 13,
                    CtTypeNode::BorrowedFunctionPointer { .. } => 14,
                    CtTypeNode::AnonymousSum(_) => 15,
                    CtTypeNode::BorrowedStorage { .. } => 16,
                };
                work.key_bytes(&mut key, &[tag])?;
                let mut references = Vec::new();
                match node {
                    CtTypeNode::Primitive(s) => work.key_bytes(&mut key, &[*s as u8])?,
                    CtTypeNode::Builtin(s) => work.key_bytes(&mut key, &[*s as u8])?,
                    CtTypeNode::Opaque { identity } => work.symbol(&mut key, identity)?,
                    CtTypeNode::Nominal { symbol, arguments } => {
                        work.symbol(&mut key, symbol)?;
                        work.number(&mut key, arguments.len() as u64)?;
                        let mut parts = parts.iter();
                        for (index, arg) in arguments.iter().enumerate() {
                            match arg {
                                CtTypeArgument::Type(_) => {
                                    work.key_bytes(&mut key, &[0])?;
                                    let part = parts.next().expect("physical Type child");
                                    work.number(&mut key, part.key.len() as u64)?;
                                    work.key_bytes(&mut key, &part.key)?;
                                }
                                CtTypeArgument::Scalar(c) => {
                                    work.key_bytes(&mut key, &[1])?;
                                    work.scalar(&mut key, c)?;
                                }
                                CtTypeArgument::Lifetime(value) => {
                                    work.key_bytes(&mut key, &[2])?;
                                    let token = work.reference(&mut key, *value, &scopes)?;
                                    work.pay(1, 16)?;
                                    references.push((index, token));
                                }
                            }
                        }
                    }
                    CtTypeNode::BorrowedFunctionPointer {
                        life, parameters, ..
                    } => {
                        let life = flat_life(life, parameters.len(), work.budget)?;
                        work.life(&mut key, &life)?
                    }
                    CtTypeNode::BorrowedStorage { life, .. } => {
                        work.storage(&mut key, life)?;
                        work.number(&mut key, u64::from(life.binder_count))?;
                        work.pay(ambient_tokens.len() as u64, 0)?;
                        for (owner, class, source) in &ambient_tokens {
                            if Some(*owner) == token {
                                work.number(&mut key, u64::from(*class))?;
                                work.number(&mut key, u64::from(source.scope.0))?;
                                work.number(&mut key, u64::from(source.parameter))?;
                            }
                        }
                    }
                    CtTypeNode::Array { length, .. } => {
                        work.number(&mut key, u64::from(*length))?
                    }
                    CtTypeNode::Address { mutable, .. } => {
                        work.key_bytes(&mut key, &[u8::from(*mutable)])?
                    }
                    _ => {}
                }
                if !matches!(node, CtTypeNode::Nominal { .. }) {
                    work.number(&mut key, parts.len() as u64)?;
                    for part in &parts {
                        work.number(&mut key, part.key.len() as u64)?;
                        work.key_bytes(&mut key, &part.key)?;
                    }
                }
                let node_count = parts
                    .iter()
                    .try_fold(1u64, |n, p| add(n, p.nodes.len() as u64))?;
                work.pay(node_count, mul(node_count, 160)?)?;
                let dynamic = match node {
                    CtTypeNode::BorrowedStorage { life, .. } => work.shape_copy(life)?,
                    CtTypeNode::BorrowedFunctionPointer { life, .. } => {
                        let bytes = life_bytes(life);
                        work.pay(bytes, bytes)?;
                        bytes
                    }
                    _ => 0,
                };
                let _ = dynamic;
                let mut nodes = Vec::new();
                let mut roots = Vec::new();
                for part in parts {
                    let offset = u32::try_from(nodes.len()).map_err(|_| bound())?;
                    let root = part.nodes.len().checked_sub(1).ok_or_else(bound)? as u32;
                    for mut pending in part.nodes {
                        let width = edge_width(&pending.node);
                        work.pay(width as u64, 0)?;
                        physical_remap(&mut pending.node, offset)?;
                        nodes.push(pending);
                    }
                    roots.push(CtTypeNodeId(root.checked_add(offset).ok_or_else(bound)?));
                }
                let mut copied = node.clone();
                let mut roots = roots.into_iter();
                match &mut copied {
                    CtTypeNode::BorrowedStorage { storage, .. } => {
                        *storage = roots.next().expect("storage child")
                    }
                    CtTypeNode::AnonymousSum(ids) => *ids = roots.collect(),
                    CtTypeNode::Nominal { arguments, .. } => {
                        for arg in arguments {
                            if let CtTypeArgument::Type(id) = arg {
                                *id = roots.next().expect("Type child");
                            }
                        }
                    }
                    _ => copied = remap(node, |_| Ok(roots.next().expect("constructor child")))?,
                }
                nodes.push(Pending {
                    node: copied,
                    owner: token,
                    references,
                });
                outputs.push(Built { nodes, key });
                if token.is_some() {
                    scopes.pop();
                }
            }
        }
    }
    let built = outputs.pop().ok_or_else(bound)?;
    work.pay(
        built.nodes.len() as u64 + tokens as u64,
        add(
            mul(tokens as u64, 8)?,
            mul(
                built.nodes.len() as u64,
                std::mem::size_of::<CtTypeNode>() as u64,
            )?,
        )?,
    )?;
    let mut owners = vec![CtTypeNodeId(0); tokens];
    for (index, pending) in built.nodes.iter().enumerate() {
        if let Some(token) = pending.owner {
            owners[token] = CtTypeNodeId(u32::try_from(index).map_err(|_| bound())?);
        }
    }
    let mut nodes = Vec::with_capacity(built.nodes.len());
    for mut pending in built.nodes {
        work.pay(edge_width(&pending.node) as u64, 0)?;
        if let CtTypeNode::Nominal { arguments, .. } = &mut pending.node {
            for (index, token) in pending.references {
                let CtTypeArgument::Lifetime(value) = &mut arguments[index] else {
                    unreachable!()
                };
                value.owner = owners[token];
            }
        }
        nodes.push(pending.node);
    }
    work.pay(
        ambient_tokens.len() as u64,
        ambient_tokens.len() as u64 * std::mem::size_of::<CtAmbientClassBinding>() as u64,
    )?;
    let mut ambient_classes = ambient_tokens
        .into_iter()
        .map(|(token, class, source)| CtAmbientClassBinding {
            owner: owners[token],
            class,
            source,
        })
        .collect::<Vec<_>>();
    work.pay(
        (ambient_classes.len() as u64).saturating_mul(ambient_classes.len() as u64),
        0,
    )?;
    ambient_classes.sort_by_key(|row| (row.owner, row.class));
    let ty = CtTypeRef {
        root: CtTypeNodeId(u32::try_from(nodes.len() - 1).map_err(|_| bound())?),
        nodes,
        ambient_classes,
    };
    let key = if keep_key { built.key } else { Vec::new() };
    // The returned value/key remain live after scratch is released.
    work.budget
        .reserve_temporary_bytes(add(retained_bytes(&ty), key.capacity() as u64)?)?;
    Ok((ty, key))
}
fn edge_width(node: &CtTypeNode) -> usize {
    match node {
        CtTypeNode::Nominal { arguments, .. } => arguments.len(),
        CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
        CtTypeNode::FunctionPointer { parameters, .. }
        | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
        | CtTypeNode::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
        _ => 2,
    }
}
pub(super) fn retained_bytes(ty: &CtTypeRef) -> u64 {
    super::retained_bytes(ty)
}

fn phantom_shape(
    reference: &CtTypeRef,
    owner: CtTypeNodeId,
    storage: CtTypeNodeId,
    shape: &StorageLifeShape,
    work: &mut Work<'_>,
) -> CompileResult<()> {
    if !shape.slots.is_empty() || !shape.retained.is_empty() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Ct,
            "CT4 physical stored-borrow relation consumer is not yet implemented",
            None,
        )]);
    }
    if shape.binder_count == 0 {
        return Err(bad("CT storage owner has an empty fixed domain"));
    }
    work.pay(
        shape.nominal_arguments.len() as u64,
        shape.nominal_arguments.len() as u64 * 8,
    )?;
    let mut pending = vec![(storage, Vec::<zil_ir::LifeTypeProjection>::new())];
    work.pay(1, 64)?;
    let mut row = 0usize;
    let mut dense = 0u32;
    while let Some((id, path)) = pending.pop() {
        work.pay(1, 0)?;
        let node = &reference.nodes[id.0 as usize];
        match node {
            CtTypeNode::Nominal { symbol, arguments } => {
                work.pay(arguments.len() as u64, arguments.len() as u64 * 64)?;
                for (index, arg) in arguments.iter().enumerate() {
                    if let CtTypeArgument::Lifetime(value) = arg {
                        if value.owner != owner {
                            return Err(bad(
                                "phantom fixed lifetime refers to a different actual owner",
                            ));
                        }
                        let StorageBinder::Fixed(binder) = value.binder else {
                            return Err(bad("fixed nominal lifetime has an invocation domain"));
                        };
                        let actual = shape
                            .nominal_arguments
                            .get(row)
                            .ok_or_else(|| bad("CT storage omits a nominal lifetime position"))?;
                        work.pay(path.len() as u64 + actual.path.len() as u64, 0)?;
                        if actual.declaration.compiler {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Ct,
                                "compiler nominal lifetime requires authentic origin inventory",
                                None,
                            )]);
                        }
                        if actual.path != path
                            || actual.declaration.module != symbol.module
                            || actual.declaration.declaration != symbol.declaration
                            || actual.argument as usize != index
                            || actual.binder != binder
                            || binder > dense
                            || binder >= shape.binder_count
                        {
                            return Err(bad(
                                "CT storage nominal lifetime row differs from its actual ordered position",
                            ));
                        }
                        if binder == dense {
                            dense += 1;
                        }
                        row += 1;
                    }
                }
                for (index, arg) in arguments.iter().enumerate().rev() {
                    if let CtTypeArgument::Type(child) = arg {
                        work.pay(path.len() as u64 + 1, (path.len() as u64 + 1) * 16 + 64)?;
                        let mut next = path.clone();
                        next.push(zil_ir::LifeTypeProjection::TypeArgument(index as u32));
                        pending.push((*child, next));
                    }
                }
            }
            CtTypeNode::BorrowedStorage { .. }
            | CtTypeNode::Address { .. }
            | CtTypeNode::Desc(_) => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "nested or physical borrowed metadata needs the complete stored relation consumer",
                    None,
                )]);
            }
            CtTypeNode::BorrowedFunctionPointer { .. } => {
                // A fresh invocation contract cannot bind an enclosing fixed
                // relation. Its own complete subtree is checked independently.
                let mut ids = vec![id];
                work.pay(1, 8)?;
                while let Some(child) = ids.pop() {
                    let nested = &reference.nodes[child.0 as usize];
                    let width = edge_width(nested);
                    work.pay(width as u64, width as u64 * 8)?;
                    if let CtTypeNode::Nominal { arguments, .. } = nested
                        && arguments.iter().any(|a|matches!(a,CtTypeArgument::Lifetime(v) if matches!(v.binder,StorageBinder::Fixed(_)))) {
                            return Err(vec![Diagnostic::unsupported(Phase::Ct,"borrowed-code signature cannot erase a fixed outer relation",None)]);
                    }
                    ids.extend(children(nested));
                }
            }
            _ => {
                let width = edge_width(node);
                work.pay(width as u64, width as u64 * 64)?;
                for (index, child) in children(node).into_iter().enumerate().rev() {
                    use zil_ir::{LifeTypeProjection as P, PlaceProjection as Q};
                    let step = match node {
                        CtTypeNode::Tuple(_) => P::Place(Q::Field(index as u32)),
                        CtTypeNode::Array { .. } | CtTypeNode::Slice(_) => P::Element,
                        CtTypeNode::FunctionPointer { parameters, .. }
                        | CtTypeNode::ForeignCallback { parameters, .. } => {
                            if index < parameters.len() {
                                P::Parameter(index as u32)
                            } else {
                                P::Result
                            }
                        }
                        CtTypeNode::ErrorUnion { .. } => P::Place(Q::ErrorArm(index == 0)),
                        CtTypeNode::Managed(_) | CtTypeNode::OwnedStore(_) => P::Pointee,
                        CtTypeNode::AnonymousSum(_) => {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Ct,
                                "stored anonymous sum metadata needs active-member relation validation",
                                None,
                            )]);
                        }
                        _ => continue,
                    };
                    work.pay(path.len() as u64 + 1, (path.len() as u64 + 1) * 16 + 64)?;
                    let mut next = path.clone();
                    next.push(step);
                    pending.push((child, next));
                }
            }
        }
    }
    if row != shape.nominal_arguments.len() || dense != shape.binder_count {
        return Err(bad(
            "CT storage has extra nominal rows or unused fixed classes",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId, ResultCategory};
    fn budget() -> CtExecutionBudget {
        CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: 10000,
            max_call_depth: 256,
            max_live_cells: 10000,
            max_temporary_bytes: 10_000_000,
            max_generated_nodes: 100000,
            max_instances: 1000,
            max_analysis_work: 10_000_000,
        })
    }
    fn graph() -> CtTypeRef {
        let mut nodes = Vec::new();
        for base in [0, 3] {
            nodes.push(CtTypeNode::Nominal {
                symbol: CtSymbolId {
                    module: ModuleId(1),
                    declaration: NodeId(7),
                    generation_path: None,
                },
                arguments: vec![CtTypeArgument::Lifetime(CtLifetimeArgument {
                    owner: CtTypeNodeId(base + 2),
                    binder: StorageBinder::Invocation(0),
                })],
            });
            nodes.push(CtTypeNode::Address {
                mutable: false,
                inner: CtTypeNodeId(base),
            });
            nodes.push(CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(base + 1)],
                result: CtTypeNodeId(base + 1),
                life: zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 1,
                    parameter_lives: vec![Some(0)],
                    result_life: Some(0),
                })
                .unwrap(),
            });
        }
        nodes.push(CtTypeNode::Tuple(vec![CtTypeNodeId(2), CtTypeNodeId(5)]));
        CtTypeRef {
            root: CtTypeNodeId(6),
            nodes,
            ambient_classes: Vec::new(),
        }
    }
    #[test]
    fn scoped_occurrences_relocate_equal_sibling_scopes_without_interning_children() {
        let input = graph();
        let mut first = budget();
        let (output, key) = canonical(&input, &mut first, true).unwrap();
        let exact = first.usage().analysis_work;
        assert_eq!(
            canonical(&output, &mut budget(), true).unwrap(),
            (output.clone(), key)
        );
        let CtTypeNode::Tuple(members) = &output.nodes[output.root.0 as usize] else {
            unreachable!()
        };
        assert_ne!(members[0], members[1]);
        for root in members {
            let CtTypeNode::BorrowedFunctionPointer {
                parameters, result, ..
            } = &output.nodes[root.0 as usize]
            else {
                unreachable!()
            };
            for address in parameters.iter().chain([result]) {
                let CtTypeNode::Address { inner, .. } = &output.nodes[address.0 as usize] else {
                    unreachable!()
                };
                let CtTypeNode::Nominal { arguments, .. } = &output.nodes[inner.0 as usize] else {
                    unreachable!()
                };
                let CtTypeArgument::Lifetime(value) = arguments[0] else {
                    unreachable!()
                };
                assert_eq!(value.owner, *root);
            }
        }
        for (work, pass) in [(exact, true), (exact - 1, false), (exact, true)] {
            let mut declared = budget().remaining();
            declared.max_analysis_work = work;
            let result = canonical(&input, &mut CtExecutionBudget::new(declared), true);
            assert_eq!(result.is_ok(), pass);
            if !pass {
                assert_eq!(
                    result.unwrap_err()[0].category,
                    ResultCategory::ResourceLimit
                );
            }
        }
    }
    #[test]
    fn scoped_shared_child_cannot_refer_to_equal_sibling_owner() {
        let mut input = graph();
        input.nodes[4] = CtTypeNode::Address {
            mutable: false,
            inner: CtTypeNodeId(0),
        };
        let errors = canonical(&input, &mut budget(), true).unwrap_err();
        assert!(errors[0].message.contains("enclosing scope"));
        for binder in [StorageBinder::Fixed(0), StorageBinder::Invocation(1)] {
            let mut wrong = graph();
            let CtTypeNode::Nominal { arguments, .. } = &mut wrong.nodes[0] else {
                unreachable!()
            };
            let CtTypeArgument::Lifetime(value) = &mut arguments[0] else {
                unreachable!()
            };
            value.binder = binder;
            assert!(canonical(&wrong, &mut budget(), true).is_err());
        }
    }
}
