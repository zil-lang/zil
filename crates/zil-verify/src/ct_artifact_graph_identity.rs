//! Canonical postorder metadata admission, independent of the CT producer.
use super::*;
impl Identity<'_, '_> {
    pub(super) fn graph(&mut self, ty: &CtTypeRef) -> Check<Vec<u8>> {
        if !ty.ambient_classes.is_empty() {
            return Err(pending_scoped_identity());
        }

        let count = ty.nodes.len();
        if count == 0 || ty.root.0 as usize != count - 1 {
            return Err(reject(None, "nominal type graph has a noncanonical root"));
        }
        if count > self.limits.max_nodes || count as u64 > self.raw.budget.max_generated_nodes {
            return Err(limit(None, "nominal type graph exceeds node quota"));
        }
        self.reserve(count.saturating_mul(64))?;
        if ty.nodes.iter().any(|n|matches!(n,CtTypeNode::BorrowedStorage {..})||matches!(n,CtTypeNode::Nominal {arguments,..}if arguments.iter().any(|a|matches!(a,CtTypeArgument::Lifetime(_))))) {
            return self.scoped_graph(ty,false);
        }

        let mut keys: Vec<Vec<u8>> = Vec::new();
        let mut widths: Vec<usize> = Vec::new();
        let mut depths: Vec<usize> = Vec::new();
        for (index, node) in ty.nodes.iter().enumerate() {
            let edges = match node {
                CtTypeNode::Nominal { arguments, .. } => arguments.len(),
                CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
                CtTypeNode::FunctionPointer { parameters, .. }
                | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
                | CtTypeNode::ForeignCallback { parameters, .. } => {
                    parameters.len().saturating_add(1)
                }
                _ => 2,
            };
            self.reserve(edges.saturating_mul(16))?;
            let children: Vec<CtTypeNodeId> = match node {
                CtTypeNode::BorrowedStorage { storage, .. } => vec![*storage],
                CtTypeNode::Nominal { arguments, .. } => arguments
                    .iter()
                    .filter_map(|arg| match arg {
                        CtTypeArgument::Type(id) => Some(*id),
                        _ => None,
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
                | CtTypeNode::Slice(element)
                | CtTypeNode::Managed(element)
                | CtTypeNode::Desc(element)
                | CtTypeNode::OwnedStore(element) => vec![*element],
                CtTypeNode::Address { inner, .. } => vec![*inner],
                CtTypeNode::ErrorUnion { error, result } => vec![*error, *result],
                CtTypeNode::Primitive(_) | CtTypeNode::Builtin(_) | CtTypeNode::Opaque { .. } => {
                    Vec::new()
                }
            };
            let mut cursor = index;
            let mut depth = 1usize;
            for child in children.iter().rev() {
                let id = child.0 as usize;
                if id >= index {
                    return Err(reject(
                        None,
                        "nominal type graph contains a cycle or forward edge",
                    ));
                }
                if cursor.checked_sub(1) != Some(id) {
                    return Err(reject(
                        None,
                        "nominal type graph has noncanonical sharing or node order",
                    ));
                }
                cursor = cursor
                    .checked_sub(widths[id])
                    .ok_or_else(|| reject(None, "nominal type subtree overlaps"))?;
                depth = depth.max(depths[id].saturating_add(1));
            }
            if depth > self.limits.max_call_depth || depth > self.raw.budget.max_call_depth as usize
            {
                return Err(limit(None, "nominal type metadata depth exceeded"));
            }
            let mut key = Vec::new();
            let tag = match node {
                CtTypeNode::BorrowedStorage { .. } => return Err(pending_scoped_identity()),
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
            };
            self.put(&mut key, &[tag])?;
            match node {
                CtTypeNode::Primitive(s) => self.put(&mut key, &[*s as u8])?,
                CtTypeNode::Builtin(s) => self.put(&mut key, &[*s as u8])?,
                CtTypeNode::Nominal { symbol, arguments } => {
                    self.graph_symbol(&mut key, symbol)?;
                    self.number(&mut key, arguments.len() as u64)?;
                    for argument in arguments {
                        match argument {
                            CtTypeArgument::Type(id) => {
                                self.put(&mut key, &[0])?;
                                let child = &keys[id.0 as usize];
                                self.number(&mut key, child.len() as u64)?;
                                self.put(&mut key, child)?;
                            }
                            CtTypeArgument::Lifetime(_) => return Err(pending_scoped_identity()),
                            CtTypeArgument::Scalar(value) => {
                                self.put(&mut key, &[1])?;
                                self.scalar(&mut key, value)?;
                            }
                        }
                    }
                }
                CtTypeNode::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => {
                    self.graph_life(&ty.nodes, parameters, *result, life, &mut key)?;
                }
                CtTypeNode::Opaque { identity } => self.graph_symbol(&mut key, identity)?,
                CtTypeNode::Array { length, .. } => self.number(&mut key, u64::from(*length))?,
                CtTypeNode::Address { mutable, .. } => self.put(&mut key, &[u8::from(*mutable)])?,
                CtTypeNode::AnonymousSum(_) => {
                    if children.len() < 2 {
                        return Err(reject(None, "nominal anonymous sum needs two members"));
                    }
                    for child in &children {
                        if matches!(ty.nodes[child.0 as usize], CtTypeNode::AnonymousSum(_)) {
                            return Err(reject(None, "nominal anonymous sum is not flattened"));
                        }
                    }
                    for pair in children.windows(2) {
                        let a = &keys[pair[0].0 as usize];
                        let b = &keys[pair[1].0 as usize];
                        self.meter.charge(a.len().saturating_add(b.len()), None)?;
                        if a >= b {
                            return Err(reject(
                                None,
                                "nominal anonymous members repeat or are not ordered",
                            ));
                        }
                    }
                }
                _ => {}
            }
            if !matches!(node, CtTypeNode::Nominal { .. }) {
                self.number(&mut key, children.len() as u64)?;
                for child in &children {
                    let c = &keys[child.0 as usize];
                    self.number(&mut key, c.len() as u64)?;
                    self.put(&mut key, c)?;
                }
            }
            widths.push(index + 1 - cursor);
            depths.push(depth);
            keys.push(key);
        }
        if widths[count - 1] != count {
            return Err(reject(
                None,
                "nominal type graph contains unreachable nodes",
            ));
        }
        Ok(keys.pop().expect("nonempty metadata graph"))
    }
    pub(super) fn graph_life(
        &mut self,
        nodes: &[CtTypeNode],
        parameters: &[CtTypeNodeId],
        result: CtTypeNodeId,
        life: &zil_ir::StorageLifeContract,
        out: &mut Vec<u8>,
    ) -> Check<()> {
        self.reserve(
            parameters
                .len()
                .saturating_mul(16)
                .saturating_add(life.bindings.len().saturating_mul(64))
                .saturating_add(life.classes.len().saturating_mul(96)),
        )?;
        for row in &life.bindings {
            self.reserve(row.position.path.len().saturating_mul(16))?;
        }
        for row in &life.classes {
            self.reserve(row.position.path.len().saturating_mul(16))?;
        }
        let life = life.to_flat(parameters.len()).ok_or_else(|| {
            vec![Diagnostic::unsupported(
                Phase::Ct,
                "rich CT5 graph signature consumer is not yet implemented",
                None,
            )]
        })?;
        self.reserve(
            life.parameter_lives
                .len()
                .saturating_mul(16)
                .saturating_add(32),
        )?;
        if life.parameter_lives.len() != parameters.len() || life.binder_count == 0 {
            return Err(reject(
                None,
                "invalid borrowed-code metadata lifetime arity",
            ));
        }
        let mut dense = 0u32;
        for (id, label) in parameters.iter().zip(&life.parameter_lives) {
            match &nodes[id.0 as usize] {
                CtTypeNode::Address { .. } => {
                    let tag = label.ok_or_else(|| {
                        reject(None, "borrowed-code metadata address lacks lifetime")
                    })?;
                    if tag > dense || tag >= life.binder_count {
                        return Err(reject(None, "borrowed-code lifetime map is not canonical"));
                    }
                    if tag == dense {
                        dense += 1;
                    }
                }
                _ if label.is_none() => {}
                _ => return Err(reject(None, "borrowed-code scalar has an address lifetime")),
            }
        }
        if dense != life.binder_count {
            return Err(reject(
                None,
                "borrowed-code metadata has an unused lifetime binder",
            ));
        }
        match &nodes[result.0 as usize] {
            CtTypeNode::Address { mutable, .. } => {
                let tag = life
                    .result_life
                    .ok_or_else(|| reject(None, "borrowed-code metadata result lacks lifetime"))?;
                if tag>=dense || !parameters.iter().zip(&life.parameter_lives).any(|(id,label)|*label==Some(tag)&&matches!(&nodes[id.0 as usize],CtTypeNode::Address{mutable:authority,..}if !mutable||*authority)){return Err(reject(None,"borrowed-code metadata result is ungrounded"));}
            }
            _ if life.result_life.is_none() => {}
            _ => {
                return Err(reject(
                    None,
                    "borrowed-code metadata value result has lifetime",
                ));
            }
        }
        self.number(out, u64::from(life.binder_count))?;
        self.number(out, life.parameter_lives.len() as u64)?;
        for label in &life.parameter_lives {
            self.put(out, &[u8::from(label.is_some())])?;
            if let Some(label) = label {
                self.number(out, u64::from(*label))?;
            }
        }
        self.put(out, &[u8::from(life.result_life.is_some())])?;
        if let Some(label) = life.result_life {
            self.number(out, u64::from(label))?;
        }
        Ok(())
    }
    pub(super) fn graph_symbol(&mut self, out: &mut Vec<u8>, symbol: &CtSymbolId) -> Check<()> {
        self.number(out, u64::from(symbol.module.0))?;
        self.number(out, u64::from(symbol.declaration.0))?;
        self.put(out, &[u8::from(symbol.generation_path.is_some())])?;
        if let Some(path) = symbol.generation_path {
            self.put(out, &path.0)?;
        }
        Ok(())
    }
}
