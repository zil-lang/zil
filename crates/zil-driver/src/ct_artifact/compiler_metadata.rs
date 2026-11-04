//! Source metadata stays separate from serializable execution data.
use super::*;
fn ids(node: &mut CtTypeNode, mut map: impl FnMut(&mut CtTypeNodeId)) {
    match node {
        CtTypeNode::Nominal { arguments, .. } => {
            for arg in arguments {
                if let CtTypeArgument::Type(id) = arg {
                    map(id)
                }
            }
        }
        CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => {
            for id in v {
                map(id)
            }
        }
        CtTypeNode::FunctionPointer { parameters, result }
        | CtTypeNode::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | CtTypeNode::ForeignCallback { parameters, result } => {
            for id in parameters {
                map(id)
            }
            map(result)
        }
        CtTypeNode::Array { element, .. }
        | CtTypeNode::Slice(element)
        | CtTypeNode::Managed(element)
        | CtTypeNode::Desc(element)
        | CtTypeNode::OwnedStore(element) => map(element),
        CtTypeNode::Address { inner, .. } => map(inner),
        CtTypeNode::ErrorUnion { error, result } => {
            map(error);
            map(result)
        }
        CtTypeNode::Primitive(_) | CtTypeNode::Builtin(_) | CtTypeNode::Opaque { .. } => {}
    }
}
impl Budget<'_> {
    pub(super) fn metadata(&mut self, graph: &CtTypeRef, span: Span) -> R<()> {
        self.work(graph.nodes.len(), span)?;
        for node in &graph.nodes {
            self.node(span)?;
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
            self.work(edges, span)?;
            Self::take(
                &mut self.bytes,
                std::mem::size_of::<CtTypeNode>().saturating_add(edges.saturating_mul(16)),
                span,
                "CT metadata copy bytes exhausted",
            )?;
            if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
                self.work(life.parameter_lives.len(), span)?;
                Self::take(
                    &mut self.bytes,
                    life.parameter_lives.len().saturating_mul(8),
                    span,
                    "CT lifetime metadata bytes exhausted",
                )?;
            }
        }
        Ok(())
    }
}
impl Compiler<'_> {
    pub(super) fn metadata_binding(&mut self, name: &str, span: Span) -> R<Option<CtTypeRef>> {
        for (actual, graph) in self.metadata_bindings.iter().rev() {
            self.budget
                .work(actual.len().min(name.len()).saturating_add(1), span)?;
            if actual == name {
                self.budget.metadata(graph, span)?;
                return Ok(Some(graph.clone()));
            }
        }
        Ok(None)
    }
    fn meta_push(
        &mut self,
        nodes: &mut Vec<CtTypeNode>,
        node: CtTypeNode,
        span: Span,
    ) -> R<CtTypeNodeId> {
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.bytes,
            std::mem::size_of::<CtTypeNode>(),
            span,
            "CT metadata node bytes exhausted",
        )?;
        let id = CtTypeNodeId(
            u32::try_from(nodes.len()).map_err(|_| error(span, "CT metadata ID overflow"))?,
        );
        nodes.push(node);
        Ok(id)
    }
    fn meta_append(
        &mut self,
        nodes: &mut Vec<CtTypeNode>,
        graph: CtTypeRef,
        span: Span,
    ) -> R<CtTypeNodeId> {
        self.budget.metadata(&graph, span)?;
        let offset =
            u32::try_from(nodes.len()).map_err(|_| error(span, "CT metadata ID overflow"))?;
        for mut node in graph.nodes {
            let mut failed = false;
            ids(&mut node, |id| match id.0.checked_add(offset) {
                Some(n) => id.0 = n,
                None => failed = true,
            });
            if failed {
                return Err(error(span, "CT metadata edge overflow"));
            }
            nodes.push(node);
        }
        Ok(CtTypeNodeId(
            graph
                .root
                .0
                .checked_add(offset)
                .ok_or_else(|| error(span, "CT metadata root overflow"))?,
        ))
    }
    fn meta_normalize(&mut self, graph: CtTypeRef, span: Span) -> R<CtTypeRef> {
        let mut meter = zil_ct::CtExecutionBudget::new(self.budget.declared());
        let result = zil_ct::canonicalize_anonymous_type(&graph, &mut meter);
        let usage = meter.usage();
        self.budget.work(
            usize::try_from(usage.analysis_work).unwrap_or(usize::MAX),
            span,
        )?;
        Budget::take(
            &mut self.budget.nodes,
            usize::try_from(usage.generated_nodes).unwrap_or(usize::MAX),
            span,
            "CT metadata normalization nodes exhausted",
        )?;
        Budget::take(
            &mut self.budget.bytes,
            usize::try_from(usage.peak_temporary_bytes).unwrap_or(usize::MAX),
            span,
            "CT metadata normalization bytes exhausted",
        )?;
        result
    }
    pub(super) fn metadata_type(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
    ) -> R<CtTypeRef> {
        let mut nodes = Vec::new();
        let root = self.meta_source(module, ty, depth, &mut nodes)?;
        self.meta_normalize(CtTypeRef { nodes, root }, ty.span)
    }
    fn meta_source(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
        nodes: &mut Vec<CtTypeNode>,
    ) -> R<CtTypeNodeId> {
        self.budget.depth(depth, ty.span)?;
        let span = ty.span;
        let node = match &ty.kind {
            s::TypeKind::Group(inner) => return self.meta_source(module, inner, depth + 1, nodes),
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 && args.is_empty() {
                    if let Some(graph) = self.metadata_binding(&path[0], span)? {
                        return self.meta_append(nodes, graph, span);
                    }
                    if let Some(data) = self.binding(&path[0], span)? {
                        let graph = self.metadata_data(&data, span, depth + 1)?;
                        return self.meta_append(nodes, graph, span);
                    }
                    if let Some(scalar) = types::scalar(&path[0]) {
                        return self.meta_push(nodes, CtTypeNode::Primitive(scalar), span);
                    }
                }
                let (owner, item) = self.named_item(module, path, span)?;
                if let s::ItemKind::TypeAlias(alias) = &item.kind {
                    if !alias.generics.is_empty() || !args.is_empty() {
                        return Err(error(
                            span,
                            "generic metadata aliases need complete bindings",
                        ));
                    }
                    self.enter_alias(owner, item.span, span)?;
                    let result = self.meta_source(
                        owner,
                        alias
                            .value
                            .as_ref()
                            .ok_or_else(|| error(span, "metadata alias has no value"))?,
                        depth + 1,
                        nodes,
                    );
                    self.active_aliases.pop();
                    return result;
                }
                let parameters = match &item.kind {
                    s::ItemKind::Aggregate(a) => &a.generics,
                    s::ItemKind::Enum(e) => &e.generics,
                    _ => return Err(error(span, "metadata name is not a nominal type")),
                };
                if parameters.len() != args.len() {
                    return Err(error(span, "metadata nominal argument count differs"));
                }
                let mut arguments = Vec::new();
                for (parameter, arg) in parameters.iter().zip(args) {
                    self.budget.node(span)?;
                    match (&parameter.kind, arg) {
                        (s::GenericParamKind::Type(_), s::GenericArg::Type(inner)) => arguments
                            .push(CtTypeArgument::Type(self.meta_source(
                                module,
                                inner,
                                depth + 1,
                                nodes,
                            )?)),
                        _ => {
                            return Err(error(
                                span,
                                "metadata scalar/lifetime argument needs a completed binding",
                            ));
                        }
                    }
                }
                CtTypeNode::Nominal {
                    symbol: self.symbol(owner, item.span)?,
                    arguments,
                }
            }
            s::TypeKind::Tuple(parts) if parts.is_empty() => {
                CtTypeNode::Primitive(CtScalarTy::Unit)
            }
            s::TypeKind::Tuple(parts) | s::TypeKind::Union(parts) => {
                let mut fields = Vec::new();
                for part in parts {
                    fields.push(self.meta_source(module, part, depth + 1, nodes)?);
                }
                if matches!(ty.kind, s::TypeKind::Union(_)) {
                    CtTypeNode::AnonymousSum(fields)
                } else {
                    CtTypeNode::Tuple(fields)
                }
            }
            s::TypeKind::Array { element, length } => {
                let element = self.meta_source(module, element, depth + 1, nodes)?;
                match length {
                    Some(value) => CtTypeNode::Array {
                        element,
                        length: self.fixed_length(value)?,
                    },
                    None => CtTypeNode::Slice(element),
                }
            }
            s::TypeKind::Function { params, result } => {
                let mut parameters = Vec::new();
                for parameter in params {
                    parameters.push(self.meta_source(module, parameter, depth + 1, nodes)?);
                }
                CtTypeNode::FunctionPointer {
                    parameters,
                    result: self.meta_source(module, result, depth + 1, nodes)?,
                }
            }
            s::TypeKind::ForLifetime { .. } => {
                let mut inner = ty;
                let mut names = Vec::new();
                while let s::TypeKind::ForLifetime {
                    lifetime,
                    inner: next,
                } = &inner.kind
                {
                    self.budget.text(lifetime, span)?;
                    names.push(lifetime.as_str());
                    inner = next;
                }
                let s::TypeKind::Function { params, result } = &inner.kind else {
                    return Err(error(span, "universal metadata needs a function signature"));
                };
                self.budget.work(
                    names.len().saturating_mul(params.len().saturating_add(1)),
                    span,
                )?;
                let life = source_relation(&names, params, result);
                let mut parameters = Vec::new();
                for p in params {
                    parameters.push(self.meta_source(module, p, depth + 1, nodes)?);
                }
                CtTypeNode::BorrowedFunctionPointer {
                    parameters,
                    result: self.meta_source(module, result, depth + 1, nodes)?,
                    life,
                }
            }
            s::TypeKind::Reference { mutable, inner, .. } => CtTypeNode::Address {
                mutable: *mutable,
                inner: self.meta_source(module, inner, depth + 1, nodes)?,
            },
            s::TypeKind::Error { error, result } => CtTypeNode::ErrorUnion {
                error: self.meta_source(module, error, depth + 1, nodes)?,
                result: self.meta_source(module, result, depth + 1, nodes)?,
            },
            _ => {
                return Err(error(
                    span,
                    "type has no complete CT metadata representation",
                ));
            }
        };
        self.meta_push(nodes, node, span)
    }
    pub(super) fn metadata_data(&mut self, ty: &CtTy, span: Span, depth: usize) -> R<CtTypeRef> {
        self.budget.depth(depth, span)?;
        let mut nodes = Vec::new();
        let node = match ty {
            CtTy::Scalar(s) => CtTypeNode::Primitive(*s),
            CtTy::Record(id) => {
                let schema = &self.records[id.0 as usize];
                self.budget
                    .work(schema.arguments.len().saturating_add(1), span)?;
                for argument in &schema.arguments {
                    if let CtSchemaArgument::Type(graph) = argument {
                        self.budget.metadata(graph, span)?;
                    }
                }
                let (symbol, args) = (schema.name.clone(), schema.arguments.clone());
                self.meta_nominal_node(&symbol, &args, &mut nodes, span)?
            }
            CtTy::Enum(id) => {
                let schema = &self.enums[id.0 as usize];
                self.budget
                    .work(schema.arguments.len().saturating_add(1), span)?;
                for argument in &schema.arguments {
                    if let CtSchemaArgument::Type(graph) = argument {
                        self.budget.metadata(graph, span)?;
                    }
                }
                let (symbol, args) = (schema.name.clone(), schema.arguments.clone());
                self.meta_nominal_node(&symbol, &args, &mut nodes, span)?
            }
            CtTy::Tuple(parts) | CtTy::AnonymousSum(parts) => {
                let mut fields = Vec::new();
                for part in parts {
                    let graph = self.metadata_data(part, span, depth + 1)?;
                    fields.push(self.meta_append(&mut nodes, graph, span)?);
                }
                if matches!(ty, CtTy::Tuple(_)) {
                    CtTypeNode::Tuple(fields)
                } else {
                    CtTypeNode::AnonymousSum(fields)
                }
            }
            CtTy::Array { element, length } => {
                let graph = self.metadata_data(element, span, depth + 1)?;
                CtTypeNode::Array {
                    element: self.meta_append(&mut nodes, graph, span)?,
                    length: *length,
                }
            }
            CtTy::List(element) => {
                let graph = self.metadata_data(element, span, depth + 1)?;
                CtTypeNode::Slice(self.meta_append(&mut nodes, graph, span)?)
            }
            _ => {
                return Err(error(
                    span,
                    "CT-only class is not an ordinary type argument",
                ));
            }
        };
        let root = self.meta_push(&mut nodes, node, span)?;
        self.meta_normalize(CtTypeRef { nodes, root }, span)
    }
    fn meta_nominal_node(
        &mut self,
        symbol: &CtSymbolId,
        args: &[CtSchemaArgument],
        nodes: &mut Vec<CtTypeNode>,
        span: Span,
    ) -> R<CtTypeNode> {
        let mut arguments = Vec::new();
        for arg in args {
            self.budget.node(span)?;
            match arg {
                CtSchemaArgument::Type(graph) => {
                    self.budget.metadata(graph, span)?;
                    arguments.push(CtTypeArgument::Type(self.meta_append(
                        nodes,
                        graph.clone(),
                        span,
                    )?));
                }
                CtSchemaArgument::Scalar(c) => arguments.push(CtTypeArgument::Scalar(c.clone())),
            }
        }
        Ok(CtTypeNode::Nominal {
            symbol: symbol.clone(),
            arguments,
        })
    }
}

// Private model ordering only. These scratch generic schemas never publish.
pub(super) fn symbolic_order(a: &CtTy, b: &CtTy) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn rank(t: &CtTy) -> u8 {
        match t {
            CtTy::Scalar(_) => 0,
            CtTy::Type => 1,
            CtTy::Syntax(_) => 2,
            CtTy::Ident => 3,
            CtTy::TokenTree => 4,
            CtTy::TokenCursor => 5,
            CtTy::Text => 6,
            CtTy::ShapeInfo => 7,
            CtTy::FieldInfo => 8,
            CtTy::ImplInfo => 9,
            CtTy::LayoutInfo => 10,
            CtTy::Tuple(_) => 11,
            CtTy::Array { .. } => 12,
            CtTy::List(_) => 13,
            CtTy::Record(_) => 14,
            CtTy::Enum(_) => 15,
            CtTy::AnonymousSum(_) => 16,
        }
    }
    fn sequence(a: &[CtTy], b: &[CtTy]) -> Ordering {
        for (a, b) in a.iter().zip(b) {
            let c = symbolic_order(a, b);
            if c != Ordering::Equal {
                return c;
            }
        }
        a.len().cmp(&b.len())
    }
    let tag = rank(a).cmp(&rank(b));
    if tag != Ordering::Equal {
        return tag;
    }
    match (a, b) {
        (CtTy::Scalar(a), CtTy::Scalar(b)) => (*a as u8).cmp(&(*b as u8)),
        (CtTy::Syntax(a), CtTy::Syntax(b)) => (*a as u8).cmp(&(*b as u8)),
        (CtTy::Tuple(a), CtTy::Tuple(b)) | (CtTy::AnonymousSum(a), CtTy::AnonymousSum(b)) => {
            sequence(a, b)
        }
        (
            CtTy::Array {
                element: a,
                length: na,
            },
            CtTy::Array {
                element: b,
                length: nb,
            },
        ) => symbolic_order(a, b).then_with(|| na.cmp(nb)),
        (CtTy::List(a), CtTy::List(b)) => symbolic_order(a, b),
        (CtTy::Record(a), CtTy::Record(b)) => a.0.cmp(&b.0),
        (CtTy::Enum(a), CtTy::Enum(b)) => a.0.cmp(&b.0),
        _ => Ordering::Equal,
    }
}
