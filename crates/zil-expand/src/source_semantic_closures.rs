//! Original lexical closure declarations. Inferred types remain original
//! parameter/result/binding obligations; Source never infers a capture ABI.
use super::*;
use std::collections::BTreeSet;

pub(super) struct ClosureUse {
    pub node: zil_base::NodeId,
    pub ty: SemanticTypeRecipe,
}
pub(super) struct ClosureTemplate {
    pub path: Vec<u32>,
    pub definition: SemanticDefinition,
    pub environment: Vec<SemanticActualRecipe>,
    pub uses: Vec<ClosureUse>,
}
impl Builder<'_, '_, '_, '_> {
    fn closure_signature(
        &mut self,
        params: &[s::ClosureParam],
        result: Option<&s::TypeRef>,
        span: Span,
    ) -> R<(
        SemanticSignatureRecipe,
        Vec<SemanticOriginalClosureParameter>,
    )> {
        let mut frames = std::mem::take(&mut self.invocations);
        self.push(
            &mut frames,
            InvocationFrame {
                inputs: Vec::new(),
                result: false,
                input_addresses: 0,
                result_life: None,
            },
            span,
        )?;
        self.invocations = frames;
        let output = (|| {
            let mut parameters = Vec::new();
            let mut originals = Vec::new();
            for (ordinal, parameter) in params.iter().enumerate() {
                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| error(span, "closure parameter ordinal overflow"))?;
                let annotation = parameter
                    .ty
                    .as_ref()
                    .map(|ty| self.ty(ty, &mut vec![1, ordinal], 0, false))
                    .transpose()?;
                let ty = if let Some(ty) = &annotation {
                    prepay_recipe(ty, self.budget, parameter.span, 0)?;
                    ty.clone()
                } else {
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Parameter(ordinal),
                    }
                };
                if let SemanticTypeRecipe::Address {
                    lifetime: SemanticOuterLifeRecipe::Class(life),
                    ..
                } = &ty
                {
                    prepay_recipe(life, self.budget, parameter.span, 0)?;
                    let frame = self.invocations.last_mut().expect("closure header frame");
                    frame.input_addresses = frame
                        .input_addresses
                        .checked_add(1)
                        .ok_or_else(|| error(span, "closure address input count overflow"))?;
                    frame.result_life = Some(life.clone());
                }
                let name = self.text(&parameter.name, parameter.span)?;
                self.push(
                    &mut originals,
                    CoreClosureParameter {
                        name,
                        ty: annotation,
                        mode: PassingMode::Infer,
                        span: parameter.span,
                    },
                    parameter.span,
                )?;
                self.push(
                    &mut parameters,
                    SemanticParameterRecipe {
                        ty,
                        mode: PassingMode::Infer,
                    },
                    parameter.span,
                )?;
            }
            self.invocations
                .last_mut()
                .expect("closure header frame")
                .result = true;
            let result = if let Some(result) = result {
                self.ty(result, &mut vec![2], 0, false)?
            } else {
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Result,
                }
            };
            let invocation_lifetimes = u32::try_from(
                self.invocations
                    .last()
                    .expect("closure header frame")
                    .inputs
                    .len(),
            )
            .map_err(|_| error(span, "closure invocation domain overflow"))?;
            Ok((
                SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes,
                    parameters,
                    result,
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                },
                originals,
            ))
        })();
        self.invocations.pop();
        output
    }
    /// The same authored child payload is preserved in its parent's literal
    /// and its own definition. Inner node IDs are definition-local and are
    /// never entered into the parent's operation directory.
    pub(super) fn original_closure_payload(
        &mut self,
        params: &[s::ClosureParam],
        result: Option<&s::TypeRef>,
        body: &s::ArrowBody,
        span: Span,
        depth: usize,
    ) -> R<(
        Vec<SemanticOriginalClosureParameter>,
        Option<SemanticTypeRecipe>,
        SemanticOriginalBlock,
    )> {
        self.budget.node(span, depth)?;
        let implicit = std::mem::take(&mut self.implicit);
        let invocations = std::mem::take(&mut self.invocations);
        let offset = self.lexical_offset;
        self.lexical_offset = offset
            .checked_add(1)
            .ok_or_else(|| error(span, "closure literal lexical depth overflow"))?;
        let output = (|| {
            let (signature, parameters) = self.closure_signature(params, result, span)?;
            let result = result.map(|_| signature.result);
            let body = self.original_arrow_body_depth(body, depth + 1)?;
            let SemanticOriginalFunctionBody::Defined(body) = body.body else {
                return Err(error(span, "authored closure lost its original body"));
            };
            Ok((parameters, result, body))
        })();
        self.implicit = implicit;
        self.invocations = invocations;
        self.lexical_offset = offset;
        output
    }
    pub(super) fn discover_closures(
        &mut self,
        function: &s::Function,
        original: &body::OriginalBodyOutput,
    ) -> R<Option<SemanticTypeRecipe>> {
        let SemanticOriginalFunctionBody::Defined(body) = &original.body else {
            return Ok(None);
        };
        let mut parameters = Vec::new();
        for parameter in &function.params {
            let name = self.text(&parameter.name, parameter.span)?;
            self.push(
                &mut parameters,
                CoreClosureParameter {
                    name,
                    ty: None,
                    mode: crate::mode(parameter),
                    span: parameter.span,
                },
                parameter.span,
            )?;
        }
        let scopes = self.original_closure_scopes(body, &parameters)?;
        if scopes.is_empty() {
            return Ok(matches!(
                function.return_type.as_ref().map(|ty| &ty.kind),
                Some(s::TypeKind::Impl(_))
            )
            .then_some(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            }));
        }
        self.budget.function_snapshot(function)?;
        let mut syntax = function.clone();
        let mut opaque = None;
        for scope in scopes {
            let node = original
                .nodes
                .iter()
                .find(|node| node.id == scope.expression)
                .ok_or_else(|| {
                    error(
                        function.span,
                        "original closure expression directory is absent",
                    )
                })?;
            self.budget
                .identity_work(original.nodes.len(), function.span)?;
            let located =
                body_nodes::function(&mut syntax.body, &node.path, self.budget, function.span)?
                    .ok_or_else(|| {
                        error(function.span, "original closure does not locate its syntax")
                    })?;
            let body_nodes::Node::Expression(expression) = located.node else {
                return Err(error(
                    function.span,
                    "original closure points at a statement",
                ));
            };
            let mut bindings = Vec::new();
            for (name, source) in scope.bindings {
                self.push(
                    &mut bindings,
                    (
                        name,
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source,
                        },
                    ),
                    function.span,
                )?;
            }
            self.discover_child(
                expression,
                &node.path,
                scope.expression,
                self.id(self.owner),
                bindings,
            )?;
            // The opaque hidden value is determined by the original return
            // constraints, including all branches, rather than a selected ABI.
            if matches!(
                function.return_type.as_ref().map(|ty| &ty.kind),
                Some(s::TypeKind::Impl(_))
            ) {
                opaque = Some(SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Result,
                });
            }
        }
        if opaque.is_none()
            && matches!(
                function.return_type.as_ref().map(|ty| &ty.kind),
                Some(s::TypeKind::Impl(_))
            )
        {
            opaque = Some(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            });
        }
        Ok(opaque)
    }
    fn discover_child(
        &mut self,
        expression: &mut s::Expr,
        path: &[u32],
        original_expression: zil_base::NodeId,
        parent: SemanticDefinitionId,
        bindings: Vec<(String, SemanticTypeRecipe)>,
    ) -> R<()> {
        let s::ExprKind::Closure {
            params,
            return_type,
            body,
            move_capture,
            ..
        } = &mut expression.kind
        else {
            return Err(error(
                expression.span,
                "lexical closure inventory points at a different operation",
            ));
        };
        let span = expression.span;
        let module = self.inventory.declarations[self.owner].module;
        let next = self
            .next_body_nodes
            .iter_mut()
            .find(|(actual, _)| *actual == module)
            .ok_or_else(|| error(span, "closure original declaration namespace is absent"))?;
        let id = SemanticDefinitionId {
            module,
            index: next.1,
        };
        next.1 = next
            .1
            .checked_add(1)
            .ok_or_else(|| error(span, "closure original namespace overflow"))?;
        let parent_implicit = std::mem::take(&mut self.implicit);
        let parent_invocations = std::mem::take(&mut self.invocations);
        let parent_offset = self.lexical_offset;
        self.lexical_offset = parent_offset
            .checked_add(1)
            .ok_or_else(|| error(span, "closure original lexical depth overflow"))?;
        let result = (|| {
            let (signature, parameters) =
                self.closure_signature(params, return_type.as_ref(), span)?;
            let original = self.original_arrow_body(body)?;
            let SemanticOriginalFunctionBody::Defined(block) = &original.body else {
                return Err(error(
                    span,
                    "authored closure has no complete original body",
                ));
            };
            let mut available = BTreeSet::new();
            for (name, _) in &bindings {
                self.budget.text(name, span)?;
                self.budget
                    .lineage_bytes(name.len() + std::mem::size_of::<String>() * 8, span)?;
                available.insert(name.clone());
            }
            let free = self.original_free_uses(block, &parameters, &available)?;
            let mut environment = Vec::new();
            let mut fields = Vec::new();
            let mut uses = Vec::new();
            let mut captured = Vec::new();
            for (name, occurrence) in free {
                self.budget
                    .identity_work(bindings.len() + original.nodes.len(), span)?;
                if !bindings.iter().any(|(binding, _)| binding == &name) {
                    return Err(error(
                        span,
                        "closure free name has no actual parent lexical binding",
                    ));
                }
                let captured_name = self.text(&name, span)?;
                let source = SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::NameAt {
                        at: SemanticOriginalNode::Expression(original_expression),
                        name: captured_name,
                    },
                });
                let ordinal = u32::try_from(environment.len())
                    .map_err(|_| error(span, "closure environment ordinal overflow"))?;
                prepay_recipe(&source, self.budget, span, 0)?;
                self.push(&mut environment, source.clone(), span)?;
                self.push(
                    &mut fields,
                    SemanticEnvironmentFormal {
                        kind: SemanticFormal::Type {
                            requirements: Vec::new(),
                        },
                        source,
                    },
                    span,
                )?;
                let use_node = original
                    .nodes
                    .iter()
                    .find(|row| row.id == occurrence)
                    .ok_or_else(|| error(span, "closure free use has no original operation"))?;
                self.budget.lineage_bytes(use_node.path.len() * 4, span)?;
                let path = use_node.path.clone();
                if !*move_capture {
                    self.budget.lineage_bytes((path.len() + 1) * 4, span)?;
                    let mut occurrence_path = Vec::with_capacity(path.len() + 1);
                    // One original name occurrence supplies both this
                    // implicit capture-head obligation and its TypeUse event.
                    // Consumers compare the declared identity directly; no
                    // private prefix conversion or ordinal guess is required.
                    occurrence_path.push(40);
                    occurrence_path.extend_from_slice(&path);
                    self.implicit(&occurrence_path, span)?;
                }
                let ty = SemanticTypeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                };
                prepay_recipe(&ty, self.budget, span, 0)?;
                self.push(&mut captured, (name, ty), span)?;
                self.push(
                    &mut uses,
                    ClosureUse {
                        node: occurrence,
                        ty: SemanticTypeRecipe::Environment {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        },
                    },
                    span,
                )?;
            }
            // Recursively inventory child templates using the child's true
            // lexical directory. No creator value ID is reused as a child ID.
            let scopes = self.original_closure_scopes(block, &parameters)?;
            for scope in scopes {
                let node = original
                    .nodes
                    .iter()
                    .find(|node| node.id == scope.expression)
                    .ok_or_else(|| error(span, "nested original closure node is absent"))?;
                self.budget.identity_work(original.nodes.len(), span)?;
                let located = body_nodes::arrow_body(body, &node.path, self.budget, span)?
                    .ok_or_else(|| error(span, "nested original closure syntax is absent"))?;
                let body_nodes::Node::Expression(expression) = located.node else {
                    return Err(error(span, "nested closure is not an expression"));
                };
                let mut bindings = Vec::new();
                for (name, source) in scope.bindings {
                    self.push(
                        &mut bindings,
                        (
                            name,
                            SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source,
                            },
                        ),
                        span,
                    )?;
                }
                for (name, ty) in &captured {
                    self.budget.identity_work(bindings.len(), span)?;
                    if !bindings.iter().any(|(local, _)| local == name) {
                        let name = self.text(name, span)?;
                        prepay_recipe(ty, self.budget, span, 0)?;
                        self.push(&mut bindings, (name, ty.clone()), span)?;
                    }
                }
                self.discover_child(expression, &node.path, scope.expression, id, bindings)?;
            }
            let mut identity = self.identity(self.owner)?;
            let component = self.text("closure", span)?;
            self.push(&mut identity.declaration_path, component, span)?;
            // Stable original declaration-local operation namespace, never a
            // runtime ClosureId or an inferred implementation target.
            let component = self.text(&id.index.to_string(), span)?;
            self.push(&mut identity.declaration_path, component, span)?;
            let definition = SemanticDefinition {
                id,
                identity,
                parent: Some(parent),
                kind: SemanticDefinitionKind::Closure,
                parameters: Vec::new(),
                implicit_lifetimes: std::mem::take(&mut self.implicit),
                requirements: Vec::new(),
                environment: fields,
                body: SemanticDefinitionBody::FunctionWithBody {
                    names: self.original_names()?,
                    signature,
                    sites: Vec::new(),
                    parameters,
                    receiver_parameter: None,
                    site_nodes: Vec::new(),
                    body: original.body,
                },
            };
            let mut indices = std::mem::take(&mut self.body_nodes);
            self.push(
                &mut indices,
                DefinitionBodyNodes {
                    definition: id,
                    nodes: original.nodes,
                },
                span,
            )?;
            self.body_nodes = indices;
            self.budget.lineage_bytes(path.len() * 4, span)?;
            let row = ClosureTemplate {
                path: path.to_vec(),
                definition,
                environment,
                uses,
            };
            let mut rows = std::mem::take(&mut self.closures);
            self.push(&mut rows, row, span)?;
            self.closures = rows;
            Ok(())
        })();
        self.lexical_offset = parent_offset;
        self.implicit = parent_implicit;
        self.invocations = parent_invocations;
        result
    }
}
