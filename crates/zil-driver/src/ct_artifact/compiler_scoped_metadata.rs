//! Source-only binding of verified fixed metadata to its real lexical owner.
//! Names are spellings; definition identity and full original ordinal are keys.
use super::*;
impl<'a> Compiler<'a> {
    pub(super) fn source_life_scope(
        &mut self,
        module: usize,
        definition: &CtSymbolId,
        span: Span,
    ) -> R<Option<SourceLifeScope>> {
        self.budget.work(1, span)?;
        if self.modules[module].source.module_id != definition.module {
            return Err(error(
                span,
                "CT lexical owner has no matching source module",
            ));
        }
        let max_depth = self.budget.limits.max_call_depth;
        let (original, tree) = if definition.generation_path.is_some() {
            let catalog = self.generated_catalog.ok_or_else(|| {
                error(
                    span,
                    "CT generated lexical owner has no frozen source catalog",
                )
            })?;
            let Some((row, original)) = source_scope::generated_original_function(
                catalog,
                definition,
                max_depth,
                |work| self.budget.work(work, span),
            )?
            else {
                return Err(error(
                    span,
                    "CT generated lexical owner has no actual frozen declaration",
                ));
            };
            (original, source_sites::SourceTreeId::Generated(row as u32))
        } else {
            let Some(original) = source_scope::original_function(
                self.modules[module].syntax,
                definition.declaration,
                max_depth,
                |work| self.budget.work(work, span),
            )?
            else {
                return Ok(None);
            };
            (original, source_sites::SourceTreeId::Original)
        };
        let item = original.item;
        let function = original.function;
        if original.generic_host && self.active_host.is_none() && !self.checking_generics {
            return Err(vec![Diagnostic::unsupported(
                Phase::Ct,
                "CT member scope requires its actual enclosing generic context",
                Some(span),
            )]);
        }
        self.budget.work(function.generics.len(), span)?;
        let mut binders = Vec::new();
        for (ordinal, parameter) in function.generics.iter().enumerate() {
            self.budget.node(parameter.span)?;
            if matches!(parameter.kind, s::GenericParamKind::Lifetime) {
                self.budget.text(&parameter.name, parameter.span)?;
                Budget::take(
                    &mut self.budget.bytes,
                    64,
                    span,
                    "CT lexical scope binding bytes exhausted",
                )?;
                binders.push((
                    parameter.name.clone(),
                    u32::try_from(ordinal)
                        .map_err(|_| error(span, "source binder ordinal overflow"))?,
                ));
            }
        }
        if binders.is_empty() {
            return Ok(None);
        }
        // Spans can diagnose a mismatch only after the actual ordinal selected
        // the original declaration. Equal/shared spans do not select an owner.
        if tree == source_sites::SourceTreeId::Original
            && (span.file != item.span.file
                || span.start < item.span.start
                || span.end > item.span.end)
        {
            return Err(error(
                span,
                "scoped query is outside its actual original definition",
            ));
        }
        Ok(Some(SourceLifeScope {
            tree,
            enclosing: self.active_host,
            definition: definition.clone(),
            span: item.span,
            binders,
            instance: CtScopeInstance::Definition,
        }))
    }
    pub(super) fn source_scoped_nominal(
        &mut self,
        module: usize,
        owner: usize,
        item: &'a s::Item,
        args: &[s::GenericArg],
        depth: usize,
        span: Span,
    ) -> R<CtTy> {
        self.budget.depth(depth, span)?;
        let site = self.source_site_for_arguments(module, args, span)?;
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a) => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            _ => return Err(error(span, "scoped name is not a nominal declaration")),
        };
        if args.len() != parameters.len() {
            return Err(error(span, "scoped nominal actual arity differs"));
        }
        let scope = self
            .source_life_scope
            .as_ref()
            .ok_or_else(|| error(span, "nominal lifetime has no actual lexical source owner"))?;
        if scope.tree == source_sites::SourceTreeId::Original
            && (scope.span.file != span.file
                || span.start < scope.span.start
                || span.end > scope.span.end)
        {
            return Err(error(
                span,
                "nominal lifetime cannot capture another definition's scope",
            ));
        }
        self.budget.work(args.len(), span)?;
        Budget::take(
            &mut self.budget.bytes,
            args.len().saturating_mul(
                std::mem::size_of::<CtTypeArgument>()
                    + std::mem::size_of::<zil_ir::NominalLifeArgument>()
                    + 4,
            ),
            span,
            "CT scoped argument vector bytes exhausted",
        )?;
        let mut actual_parameters = Vec::with_capacity(args.len());
        let mut nodes = Vec::new();
        let mut nested_ambient = Vec::new();
        let mut arguments = Vec::with_capacity(args.len());
        let mut rows = Vec::with_capacity(args.len());
        let symbol = self.item_symbol(owner, item)?;
        for (ordinal, (parameter, arg)) in parameters.iter().zip(args).enumerate() {
            self.budget.node(span)?;
            Budget::take(
                &mut self.budget.bytes,
                128,
                span,
                "CT scoped nominal arguments exhausted",
            )?;
            match (&parameter.kind, arg) {
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    let scope = self
                        .source_life_scope
                        .as_ref()
                        .ok_or_else(|| error(span, "nominal lifetime scope disappeared"))?;
                    let mut actual = None;
                    for (bound, ordinal) in scope.binders.iter().rev() {
                        self.budget
                            .work(name.len().min(bound.len()).saturating_add(1), span)?;
                        if name == bound {
                            actual = Some(*ordinal);
                            break;
                        }
                    }
                    let actual = actual.ok_or_else(|| {
                        error(
                            span,
                            "nominal lifetime is not declared in the actual source definition",
                        )
                    })?;
                    self.budget.work(actual_parameters.len() + 1, span)?;
                    let binder = if let Some(index) =
                        actual_parameters.iter().position(|old| *old == actual)
                    {
                        index as u32
                    } else {
                        actual_parameters.push(actual);
                        (actual_parameters.len() - 1) as u32
                    };
                    arguments.push(CtTypeArgument::Lifetime(CtLifetimeArgument {
                        owner: CtTypeNodeId(u32::MAX),
                        binder: zil_ir::StorageBinder::Fixed(binder),
                    }));
                    rows.push(zil_ir::NominalLifeArgument {
                        path: vec![],
                        declaration: zil_ir::LifeDeclarationId {
                            module: symbol.module,
                            declaration: symbol.declaration,
                            compiler: false,
                        },
                        argument: ordinal as u32,
                        binder,
                    });
                }
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    let graph = self.metadata_type(module, ty, depth + 1)?;
                    self.metadata_copy(&graph, depth + 1, span)?;
                    self.budget.metadata(&graph, span)?;
                    let offset = u32::try_from(nodes.len())
                        .map_err(|_| error(span, "scoped graph ID overflow"))?;
                    if !graph.ambient_classes.is_empty() {
                        self.budget.work(graph.ambient_classes.len(), span)?;
                        Budget::take(
                            &mut self.budget.bytes,
                            nested_ambient
                                .len()
                                .saturating_add(graph.ambient_classes.len())
                                .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
                            span,
                            "CT nested ambient class backing exhausted",
                        )?;
                        nested_ambient.reserve_exact(graph.ambient_classes.len());
                        for mut row in graph.ambient_classes {
                            row.owner.0 =
                                row.owner.0.checked_add(offset).ok_or_else(|| {
                                    error(span, "CT nested ambient owner overflow")
                                })?;
                            nested_ambient.push(row);
                        }
                    }
                    for mut node in graph.nodes {
                        let mut invalid = false;
                        metadata::ids(&mut node, |id| {
                            if let Some(value) = id.0.checked_add(offset) {
                                id.0 = value
                            } else {
                                invalid = true
                            }
                        });
                        if invalid {
                            return Err(error(span, "scoped graph relocation overflow"));
                        }
                        nodes.push(node);
                    }
                    arguments.push(CtTypeArgument::Type(CtTypeNodeId(
                        graph
                            .root
                            .0
                            .checked_add(offset)
                            .ok_or_else(|| error(span, "scoped root overflow"))?,
                    )));
                }
                (s::GenericParamKind::Const(_), arg) => arguments.push(CtTypeArgument::Scalar(
                    self.source_scalar_argument(parameter, arg, span)?,
                )),
                _ => {
                    return Err(error(
                        span,
                        "scoped nominal actual kind differs from its original parameter",
                    ));
                }
            }
        }
        let nominal = CtTypeNodeId(
            u32::try_from(nodes.len()).map_err(|_| error(span, "scoped nominal ID overflow"))?,
        );
        let fixed = CtTypeNodeId(
            nominal
                .0
                .checked_add(1)
                .ok_or_else(|| error(span, "scoped owner ID overflow"))?,
        );
        self.budget.work(arguments.len(), span)?;
        for argument in &mut arguments {
            if let CtTypeArgument::Lifetime(value) = argument {
                value.owner = fixed;
            }
        }
        self.budget.node(span)?;
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.bytes,
            256,
            span,
            "CT scoped graph root bytes exhausted",
        )?;
        nodes.push(CtTypeNode::Nominal { symbol, arguments });
        nodes.push(CtTypeNode::BorrowedStorage {
            storage: nominal,
            life: zil_ir::StorageLifeShape {
                binder_count: actual_parameters.len() as u32,
                slots: vec![],
                retained: vec![],
                nominal_arguments: rows,
            },
        });
        let definition = self
            .source_life_scope
            .as_ref()
            .ok_or_else(|| error(span, "ambient nominal lost actual definition"))?
            .definition
            .clone();
        let instance = &self.source_life_scope.as_ref().unwrap().instance;
        if let CtScopeInstance::Instantiated(actuals) = instance {
            Self::actual_cost(&mut self.budget, actuals, span)?;
        }
        let scope = CtAmbientLifeScope {
            enclosing: self.source_life_scope.as_ref().unwrap().enclosing,
            declaration: definition,
            binder_site: Vec::new(),
            instance: self.source_life_scope.as_ref().unwrap().instance.clone(),
        };
        self.budget
            .work(self.ambient_life_scopes.len().saturating_mul(32), span)?;
        if matches!(scope.instance, CtScopeInstance::Instantiated(_)) {
            for candidate in &self.ambient_life_scopes {
                if candidate.declaration == scope.declaration
                    && matches!(candidate.instance, CtScopeInstance::Instantiated(_))
                {
                    self.budget.scope_comparison(candidate, span)?;
                    self.budget.scope_comparison(&scope, span)?;
                }
            }
        }
        let scope_id = if let Some(index) = self
            .ambient_life_scopes
            .iter()
            .position(|old| old == &scope)
        {
            index
        } else {
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<CtAmbientLifeScope>(),
                span,
                "CT ambient scope table bytes exhausted",
            )?;
            self.ambient_life_scopes.push(scope);
            self.ambient_life_scopes.len() - 1
        };
        let scope_id = CtAmbientScopeId(
            u32::try_from(scope_id).map_err(|_| error(span, "CT ambient scope ID overflow"))?,
        );
        self.budget.work(actual_parameters.len(), span)?;
        Budget::take(
            &mut self.budget.bytes,
            actual_parameters
                .len()
                .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
            span,
            "CT ambient mapping bytes exhausted",
        )?;
        let ambient_classes = actual_parameters
            .iter()
            .enumerate()
            .map(|(class, parameter)| CtAmbientClassBinding {
                owner: fixed,
                class: class as u32,
                source: CtAmbientLifeReference {
                    scope: scope_id,
                    parameter: *parameter,
                },
            })
            .collect::<Vec<_>>();
        let ambient_classes = if nested_ambient.is_empty() {
            ambient_classes
        } else {
            self.budget.work(ambient_classes.len(), span)?;
            Budget::take(
                &mut self.budget.bytes,
                nested_ambient
                    .len()
                    .saturating_add(ambient_classes.len())
                    .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>()),
                span,
                "CT joint ambient class backing exhausted",
            )?;
            nested_ambient.reserve_exact(ambient_classes.len());
            nested_ambient.extend(ambient_classes);
            nested_ambient
        };
        let context = self.meta_normalize(
            CtTypeRef {
                nodes,
                root: fixed,
                ambient_classes,
            },
            span,
        )?;
        self.budget.metadata(&context, span)?;
        let CtTypeNode::BorrowedStorage { storage, .. } = context.nodes[context.root.0 as usize]
        else {
            unreachable!()
        };
        let CtTypeNode::Nominal { arguments, .. } = &context.nodes[storage.0 as usize] else {
            unreachable!()
        };
        let mut schema_args = Vec::new();
        for argument in arguments {
            self.budget.node(span)?;
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<CtSchemaArgument>(),
                span,
                "CT scoped schema argument bytes exhausted",
            )?;
            schema_args.push(match argument {
                CtTypeArgument::Type(id) => CtSchemaArgument::TypeInContext(*id),
                CtTypeArgument::Scalar(value) => CtSchemaArgument::Scalar(value.clone()),
                CtTypeArgument::Lifetime(value) => CtSchemaArgument::Lifetime(*value),
            });
        }
        // Pay both the instance table and later private source-binding capture.
        self.budget.metadata(&context, span)?;
        self.budget.work(actual_parameters.len() + 32, span)?;
        Budget::take(
            &mut self.budget.bytes,
            std::mem::size_of::<SourceLifeBinding>() + actual_parameters.len() * 4,
            span,
            "CT scoped source binding bytes exhausted",
        )?;
        let previous_site = self.current_nominal_site.replace(site);
        let result = self.closed_nominal_context(
            owner,
            item,
            schema_args,
            Some((context, actual_parameters)),
            depth + 1,
            span,
        );
        self.current_nominal_site = previous_site;
        result
    }
    pub(super) fn registered_scoped_data_type(&mut self, graph: &CtTypeRef, span: Span) -> R<CtTy> {
        for (enumeration, count) in [(false, self.records.len()), (true, self.enums.len())] {
            for id in 0..count {
                self.budget.work(1, span)?;
                let context = if enumeration {
                    self.enums[id].life_context.as_ref()
                } else {
                    self.records[id].life_context.as_ref()
                };
                if let Some(context) = context {
                    self.budget.metadata(context, span)?;
                    self.budget.metadata(graph, span)?;
                    if context == graph {
                        self.budget.work(self.source_life_bindings.len(), span)?;
                        if !self
                            .source_life_bindings
                            .iter()
                            .any(|b| b.enumeration == enumeration && b.schema as usize == id)
                        {
                            return Err(error(
                                span,
                                "scoped data type has no authentic source binding",
                            ));
                        }
                        return Ok(if enumeration {
                            CtTy::Enum(CtEnumSchemaId(id as u32))
                        } else {
                            CtTy::Record(CtRecordSchemaId(id as u32))
                        });
                    }
                }
            }
        }
        Err(vec![Diagnostic::unsupported(
            Phase::Ct,
            "scoped type extraction needs its registered joint value context",
            Some(span),
        )])
    }
    pub(super) fn copy_registered_scoped_data(
        &mut self,
        graph: &CtTypeRef,
        depth: usize,
        span: Span,
    ) -> R<()> {
        self.budget.depth(depth, span)?;
        let ty = self.registered_scoped_data_type(graph, span)?;
        for old in &self.active_copy_metadata {
            self.budget.metadata(old, span)?;
            self.budget.metadata(graph, span)?;
            if old == graph {
                return Err(error(
                    span,
                    "recursive scoped Copy data has no finite layout",
                ));
            }
        }
        self.budget.metadata(graph, span)?;
        self.active_copy_metadata.push(graph.clone());
        let result = (|| {
            let count = match ty {
                CtTy::Record(id) => self.records[id.0 as usize].fields.len(),
                CtTy::Enum(id) => {
                    let variants = &self.enums[id.0 as usize].variants;
                    self.budget.work(variants.len(), span)?;
                    variants.iter().try_fold(0usize, |n, fields| {
                        n.checked_add(fields.len())
                            .ok_or_else(|| error(span, "scoped field count overflow"))
                    })?
                }
                _ => unreachable!(),
            };
            self.budget.work(count, span)?;
            Budget::take(
                &mut self.budget.bytes,
                count.saturating_mul(std::mem::size_of::<&CtTy>()),
                span,
                "CT scoped field traversal bytes exhausted",
            )?;
            let mut fields = Vec::with_capacity(count);
            match ty {
                CtTy::Record(id) => fields.extend(self.records[id.0 as usize].fields.iter()),
                CtTy::Enum(id) => {
                    fields.extend(self.enums[id.0 as usize].variants.iter().flatten())
                }
                _ => unreachable!(),
            }
            for ty in &fields {
                self.budget.copy_ty(ty, depth + 1, span)?;
            }
            let fields = fields.into_iter().cloned().collect::<Vec<_>>();
            for ty in fields {
                self.scoped_field_copy(&ty, depth + 1, span)?;
            }
            Ok(())
        })();
        self.active_copy_metadata.pop();
        result
    }
    fn scoped_field_copy(&mut self, ty: &CtTy, depth: usize, span: Span) -> R<()> {
        self.budget.depth(depth, span)?;
        self.budget.node(span)?;
        match ty {
            CtTy::Scalar(_) => Ok(()),
            CtTy::Tuple(fields) => {
                for field in fields {
                    self.scoped_field_copy(field, depth + 1, span)?;
                }
                Ok(())
            }
            CtTy::Array { element, .. } => self.scoped_field_copy(element, depth + 1, span),
            CtTy::Record(_) | CtTy::Enum(_) => {
                let graph = self.metadata_data(ty, span, depth + 1)?;
                self.metadata_copy(&graph, depth + 1, span)
            }
            _ => Err(error(
                span,
                "CT-only field metadata does not establish ordinary Copy qualification",
            )),
        }
    }
}
