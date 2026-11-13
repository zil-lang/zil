use super::*;

impl Builder<'_, '_, '_, '_> {
    fn life(&mut self, name: Option<&str>, path: &[u32], span: Span) -> R<SemanticLifeRecipe> {
        if let Some(name) = name {
            for (depth, frame) in self.bound_lives.iter().rev().enumerate() {
                for (index, bound) in frame.iter().enumerate().rev() {
                    self.budget.node(span, depth)?;
                    self.budget.text(name, span)?;
                    self.budget.text(bound, span)?;
                    if bound == name {
                        return Ok(SemanticLifeRecipe::Bound {
                            depth: u32::try_from(depth)
                                .map_err(|_| error(span, "bound life depth overflow"))?,
                            index: u32::try_from(index)
                                .map_err(|_| error(span, "bound life ordinal overflow"))?,
                        });
                    }
                }
            }
            self.formal(name, 2, span)?
                .map(SemanticLifeRecipe::Formal)
                .ok_or_else(|| error(span, "life occurrence has no original lexical formal"))
        } else if let Some(frame) = self.invocations.last_mut() {
            self.budget.identity_work(
                frame
                    .inputs
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
                span,
            )?;
            if frame.result {
                if frame.input_addresses != 1 {
                    return Err(error(
                        span,
                        "elided callable result has no unique original input occurrence",
                    ));
                }
                return frame.result_life.clone().ok_or_else(|| {
                    error(span, "elided callable result lacks its actual input class")
                });
            }
            if let Some(index) = frame.inputs.iter().position(|old| old == path) {
                return Ok(SemanticLifeRecipe::Invocation(
                    u32::try_from(index)
                        .map_err(|_| error(span, "callable invocation ordinal overflow"))?,
                ));
            }
            let index = u32::try_from(frame.inputs.len())
                .map_err(|_| error(span, "callable invocation ordinal overflow"))?;
            self.budget.lineage_bytes(
                path.len()
                    .checked_mul(std::mem::size_of::<u32>())
                    .ok_or_else(|| error(span, "callable occurrence path overflow"))?,
                span,
            )?;
            if frame.inputs.len() == frame.inputs.capacity() {
                let growth = frame.inputs.capacity().max(4);
                self.budget.lineage_bytes(
                    growth
                        .checked_mul(std::mem::size_of::<Vec<u32>>())
                        .ok_or_else(|| error(span, "callable invocation inventory overflow"))?,
                    span,
                )?;
                frame
                    .inputs
                    .try_reserve_exact(growth)
                    .map_err(|_| error(span, "callable invocation allocation failed"))?;
            }
            frame.inputs.push(path.to_vec());
            Ok(SemanticLifeRecipe::Invocation(index))
        } else {
            self.implicit(path, span)
        }
    }
    fn callable_signature(
        &mut self,
        parameters: &[s::TypeRef],
        result: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticSignatureRecipe> {
        self.budget.node(result.span, depth)?;
        let mut frames = std::mem::take(&mut self.invocations);
        self.push(
            &mut frames,
            InvocationFrame {
                inputs: Vec::new(),
                result: false,
                input_addresses: 0,
                result_life: None,
            },
            result.span,
        )?;
        self.invocations = frames;
        let output = (|| {
            let mut inputs = Vec::new();
            for (index, input) in parameters.iter().enumerate() {
                path.push(index as u32);
                let ty = self.ty(input, path, depth + 1, false)?;
                path.pop();
                if let SemanticTypeRecipe::Address {
                    lifetime: SemanticOuterLifeRecipe::Class(life),
                    ..
                } = &ty
                {
                    // Elision binds an actual input occurrence, including an
                    // explicitly named outer class. A hidden whole-type
                    // domain or a weak description is not an address input.
                    if !matches!(
                        life,
                        SemanticLifeRecipe::Formal(_)
                            | SemanticLifeRecipe::Implicit { .. }
                            | SemanticLifeRecipe::Bound { .. }
                            | SemanticLifeRecipe::Invocation(_)
                    ) {
                        return Err(unsupported(
                            input.span,
                            "callable elision requires its actual original outer input class",
                        ));
                    }
                    let frame = self.invocations.last_mut().expect("actual callable frame");
                    frame.input_addresses = frame
                        .input_addresses
                        .checked_add(1)
                        .ok_or_else(|| error(input.span, "callable input occurrence overflow"))?;
                    frame.result_life = Some(life.clone());
                }
                self.push(
                    &mut inputs,
                    SemanticParameterRecipe {
                        ty,
                        mode: PassingMode::Infer,
                    },
                    input.span,
                )?;
            }
            self.invocations
                .last_mut()
                .expect("actual callable frame")
                .result = true;
            path.push(parameters.len() as u32);
            let result = self.ty(result, path, depth + 1, false)?;
            path.pop();
            let invocation_lifetimes = u32::try_from(
                self.invocations
                    .last()
                    .expect("actual callable frame")
                    .inputs
                    .len(),
            )
            .map_err(|_| error(Span::default(), "callable invocation domain overflow"))?;
            Ok(SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes,
                parameters: inputs,
                result,
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
            })
        })();
        self.invocations.pop();
        output
    }
    fn self_scope(&mut self, span: Span) -> R<SemanticScopeRef> {
        let mut row = Some(self.owner);
        let mut distance = 0u32;
        while let Some(index) = row {
            self.budget.node(span, 0)?;
            let source = &self.inventory.declarations[index];
            if matches!(
                source.item.kind,
                s::ItemKind::Aggregate(_)
                    | s::ItemKind::Enum(_)
                    | s::ItemKind::Trait(_)
                    | s::ItemKind::Impl(_)
            ) {
                return Ok(SemanticScopeRef::Ancestor(distance));
            }
            row = source.parent;
            distance = distance
                .checked_add(1)
                .ok_or_else(|| error(span, "Self scope depth overflow"))?;
        }
        Err(error(span, "Self has no original nominal/trait/impl owner"))
    }
    fn actuals(
        &mut self,
        row: usize,
        arguments: &[s::GenericArg],
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<Vec<SemanticActualRecipe>> {
        let source = &self.inventory.declarations[row];
        let parameters = source.parameters();
        if parameters.len() != arguments.len() {
            return Err(error(
                source.item.span,
                "semantic application loses original mixed actual order",
            ));
        }
        let mut output = Vec::new();
        for (index, (formal, argument)) in parameters.iter().zip(arguments).enumerate() {
            path.push(index as u32);
            let value = match (&formal.kind, argument) {
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    SemanticActualRecipe::Life(self.life(Some(name), path, source.item.span)?)
                }
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    SemanticActualRecipe::Type(self.ty(ty, path, depth + 1, false)?)
                }
                (s::GenericParamKind::Const(carrier), s::GenericArg::Const(value)) => {
                    // The original carrier is normalized in its declaration
                    // scope, never guessed from the literal's default width.
                    // Dependent carriers need actual substitution first.
                    if !parameters.is_empty()
                        && matches!(&carrier.kind,s::TypeKind::Named{path,args} if args.is_empty() && parameters.iter().any(|p|path.as_slice()==[p.name.as_str()]))
                    {
                        return Err(unsupported(
                            carrier.span,
                            "dependent Const carrier needs original formal substitution",
                        ));
                    }
                    let carrier = self.ty(carrier, path, depth + 1, false)?;
                    let value = self.constant(value, &carrier, path, depth + 1)?;
                    SemanticActualRecipe::Const { carrier, value }
                }
                _ => {
                    return Err(error(
                        source.item.span,
                        "semantic actual kind disagrees with its original formal",
                    ));
                }
            };
            path.pop();
            self.push(&mut output, value, source.item.span)?;
        }
        Ok(output)
    }
    fn application(
        &mut self,
        ty: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticTraitRecipe> {
        let s::TypeKind::Named { path: names, args } = &ty.kind else {
            return Err(unsupported(
                ty.span,
                "trait premise must retain its original application",
            ));
        };
        let row = self.named(names, ty.span)?;
        if !matches!(
            self.inventory.declarations[row].item.kind,
            s::ItemKind::Trait(_)
        ) {
            return Err(error(
                ty.span,
                "semantic trait application names a nontrait declaration",
            ));
        }
        let arguments = self.actuals(row, args, path, depth + 1)?;
        Ok(SemanticTraitRecipe {
            declaration: LifeDeclarationId {
                module: self.id(row).module,
                declaration: self.inventory.declarations[row].declaration,
                compiler: false,
            },
            arguments,
        })
    }
    pub(super) fn ty(
        &mut self,
        ty: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
        payload: bool,
    ) -> R<SemanticTypeRecipe> {
        self.budget.node(ty.span, depth)?;
        self.budget
            .lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), ty.span)?;
        match &ty.kind {
            s::TypeKind::Group(inner) => self.ty(inner, path, depth + 1, payload),
            s::TypeKind::Named { path: names, args } => {
                if let [name] = names.as_slice() {
                    if args.is_empty() {
                        if let Some(formal) = self.formal(name, 0, ty.span)? {
                            return Ok(SemanticTypeRecipe::Formal(formal));
                        }
                        if name == "Self" {
                            return Ok(SemanticTypeRecipe::SelfType {
                                scope: self.self_scope(ty.span)?,
                            });
                        }
                        if name == "ByteBuffer" || name == "TaskI32" {
                            let inner =
                                Box::new(SemanticTypeRecipe::Atom(if name == "ByteBuffer" {
                                    Type::ByteBuffer
                                } else {
                                    Type::TaskI32
                                }));
                            return Ok(if payload {
                                *inner
                            } else {
                                SemanticTypeRecipe::Desc {
                                    lifetime: SemanticOuterLifeRecipe::Class(
                                        self.implicit(path, ty.span)?,
                                    ),
                                    inner,
                                }
                            });
                        }
                        let atom = crate::type_ref(ty);
                        if atom.is_b0_scalar()
                            || matches!(
                                atom,
                                Type::Unit
                                    | Type::ByteBuffer
                                    | Type::ForeignHandle
                                    | Type::CWideString
                                    | Type::TaskI32
                            )
                        {
                            return Ok(SemanticTypeRecipe::Atom(atom));
                        }
                    }
                    if matches!(
                        name.as_str(),
                        "Managed"
                            | "Desc"
                            | "Store"
                            | "Mutex"
                            | "Registration"
                            | "AsyncRegistration"
                    ) {
                        let [s::GenericArg::Type(inner)] = args.as_slice() else {
                            return Err(error(
                                ty.span,
                                "sealed semantic constructor needs its complete type argument",
                            ));
                        };
                        path.push(0);
                        let inner = Box::new(self.ty(
                            inner,
                            path,
                            depth + 1,
                            name == "Managed" || name == "Desc",
                        )?);
                        path.pop();
                        let result = match name.as_str() {
                            "Managed" => SemanticTypeRecipe::Managed(inner),
                            "Desc" => SemanticTypeRecipe::Desc {
                                lifetime: SemanticOuterLifeRecipe::Class(
                                    self.implicit(path, ty.span)?,
                                ),
                                inner,
                            },
                            "Store" => SemanticTypeRecipe::OwnedStore(inner),
                            "Mutex" => SemanticTypeRecipe::Mutex(inner),
                            "Registration" => SemanticTypeRecipe::Registration(inner),
                            _ => SemanticTypeRecipe::AsyncRegistration(inner),
                        };
                        return Ok(if payload || matches!(name.as_str(), "Managed" | "Desc") {
                            result
                        } else {
                            SemanticTypeRecipe::Desc {
                                lifetime: SemanticOuterLifeRecipe::Class(
                                    self.implicit(path, ty.span)?,
                                ),
                                inner: Box::new(result),
                            }
                        });
                    }
                    if name == "OwnedDyn" {
                        let [s::GenericArg::Type(interface), s::GenericArg::Type(context)] =
                            args.as_slice()
                        else {
                            return Err(error(
                                ty.span,
                                "OwnedDyn must retain its complete interface and Context",
                            ));
                        };
                        let interface =
                            self.dyn_interface(interface, Some(context), path, depth + 1)?;
                        return Ok(SemanticTypeRecipe::Dyn {
                            owned: true,
                            interface: Box::new(interface),
                        });
                    }
                }
                let row = self.named(names, ty.span)?;
                let arguments = self.actuals(row, args, path, depth + 1)?;
                let result = SemanticTypeRecipe::Nominal {
                    definition: self.id(row),
                    arguments,
                };
                if !payload
                    && matches!(&self.inventory.declarations[row].item.kind,s::ItemKind::Aggregate(a) if a.kind==s::AggregateKind::Resource)
                {
                    Ok(SemanticTypeRecipe::Desc {
                        lifetime: SemanticOuterLifeRecipe::Class(self.implicit(path, ty.span)?),
                        inner: Box::new(result),
                    })
                } else if matches!(
                    self.inventory.declarations[row].item.kind,
                    s::ItemKind::TypeAlias(_) | s::ItemKind::Trait(_)
                ) {
                    Err(unsupported(
                        ty.span,
                        "alias/trait value needs its actual original recipe substitution",
                    ))
                } else {
                    Ok(result)
                }
            }
            s::TypeKind::Reference {
                mutable,
                lifetime,
                inner,
            } => {
                let life = self.life(lifetime.as_deref(), path, ty.span)?;
                path.push(0);
                let inner = Box::new(self.ty(inner, path, depth + 1, false)?);
                path.pop();
                Ok(SemanticTypeRecipe::Address {
                    mutable: *mutable,
                    lifetime: SemanticOuterLifeRecipe::Class(life),
                    inner,
                })
            }
            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) => {
                let mut output = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    path.push(index as u32);
                    let value = self.ty(field, path, depth + 1, false)?;
                    path.pop();
                    self.push(&mut output, value, field.span)?;
                }
                Ok(if matches!(ty.kind, s::TypeKind::Tuple(_)) {
                    SemanticTypeRecipe::Tuple(output)
                } else {
                    SemanticTypeRecipe::AnonymousSum(output)
                })
            }
            s::TypeKind::Array { element, length } => {
                path.push(0);
                let element = Box::new(self.ty(element, path, depth + 1, false)?);
                path.pop();
                if let Some(length) = length {
                    let carrier = SemanticTypeRecipe::Atom(Type::Usize);
                    path.push(1);
                    let length = Box::new(self.constant(length, &carrier, path, depth + 1)?);
                    path.pop();
                    Ok(SemanticTypeRecipe::Array { element, length })
                } else {
                    Ok(SemanticTypeRecipe::Slice(element))
                }
            }
            s::TypeKind::Error {
                error: failure,
                result,
            } => {
                path.push(0);
                let error = Box::new(self.ty(failure, path, depth + 1, false)?);
                path.pop();
                path.push(1);
                let result = Box::new(self.ty(result, path, depth + 1, false)?);
                path.pop();
                Ok(SemanticTypeRecipe::ErrorUnion { error, result })
            }
            s::TypeKind::Dyn { .. } => Ok(SemanticTypeRecipe::Dyn {
                owned: false,
                interface: Box::new(self.dyn_interface(ty, None, path, depth + 1)?),
            }),
            s::TypeKind::ForLifetime { .. } => {
                let mut inner = ty;
                let mut names = Vec::new();
                while let s::TypeKind::ForLifetime {
                    lifetime,
                    inner: next,
                } = &inner.kind
                {
                    self.budget
                        .node(inner.span, depth.saturating_add(names.len()))?;
                    let name = self.text(lifetime, inner.span)?;
                    self.push(&mut names, name, inner.span)?;
                    inner = next;
                }
                if !matches!(
                    inner.kind,
                    s::TypeKind::Function { .. } | s::TypeKind::ForeignFunction { .. }
                ) {
                    return Err(unsupported(
                        ty.span,
                        "quantified value type needs its actual Code signature",
                    ));
                }
                let count = u32::try_from(names.len())
                    .map_err(|_| error(ty.span, "Code quantifier inventory overflow"))?;
                let mut frames = std::mem::take(&mut self.bound_lives);
                self.push(&mut frames, names, ty.span)?;
                self.bound_lives = frames;
                let output = self.ty(inner, path, depth + 1, payload);
                self.bound_lives.pop();
                let SemanticTypeRecipe::Code {
                    kind,
                    mut signature,
                } = output?
                else {
                    return Err(error(
                        ty.span,
                        "quantified Code normalization lost its actual signature",
                    ));
                };
                signature.bound_lifetimes = count;
                Ok(SemanticTypeRecipe::Code { kind, signature })
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                let signature = self.callable_signature(params, result, path, depth + 1)?;
                self.budget
                    .lineage_bytes(std::mem::size_of::<SemanticSignatureRecipe>(), ty.span)?;
                Ok(SemanticTypeRecipe::Code {
                    kind: if matches!(ty.kind, s::TypeKind::ForeignFunction { .. }) {
                        SemanticCodeKind::ForeignCallback
                    } else {
                        SemanticCodeKind::FunctionPointer
                    },
                    signature: Box::new(signature),
                })
            }
            _ => Err(unsupported(
                ty.span,
                "original semantic type needs its complete scoped constructor",
            )),
        }
    }
    fn dyn_interface(
        &mut self,
        ty: &s::TypeRef,
        context: Option<&s::TypeRef>,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticDynInterfaceRecipe> {
        let s::TypeKind::Dyn {
            path: names,
            args,
            bindings,
        } = &ty.kind
        else {
            return Err(error(ty.span, "dynamic interface syntax is absent"));
        };
        self.budget.ty(ty)?;
        let named = s::TypeRef {
            kind: s::TypeKind::Named {
                path: names.clone(),
                args: args.clone(),
            },
            span: ty.span,
        };
        let application = self.application(&named, path, depth + 1)?;
        if !bindings.is_empty() {
            return Err(unsupported(
                ty.span,
                "dynamic associated bindings need full original member resolution",
            ));
        }
        let context = match context {
            Some(ty) => {
                SemanticDynContextRecipe::Type(Box::new(self.ty(ty, path, depth + 1, false)?))
            }
            None => SemanticDynContextRecipe::Closed,
        };
        Ok(SemanticDynInterfaceRecipe {
            application,
            context,
            associated: Vec::new(),
        })
    }
    fn requirement(
        &mut self,
        subject: SemanticTypeRecipe,
        bound: &s::TypeRef,
        path: &mut Vec<u32>,
        depth: usize,
    ) -> R<SemanticRequirementRecipe> {
        self.budget.node(bound.span, depth)?;
        if let s::TypeKind::Group(inner) = &bound.kind {
            return self.requirement(subject, inner, path, depth + 1);
        }
        if let s::TypeKind::ForLifetime { lifetime, inner } = &bound.kind {
            let name = self.text(lifetime, bound.span)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<String>(), bound.span)?;
            let mut frames = std::mem::take(&mut self.bound_lives);
            self.push(&mut frames, vec![name], bound.span)?;
            self.bound_lives = frames;
            // Restore the lexical frame even on failure; no incomplete
            // normalized requirement is published into the original program.
            let requirement = self.requirement(subject, inner, path, depth + 1);
            self.bound_lives.pop();
            let requirement = requirement?;
            let mut requirements = Vec::new();
            self.push(&mut requirements, requirement, bound.span)?;
            return Ok(SemanticRequirementRecipe::ForAll {
                lifetimes: 1,
                requirements,
            });
        }
        if let s::TypeKind::Named { path: names, args } = &bound.kind
            && let [name] = names.as_slice()
            && let Some(kind) = match name.as_str() {
                "Fn" => Some(CallableKind::Fn),
                "FnMut" => Some(CallableKind::FnMut),
                "FnOnce" => Some(CallableKind::FnOnce),
                _ => None,
            }
        {
            let [s::GenericArg::Type(parameters), s::GenericArg::Type(result)] = args.as_slice()
            else {
                return Err(error(
                    bound.span,
                    "callable premise lacks its full original signature",
                ));
            };
            let s::TypeKind::Tuple(parameters) = &parameters.kind else {
                return Err(error(
                    bound.span,
                    "callable premise parameters are not the parser's actual tuple",
                ));
            };
            let signature = self.callable_signature(parameters, result, path, depth + 1)?;
            self.budget
                .lineage_bytes(std::mem::size_of::<SemanticSignatureRecipe>(), bound.span)?;
            return Ok(SemanticRequirementRecipe::Callable {
                ty: subject,
                kind,
                signature: Box::new(signature),
            });
        }
        if let s::TypeKind::Named { path: names, args } = &bound.kind
            && args.is_empty()
            && let [name] = names.as_slice()
        {
            let builtin = match name.as_str() {
                "Copy" => Some(DynStructuralBound::Copy),
                "Value" => Some(DynStructuralBound::Value),
                "StoreElement" => Some(DynStructuralBound::StoreElement),
                "SyncDrop" => Some(DynStructuralBound::SyncDrop),
                _ => None,
            };
            if let Some(bound) = builtin {
                return Ok(SemanticRequirementRecipe::Structural { ty: subject, bound });
            }
        }
        Ok(SemanticRequirementRecipe::Trait {
            ty: subject,
            application: self.application(bound, path, depth + 1)?,
        })
    }
    pub(super) fn definition(&mut self, index: usize) -> R<Option<SemanticDefinition>> {
        let row = &self.inventory.declarations[index];
        let mut parameters = Vec::new();
        let mut requirements = Vec::new();
        for (ordinal, formal) in row.parameters().iter().enumerate() {
            let mut path = vec![0, ordinal as u32];
            let value = match &formal.kind {
                s::GenericParamKind::Lifetime => SemanticFormal::Lifetime,
                s::GenericParamKind::Const(carrier) => SemanticFormal::Const {
                    carrier: self.ty(carrier, &mut path, 0, false)?,
                },
                s::GenericParamKind::Type(bounds) => {
                    let mut constraints = Vec::new();
                    for bound in bounds {
                        let subject = SemanticTypeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        });
                        let constraint = self.requirement(subject, bound, &mut path, 0)?;
                        self.push(&mut constraints, constraint, bound.span)?;
                    }
                    SemanticFormal::Type {
                        requirements: constraints,
                    }
                }
            };
            self.push(&mut parameters, value, formal.span)?;
        }
        let (kind, body) = match &row.item.kind {
            s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                for clause in &function.where_clause {
                    self.where_clause(clause, &mut requirements)?;
                }
                let mut inputs = Vec::new();
                for (ordinal, input) in function.params.iter().enumerate() {
                    let ty = self.ty(&input.ty, &mut vec![1, ordinal as u32], 0, false)?;
                    self.push(
                        &mut inputs,
                        SemanticParameterRecipe {
                            ty,
                            mode: crate::mode(input),
                        },
                        input.span,
                    )?;
                }
                let result = match &function.return_type {
                    Some(ty) => self.ty(ty, &mut vec![2], 0, false)?,
                    None => SemanticTypeRecipe::Atom(Type::Unit),
                };
                (
                    if row.parent.is_some() {
                        SemanticDefinitionKind::Method
                    } else {
                        SemanticDefinitionKind::Function
                    },
                    SemanticDefinitionBody::Function {
                        signature: SemanticSignatureRecipe {
                            bound_lifetimes: 0,
                            invocation_lifetimes: 0,
                            parameters: inputs,
                            result,
                            result_mode: self.result_mode(function)?,
                            call_kind: CallKind::Direct,
                        },
                        sites: Vec::new(),
                    },
                )
            }
            s::ItemKind::Aggregate(a) => {
                let mut fields = Vec::new();
                for (ordinal, field) in a.fields.iter().enumerate() {
                    let mut ty =
                        self.ty(&field.ty, &mut vec![3, ordinal as u32], 0, field.manages)?;
                    if field.manages {
                        ty = SemanticTypeRecipe::Managed(Box::new(ty));
                    }
                    let name = self.text(&field.name, field.span)?;
                    self.push(&mut fields, SemanticFieldRecipe { name, ty }, field.span)?;
                }
                (
                    SemanticDefinitionKind::Nominal,
                    SemanticDefinitionBody::Nominal {
                        declaration: LifeDeclarationId {
                            module: row.module,
                            declaration: row.declaration,
                            compiler: false,
                        },
                        kind: match a.kind {
                            s::AggregateKind::Struct => LifeNominalKind::Record,
                            s::AggregateKind::Package => LifeNominalKind::Package,
                            s::AggregateKind::Resource => LifeNominalKind::Resource,
                        },
                        body: SemanticNominalBody::Product { fields },
                    },
                )
            }
            s::ItemKind::Enum(e) => {
                let mut variants = Vec::new();
                for (vi, variant) in e.variants.iter().enumerate() {
                    let mut fields = Vec::new();
                    match &variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(values) => {
                            for (fi, field) in values.iter().enumerate() {
                                let mut ty = self.ty(
                                    &field.ty,
                                    &mut vec![4, vi as u32, fi as u32],
                                    0,
                                    field.manages,
                                )?;
                                if field.manages {
                                    ty = SemanticTypeRecipe::Managed(Box::new(ty));
                                }
                                self.push(&mut fields, ty, field.span)?;
                            }
                        }
                        s::VariantFields::Record(values) => {
                            for (fi, field) in values.iter().enumerate() {
                                let mut ty = self.ty(
                                    &field.ty,
                                    &mut vec![4, vi as u32, fi as u32],
                                    0,
                                    field.manages,
                                )?;
                                if field.manages {
                                    ty = SemanticTypeRecipe::Managed(Box::new(ty));
                                }
                                self.push(&mut fields, ty, field.span)?;
                            }
                        }
                    }
                    let name = self.text(&variant.name, variant.span)?;
                    self.push(
                        &mut variants,
                        SemanticVariantRecipe { name, fields },
                        variant.span,
                    )?;
                }
                (
                    SemanticDefinitionKind::Nominal,
                    SemanticDefinitionBody::Nominal {
                        declaration: LifeDeclarationId {
                            module: row.module,
                            declaration: row.declaration,
                            compiler: false,
                        },
                        kind: LifeNominalKind::Enum,
                        body: SemanticNominalBody::Enum { variants },
                    },
                )
            }
            s::ItemKind::Trait(t) => {
                for bound in &t.bounds {
                    let value = self.requirement(
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(0),
                        },
                        bound,
                        &mut vec![5],
                        0,
                    )?;
                    self.push(&mut requirements, value, bound.span)?;
                }
                (
                    SemanticDefinitionKind::Scope,
                    SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                        declaration: LifeDeclarationId {
                            module: row.module,
                            declaration: row.declaration,
                            compiler: false,
                        },
                    }),
                )
            }
            s::ItemKind::Impl(i) => {
                for clause in &i.where_clause {
                    self.where_clause(clause, &mut requirements)?;
                }
                let self_type = self.ty(&i.target, &mut vec![6], 0, true)?;
                let declaration = LifeDeclarationId {
                    module: row.module,
                    declaration: row.declaration,
                    compiler: false,
                };
                (
                    SemanticDefinitionKind::Scope,
                    SemanticDefinitionBody::Scope(match &i.trait_type {
                        Some(ty) => SemanticScopeRecipe::TraitImpl {
                            declaration,
                            self_type,
                            trait_application: self.application(ty, &mut vec![7], 0)?,
                        },
                        None => SemanticScopeRecipe::InherentImpl {
                            declaration,
                            self_type,
                        },
                    }),
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(SemanticDefinition {
            id: self.id(index),
            identity: self.identity(index)?,
            parent: row.parent.map(|parent| self.id(parent)),
            kind,
            parameters,
            implicit_lifetimes: std::mem::take(&mut self.implicit),
            requirements,
            environment: Vec::new(),
            body,
        }))
    }
    fn where_clause(
        &mut self,
        clause: &s::BoundClause,
        output: &mut Vec<SemanticRequirementRecipe>,
    ) -> R<()> {
        match &clause.subject {
            s::BoundSubject::Type(subject) => {
                for (index, bound) in clause.bounds.iter().enumerate() {
                    let subject = self.ty(subject, &mut vec![8, index as u32], 0, false)?;
                    let value = self.requirement(subject, bound, &mut vec![9, index as u32], 0)?;
                    self.push(output, value, clause.span)?;
                }
            }
            s::BoundSubject::Lifetime(longer) => {
                let shorter = clause.lifetime_bound.as_ref().ok_or_else(|| {
                    error(
                        clause.span,
                        "outlives premise lacks its actual shorter lifetime",
                    )
                })?;
                let longer = self.life(Some(longer), &[], clause.span)?;
                let shorter = self.life(Some(shorter), &[], clause.span)?;
                self.push(
                    output,
                    SemanticRequirementRecipe::Outlives { longer, shorter },
                    clause.span,
                )?;
            }
        }
        Ok(())
    }
    fn result_mode(&mut self, function: &s::Function) -> R<PassingMode> {
        let mut result = PassingMode::Infer;
        for clause in &function.contract {
            self.budget.node(clause.span, 0)?;
            if clause.name != "result" {
                continue;
            }
            let [term] = clause.terms.as_slice() else {
                return Err(error(
                    clause.span,
                    "result mode requires one actual source term",
                ));
            };
            if !term.type_args.is_empty() || !term.arguments.is_empty() || term.index.is_some() {
                return Err(error(
                    clause.span,
                    "result mode has unexpected source operands",
                ));
            }
            let mode = match term.path.as_slice() {
                [name] if name == "create" => PassingMode::Create,
                [name] if name == "give" => PassingMode::Give,
                _ => {
                    return Err(unsupported(
                        clause.span,
                        "semantic result mode needs its original contract consumer",
                    ));
                }
            };
            if result != PassingMode::Infer {
                return Err(error(
                    clause.span,
                    "resource result modes are mutually exclusive",
                ));
            }
            result = mode;
        }
        Ok(result)
    }
}
