//! Return-domain transport is recorded at already checked original operations.
//! Whole-domain edges preserve every intrinsic index, including unknown T_N.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskResultSource {
    pub result_path: Vec<LifeTypeProjection>,
    pub parameter: u32,
    pub input_path: Vec<LifeTypeProjection>,
    /// None transports the complete child domain, k -> k. Some transports one
    /// independently proved class at these exact child boundaries.
    pub class: Option<u32>,
}
#[derive(Clone)]
pub(crate) struct TaskReturnSources {
    pub statement: NodeId,
    pub sources: Option<Vec<TaskResultSource>>,
}
#[derive(Clone)]
pub(super) struct ReturnValue {
    pub expression: NodeId,
    pub ty: SemanticTypeRecipe,
    pub sources: Option<Vec<TaskResultSource>>,
    pub injection: Option<bool>,
}
pub(super) fn source_snapshot(
    values: &[TaskResultSource],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    for value in values {
        let depth = value
            .result_path
            .len()
            .saturating_add(value.input_path.len());
        if depth > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original Task result source path exceeded",
                None,
            ));
        }
        *left = left.checked_sub(depth.saturating_add(4)).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original Task result source work exceeded",
                None,
            )
        })?;
    }
    Ok(())
}
impl Check<'_, '_> {
    fn copy_task_sources(
        &mut self,
        value: &Option<Vec<TaskResultSource>>,
    ) -> R<Option<Vec<TaskResultSource>>> {
        if let Some(values) = value {
            source_snapshot(values, self.limits, self.left)?;
        }
        Ok(value.clone())
    }
    fn append_task_sources(
        &mut self,
        output: &mut Vec<TaskResultSource>,
        values: Vec<TaskResultSource>,
        depth: usize,
    ) -> R<()> {
        for value in values {
            self.pay(output.len().saturating_add(1), depth)?;
            source_snapshot(output, self.limits, self.left)?;
            source_snapshot(std::slice::from_ref(&value), self.limits, self.left)?;
            if !output.contains(&value) {
                output.push(value);
            }
        }
        Ok(())
    }
    fn project_task_sources(
        &mut self,
        source: Option<Vec<TaskResultSource>>,
        path: &[LifeTypeProjection],
        depth: usize,
    ) -> R<Option<Vec<TaskResultSource>>> {
        self.pay(path.len().saturating_add(1), depth)?;
        let Some(source) = source else {
            return Ok(None);
        };
        source_snapshot(&source, self.limits, self.left)?;
        let mut result = Vec::new();
        for mut row in source {
            self.pay(
                path.len()
                    .saturating_add(row.result_path.len())
                    .saturating_add(row.input_path.len()),
                depth,
            )?;
            if path.starts_with(&row.result_path) && row.class.is_none() {
                row.input_path
                    .extend_from_slice(&path[row.result_path.len()..]);
                row.result_path.clear();
                result.push(row);
            } else if row.result_path.starts_with(path) {
                row.result_path.drain(..path.len());
                result.push(row);
            }
        }
        Ok(Some(result))
    }
    fn prefix_task_sources(
        &mut self,
        source: Option<Vec<TaskResultSource>>,
        prefix: &[LifeTypeProjection],
        depth: usize,
    ) -> R<Option<Vec<TaskResultSource>>> {
        let Some(mut source) = source else {
            return Ok(None);
        };
        for row in &mut source {
            self.pay(
                prefix
                    .len()
                    .saturating_add(row.result_path.len())
                    .saturating_add(1),
                depth,
            )?;
            let mut path = prefix.to_vec();
            path.extend_from_slice(&row.result_path);
            row.result_path = path;
        }
        Ok(Some(source))
    }
    /// Only immutable bindings are replayed here. Mutable/loop value inventory
    /// belongs to the existing exact-path control-flow pass, never one-pass OR.
    pub(super) fn task_value_sources(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<Vec<TaskResultSource>>> {
        self.pay(1, depth)?;
        let source = match &expression.kind {
            CoreExprKind::Name(name) => {
                let Some(binding) = variables.get(name) else {
                    return Ok(None);
                };
                if binding.origin_owner != owner || binding.mutable || binding.written {
                    return Ok(None);
                }
                match &binding.origin {
                    Query::Parameter(parameter) => {
                        self.pay(1, depth)?;
                        Some(vec![TaskResultSource {
                            result_path: Vec::new(),
                            parameter: *parameter,
                            input_path: Vec::new(),
                            class: None,
                        }])
                    }
                    Query::Binding(node) => {
                        let source = self.proof.task_binding_sources.get(&(owner, *node));
                        match source {
                            Some(source) => {
                                if let Some(source) = source {
                                    source_snapshot(source, self.limits, self.left)?;
                                }
                                source.clone()
                            }
                            None => None,
                        }
                    }
                    _ => None,
                }
            }
            CoreExprKind::Group(inner)
            | CoreExprKind::Ascribed { value: inner, .. }
            | CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand: inner,
            } => self.task_value_sources(owner, inner, variables, depth + 1)?,
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let source = self.task_value_sources(owner, operand, variables, depth + 1)?;
                self.project_task_sources(source, &[LifeTypeProjection::Pointee], depth + 1)?
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                let inner = self.task_value_sources(owner, operand, variables, depth + 1)?;
                let Some(mut source) =
                    self.prefix_task_sources(inner, &[LifeTypeProjection::Pointee], depth + 1)?
                else {
                    return Ok(None);
                };
                let Some(bound) = self
                    .proof
                    .view_bounds
                    .get(&(owner, Query::Expression(expression.id)))
                else {
                    return Ok(None);
                };
                let parameter = bound.parameter;
                let result = self
                    .proof
                    .injections
                    .get(&(owner, expression.id))
                    .map(|value| &value.payload)
                    .or_else(|| {
                        self.proof
                            .types
                            .get(&(owner, Query::Expression(expression.id)))
                    });
                let input = self.proof.types.get(&(owner, Query::Parameter(parameter)));
                let (
                    Some(SemanticTypeRecipe::Address {
                        lifetime: SemanticOuterLifeRecipe::Class(result),
                        ..
                    }),
                    Some(
                        SemanticTypeRecipe::Address {
                            lifetime: SemanticOuterLifeRecipe::Class(input),
                            ..
                        }
                        | SemanticTypeRecipe::Desc {
                            lifetime: SemanticOuterLifeRecipe::Class(input),
                            ..
                        },
                    ),
                ) = (result, input)
                else {
                    return Ok(None);
                };
                crate::dyn_metadata::semantic_operand_snapshot(result, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(input, self.limits, self.left, 1)?;
                let result = result.clone();
                let input = input.clone();
                if !self.same_life(&result, &input)? {
                    return Ok(None);
                }
                self.pay(1, depth)?;
                source.push(TaskResultSource {
                    result_path: Vec::new(),
                    parameter,
                    input_path: Vec::new(),
                    class: Some(0),
                });
                Some(source)
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                let mut sources = Vec::new();
                self.pay(fields.len(), depth)?;
                for (ordinal, field) in fields.iter().enumerate() {
                    let source = self.task_value_sources(owner, field, variables, depth + 1)?;
                    let path = if matches!(expression.kind, CoreExprKind::Tuple(_)) {
                        LifeTypeProjection::Place(PlaceProjection::Field(ordinal as u32))
                    } else {
                        LifeTypeProjection::Element
                    };
                    let Some(field) = self.prefix_task_sources(source, &[path], depth + 1)? else {
                        return Ok(None);
                    };
                    self.append_task_sources(&mut sources, field, depth)?;
                }
                Some(sources)
            }
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let Some(mut left) =
                    self.task_value_sources(owner, then_value, variables, depth + 1)?
                else {
                    return Ok(None);
                };
                let Some(right) =
                    self.task_value_sources(owner, else_value, variables, depth + 1)?
                else {
                    return Ok(None);
                };
                self.append_task_sources(&mut left, right, depth)?;
                Some(left)
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::Unit
            | CoreExprKind::String(_) => Some(Vec::new()),
            CoreExprKind::Call { arguments, .. } => {
                let Some(site) =
                    self.site(owner, SemanticOriginalNode::Expression(expression.id))?
                else {
                    return Ok(None);
                };
                let Some(target) = self.proof.call_targets.get(&(owner, site.id)).copied() else {
                    return Ok(None);
                };
                if !self.proof.complete.contains(&target) {
                    return Ok(None);
                }
                if self.definition(target)?.kind != SemanticDefinitionKind::Function {
                    return Ok(None);
                }
                let mut sources = Vec::new();
                self.pay(self.proof.task_returns.len(), depth)?;
                let returns: Vec<_> = self
                    .proof
                    .task_returns
                    .range((target, NodeId(0))..=(target, NodeId(u32::MAX)))
                    .map(|(_, value)| value)
                    .collect();
                for value in &returns {
                    if value.injection.is_some() {
                        return Ok(None);
                    }
                    if let Some(value) = &value.sources {
                        source_snapshot(value, self.limits, self.left)?;
                    }
                }
                let returns: Vec<_> = returns
                    .into_iter()
                    .map(|value| value.sources.clone())
                    .collect();
                if returns.is_empty() {
                    return Ok(None);
                }
                for returned in returns {
                    let Some(returned) = returned else {
                        return Ok(None);
                    };
                    for edge in returned {
                        let Some(argument) = arguments.get(edge.parameter as usize) else {
                            return Ok(None);
                        };
                        let argument =
                            self.task_value_sources(owner, argument, variables, depth + 1)?;
                        let selected =
                            self.project_task_sources(argument, &edge.input_path, depth + 1)?;
                        let Some(mut selected) =
                            self.prefix_task_sources(selected, &edge.result_path, depth + 1)?
                        else {
                            return Ok(None);
                        };
                        if let Some(class) = edge.class {
                            for source in &mut selected {
                                if source.class.is_some_and(|other| other != class) {
                                    return Ok(None);
                                }
                                source.class = Some(class);
                            }
                        }
                        self.append_task_sources(&mut sources, selected, depth)?;
                    }
                }
                Some(sources)
            }
            _ => None,
        };
        Ok(source)
    }
    pub(super) fn record_task_binding(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        initializer: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let source = self.task_value_sources(owner, initializer, variables, depth + 1)?;
        self.pay(1, depth)?;
        self.proof
            .proof_mut()?
            .task_binding_sources
            .insert((owner, statement), source);
        Ok(())
    }
    pub(super) fn record_task_return(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        expression: &SemanticOriginalExpr,
        ty: &SemanticTypeRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let mut sources = self.task_value_sources(owner, expression, variables, depth + 1)?;
        let injection = self
            .proof
            .injections
            .get(&(owner, expression.id))
            .map(|value| value.is_error);
        let ty = copy_recipe(ty, self.limits, self.left)?;
        self.pay(1, depth)?;
        self.proof.proof_mut()?.task_returns.insert(
            (owner, statement),
            ReturnValue {
                expression: expression.id,
                ty,
                sources,
                injection,
            },
        );
        Ok(())
    }
}
impl Proof {
    pub(super) fn task_result_sources(
        &self,
        program: &SemanticModule,
        worker: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Vec<TaskReturnSources>> {
        if !self.complete.contains(&worker) {
            return Err(bad(
                "Task return source has no complete original worker body",
            ));
        }
        *left = left
            .checked_sub(self.task_returns.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "Task original return lookup exceeded", None)
            })?;
        let mut result = Vec::new();
        for ((_, statement), returned) in self
            .task_returns
            .range((worker, NodeId(0))..=(worker, NodeId(u32::MAX)))
        {
            *left = left.checked_sub(2).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "Task original return row work exceeded", None)
            })?;
            let mut check =
                super::nominal_where::borrowed_check(program, self, worker, limits, left)?;
            let mut sources = check.copy_task_sources(&returned.sources)?;
            if let Some(is_error) = returned.injection {
                sources = check.prefix_task_sources(
                    sources,
                    &[LifeTypeProjection::Place(PlaceProjection::ErrorArm(
                        is_error,
                    ))],
                    0,
                )?;
                let SemanticTypeRecipe::ErrorUnion { error, result } = &returned.ty else {
                    return Err(bad(
                        "original Task return injection lost its complete union",
                    ));
                };
                let inactive = if is_error {
                    result.as_ref()
                } else {
                    error.as_ref()
                };
                if !check.original_zero_domain(worker, inactive, 0)? {
                    sources = None;
                }
            }
            if sources.is_none() && check.original_zero_domain(worker, &returned.ty, 0)? {
                sources = Some(Vec::new());
            }
            result.push(TaskReturnSources {
                statement: *statement,
                sources,
            });
        }
        Ok(result)
    }
}
