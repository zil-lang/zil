//! Independent CT4 scope admission. Physical postorder is separate from semantic
//! owner references. No CT producer or normalizer is called from this kernel.
use super::*;
use zil_ir::{LifeTypeProjection as P, PlaceProjection as Q, StorageBinder, StorageLifeShape};
#[derive(Clone, Copy)]
struct Scope {
    id: CtTypeNodeId,
    fixed: bool,
    count: u32,
}
enum Visit {
    Enter(CtTypeNodeId, usize),
    Exit(CtTypeNodeId, usize, bool),
}
impl Identity<'_, '_> {
    pub(super) fn scoped_graph(&mut self, ty: &CtTypeRef, allow_fixed: bool) -> Check<Vec<u8>> {
        let count = ty.nodes.len();
        if count == 0 || ty.root.0 as usize != count - 1 {
            return Err(reject(None, "CT4 metadata root is not canonical"));
        }
        if count > self.limits.max_nodes || count as u64 > self.raw.budget.max_generated_nodes {
            return Err(limit(None, "CT4 metadata node quota exceeded"));
        }
        self.reserve(
            count
                .saturating_mul(64)
                .saturating_add(ty.ambient_classes.len().saturating_mul(32)),
        )?;
        let mut previous = None;
        for row in &ty.ambient_classes {
            let key = (row.owner, row.class);
            if previous.is_some_and(|old| old >= key) {
                return Err(reject(
                    None,
                    "ambient mapping order or duplicate is invalid",
                ));
            }
            previous = Some(key);
            if !matches!(ty.nodes.get(row.owner.0 as usize),Some(CtTypeNode::BorrowedStorage{life,..}) if row.class<life.binder_count)
            {
                return Err(reject(
                    None,
                    "ambient class mapping does not select an actual Fixed class",
                ));
            }
            if row.source.scope.0 as usize >= self.raw.ambient_life_scopes.len() {
                return Err(reject(None, "ambient mapping has no scope inventory row"));
            }
        }
        let mut widths: Vec<usize> = Vec::new();
        for (index, node) in ty.nodes.iter().enumerate() {
            let ids = self.scoped_children(node)?;
            let mut cursor = index;
            for child in ids.iter().rev() {
                let id = child.0 as usize;
                if id >= index || cursor.checked_sub(1) != Some(id) {
                    return Err(reject(
                        None,
                        "CT4 graph has noncanonical physical sharing, order or cycle",
                    ));
                }
                cursor = cursor
                    .checked_sub(widths[id])
                    .ok_or_else(|| reject(None, "CT4 subtrees overlap"))?;
            }
            widths.push(index + 1 - cursor);
        }
        if widths[count - 1] != count {
            return Err(reject(None, "CT4 graph has unreachable nodes"));
        }
        self.reserve(128)?;
        let mut visits = vec![Visit::Enter(ty.root, 1)];
        let mut scopes: Vec<Scope> = Vec::new();
        let mut outputs: Vec<Vec<u8>> = Vec::new();
        while let Some(visit) = visits.pop() {
            self.meter.charge(1, None)?;
            match visit {
                Visit::Enter(id, depth) => {
                    if depth > self.limits.max_call_depth
                        || depth > self.raw.budget.max_call_depth as usize
                    {
                        return Err(limit(None, "CT4 scoped metadata depth exceeded"));
                    }
                    let node = &ty.nodes[id.0 as usize];
                    let owner = match node {
                        CtTypeNode::BorrowedStorage { storage, life } => {
                            if !allow_fixed && ty.ambient_classes.is_empty() {
                                return Err(reject(
                                    None,
                                    "fixed metadata argument requires one joint schema context",
                                ));
                            }
                            self.scoped_phantom_shape(ty, id, *storage, life)?;
                            self.meter.charge(ty.ambient_classes.len(), None)?;
                            let rows = ty
                                .ambient_classes
                                .iter()
                                .filter(|row| row.owner == id)
                                .count();
                            if rows != life.binder_count as usize {
                                return Err(reject(
                                    None,
                                    "free Fixed classes lack complete ambient mappings",
                                ));
                            }
                            self.meter
                                .charge(rows.saturating_mul(ty.ambient_classes.len()), None)?;
                            for class in 0..life.binder_count {
                                if ty
                                    .ambient_classes
                                    .iter()
                                    .filter(|row| row.owner == id && row.class == class)
                                    .count()
                                    != 1
                                {
                                    return Err(reject(
                                        None,
                                        "ambient class mapping missing or repeated",
                                    ));
                                }
                            }
                            Some((true, life.binder_count))
                        }
                        CtTypeNode::BorrowedFunctionPointer { life, .. } => {
                            Some((false, life.invocation_binder_count))
                        }
                        _ => None,
                    };
                    self.reserve(64)?;
                    if let Some((fixed, count)) = owner {
                        scopes.push(Scope { id, fixed, count });
                    }
                    let ids = self.scoped_children(node)?;
                    self.reserve(ids.len().saturating_mul(64))?;
                    visits.push(Visit::Exit(id, ids.len(), owner.is_some()));
                    for child in ids.into_iter().rev() {
                        visits.push(Visit::Enter(child, depth + 1));
                    }
                }
                Visit::Exit(id, count, owner) => {
                    let node = &ty.nodes[id.0 as usize];
                    let parts = outputs.split_off(
                        outputs
                            .len()
                            .checked_sub(count)
                            .ok_or_else(|| reject(None, "CT4 key child stack is invalid"))?,
                    );
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
                    self.put(&mut key, &[tag])?;
                    match node {
                        CtTypeNode::Primitive(s) => self.put(&mut key, &[*s as u8])?,
                        CtTypeNode::Builtin(s) => self.put(&mut key, &[*s as u8])?,
                        CtTypeNode::Opaque { identity } => self.graph_symbol(&mut key, identity)?,
                        CtTypeNode::Nominal { symbol, arguments } => {
                            self.graph_symbol(&mut key, symbol)?;
                            self.number(&mut key, arguments.len() as u64)?;
                            let mut children = parts.iter();
                            for argument in arguments {
                                match argument {
                                    CtTypeArgument::Type(_) => {
                                        self.put(&mut key, &[0])?;
                                        let child = children.next().ok_or_else(|| {
                                            reject(None, "CT4 type argument child is missing")
                                        })?;
                                        self.number(&mut key, child.len() as u64)?;
                                        self.put(&mut key, child)?;
                                    }
                                    CtTypeArgument::Scalar(c) => {
                                        self.put(&mut key, &[1])?;
                                        self.scalar(&mut key, c)?;
                                    }
                                    CtTypeArgument::Lifetime(value) => {
                                        self.put(&mut key, &[2])?;
                                        self.meter.charge(scopes.len().saturating_add(1), None)?;
                                        let (depth,scope)=scopes.iter().rev().enumerate().find(|(_,s)|s.id==value.owner).ok_or_else(||reject(None,"CT4 lifetime owner is not an actual enclosing scope"))?;
                                        let (fixed, index) = match value.binder {
                                            StorageBinder::Fixed(n) => (true, n),
                                            StorageBinder::Invocation(n) => (false, n),
                                        };
                                        if fixed != scope.fixed || index >= scope.count {
                                            return Err(reject(
                                                None,
                                                "CT4 lifetime domain or ordinal is invalid",
                                            ));
                                        }
                                        self.number(&mut key, depth as u64)?;
                                        self.put(&mut key, &[u8::from(fixed)])?;
                                        self.number(&mut key, u64::from(index))?;
                                    }
                                }
                            }
                        }
                        CtTypeNode::BorrowedFunctionPointer {
                            parameters,
                            result,
                            life,
                        } => self.graph_life(&ty.nodes, parameters, *result, life, &mut key)?,
                        CtTypeNode::BorrowedStorage { life, .. } => {
                            self.scoped_storage_key(&mut key, life)?;
                            self.number(&mut key, u64::from(life.binder_count))?;
                            self.meter.charge(ty.ambient_classes.len(), None)?;
                            for row in &ty.ambient_classes {
                                if row.owner == id {
                                    self.number(&mut key, u64::from(row.class))?;
                                    self.number(&mut key, u64::from(row.source.scope.0))?;
                                    self.number(&mut key, u64::from(row.source.parameter))?;
                                }
                            }
                        }
                        CtTypeNode::Array { length, .. } => {
                            self.number(&mut key, u64::from(*length))?
                        }
                        CtTypeNode::Address { mutable, .. } => {
                            self.put(&mut key, &[u8::from(*mutable)])?
                        }
                        CtTypeNode::AnonymousSum(ids) => {
                            if ids.len() < 2
                                || ids.iter().any(|id| {
                                    matches!(ty.nodes[id.0 as usize], CtTypeNode::AnonymousSum(_))
                                })
                            {
                                return Err(reject(
                                    None,
                                    "CT4 anonymous sum is not flat or has too few members",
                                ));
                            }
                            for pair in parts.windows(2) {
                                self.meter
                                    .charge(pair[0].len().saturating_add(pair[1].len()), None)?;
                                if pair[0] >= pair[1] {
                                    return Err(reject(
                                        None,
                                        "CT4 anonymous member keys repeat or are not ordered",
                                    ));
                                }
                            }
                        }
                        _ => {}
                    }
                    if !matches!(node, CtTypeNode::Nominal { .. }) {
                        self.number(&mut key, parts.len() as u64)?;
                        for child in parts {
                            self.number(&mut key, child.len() as u64)?;
                            self.put(&mut key, &child)?;
                        }
                    }
                    outputs.push(key);
                    if owner {
                        scopes.pop();
                    }
                }
            }
        }
        outputs
            .pop()
            .ok_or_else(|| reject(None, "CT4 metadata key is empty"))
    }
    fn scoped_children(&mut self, node: &CtTypeNode) -> Check<Vec<CtTypeNodeId>> {
        let width = match node {
            CtTypeNode::Nominal { arguments, .. } => arguments.len(),
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
            CtTypeNode::FunctionPointer { parameters, .. }
            | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
            | CtTypeNode::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
            _ => 2,
        };
        self.reserve(width.saturating_mul(16))?;
        Ok(match node {
            CtTypeNode::BorrowedStorage { storage, .. } => vec![*storage],
            CtTypeNode::Nominal { arguments, .. } => arguments
                .iter()
                .filter_map(|a| {
                    if let CtTypeArgument::Type(id) = a {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect(),
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.clone(),
            CtTypeNode::FunctionPointer { parameters, result }
            | CtTypeNode::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | CtTypeNode::ForeignCallback { parameters, result } => {
                parameters.iter().copied().chain([*result]).collect()
            }
            CtTypeNode::Array { element, .. }
            | CtTypeNode::Address { inner: element, .. }
            | CtTypeNode::Slice(element)
            | CtTypeNode::Managed(element)
            | CtTypeNode::Desc(element)
            | CtTypeNode::OwnedStore(element) => vec![*element],
            CtTypeNode::ErrorUnion { error, result } => vec![*error, *result],
            _ => vec![],
        })
    }
    fn scoped_path(&mut self, key: &mut Vec<u8>, path: &[P]) -> Check<()> {
        self.number(key, path.len() as u64)?;
        for step in path {
            let (tag, n) = match step {
                P::Place(Q::Field(n)) => (0, u64::from(*n)),
                P::Place(Q::Variant(v)) => {
                    self.put(key, &[1])?;
                    self.number(key, u64::from(v.variant))?;
                    self.number(key, u64::from(v.index))?;
                    continue;
                }
                P::Place(Q::ErrorArm(v)) => (2, u64::from(*v)),
                P::TypeArgument(n) => (3, u64::from(*n)),
                P::Port(n) => (4, u64::from(*n)),
                P::AssociatedType(n) => (5, u64::from(*n)),
                P::Pointee => (6, 0),
                P::Element => (7, 0),
                P::Parameter(n) => (8, u64::from(*n)),
                P::Result => (9, 0),
            };
            self.put(key, &[tag])?;
            self.number(key, n)?;
        }
        Ok(())
    }
    fn scoped_storage_key(&mut self, key: &mut Vec<u8>, shape: &StorageLifeShape) -> Check<()> {
        self.number(key, u64::from(shape.binder_count))?;
        self.number(key, shape.slots.len() as u64)?;
        for row in &shape.slots {
            self.number(key, row.path.len() as u64)?;
            for step in &row.path {
                self.scoped_path(key, &[P::Place(*step)])?;
            }
            self.number(key, u64::from(row.binder))?;
            self.put(key, &[row.mode as u8])?;
        }
        self.number(key, shape.retained.len() as u64)?;
        for row in &shape.retained {
            self.scoped_path(key, &row.path)?;
            self.number(key, u64::from(row.class))?;
            self.number(key, u64::from(row.binder))?;
        }
        self.number(key, shape.nominal_arguments.len() as u64)?;
        for row in &shape.nominal_arguments {
            self.scoped_path(key, &row.path)?;
            self.number(key, u64::from(row.declaration.module.0))?;
            self.number(key, u64::from(row.declaration.declaration.0))?;
            self.put(key, &[u8::from(row.declaration.compiler)])?;
            self.number(key, u64::from(row.argument))?;
            self.number(key, u64::from(row.binder))?;
        }
        Ok(())
    }
    fn scoped_phantom_shape(
        &mut self,
        ty: &CtTypeRef,
        owner: CtTypeNodeId,
        storage: CtTypeNodeId,
        shape: &StorageLifeShape,
    ) -> Check<()> {
        if !shape.slots.is_empty() || !shape.retained.is_empty() {
            return Err(pending_scoped_identity());
        }
        if shape.binder_count == 0 {
            return Err(reject(None, "CT4 fixed domain is empty"));
        }
        self.reserve(64)?;
        let mut pending = vec![(storage, Vec::<P>::new())];
        let mut row = 0;
        let mut dense = 0;
        while let Some((id, path)) = pending.pop() {
            self.meter.charge(1, None)?;
            let node = &ty.nodes[id.0 as usize];
            match node {
                CtTypeNode::Nominal { symbol, arguments } => {
                    self.reserve(arguments.len().saturating_mul(64))?;
                    for (index, argument) in arguments.iter().enumerate() {
                        if let CtTypeArgument::Lifetime(value) = argument {
                            let actual = shape.nominal_arguments.get(row).ok_or_else(|| {
                                reject(None, "CT4 fixed shape omits a nominal position")
                            })?;
                            if actual.declaration.compiler {
                                return Err(pending_scoped_identity());
                            }
                            let StorageBinder::Fixed(binder) = value.binder else {
                                return Err(reject(
                                    None,
                                    "CT4 fixed nominal actual has invocation domain",
                                ));
                            };
                            self.meter
                                .charge(path.len().saturating_add(actual.path.len()), None)?;
                            if value.owner != owner
                                || actual.path != path
                                || actual.declaration.module != symbol.module
                                || actual.declaration.declaration != symbol.declaration
                                || actual.argument as usize != index
                                || actual.binder != binder
                                || binder > dense
                                || binder >= shape.binder_count
                            {
                                return Err(reject(
                                    None,
                                    "CT4 nominal fixed row is not its exact actual position",
                                ));
                            }
                            if binder == dense {
                                dense += 1;
                            }
                            row += 1;
                        }
                    }
                    for (index, argument) in arguments.iter().enumerate().rev() {
                        if let CtTypeArgument::Type(child) = argument {
                            self.reserve(
                                path.len()
                                    .saturating_add(1)
                                    .saturating_mul(16)
                                    .saturating_add(64),
                            )?;
                            let mut next = path.clone();
                            next.push(P::TypeArgument(index as u32));
                            pending.push((*child, next));
                        }
                    }
                }
                CtTypeNode::BorrowedStorage { .. }
                | CtTypeNode::Address { .. }
                | CtTypeNode::Desc(_) => return Err(pending_scoped_identity()),
                CtTypeNode::BorrowedFunctionPointer { .. } => {
                    self.reserve(8)?;
                    let mut ids = vec![id];
                    while let Some(child) = ids.pop() {
                        let node = &ty.nodes[child.0 as usize];
                        let children = self.scoped_children(node)?;
                        if let CtTypeNode::Nominal { arguments, .. } = node
                            && arguments.iter().any(|a|matches!(a,CtTypeArgument::Lifetime(v)if matches!(v.binder,StorageBinder::Fixed(_)))) {return Err(pending_scoped_identity());}
                        ids.extend(children);
                    }
                }
                _ => {
                    for (index, child) in self.scoped_children(node)?.into_iter().enumerate().rev()
                    {
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
                            CtTypeNode::AnonymousSum(_) => return Err(pending_scoped_identity()),
                            _ => continue,
                        };
                        self.reserve(
                            path.len()
                                .saturating_add(1)
                                .saturating_mul(16)
                                .saturating_add(64),
                        )?;
                        let mut next = path.clone();
                        next.push(step);
                        pending.push((child, next));
                    }
                }
            }
        }
        if row != shape.nominal_arguments.len() || dense != shape.binder_count {
            return Err(reject(
                None,
                "CT4 fixed domain has extra rows or unused classes",
            ));
        }
        Ok(())
    }
    pub(super) fn context(
        &mut self,
        out: &mut Vec<u8>,
        symbol: &CtSymbolId,
        arguments: &[CtSchemaArgument],
        context: &CtTypeRef,
    ) -> Check<()> {
        self.meter.charge(
            context
                .nodes
                .len()
                .saturating_add(arguments.len())
                .saturating_add(1),
            None,
        )?;
        let Some(CtTypeNode::BorrowedStorage { storage, .. }) =
            context.nodes.get(context.root.0 as usize)
        else {
            return Err(reject(
                None,
                "CT4 schema context has no fixed nominal owner",
            ));
        };
        let Some(CtTypeNode::Nominal {
            symbol: actual,
            arguments: actuals,
        }) = context.nodes.get(storage.0 as usize)
        else {
            return Err(reject(
                None,
                "CT4 schema context root is not a nominal application",
            ));
        };
        if symbol != actual || arguments.len() != actuals.len() {
            return Err(reject(None, "CT4 schema context name or arity differs"));
        }
        for (argument, actual) in arguments.iter().zip(actuals) {
            let same = match (argument, actual) {
                (CtSchemaArgument::TypeInContext(a), CtTypeArgument::Type(b)) => a == b,
                (CtSchemaArgument::Scalar(a), CtTypeArgument::Scalar(b)) => {
                    self.scalar(&mut Vec::new(), a)?;
                    a == b
                }
                (CtSchemaArgument::Lifetime(a), CtTypeArgument::Lifetime(b)) => a == b,
                _ => false,
            };
            if !same {
                return Err(reject(
                    None,
                    "CT4 schema actual is not the exact ordered context projection",
                ));
            }
        }
        let key = self.scoped_graph(context, true)?;
        self.put(out, &[2])?;
        self.number(out, key.len() as u64)?;
        self.put(out, &key)
    }
}
