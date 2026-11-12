//! Source metadata stays separate from serializable execution data.
use super::*;
pub(crate) fn ids(node: &mut CtTypeNode, mut map: impl FnMut(&mut CtTypeNodeId)) {
    match node {
        CtTypeNode::BorrowedStorage { storage, .. } => map(storage),
        CtTypeNode::Nominal { arguments, .. } => {
            for arg in arguments {
                match arg {
                    CtTypeArgument::Type(id) => map(id),
                    CtTypeArgument::Lifetime(life) => map(&mut life.owner),
                    CtTypeArgument::Scalar(_) => {}
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
    /// Comparison only: charge every fixed payload and variable edge/path
    /// before a structural scope equality. This allocates no graph copy.
    pub(super) fn scope_comparison(&mut self, scope: &CtAmbientLifeScope, span: Span) -> R<()> {
        self.work(
            32usize.saturating_add(scope.binder_site.len().saturating_mul(4)),
            span,
        )?;
        let CtScopeInstance::Instantiated(actuals) = &scope.instance else {
            return Ok(());
        };
        self.scope_actuals_comparison(actuals, span)
    }
    pub(super) fn scope_actuals_comparison(
        &mut self,
        actuals: &[CtScopeActual],
        span: Span,
    ) -> R<()> {
        self.work(
            actuals
                .len()
                .saturating_mul(std::mem::size_of::<CtScopeActual>()),
            span,
        )?;
        for actual in actuals {
            let CtScopeActual::Type(graph) = actual else {
                if let CtScopeActual::Scalar(Constant::WideString(text)) = actual {
                    self.work(text.len().saturating_mul(2), span)?;
                }
                continue;
            };
            self.work(
                graph
                    .nodes
                    .len()
                    .saturating_mul(std::mem::size_of::<CtTypeNode>())
                    .saturating_add(
                        graph
                            .ambient_classes
                            .len()
                            .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
                    ),
                span,
            )?;
            for node in &graph.nodes {
                match node {
                    CtTypeNode::Nominal { arguments, .. } => {
                        self.work(
                            arguments
                                .len()
                                .saturating_mul(std::mem::size_of::<CtTypeArgument>()),
                            span,
                        )?;
                        for argument in arguments {
                            if let CtTypeArgument::Scalar(Constant::WideString(text)) = argument {
                                self.work(text.len().saturating_mul(2), span)?;
                            }
                        }
                    }
                    CtTypeNode::Tuple(ids) | CtTypeNode::AnonymousSum(ids) => {
                        self.work(ids.len().saturating_mul(4), span)?
                    }
                    CtTypeNode::FunctionPointer { parameters, .. }
                    | CtTypeNode::ForeignCallback { parameters, .. } => {
                        self.work(parameters.len().saturating_mul(4), span)?
                    }
                    CtTypeNode::BorrowedFunctionPointer {
                        parameters, life, ..
                    } => {
                        self.work(
                            parameters
                                .len()
                                .saturating_mul(4)
                                .saturating_add(life.bindings.len().saturating_mul(
                                    std::mem::size_of::<zil_ir::StorageLifeBinding>(),
                                ))
                                .saturating_add(life.classes.len().saturating_mul(
                                    std::mem::size_of::<zil_ir::SignatureLifeClassBinding>(),
                                )),
                            span,
                        )?;
                        for row in &life.bindings {
                            self.work(
                                row.position
                                    .path
                                    .len()
                                    .saturating_mul(
                                        std::mem::size_of::<zil_ir::ContractProjection>(),
                                    ),
                                span,
                            )?;
                        }
                        self.work(
                            life.static_bindings
                                .len()
                                .saturating_mul(std::mem::size_of::<zil_ir::StoragePosition>()),
                            span,
                        )?;
                        for position in &life.static_bindings {
                            self.work(
                                position
                                    .path
                                    .len()
                                    .saturating_mul(
                                        std::mem::size_of::<zil_ir::ContractProjection>(),
                                    ),
                                span,
                            )?;
                        }
                        for row in &life.classes {
                            self.work(
                                row.position
                                    .path
                                    .len()
                                    .saturating_mul(
                                        std::mem::size_of::<zil_ir::LifeTypeProjection>(),
                                    ),
                                span,
                            )?;
                        }
                    }
                    CtTypeNode::BorrowedStorage { life, .. } => {
                        self.work(
                            life.slots
                                .len()
                                .saturating_mul(std::mem::size_of::<zil_ir::StorageLifeSlot>())
                                .saturating_add(life.retained.len().saturating_mul(
                                    std::mem::size_of::<zil_ir::StorageRetainedBinding>(),
                                ))
                                .saturating_add(life.nominal_arguments.len().saturating_mul(
                                    std::mem::size_of::<zil_ir::NominalLifeArgument>(),
                                )),
                            span,
                        )?;
                        for row in &life.slots {
                            self.work(
                                row.path
                                    .len()
                                    .saturating_mul(std::mem::size_of::<zil_ir::PlaceProjection>()),
                                span,
                            )?;
                        }
                        for row in &life.retained {
                            self.work(
                                row.path.len().saturating_mul(std::mem::size_of::<
                                    zil_ir::LifeTypeProjection,
                                >()),
                                span,
                            )?;
                        }
                        for row in &life.nominal_arguments {
                            self.work(
                                row.path.len().saturating_mul(std::mem::size_of::<
                                    zil_ir::LifeTypeProjection,
                                >()),
                                span,
                            )?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    pub(super) fn metadata(&mut self, graph: &CtTypeRef, span: Span) -> R<()> {
        self.work(graph.ambient_classes.len(), span)?;
        Self::take(
            &mut self.bytes,
            graph
                .ambient_classes
                .len()
                .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
            span,
            "CT ambient mapping copy bytes exhausted",
        )?;
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
            if let CtTypeNode::BorrowedStorage { life, .. } = node {
                self.storage_metadata(life, span)?;
            }
            self.work(edges, span)?;
            Self::take(
                &mut self.bytes,
                std::mem::size_of::<CtTypeNode>().saturating_add(edges.saturating_mul(16)),
                span,
                "CT metadata copy bytes exhausted",
            )?;
            if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
                self.signature_metadata(life, span)?;
            }
        }
        Ok(())
    }
    pub(super) fn rich_relation(
        &mut self,
        life: zil_ir::FunctionLifeContract,
        span: Span,
    ) -> R<zil_ir::StorageLifeContract> {
        self.work(life.parameter_lives.len().saturating_add(1), span)?;
        Self::take(
            &mut self.bytes,
            life.parameter_lives
                .len()
                .saturating_add(1)
                .saturating_mul(std::mem::size_of::<zil_ir::StorageLifeBinding>()),
            span,
            "CT relation embedding bytes exhausted",
        )?;
        zil_ir::StorageLifeContract::from_flat(life)
            .ok_or_else(|| error(span, "CT relation parameter identity overflow"))
    }
    pub(super) fn signature_metadata(
        &mut self,
        life: &zil_ir::StorageLifeContract,
        span: Span,
    ) -> R<()> {
        self.work(
            life.bindings
                .len()
                .saturating_add(life.classes.len())
                .saturating_add(life.static_bindings.len())
                .saturating_add(2),
            span,
        )?;
        let mut bytes = life
            .bindings
            .len()
            .saturating_mul(std::mem::size_of::<zil_ir::StorageLifeBinding>())
            .saturating_add(
                life.classes
                    .len()
                    .saturating_mul(std::mem::size_of::<zil_ir::SignatureLifeClassBinding>()),
            );
        for row in &life.bindings {
            self.work(row.position.path.len(), span)?;
            bytes = bytes.saturating_add(
                row.position
                    .path
                    .len()
                    .saturating_mul(std::mem::size_of::<zil_ir::ContractProjection>()),
            );
        }
        bytes = bytes.saturating_add(
            life.static_bindings
                .len()
                .saturating_mul(std::mem::size_of::<zil_ir::StoragePosition>()),
        );
        for position in &life.static_bindings {
            self.work(position.path.len(), span)?;
            bytes = bytes.saturating_add(
                position
                    .path
                    .len()
                    .saturating_mul(std::mem::size_of::<zil_ir::ContractProjection>()),
            );
        }
        for row in &life.classes {
            self.work(row.position.path.len(), span)?;
            bytes = bytes.saturating_add(
                row.position
                    .path
                    .len()
                    .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>()),
            );
        }
        Self::take(
            &mut self.bytes,
            bytes,
            span,
            "CT rich signature copy bytes exhausted",
        )
    }
    fn storage_metadata(&mut self, life: &zil_ir::StorageLifeShape, span: Span) -> R<()> {
        self.work(
            life.slots
                .len()
                .saturating_add(life.retained.len())
                .saturating_add(life.nominal_arguments.len()),
            span,
        )?;
        let mut bytes = 0usize;
        for slot in &life.slots {
            self.work(slot.path.len(), span)?;
            bytes = bytes
                .saturating_add(std::mem::size_of::<zil_ir::StorageLifeSlot>())
                .saturating_add(
                    slot.path
                        .len()
                        .saturating_mul(std::mem::size_of::<zil_ir::PlaceProjection>()),
                );
        }
        for row in &life.retained {
            self.work(row.path.len(), span)?;
            bytes = bytes
                .saturating_add(std::mem::size_of::<zil_ir::StorageRetainedBinding>())
                .saturating_add(
                    row.path
                        .len()
                        .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>()),
                );
        }
        for row in &life.nominal_arguments {
            self.work(row.path.len(), span)?;
            bytes = bytes
                .saturating_add(std::mem::size_of::<zil_ir::NominalLifeArgument>())
                .saturating_add(
                    row.path
                        .len()
                        .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>()),
                );
        }
        Budget::take(
            &mut self.bytes,
            bytes,
            span,
            "CT4 storage metadata bytes exhausted",
        )
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
        // Ambient owners are graph-local node references too. Moving only the
        // physical nodes would silently erase the actual external life source.
        if !graph.ambient_classes.is_empty() {
            self.budget.work(graph.ambient_classes.len(), span)?;
            Budget::take(
                &mut self.budget.bytes,
                self.metadata_ambient
                    .len()
                    .saturating_add(graph.ambient_classes.len())
                    .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
                span,
                "CT metadata ambient relocation backing exhausted",
            )?;
            self.metadata_ambient
                .reserve_exact(graph.ambient_classes.len());
            for mut row in graph.ambient_classes {
                row.owner.0 = row
                    .owner
                    .0
                    .checked_add(offset)
                    .ok_or_else(|| error(span, "CT metadata ambient owner overflow"))?;
                self.metadata_ambient.push(row);
            }
        }
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
    pub(super) fn meta_normalize(&mut self, graph: CtTypeRef, span: Span) -> R<CtTypeRef> {
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
    pub(super) fn metadata_source_has_ambient(&mut self, ty: &s::TypeRef, depth: usize) -> R<bool> {
        self.budget.depth(depth, ty.span)?;
        self.budget.work(1, ty.span)?;
        match &ty.kind {
            s::TypeKind::Named { path, args } => {
                if path.len() == 1
                    && args.is_empty()
                    && let Some(graph) = self.metadata_binding(&path[0], ty.span)?
                {
                    return Ok(!graph.ambient_classes.is_empty()
                        || matches!(
                            graph.nodes.get(graph.root.0 as usize),
                            Some(CtTypeNode::BorrowedStorage { .. })
                        ));
                }
                for arg in args {
                    match arg {
                        s::GenericArg::Lifetime(_) => return Ok(true),
                        s::GenericArg::Type(inner)
                            if self.metadata_source_has_ambient(inner, depth + 1)? =>
                        {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
                Ok(false)
            }
            s::TypeKind::Tuple(parts) | s::TypeKind::Union(parts) => {
                for part in parts {
                    if self.metadata_source_has_ambient(part, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            s::TypeKind::Group(inner) | s::TypeKind::Array { element: inner, .. } => {
                self.metadata_source_has_ambient(inner, depth + 1)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn metadata_type(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
    ) -> R<CtTypeRef> {
        if let s::TypeKind::Named { path, args } = &ty.kind
            && args
                .iter()
                .any(|arg| matches!(arg, s::GenericArg::Lifetime(_)))
            && !self.checking_generics
        {
            let (owner, item) = self.named_item(module, path, ty.span)?;
            if self.metadata_graph_only {
                return self
                    .source_scoped_nominal_graph(module, owner, item, args, depth + 1, ty.span)
                    .map(|(graph, _, _)| graph);
            }
            let data = self.source_scoped_nominal(module, owner, item, args, depth + 1, ty.span)?;
            return self.metadata_data(&data, ty.span, depth + 1);
        }
        let prior_ambient = std::mem::take(&mut self.metadata_ambient);
        let mut nodes = Vec::new();
        let root = self.meta_source(module, ty, depth, &mut nodes);
        let ambient_classes = std::mem::replace(&mut self.metadata_ambient, prior_ambient);
        let root = root?;
        self.meta_normalize(
            CtTypeRef {
                scoped_catalogue: None,
                nodes,
                root,
                ambient_classes,
            },
            ty.span,
        )
    }
    fn meta_source(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
        nodes: &mut Vec<CtTypeNode>,
    ) -> R<CtTypeNodeId> {
        self.meta_scoped(module, ty, depth, nodes, &[])
    }
    fn meta_scoped(
        &mut self,
        module: usize,
        ty: &s::TypeRef,
        depth: usize,
        nodes: &mut Vec<CtTypeNode>,
        lifetimes: &[(String, CtLifetimeArgument)],
    ) -> R<CtTypeNodeId> {
        self.budget.depth(depth, ty.span)?;
        let span = ty.span;
        if lifetimes.is_empty()
            && !self.checking_generics
            && let s::TypeKind::Named { args, .. } = &ty.kind
            && args
                .iter()
                .any(|arg| matches!(arg, s::GenericArg::Lifetime(_)))
        {
            let graph = self.metadata_type(module, ty, depth + 1)?;
            return self.meta_append(nodes, graph, span);
        }
        let node = match &ty.kind {
            s::TypeKind::Group(inner) => {
                return self.meta_scoped(module, inner, depth + 1, nodes, lifetimes);
            }
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
                    let old_metadata = std::mem::take(&mut self.metadata_bindings);
                    let old_data = std::mem::take(&mut self.bindings);
                    let old_scalars = std::mem::take(&mut self.scalar_bindings);
                    let result = self.meta_scoped(
                        owner,
                        alias
                            .value
                            .as_ref()
                            .ok_or_else(|| error(span, "metadata alias has no value"))?,
                        depth + 1,
                        nodes,
                        &[],
                    );
                    self.active_aliases.pop();
                    self.metadata_bindings = old_metadata;
                    self.bindings = old_data;
                    self.scalar_bindings = old_scalars;
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
                            .push(CtTypeArgument::Type(self.meta_scoped(
                                module,
                                inner,
                                depth + 1,
                                nodes,
                                lifetimes,
                            )?)),
                        (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                            let mut found = None;
                            for (bound, value) in lifetimes.iter().rev() {
                                self.budget
                                    .work(name.len().min(bound.len()).saturating_add(1), span)?;
                                if name == bound {
                                    found = Some(*value);
                                    break;
                                }
                            }
                            arguments.push(CtTypeArgument::Lifetime(found.ok_or_else(||error(span,"nominal lifetime is not bound by its actual enclosing metadata scope"))?));
                        }
                        (s::GenericParamKind::Const(_), arg) => {
                            arguments.push(CtTypeArgument::Scalar(
                                self.source_scalar_argument(parameter, arg, span)?,
                            ))
                        }
                        _ => {
                            return Err(error(
                                span,
                                "metadata scalar/lifetime argument needs a completed binding",
                            ));
                        }
                    }
                }
                CtTypeNode::Nominal {
                    symbol: self.item_symbol(owner, item)?,
                    arguments,
                }
            }
            s::TypeKind::Tuple(parts) if parts.is_empty() => {
                CtTypeNode::Primitive(CtScalarTy::Unit)
            }
            s::TypeKind::Tuple(parts) | s::TypeKind::Union(parts) => {
                let mut fields = Vec::new();
                for part in parts {
                    fields.push(self.meta_scoped(module, part, depth + 1, nodes, lifetimes)?);
                }
                if matches!(ty.kind, s::TypeKind::Union(_)) {
                    CtTypeNode::AnonymousSum(fields)
                } else {
                    CtTypeNode::Tuple(fields)
                }
            }
            s::TypeKind::Array { element, length } => {
                let element = self.meta_scoped(module, element, depth + 1, nodes, lifetimes)?;
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
                    parameters.push(self.meta_scoped(
                        module,
                        parameter,
                        depth + 1,
                        nodes,
                        lifetimes,
                    )?);
                }
                CtTypeNode::FunctionPointer {
                    parameters,
                    result: self.meta_scoped(module, result, depth + 1, nodes, lifetimes)?,
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
                let placeholder = CtTypeNodeId(
                    u32::MAX
                        .checked_sub(
                            u32::try_from(lifetimes.len())
                                .map_err(|_| error(span, "metadata scope ID overflow"))?,
                        )
                        .ok_or_else(|| error(span, "metadata scope ID overflow"))?,
                );
                let first = nodes.len();
                let mut scoped = Vec::new();
                for (name, value) in lifetimes {
                    self.budget.text(name, span)?;
                    self.budget.work(32, span)?;
                    scoped.push((name.clone(), *value));
                }
                for (parameter, label) in params.iter().zip(&life.parameter_lives) {
                    let mut parameter = parameter;
                    while let s::TypeKind::Group(inner) = &parameter.kind {
                        parameter = inner;
                    }
                    if let (
                        s::TypeKind::Reference {
                            lifetime: Some(name),
                            ..
                        },
                        Some(binder),
                    ) = (&parameter.kind, label)
                    {
                        self.budget.text(name, span)?;
                        self.budget.work(32, span)?;
                        scoped.push((
                            name.clone(),
                            CtLifetimeArgument {
                                owner: placeholder,
                                binder: zil_ir::StorageBinder::Invocation(*binder),
                            },
                        ));
                    }
                }
                let mut parameters = Vec::new();
                for p in params {
                    parameters.push(self.meta_scoped(module, p, depth + 1, nodes, &scoped)?);
                }
                let result = self.meta_scoped(module, result, depth + 1, nodes, &scoped)?;
                let owner = CtTypeNodeId(
                    u32::try_from(nodes.len())
                        .map_err(|_| error(span, "metadata owner ID overflow"))?,
                );
                self.budget.work(nodes.len().saturating_sub(first), span)?;
                for node in &mut nodes[first..] {
                    if let CtTypeNode::Nominal { arguments, .. } = node {
                        self.budget.work(arguments.len(), span)?;
                        for argument in arguments {
                            if let CtTypeArgument::Lifetime(value) = argument
                                && value.owner == placeholder
                            {
                                value.owner = owner;
                            }
                        }
                    }
                }
                if life.binder_count == 0 {
                    CtTypeNode::FunctionPointer { parameters, result }
                } else {
                    CtTypeNode::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life: self.budget.rich_relation(life, span)?,
                    }
                }
            }
            s::TypeKind::Reference {
                mutable,
                inner,
                lifetime,
            } => {
                if let Some(name) = lifetime {
                    let mut found = false;
                    for (bound, _) in lifetimes.iter().rev() {
                        self.budget
                            .work(name.len().min(bound.len()).saturating_add(1), span)?;
                        if name == bound {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(error(
                            span,
                            "address lifetime is not bound by its actual metadata scope",
                        ));
                    }
                }
                CtTypeNode::Address {
                    mutable: *mutable,
                    inner: self.meta_scoped(module, inner, depth + 1, nodes, lifetimes)?,
                }
            }
            s::TypeKind::Error { error, result } => CtTypeNode::ErrorUnion {
                error: self.meta_scoped(module, error, depth + 1, nodes, lifetimes)?,
                result: self.meta_scoped(module, result, depth + 1, nodes, lifetimes)?,
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
        let prior_ambient = std::mem::take(&mut self.metadata_ambient);
        let result = self.metadata_data_inner(ty, span, depth);
        self.metadata_ambient = prior_ambient;
        result
    }
    fn metadata_data_inner(&mut self, ty: &CtTy, span: Span, depth: usize) -> R<CtTypeRef> {
        self.budget.depth(depth, span)?;
        let mut nodes = Vec::new();
        let node = match ty {
            CtTy::Scalar(s) => CtTypeNode::Primitive(*s),
            CtTy::Record(id) => {
                let schema = &self.records[id.0 as usize];
                if let Some(context) = &schema.life_context {
                    self.budget.metadata(context, span)?;
                    return Ok(context.clone());
                }
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
                if let Some(context) = &schema.life_context {
                    self.budget.metadata(context, span)?;
                    return Ok(context.clone());
                }
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
        let ambient_classes = std::mem::take(&mut self.metadata_ambient);
        self.meta_normalize(
            CtTypeRef {
                scoped_catalogue: None,
                nodes,
                root,
                ambient_classes,
            },
            span,
        )
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
                CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "CT4 scoped nominal metadata construction is not yet implemented",
                        Some(span),
                    )]);
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
