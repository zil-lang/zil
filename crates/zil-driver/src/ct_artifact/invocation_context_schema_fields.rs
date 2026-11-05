//! Parent schema indices locate immutable nominal instantiations; the source
//! field Type, complete actuals and actual data layout independently prove them.
use super::*;
use crate::ct_artifact::compiler::{SourceLifeBinding, SourceSchemaId};
use crate::ct_artifact::execute::value_context::{self, ValueContext};

impl FrozenInvocationContext {
    fn schema_parts<'a>(
        &self,
        id: SourceSchemaId,
        raw: &'a CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(
        &'a CtSymbolId,
        &'a [CtSchemaArgument],
        Option<&'a CtTypeRef>,
    )> {
        step(budget, 0)?;
        match id {
            SourceSchemaId::Record(id) => raw
                .record_schemas
                .get(id.0 as usize)
                .map(|s| (&s.name, s.arguments.as_slice(), s.life_context.as_ref())),
            SourceSchemaId::Enum(id) => raw
                .enum_schemas
                .get(id.0 as usize)
                .map(|s| (&s.name, s.arguments.as_slice(), s.life_context.as_ref())),
        }
        .ok_or_else(|| unsupported("CT nominal field parent schema is absent"))
    }
    fn schema_declaration<'a>(
        &'a self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(&'a s::Module, &'a s::Item)> {
        if let Some(fingerprint) = symbol.generation_path {
            let catalog = self
                .generated_catalog
                .as_ref()
                .ok_or_else(|| unsupported("CT nominal field generated catalog is absent"))?;
            budget.reserve_analysis_work(catalog.trees.len() as u64)?;
            let mut selected = None;
            for tree in &catalog.trees {
                budget.reserve_analysis_work(
                    (tree.identity_path_len() as u64)
                        .saturating_mul(4)
                        .saturating_add(tree.declarations.len() as u64)
                        .saturating_add(32),
                )?;
                if tree.module.id != symbol.module
                    || tree.generation_fingerprint() != Some(fingerprint)
                {
                    continue;
                }
                if let Some(local) = tree
                    .declarations
                    .iter()
                    .position(|id| *id == symbol.declaration)
                {
                    let item = crate::ct_artifact::compiler::source_scope::original_declaration(
                        &tree.module,
                        zil_base::NodeId(local as u32),
                        budget.remaining().max_call_depth as usize,
                        |n| budget.reserve_analysis_work(n as u64),
                    )?
                    .ok_or_else(|| {
                        unsupported("CT nominal field generated declaration is absent")
                    })?;
                    if selected.replace((&*tree.module, item.item)).is_some() {
                        return Err(unsupported(
                            "CT nominal field generated declaration is ambiguous",
                        ));
                    }
                }
            }
            selected.ok_or_else(|| unsupported("CT nominal field generated inventory is absent"))
        } else {
            budget.reserve_analysis_work(self.modules.len() as u64)?;
            let module = self
                .modules
                .iter()
                .find(|m| m.module.id == symbol.module)
                .ok_or_else(|| unsupported("CT nominal field original module is absent"))?;
            let item = crate::ct_artifact::compiler::source_scope::original_declaration(
                &module.module,
                symbol.declaration,
                budget.remaining().max_call_depth as usize,
                |n| budget.reserve_analysis_work(n as u64),
            )?
            .ok_or_else(|| unsupported("CT nominal field original declaration is absent"))?;
            Ok((&module.module, item.item))
        }
    }
    fn nominal_environment<'a>(
        &'a self,
        id: SourceSchemaId,
        root: &SourceEnvironment<'a>,
        raw: &'a CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(&'a s::Module, &'a s::Item, SourceEnvironment<'a>)> {
        let (definition, arguments, graph) = self.schema_parts(id, raw, budget)?;
        let (module, item) = self.schema_declaration(definition, budget)?;
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a)
                if matches!(id, SourceSchemaId::Record(_))
                    && a.kind == s::AggregateKind::Struct =>
            {
                &a.generics
            }
            s::ItemKind::Enum(e) if matches!(id, SourceSchemaId::Enum(_)) => &e.generics,
            _ => {
                return Err(unsupported(
                    "CT nominal field parent kind differs from its actual original declaration",
                ));
            }
        };
        budget.reserve_analysis_work((parameters.len() + arguments.len()) as u64)?;
        if parameters.len() != arguments.len() {
            return Err(unsupported(
                "CT nominal field parent loses a complete original generic ordinal",
            ));
        }
        if let Some(graph) = graph {
            let bytes = value_context::graph_cost(graph, budget)?;
            budget.reserve_analysis_work(bytes)?;
            let Some(CtTypeNode::BorrowedStorage { storage, .. }) =
                graph.nodes.get(graph.root.0 as usize)
            else {
                return Err(unsupported(
                    "CT nominal field parent has no genuine contextual root",
                ));
            };
            let Some(CtTypeNode::Nominal {
                symbol,
                arguments: actuals,
            }) = graph.nodes.get(storage.0 as usize)
            else {
                return Err(unsupported(
                    "CT nominal field parent contextual storage is absent",
                ));
            };
            budget.reserve_analysis_work(32 + actuals.len() as u64)?;
            if symbol != definition || actuals.len() != arguments.len() {
                return Err(unsupported(
                    "CT nominal field parent graph identity or complete vector differs",
                ));
            }
            for ((parameter, argument), actual) in parameters.iter().zip(arguments).zip(actuals) {
                step(budget, 0)?;
                match (&parameter.kind, argument, actual) {
                    (
                        s::GenericParamKind::Type(_),
                        CtSchemaArgument::TypeInContext(a),
                        CtTypeArgument::Type(b),
                    ) if a == b => {}
                    (
                        s::GenericParamKind::Type(_),
                        CtSchemaArgument::Type(a),
                        CtTypeArgument::Type(b),
                    ) => self.graph_subtype_equal(graph, *b, a, a.root, budget, 0)?,
                    (
                        s::GenericParamKind::Const(_),
                        CtSchemaArgument::Scalar(a),
                        CtTypeArgument::Scalar(b),
                    ) if a == b => {}
                    (
                        s::GenericParamKind::Lifetime,
                        CtSchemaArgument::Lifetime(a),
                        CtTypeArgument::Lifetime(b),
                    ) if a == b && a.owner == graph.root => {}
                    _ => {
                        return Err(unsupported(
                            "CT nominal field parent actual differs from its independently checked contextual ordinal",
                        ));
                    }
                }
            }
        } else {
            for (parameter, argument) in parameters.iter().zip(arguments) {
                step(budget, 0)?;
                if !matches!(
                    (&parameter.kind, argument),
                    (s::GenericParamKind::Type(_), CtSchemaArgument::Type(_))
                        | (s::GenericParamKind::Const(_), CtSchemaArgument::Scalar(_))
                ) {
                    return Err(unsupported(
                        "CT nominal field closed parent has an unproved actual kind",
                    ));
                }
            }
        }
        Ok((
            module,
            item,
            SourceEnvironment {
                function: root.function,
                scope: root.scope,
                scope_id: root.scope_id,
                host: root.host,
                nominal: Some(NominalEnvironment {
                    definition,
                    parameters,
                    arguments,
                    graph,
                }),
            },
        ))
    }
    fn nominal_field_contains(
        &self,
        module: &s::Module,
        item: &s::Item,
        node: SourceNode<'_>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<bool> {
        let contains = |ty: &s::TypeRef, budget: &mut CtExecutionBudget| {
            source_sites::contains_node(
                module,
                SourceNode::Type(ty),
                node,
                budget.remaining().max_call_depth as usize,
                |n| budget.reserve_analysis_work(n as u64),
            )
        };
        match &item.kind {
            s::ItemKind::Aggregate(a) => {
                budget.reserve_analysis_work(a.fields.len() as u64)?;
                for field in &a.fields {
                    if contains(&field.ty, budget)? {
                        return Ok(true);
                    }
                }
            }
            s::ItemKind::Enum(e) => {
                budget.reserve_analysis_work(e.variants.len() as u64)?;
                for variant in &e.variants {
                    match &variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(fields) => {
                            budget.reserve_analysis_work(fields.len() as u64)?;
                            for field in fields {
                                if contains(&field.ty, budget)? {
                                    return Ok(true);
                                }
                            }
                        }
                        s::VariantFields::Record(fields) => {
                            budget.reserve_analysis_work(fields.len() as u64)?;
                            for field in fields {
                                if contains(&field.ty, budget)? {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn check_nominal_field(
        &self,
        source: &s::TypeRef,
        manages: bool,
        actual: &CtTy,
        env: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        view: &ValueContext,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        stack: &mut Vec<SourceSchemaId>,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        if manages {
            return Err(unsupported(
                "CT nominal field cannot manage runtime authority",
            ));
        }
        let before = budget.live_temporary_bytes();
        let result = (|| {
            let graph = view.type_ref(actual, budget)?;
            self.source_type(
                source,
                &graph,
                graph.root,
                env,
                raw,
                index,
                budget,
                depth + 1,
            )
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        result?;
        self.check_nominal_children(actual, env, raw, view, index, budget, stack, depth + 1)
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn check_nominal_children(
        &self,
        actual: &CtTy,
        root: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        view: &ValueContext,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        stack: &mut Vec<SourceSchemaId>,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        match actual {
            CtTy::Record(id) => self.check_nominal_storage(
                SourceSchemaId::Record(*id),
                root,
                raw,
                view,
                index,
                budget,
                stack,
                depth + 1,
            ),
            CtTy::Enum(id) => self.check_nominal_storage(
                SourceSchemaId::Enum(*id),
                root,
                raw,
                view,
                index,
                budget,
                stack,
                depth + 1,
            ),
            CtTy::Tuple(types) | CtTy::AnonymousSum(types) => {
                budget.reserve_analysis_work(types.len() as u64)?;
                for ty in types {
                    self.check_nominal_children(
                        ty,
                        root,
                        raw,
                        view,
                        index,
                        budget,
                        stack,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            CtTy::Array { element, .. } | CtTy::List(element) => self.check_nominal_children(
                element,
                root,
                raw,
                view,
                index,
                budget,
                stack,
                depth + 1,
            ),
            _ => Ok(()),
        }
    }
    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    fn check_nominal_storage(
        &self,
        id: SourceSchemaId,
        root: &SourceEnvironment<'_>,
        raw: &CtArtifactProgram,
        view: &ValueContext,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        stack: &mut Vec<SourceSchemaId>,
        depth: usize,
    ) -> CompileResult<()> {
        step(budget, depth)?;
        budget.reserve_analysis_work(stack.len() as u64)?;
        if stack.contains(&id) {
            return Err(unsupported("CT nominal field schema dependency cycles"));
        }
        if stack.len() == stack.capacity() {
            let width = std::mem::size_of::<SourceSchemaId>() as u64;
            let old_bytes = stack.capacity() as u64 * width;
            let new_bytes = (stack.len() as u64 + 1).saturating_mul(width);
            budget.reserve_analysis_work(stack.len() as u64)?;
            budget.reserve_temporary_bytes(new_bytes)?;
            stack.reserve_exact(1);
            budget.release_temporary_bytes(old_bytes);
        }
        stack.push(id);
        let result = (|| {
            let (_, item, env) = self.nominal_environment(id, root, raw, budget)?;
            match (&item.kind, id) {
                (s::ItemKind::Aggregate(a), SourceSchemaId::Record(id)) => {
                    let actual = &raw.record_schemas[id.0 as usize].fields;
                    budget.reserve_analysis_work((a.fields.len() + actual.len()) as u64)?;
                    if a.fields.len() != actual.len() {
                        return Err(unsupported(
                            "CT nominal record field arity differs from its original declaration",
                        ));
                    }
                    for (field, ty) in a.fields.iter().zip(actual) {
                        self.check_nominal_field(
                            &field.ty,
                            field.manages,
                            ty,
                            &env,
                            raw,
                            view,
                            index,
                            budget,
                            stack,
                            depth + 1,
                        )?;
                    }
                }
                (s::ItemKind::Enum(e), SourceSchemaId::Enum(id)) => {
                    let actual = &raw.enum_schemas[id.0 as usize].variants;
                    budget.reserve_analysis_work((e.variants.len() + actual.len()) as u64)?;
                    if e.variants.len() != actual.len() {
                        return Err(unsupported(
                            "CT nominal enum variant arity differs from its original declaration",
                        ));
                    }
                    for (variant, types) in e.variants.iter().zip(actual) {
                        budget.reserve_analysis_work(types.len() as u64)?;
                        match &variant.fields {
                            s::VariantFields::Unit => {
                                if !types.is_empty() {
                                    return Err(unsupported(
                                        "CT nominal Unit variant has unexpected fields",
                                    ));
                                }
                            }
                            s::VariantFields::Tuple(fields) => {
                                budget.reserve_analysis_work(fields.len() as u64)?;
                                if fields.len() != types.len() {
                                    return Err(unsupported(
                                        "CT nominal enum payload arity differs from its original declaration",
                                    ));
                                }
                                for (field, ty) in fields.iter().zip(types) {
                                    self.check_nominal_field(
                                        &field.ty,
                                        field.manages,
                                        ty,
                                        &env,
                                        raw,
                                        view,
                                        index,
                                        budget,
                                        stack,
                                        depth + 1,
                                    )?;
                                }
                            }
                            s::VariantFields::Record(fields) => {
                                budget.reserve_analysis_work(fields.len() as u64)?;
                                if fields.len() != types.len() {
                                    return Err(unsupported(
                                        "CT nominal enum payload arity differs from its original declaration",
                                    ));
                                }
                                for (field, ty) in fields.iter().zip(types) {
                                    self.check_nominal_field(
                                        &field.ty,
                                        field.manages,
                                        ty,
                                        &env,
                                        raw,
                                        view,
                                        index,
                                        budget,
                                        stack,
                                        depth + 1,
                                    )?;
                                }
                            }
                        }
                    }
                }
                _ => return Err(unsupported("CT nominal schema field kind differs")),
            }
            Ok(())
        })();
        stack.pop();
        result
    }
    fn nominal_binding_origin(
        &self,
        binding: &SourceLifeBinding,
        position: usize,
        facts: &SourceCompilation,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtFunctionId> {
        let mut current = binding;
        let mut before = position;
        let mut claimed = binding.caller;
        let mut depth = 0;
        loop {
            step(budget, depth)?;
            budget.reserve_analysis_work(32)?;
            if current.definition != binding.definition {
                return Err(unsupported(
                    "CT nominal field parent belongs to another actual source definition",
                ));
            }
            if let Some(caller) = current.caller {
                if claimed.is_some_and(|old| old != caller) {
                    return Err(unsupported(
                        "CT nominal field parent belongs to another actual source invocation",
                    ));
                }
                claimed = Some(caller);
            }
            let Some(parent) = current.parent else {
                let root = current.caller.ok_or_else(|| {
                    unsupported("CT nominal schema root has no authentic original function")
                })?;
                if claimed != Some(root) {
                    return Err(unsupported(
                        "CT nominal field chain has no unique actual root invocation",
                    ));
                }
                return Ok(root);
            };
            budget.reserve_analysis_work(before as u64)?;
            let (position, parent) = facts.bindings[..before].iter().enumerate()
                .find(|(_, candidate)| schema_id(candidate) == parent)
                .ok_or_else(|| unsupported("CT nominal field parent is missing, cyclic or not earlier in its actual source chain"))?;
            current = parent;
            before = position;
            depth += 1;
        }
    }

    // Keep the source, candidate, scope and request meter explicit at this proof boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn validate_nominal_field_source(
        &self,
        binding: &SourceLifeBinding,
        position: usize,
        facts: &SourceCompilation,
        graph: &CtTypeRef,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let parent = binding
            .parent
            .ok_or_else(|| unsupported("CT nominal field parent is absent"))?;
        let caller = self.nominal_binding_origin(binding, position, facts, budget)?;
        let (_, _, parent_graph) = self.schema_parts(parent, raw, budget)?;
        let parent_graph = parent_graph.ok_or_else(|| {
            unsupported("CT nominal field closed parent needs its own source certificate")
        })?;
        let bytes = value_context::graph_cost(parent_graph, budget)?;
        budget.reserve_analysis_work(
            bytes
                .saturating_mul(2)
                .saturating_add(self.authenticated_schema_count() as u64),
        )?;
        let (enumeration, schema) = schema_parts(parent);
        if !self.authenticated_source_schema(enumeration, schema, parent_graph) {
            return Err(unsupported(
                "CT nominal field parent lacks an independently checked exact graph",
            ));
        }
        let function = raw
            .functions
            .get(caller.0 as usize)
            .ok_or_else(|| unsupported("CT nominal field original function placement is absent"))?;
        let root = self.function_environment(function, facts, raw, budget)?;
        let (module, item, env) = self.nominal_environment(parent, &root, raw, budget)?;
        let site = binding.site.ok_or_else(|| {
            unsupported("CT nominal field child has no actual frozen structural site")
        })?;
        let location = self.source_location(site, budget)?;
        if !self.nominal_field_contains(module, item, location.node, budget)? {
            return Err(unsupported(
                "CT nominal field child site is outside its actual parent declaration fields",
            ));
        }
        match location.node {
            SourceNode::Type(ty) => {
                self.source_type(ty, graph, graph.root, &env, raw, index, budget, 0)
            }
            SourceNode::Expr(expr) => {
                self.source_expression_type(expr, graph, &env, raw, index, budget, 0)
            }
        }
    }
    pub(super) fn validate_nominal_schema_fields(
        &self,
        binding: &SourceLifeBinding,
        position: usize,
        facts: &SourceCompilation,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let caller = self.nominal_binding_origin(binding, position, facts, budget)?;
        let function = raw.functions.get(caller.0 as usize).ok_or_else(|| {
            unsupported("CT nominal schema original function placement is absent")
        })?;
        let root = self.function_environment(function, facts, raw, budget)?;
        let before = budget.live_temporary_bytes();
        let result = (|| {
            let bytes =
                value_context::schema_copy_cost(&raw.record_schemas, &raw.enum_schemas, budget)?;
            budget.reserve_temporary_bytes(bytes)?;
            let view = ValueContext {
                records: raw.record_schemas.clone(),
                enums: raw.enum_schemas.clone(),
                ty: CtTy::Scalar(CtScalarTy::Unit),
                bytes: 0,
                ambient_life_scopes: Vec::new(),
                invocation_context: None,
            };
            self.check_nominal_storage(
                schema_id(binding),
                &root,
                raw,
                &view,
                index,
                budget,
                &mut Vec::new(),
                0,
            )
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        result
    }
}
fn schema_id(binding: &SourceLifeBinding) -> SourceSchemaId {
    if binding.enumeration {
        SourceSchemaId::Enum(CtEnumSchemaId(binding.schema))
    } else {
        SourceSchemaId::Record(CtRecordSchemaId(binding.schema))
    }
}
fn schema_parts(id: SourceSchemaId) -> (bool, u32) {
    match id {
        SourceSchemaId::Record(id) => (false, id.0),
        SourceSchemaId::Enum(id) => (true, id.0),
    }
}
