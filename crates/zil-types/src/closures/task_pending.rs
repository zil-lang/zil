//! Private logical Task constraints before the actual worker/captures exist.
//! No branch here constructs a retained context or a public pending type.
use super::*;
use std::rc::Rc;

pub(super) struct Request {
    pub call: zil_base::NodeId,
    pub worker: Signature,
    pub inputs: Vec<LocalType>,
    pub recover_inputs: bool,
    creation: std::cell::Cell<Option<Variable>>,
    values: Vec<Variable>,
}
#[derive(Clone)]
pub(super) enum Pending {
    Task {
        result: Variable,
        requests: Rc<std::cell::RefCell<Vec<Rc<Request>>>>,
    },
    Managed(Variable),
    Description(Variable),
    Union {
        error: Variable,
        result: Variable,
    },
    Failure {
        inputs: Variable,
        task: Variable,
    },
    Poll(Variable),
    Callable {
        index: usize,
        signature: Rc<Type>,
        mode: Option<bool>,
    },
}
impl Pending {
    pub(super) fn width(&self) -> usize {
        match self {
            Self::Task { .. } | Self::Poll(_) | Self::Managed(_) | Self::Description(_) => 1,
            Self::Union { .. } | Self::Failure { .. } => 2,
            Self::Callable { .. } => 0,
        }
    }
    pub(super) fn variables(&self) -> Vec<Variable> {
        match self {
            Self::Task { result, .. }
            | Self::Poll(result)
            | Self::Managed(result)
            | Self::Description(result) => vec![*result],
            Self::Union { error, result } => vec![*error, *result],
            Self::Failure { inputs, task } => vec![*inputs, *task],
            Self::Callable { .. } => Vec::new(),
        }
    }
}
impl Graph {
    fn pending_variable(&mut self, pending: Pending, span: Span) -> R<Variable> {
        self.spend(span)?;
        if self.nodes.len() >= self.node_limit {
            return Err(limit("Task constraint variable limit exceeded", span));
        }
        let variable = self.fresh(AGGREGATE, false);
        if matches!(pending, Pending::Task { .. }) {
            self.has_pending_tasks = true;
        }
        self.nodes[variable].shape = Some(Shape::Pending(pending));
        Ok(variable)
    }
    pub(super) fn pending(&self, variable: Variable, span: Span) -> R<Option<Pending>> {
        self.spend(span)?;
        Ok(match &self.nodes[self.root(variable)].shape {
            Some(Shape::Pending(value)) => Some(value.clone()),
            _ => None,
        })
    }
    pub(super) fn pending_bind(&mut self, pending: Pending, ty: &Type, span: Span) -> R<()> {
        let physical = stored::physical(ty);
        match (pending, physical) {
            (Pending::Callable { mode: None, .. }, Type::Closure(_)) => Ok(()),
            (
                Pending::Callable {
                    mode: Some(available),
                    ..
                },
                Type::Address { mutable, inner },
            ) if (available || !mutable) && matches!(inner.as_ref(), Type::Closure(_)) => Ok(()),
            (
                Pending::Task { result, .. },
                Type::Task {
                    result: expected, ..
                },
            ) => self.bind(result, expected, span),
            (Pending::Description(payload), Type::Desc(expected)) => {
                self.bind(payload, expected, span)
            }
            (Pending::Managed(payload), Type::Managed(expected)) => {
                self.bind(payload, expected, span)
            }
            (
                Pending::Union { error, result },
                Type::ErrorUnion {
                    error: a,
                    result: b,
                },
            ) => {
                self.bind(error, a, span)?;
                self.bind(result, b, span)
            }
            (
                Pending::Failure { inputs, .. },
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) if matches!(fields.as_slice(), [(reason,_),(input,_)] if reason=="reason" && input=="inputs") => {
                self.bind(inputs, &fields[1].1, span)
            }
            (Pending::Poll(result), Type::Enum { variants, .. }) if matches!(variants.as_slice(), [(pending,p),(ready,r)] if pending=="Pending" && p.is_empty() && ready=="Ready" && r.len()==1) => {
                self.bind(result, &variants[1].1[0], span)
            }
            _ => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "Task logical operation differs from its original type constraint",
            )),
        }
    }
    pub(super) fn pending_unify(
        &mut self,
        a: &Pending,
        b: &Pending,
        span: Span,
        depth: usize,
    ) -> R<()> {
        match (a, b) {
            (
                Pending::Callable {
                    index: a,
                    signature: left,
                    mode: am,
                },
                Pending::Callable {
                    index: b,
                    signature: right,
                    mode: bm,
                },
            ) if a == b && am == bm => {
                self.reserve_type(left, 0, span)?;
                self.reserve_type(right, 0, span)?;
                if left != right {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        span,
                        "Task callable requests have different complete signatures",
                    ));
                }
                Ok(())
            }
            (
                Pending::Task {
                    result: a,
                    requests: left,
                },
                Pending::Task {
                    result: b,
                    requests: right,
                },
            ) => {
                self.unify_depth(*a, *b, span, depth + 1)?;
                if !Rc::ptr_eq(left, right) {
                    self.spend_count(
                        left.borrow().len().saturating_add(right.borrow().len()),
                        span,
                    )?;
                    let additions = right.borrow().clone();
                    left.borrow_mut().extend(additions);
                }
                Ok(())
            }
            (Pending::Managed(a), Pending::Managed(b))
            | (Pending::Description(a), Pending::Description(b))
            | (Pending::Poll(a), Pending::Poll(b)) => self.unify_depth(*a, *b, span, depth + 1),
            (
                Pending::Union {
                    error: a,
                    result: b,
                },
                Pending::Union {
                    error: c,
                    result: d,
                },
            )
            | (Pending::Failure { inputs: a, task: b }, Pending::Failure { inputs: c, task: d }) => {
                self.unify_depth(*a, *c, span, depth + 1)?;
                self.unify_depth(*b, *d, span, depth + 1)
            }
            _ => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "different private Task operation shapes",
            )),
        }
    }
    pub(super) fn pending_union(
        &mut self,
        error: Variable,
        result: Variable,
        span: Span,
    ) -> R<Variable> {
        self.pending_variable(Pending::Union { error, result }, span)
    }
    pub(super) fn pending_managed(&mut self, payload: Variable, span: Span) -> R<Variable> {
        self.pending_variable(Pending::Managed(payload), span)
    }
    pub(super) fn pending_arm(
        &self,
        union: Variable,
        is_error: bool,
        span: Span,
    ) -> R<Option<Variable>> {
        Ok(match self.pending(union, span)? {
            Some(Pending::Union { error, result }) => Some(if is_error { error } else { result }),
            _ => None,
        })
    }
    pub(super) fn pending_project(
        &mut self,
        base: Variable,
        projection: &Projection,
        span: Span,
    ) -> R<Option<Variable>> {
        let Some(Pending::Failure { inputs, .. }) = self.pending(base, span)? else {
            return Ok(None);
        };
        match projection {
            Projection::Field(name) if name == "inputs" => Ok(Some(inputs)),
            Projection::Field(name) if name == "reason" => {
                let value = self.fresh(AGGREGATE, false);
                self.bind(value, &tasks::create_error(), span)?;
                Ok(Some(value))
            }
            _ => Err(error(
                "E-UNKNOWN-MEMBER",
                span,
                "Task failure has no such logical field",
            )),
        }
    }
}
impl Graph {
    fn complete_task_constraint(&mut self, variable: Variable, actual: &Type, span: Span) -> R<()> {
        self.spend(span)?;
        let root = self.root(variable);
        let pending = self.pending(variable, span)?;
        if let Some(previous) = self.clone_type(&self.nodes[root].concrete, span)? {
            if previous != *actual {
                let task = task_factories::result_task(actual).ok_or_else(|| {
                    error(
                        "E-TASK-INFERENCE",
                        span,
                        "actual completion changes a fixed non-Task result",
                    )
                })?;
                let mut complete = previous;
                self.reserve_type(task, 0, span)?;
                task_factories::replace_task(&mut complete, task, span)?;
                if complete != *actual {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        span,
                        "actual completion changes an original Task result/wrapper constraint",
                    ));
                }
                self.nodes[root].concrete = None;
            }
        }
        if let Some(pending) = pending {
            match (pending, stored::physical(actual)) {
                (Pending::Callable { .. }, _) => {}
                (Pending::Task { result, .. }, Type::Task { result: actual, .. }) => {
                    self.complete_task_constraint(result, actual, span)?
                }
                (Pending::Description(payload), Type::Desc(actual)) => {
                    self.complete_task_constraint(payload, actual, span)?
                }
                (Pending::Managed(payload), Type::Managed(actual)) => {
                    self.complete_task_constraint(payload, actual, span)?
                }
                (
                    Pending::Union { error, result },
                    Type::ErrorUnion {
                        error: a,
                        result: b,
                    },
                ) => {
                    self.complete_task_constraint(error, a, span)?;
                    self.complete_task_constraint(result, b, span)?;
                }
                (
                    Pending::Failure { inputs, .. },
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                ) if fields.len() == 2 => {
                    self.complete_task_constraint(inputs, &fields[1].1, span)?
                }
                (Pending::Poll(result), Type::Enum { variants, .. })
                    if variants.len() == 2 && variants[1].1.len() == 1 =>
                {
                    self.complete_task_constraint(result, &variants[1].1[0], span)?
                }
                _ => {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        span,
                        "actual completion changes a private Task operation role",
                    ));
                }
            }
        }
        self.bind(variable, actual, span)
    }
    fn complete_actual_tasks(
        &mut self,
        function: FunctionRef<'_>,
        ledger: &crate::semantics::emission::Ledger,
        closures: &[&super::Pending],
        result: Variable,
    ) -> R<()> {
        let mut requests = BTreeMap::new();
        self.spend_count(self.nodes.len(), function.span)?;
        for node in &self.nodes {
            if let Some(Shape::Pending(Pending::Task {
                requests: group, ..
            })) = &node.shape
            {
                self.spend_count(group.borrow().len(), function.span)?;
                for request in group.borrow().iter() {
                    if let Some(previous) = requests.get(&request.call) {
                        if !Rc::ptr_eq(previous, request) {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "one original Task request was inferred with multiple distinct identities",
                            ));
                        }
                    } else {
                        requests.insert(request.call, request.clone());
                    }
                }
            }
        }
        if requests.is_empty() {
            return Ok(());
        }
        for request in requests.values() {
            self.spend_count(
                request
                    .inputs
                    .len()
                    .saturating_add(ledger.frames.len().saturating_add(1).ilog2() as usize + 1),
                function.span,
            )?;
            if request.inputs.len() != request.worker.parameters.len() {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "private Task request lost its complete input arity",
                ));
            }
            let Some(frame) = ledger.frames.get(&function.id) else {
                if ledger.has_original_context() {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "Task request has no real emitter frame",
                    ));
                }
                // The raw lane still checks its real body with the ordinary
                // builder; absence of a Source ledger proves no empty domain.
                continue;
            };
            self.spend_count(
                frame
                    .expressions
                    .len()
                    .saturating_add(function.blocks.len()),
                function.span,
            )?;
            let expression = frame.expressions.get(&request.call).ok_or_else(|| {
                error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "private Task request was not actually emitted",
                )
            })?;
            let mut selected = None;
            for point in &expression.instructions {
                self.spend_count(function.blocks.len().saturating_add(1), function.span)?;
                let instruction = function
                    .blocks
                    .iter()
                    .find(|block| block.id == point.block)
                    .and_then(|block| block.instructions.get(point.instruction))
                    .ok_or_else(|| {
                        error(
                            "E-TASK-INFERENCE",
                            function.span,
                            "Task request leaves its real instruction boundary",
                        )
                    })?;
                if instruction.result != expression.value {
                    continue;
                }
                let (worker, arguments, recover) = match &instruction.kind {
                    InstructionKind::TaskCreate {
                        worker, arguments, ..
                    } => (*worker, arguments, false),
                    InstructionKind::TaskCreateOwned {
                        worker, arguments, ..
                    } => (*worker, arguments, true),
                    _ => continue,
                };
                if selected
                    .replace((worker, arguments, instruction.result))
                    .is_some()
                {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "one Task request selects multiple actual creation events",
                    ));
                }
                if recover != request.recover_inputs {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "Task request changes its actual failure-input role",
                    ));
                }
            }
            let (worker, arguments, value) = selected.ok_or_else(|| {
                error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "Task request has no matching actual creation result",
                )
            })?;
            if arguments.len() != request.inputs.len() {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "Task request changes its actual input arity",
                ));
            }
            if let Some((module, creator)) = request.worker.template {
                let actual = ledger.frames.get(&worker).ok_or_else(|| {
                    error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "specialized Task worker lacks its real origin",
                    )
                })?;
                if actual.origin.module != module
                    || actual.origin.creator != creator
                    || actual.origin.closure.is_some()
                {
                    return Err(error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "Task request changes its real generic worker origin",
                    ));
                }
            } else if request.worker.id != Some(worker) {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "Task request changes its actual ordinary worker target",
                ));
            }
            let actual = value
                .and_then(|id| function.values.get(id.0 as usize))
                .ok_or_else(|| {
                    error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "actual Task creation has no full result type",
                    )
                })?;
            let Type::ErrorUnion { result: owner, .. } = actual else {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "actual Task creation lost its error/owner result",
                ));
            };
            let Some(Type::Task {
                result: payload, ..
            }) = tasks::payload(owner)
            else {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "actual Task creation has no whole Task payload",
                ));
            };
            self.reserve_type(payload, 0, function.span)?;
            self.reserve_type(&request.worker.result, 0, function.span)?;
            if **payload != request.worker.result {
                return Err(error(
                    "E-TASK-INFERENCE",
                    function.span,
                    "actual Task creation changes its original result type constraint",
                ));
            }
            for (ordinal, ((term, argument), constraint)) in request
                .inputs
                .iter()
                .zip(arguments)
                .zip(&request.values)
                .enumerate()
            {
                let ty = function.values.get(argument.0 as usize).ok_or_else(|| {
                    error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "Task input has no full actual value type",
                    )
                })?;
                match (term, ty) {
                    (LocalType::Scalar(variable), _) => {
                        self.complete_task_constraint(*variable, ty, function.span)?
                    }
                    (LocalType::Resource(variable, true), Type::Managed(payload))
                    | (LocalType::Resource(variable, false), Type::Desc(payload)) => {
                        self.complete_task_constraint(*variable, payload, function.span)?
                    }
                    (LocalType::Address(variable, available), Type::Address { inner, mutable })
                        if *available || !*mutable =>
                    {
                        self.complete_task_constraint(*variable, inner, function.span)?
                    }
                    (LocalType::Closure(expected, None, _), Type::Closure(actual))
                        if expected == actual => {}
                    (
                        LocalType::Closure(expected, Some(available), _),
                        Type::Address { mutable, inner },
                    ) if (*available || !*mutable)
                        && matches!(inner.as_ref(),Type::Closure(actual) if expected==actual) => {}
                    (LocalType::Nested(index, mode), _) => {
                        let actual = match (mode, ty) {
                            (None, Type::Closure(id)) => *id,
                            (Some(available), Type::Address { mutable, inner })
                                if *available || !*mutable =>
                            {
                                match inner.as_ref() {
                                    Type::Closure(id) => *id,
                                    _ => {
                                        return Err(error(
                                            "E-TASK-INFERENCE",
                                            function.span,
                                            "Task callable borrow has no actual environment",
                                        ));
                                    }
                                }
                            }
                            _ => {
                                return Err(error(
                                    "E-TASK-INFERENCE",
                                    function.span,
                                    "Task callable changes its actual passing mode",
                                ));
                            }
                        };
                        let Some(Pending::Callable {
                            index: expected,
                            signature,
                            ..
                        }) = self.pending(*constraint, function.span)?
                        else {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable lost its original logical signature",
                            ));
                        };
                        if expected != *index {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable changes its private signature occurrence",
                            ));
                        }
                        self.spend_count(closures.len(), function.span)?;
                        let mut entries = closures.iter().filter(|entry| entry.id == actual);
                        let entry = entries.next().ok_or_else(|| {
                            error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable has no real completed entry",
                            )
                        })?;
                        if entries.next().is_some() {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable entry identity is repeated",
                            ));
                        }
                        let function = entry.typed.as_ref().ok_or_else(|| {
                            error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable body is not yet complete",
                            )
                        })?;
                        let Type::FunctionPointer { parameters, result } = signature.as_ref()
                        else {
                            unreachable!("private callable signature")
                        };
                        if parameters.len() != function.parameters.len() {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable changes its full parameter arity",
                            ));
                        }
                        for (expected, local) in parameters.iter().zip(&function.parameters) {
                            let actual = &function
                                .locals
                                .get(local.0 as usize)
                                .ok_or_else(|| {
                                    error(
                                        "E-TASK-INFERENCE",
                                        function.span,
                                        "Task callable parameter is absent",
                                    )
                                })?
                                .ty;
                            self.reserve_type(expected, 0, function.span)?;
                            self.reserve_type(actual, 0, function.span)?;
                            if expected != actual {
                                return Err(error(
                                    "E-TASK-INFERENCE",
                                    function.span,
                                    "Task callable changes its actual complete parameter type",
                                ));
                            }
                        }
                        self.reserve_type(result, 0, function.span)?;
                        self.reserve_type(&function.return_type, 0, function.span)?;
                        if result.as_ref() != &function.return_type {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable changes its actual complete result",
                            ));
                        }
                        if let Some(bound) = request
                            .worker
                            .callable
                            .iter()
                            .find(|bound| bound.index as usize == ordinal)
                            && !helpers::compatible(entry.callable_kind(), bound.kind)
                        {
                            return Err(error(
                                "E-TASK-INFERENCE",
                                function.span,
                                "Task callable changes its original call mode",
                            ));
                        }
                        self.complete_task_constraint(*constraint, ty, function.span)?;
                    }
                    _ => {
                        return Err(error(
                            "E-TASK-INFERENCE",
                            function.span,
                            "actual Task input changes its private logical constraint",
                        ));
                    }
                }
            }
            self.complete_task_constraint(
                request.creation.get().ok_or_else(|| {
                    error(
                        "E-TASK-INFERENCE",
                        function.span,
                        "Task request lost its own creation constraint",
                    )
                })?,
                actual,
                function.span,
            )?;
        }
        self.complete_task_constraint(result, function.result()?, function.span)
    }
}
impl Inference {
    pub(crate) fn complete_actual_tasks(
        &mut self,
        function: FunctionRef<'_>,
        ledger: &crate::semantics::emission::Ledger,
        closures: &[&super::Pending],
        left: &mut usize,
    ) -> R<()> {
        if !self.graph.has_pending_tasks {
            return Ok(());
        }
        let result = self.result;
        self.graph.thread_work(left, |graph| {
            graph.complete_actual_tasks(function, ledger, closures, result)
        })
    }
}
impl InferBuilder<'_, '_> {
    fn pending_value(&mut self, term: LocalType, span: Span) -> R<Variable> {
        match term {
            LocalType::Scalar(value) => Ok(value),
            LocalType::Resource(value, true) => self.graph.pending_managed(value, span),
            LocalType::Resource(value, false) => self
                .graph
                .pending_variable(Pending::Description(value), span),
            LocalType::Address(value, mutable) => {
                let result = self.graph.fresh(AGGREGATE, false);
                self.graph.address(result, value, Some(mutable), span)?;
                Ok(result)
            }
            LocalType::Nested(index, mode) => {
                let signature = self.nested_signature(index, span)?;
                self.graph.reserve_type(&signature, 0, span)?;
                self.graph.pending_variable(
                    Pending::Callable {
                        index,
                        signature: Rc::new(signature),
                        mode,
                    },
                    span,
                )
            }
            LocalType::Closure(id, Some(mutable), _) => {
                let inner = self.graph.fresh(AGGREGATE, false);
                self.graph.bind(inner, &Type::Closure(id), span)?;
                let result = self.graph.fresh(AGGREGATE, false);
                self.graph.address(result, inner, Some(mutable), span)?;
                Ok(result)
            }
            LocalType::Closure(id, None, _) => {
                let result = self.graph.fresh(AGGREGATE, false);
                self.graph.bind(result, &Type::Closure(id), span)?;
                Ok(result)
            }
            _ => Err(unsupported(
                span,
                "Task input retains an unresolved private callable constraint",
            )),
        }
    }
    pub(super) fn pending_create(
        &mut self,
        call: zil_base::NodeId,
        signature: Signature,
        inputs: Vec<LocalType>,
        recover_inputs: bool,
        span: Span,
    ) -> R<LocalType> {
        self.graph
            .spend_count(inputs.len().saturating_add(4), span)?;
        let result = self.graph.fresh(VALUE, false);
        self.graph.bind(result, &signature.result, span)?;
        let mut fields = Vec::new();
        for input in &inputs {
            fields.push(self.pending_value(*input, span)?)
        }
        self.graph.spend_count(fields.len(), span)?;
        let values = fields.clone();
        let tuple = if fields.is_empty() {
            self.graph.fresh(UNIT, false)
        } else {
            let tuple = self.graph.fresh(AGGREGATE, false);
            self.graph.tuple(tuple, fields, span)?;
            tuple
        };
        let request = Rc::new(Request {
            call,
            worker: signature,
            inputs,
            recover_inputs,
            creation: std::cell::Cell::new(None),
            values,
        });
        let task = self.graph.pending_variable(
            Pending::Task {
                result,
                requests: Rc::new(std::cell::RefCell::new(vec![request.clone()])),
            },
            span,
        )?;
        let owner = self.graph.pending_managed(task, span)?;
        let failure = if recover_inputs {
            self.graph.pending_variable(
                Pending::Failure {
                    inputs: tuple,
                    task,
                },
                span,
            )?
        } else {
            let failure = self.graph.fresh(AGGREGATE, false);
            self.graph.bind(failure, &tasks::create_error(), span)?;
            failure
        };
        let union = self.graph.pending_variable(
            Pending::Union {
                error: failure,
                result: owner,
            },
            span,
        )?;
        request.creation.set(Some(union));
        Ok(LocalType::Scalar(union))
    }
    pub(super) fn pending_operation(
        &mut self,
        name: &str,
        term: LocalType,
        span: Span,
    ) -> R<Option<LocalType>> {
        let available = match term {
            LocalType::Address(_, mutable) => Some(mutable),
            _ => None,
        };
        let (value, owned) = match term {
            LocalType::Resource(v, owned) => (v, owned),
            LocalType::Address(v, _) | LocalType::Scalar(v) => (v, false),
            _ => return Ok(None),
        };
        let value = match self.graph.pending(value, span)? {
            Some(Pending::Managed(payload)) => payload,
            _ => value,
        };
        let Some(Pending::Task { result, .. }) = self.graph.pending(value, span)? else {
            return Ok(None);
        };
        if matches!(
            name,
            "task_poll" | "task_poll_close" | "task_request_cancel"
        ) && available != Some(true)
        {
            return Err(error(
                "E-TASK-OPERAND",
                span,
                "Task operation requires its actual exclusive head borrow",
            ));
        }
        let logical = match name {
            "task_poll" => {
                let poll = self.graph.pending_variable(Pending::Poll(result), span)?;
                let error = self.graph.fresh(AGGREGATE, false);
                self.graph.bind(error, &tasks::poll_state(), span)?;
                LocalType::Scalar(self.graph.pending_variable(
                    Pending::Union {
                        error,
                        result: poll,
                    },
                    span,
                )?)
            }
            "task_poll_close" => return self.known(&tasks::poll_type(&Type::Unit), span).map(Some),
            "task_request_cancel" => return self.known(&Type::Unit, span).map(Some),
            "task_finish_destroy" if owned => return self.known(&Type::Unit, span).map(Some),
            "task_valid" => return self.known(&Type::Bool, span).map(Some),
            _ => {
                return Err(error(
                    "E-TASK-OPERAND",
                    span,
                    "Task operation does not match its private owner/borrow constraint",
                ));
            }
        };
        Ok(Some(logical))
    }
}

impl Inference {
    pub(super) fn draft_error(&self, span: Span) -> R<Option<Type>> {
        let Some(Pending::Union { error, .. }) = self.graph.pending(self.result, span)? else {
            return Ok(None);
        };
        self.graph.ty(error)
    }
    /// A genuine Return value closes its logical result edge. Error-only
    /// returns leave the successful Task request private until it is emitted.
    pub(super) fn emitted_return(
        &mut self,
        actual: &Type,
        span: Span,
        left: &mut usize,
    ) -> R<Option<Type>> {
        let output = self.result;
        self.graph.thread_work(left, |graph| {
            if let Some(Pending::Union { error, result }) = graph.pending(output, span)? {
                if !matches!(actual, Type::ErrorUnion { .. }) {
                    if graph.ty(error)?.as_ref() == Some(actual) {
                        return Ok(None);
                    }
                    graph.complete_task_constraint(result, actual, span)?;
                    graph.solve()?;
                    return graph.ty(output);
                }
            }
            graph.complete_task_constraint(output, actual, span)?;
            graph.solve()?;
            graph.ty(output)
        })
    }
}
impl Inference {
    pub(super) fn emitted_error(&mut self, actual: &Type, span: Span, left: &mut usize) -> R<()> {
        let result = self.result;
        self.graph.thread_work(left, |graph| {
            let Some(Pending::Union { error, .. }) = graph.pending(result, span)? else {
                return Err(error(
                    "E-TRY-RETURN-TYPE",
                    span,
                    "? requires an enclosing error-union result",
                ));
            };
            graph.complete_task_constraint(error, actual, span)?;
            graph.solve()
        })
    }
}
