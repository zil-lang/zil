//! Closed generic instances key on complete metadata, separate from scratch Copy models.
use super::*;
use cfg::{Builder, Value};
impl Compiler<'_> {
    fn graph_bindings(
        &mut self,
        names: &[String],
        arguments: &[CtTypeRef],
        span: Span,
    ) -> R<Vec<(String, CtTypeRef)>> {
        if names.len() != arguments.len() {
            return Err(error(span, "CT generic metadata arity differs"));
        }
        let mut bindings = Vec::new();
        for (name, graph) in names.iter().zip(arguments) {
            self.budget.text(name, span)?;
            self.budget.metadata(graph, span)?;
            bindings.push((name.clone(), graph.clone()));
        }
        Ok(bindings)
    }
    pub(super) fn metadata_instance_bindings(
        &mut self,
        declaration: usize,
        arguments: &[CtTypeRef],
    ) -> R<Vec<(String, CtTypeRef)>> {
        let f = self.declarations[declaration].function;
        let names = self.generic_names(&f.generics, &f.where_clause, f.span)?;
        for argument in arguments {
            self.metadata_copy(argument, 0, f.span)?;
        }
        self.graph_bindings(&names, arguments, f.span)
    }
    fn metadata_instance_header(
        &mut self,
        declaration: usize,
        arguments: &[CtTypeRef],
    ) -> R<(Vec<CtTy>, CtTy)> {
        let bindings = self.metadata_instance_bindings(declaration, arguments)?;
        let old_meta = std::mem::replace(&mut self.metadata_bindings, bindings);
        let old_data = std::mem::take(&mut self.bindings);
        let result = self.header(declaration);
        self.metadata_bindings = old_meta;
        self.bindings = old_data;
        result
    }
    fn enqueue_metadata_instance(
        &mut self,
        declaration: usize,
        arguments: &[CtTypeRef],
    ) -> R<CtFunctionId> {
        let span = self.declarations[declaration].item.span;
        self.ensure_generic_definition(declaration)?;
        self.metadata_instance_bindings(declaration, arguments)?;
        for i in 0..self.generic_metadata_selected.len() {
            self.budget.work(1, span)?;
            let (d, old, id) = &self.generic_metadata_selected[i];
            for graph in old.iter().chain(arguments) {
                self.budget.metadata(graph, span)?;
            }
            if *d == declaration && old == arguments {
                return Ok(*id);
            }
        }
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.functions,
            1,
            span,
            "CT source metadata instance budget exhausted",
        )?;
        for graph in arguments {
            self.budget.metadata(graph, span)?;
            self.budget.metadata(graph, span)?;
        }
        let id = CtFunctionId(self.functions.len() as u32);
        self.functions.push(None);
        self.generic_metadata_selected
            .push((declaration, arguments.to_vec(), id));
        self.budget.node(span)?;
        self.pending
            .push_back((declaration, PendingTypes::Metadata(arguments.to_vec()), id));
        Ok(id)
    }
    fn infer_metadata(
        &mut self,
        module: usize,
        pattern: &s::TypeRef,
        actual: &CtTy,
        names: &[String],
        slots: &mut [Option<CtTypeRef>],
        depth: usize,
    ) -> R<()> {
        if !self.has_unbound(pattern, names, slots, depth)? {
            return Ok(());
        }
        let graph = self.metadata_data(actual, pattern.span, depth + 1)?;
        self.infer_metadata_node(module, pattern, &graph, graph.root, names, slots, depth + 1)
    }
    #[allow(clippy::too_many_arguments)]
    fn infer_metadata_node(
        &mut self,
        module: usize,
        pattern: &s::TypeRef,
        graph: &CtTypeRef,
        id: CtTypeNodeId,
        names: &[String],
        slots: &mut [Option<CtTypeRef>],
        depth: usize,
    ) -> R<()> {
        self.budget.depth(depth, pattern.span)?;
        if let s::TypeKind::Named { path, args } = &pattern.kind
            && path.len() == 1
            && args.is_empty()
        {
            self.budget.work(names.len(), pattern.span)?;
            if let Some(index) = names.iter().position(|n| n == &path[0]) {
                let actual = self.metadata_subgraph(graph, id, pattern.span)?;
                if let Some(old) = &slots[index] {
                    self.budget.metadata(old, pattern.span)?;
                    self.budget.metadata(&actual, pattern.span)?;
                    if old != &actual {
                        return Err(error(
                            pattern.span,
                            "inconsistent complete CT generic metadata",
                        ));
                    }
                } else {
                    slots[index] = Some(actual);
                }
                return Ok(());
            }
        }
        let node = graph
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| error(pattern.span, "generic metadata child is absent"))?;
        match (&pattern.kind, node) {
            (s::TypeKind::Group(inner), _) | (s::TypeKind::ForLifetime { inner, .. }, _) => {
                self.infer_metadata_node(module, inner, graph, id, names, slots, depth + 1)?
            }
            (s::TypeKind::Tuple(fields), CtTypeNode::Tuple(types))
                if fields.len() == types.len() =>
            {
                for (field, id) in fields.iter().zip(types) {
                    self.infer_metadata_node(module, field, graph, *id, names, slots, depth + 1)?;
                }
            }
            (s::TypeKind::Array { element, .. }, CtTypeNode::Array { element: id, .. }) => {
                self.infer_metadata_node(module, element, graph, *id, names, slots, depth + 1)?
            }
            (s::TypeKind::Reference { inner, .. }, CtTypeNode::Address { inner: id, .. }) => {
                self.infer_metadata_node(module, inner, graph, *id, names, slots, depth + 1)?
            }
            (
                s::TypeKind::Function { params, result },
                CtTypeNode::FunctionPointer {
                    parameters,
                    result: output,
                }
                | CtTypeNode::BorrowedFunctionPointer {
                    parameters,
                    result: output,
                    ..
                },
            ) if params.len() == parameters.len() => {
                for (p, id) in params.iter().zip(parameters) {
                    self.infer_metadata_node(module, p, graph, *id, names, slots, depth + 1)?;
                }
                self.infer_metadata_node(module, result, graph, *output, names, slots, depth + 1)?;
            }
            (s::TypeKind::Named { path, args }, CtTypeNode::Nominal { symbol, arguments })
                if !args.is_empty() =>
            {
                let (owner, item) = self.named_item(module, path, pattern.span)?;
                if self.symbol(owner, item.span)? != *symbol || args.len() != arguments.len() {
                    return Err(error(
                        pattern.span,
                        "generic metadata nominal owner or arity differs",
                    ));
                }
                for (pattern, argument) in args.iter().zip(arguments) {
                    if let (s::GenericArg::Type(pattern), CtTypeArgument::Type(id)) =
                        (pattern, argument)
                    {
                        self.infer_metadata_node(
                            module,
                            pattern,
                            graph,
                            *id,
                            names,
                            slots,
                            depth + 1,
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn partial_metadata_type(
        &mut self,
        module: usize,
        pattern: &s::TypeRef,
        names: &[String],
        slots: &[Option<CtTypeRef>],
        depth: usize,
    ) -> R<Option<CtTy>> {
        if self.has_unbound(pattern, names, slots, depth)? {
            return Ok(None);
        }
        let mut bindings = Vec::new();
        for (name, slot) in names.iter().zip(slots) {
            if let Some(graph) = slot {
                self.budget.text(name, pattern.span)?;
                self.budget.metadata(graph, pattern.span)?;
                bindings.push((name.clone(), graph.clone()));
            }
        }
        let old_meta = std::mem::replace(&mut self.metadata_bindings, bindings);
        let old_data = std::mem::take(&mut self.bindings);
        let result = self.ty(module, pattern, depth + 1);
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        result.map(Some)
    }
}
impl Builder<'_, '_> {
    pub(super) fn metadata_generic_call(
        &mut self,
        declaration: usize,
        explicit: Option<&[s::GenericArg]>,
        arguments: &[s::Argument],
        expected: Option<&CtTy>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let d = self.compiler.declarations[declaration];
        let names =
            self.compiler
                .generic_names(&d.function.generics, &d.function.where_clause, span)?;
        if arguments.len() != d.function.params.len() {
            return Err(error(span, "CT generic actual argument count differs"));
        }
        self.compiler.budget.node(span)?;
        self.compiler.budget.work(names.len(), span)?;
        let mut slots = vec![None; names.len()];
        if let Some(explicit) = explicit {
            if explicit.len() != names.len() {
                return Err(error(span, "CT explicit metadata argument count differs"));
            }
            for (slot, argument) in slots.iter_mut().zip(explicit) {
                let s::GenericArg::Type(ty) = argument else {
                    return Err(error(
                        span,
                        "CT generic value argument needs a scalar binder",
                    ));
                };
                *slot = Some(self.compiler.metadata_type(self.module, ty, depth + 1)?);
            }
        }
        if let (Some(pattern), Some(actual)) = (&d.function.return_type, expected) {
            self.compiler.infer_metadata(
                d.module,
                pattern,
                actual,
                &names,
                &mut slots,
                depth + 1,
            )?;
        }
        let mut actual = Vec::new();
        let mut actual_types = Vec::new();
        for (i, argument) in arguments.iter().enumerate() {
            self.compiler.budget.node(argument.span)?;
            let hint = self.compiler.partial_metadata_type(
                d.module,
                &d.function.params[i].ty,
                &names,
                &slots,
                depth + 1,
            )?;
            let value = self.expr(&argument.value, hint.as_ref(), depth + 1)?;
            self.compiler.infer_metadata(
                d.module,
                &d.function.params[i].ty,
                &value.ty,
                &names,
                &mut slots,
                depth + 1,
            )?;
            actual.push(value.id);
            actual_types.push(value.ty);
        }
        let concrete = slots
            .into_iter()
            .map(|slot| {
                slot.ok_or_else(|| error(span, "CT complete type argument cannot be inferred"))
            })
            .collect::<R<Vec<_>>>()?;
        let (parameters, result) = self
            .compiler
            .metadata_instance_header(declaration, &concrete)?;
        for (actual, expected) in actual_types.iter().zip(&parameters) {
            self.compiler.budget.ty(actual, 0, span)?;
            self.compiler.budget.ty(expected, 0, span)?;
            if actual != expected {
                return Err(error(span, "CT generic actual complete type differs"));
            }
        }
        let function = self
            .compiler
            .enqueue_metadata_instance(declaration, &concrete)?;
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                span,
            )?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            self.emit(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                result,
                span,
            )
        }
    }
}
