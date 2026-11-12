//! Definition-time proof for the bounded borrowed registration template.
//! No concrete environment is invented. The retained F supplies no phase fact.
use super::*;
use std::collections::BTreeMap;

const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const REQUEST: &str = "async_registration_request_close";
const POLL: &str = "async_registration_poll_close";

fn bit(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |v, s| v | bit(*s))
}
#[derive(Clone, Copy)]
enum Value {
    Root,
    OtherRoot,
    OtherPoll,
    Scalar,
    Poll { epoch: usize, pending: u8 },
}

struct Proof<'a, P> {
    function: &'a CoreFunction,
    contract: &'a FunctionProtocolContract,
    input: ProtocolState,
    phase: u8,
    epoch: usize,
    locals: BTreeMap<String, Value>,
    pay: P,
}
impl<P: FnMut(usize) -> R<()>> Proof<'_, P> {
    fn reject(&self, span: Span) -> Diagnostic {
        unsupported(
            span,
            "registration protocol definition proof supports straight-line primitive calls, local aliases and returns; other template bodies require a separately implemented symbolic proof",
        )
    }
    fn expr(&mut self, e: &CoreExpr) -> R<Value> {
        (self.pay)(1)?;
        match &e.kind {
            CoreExprKind::Name(name) => {
                (self.pay)(name.len().saturating_add(self.locals.len()))?;
                self.locals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.reject(e.span))
            }
            CoreExprKind::Unit | CoreExprKind::Bool(_) | CoreExprKind::Literal(_) => {
                Ok(Value::Scalar)
            }
            CoreExprKind::Call { callee, arguments } => {
                let CoreExprKind::Name(name) = &callee.kind else {
                    return Err(self.reject(e.span));
                };
                (self.pay)(name.len().saturating_add(arguments.len()))?;
                if !matches!(name.as_str(), REQUEST | POLL) || self.locals.contains_key(name) {
                    return Err(self.reject(e.span));
                }
                let [argument] = arguments.as_slice() else {
                    return Err(self.reject(e.span));
                };
                let CoreExprKind::Name(root) = &argument.kind else {
                    return Err(self.reject(argument.span));
                };
                (self.pay)(root.len().saturating_add(self.locals.len()))?;
                if matches!(self.locals.get(root), Some(Value::OtherRoot)) {
                    // This per-role definition check proves only the selected
                    // root. Returning another role's Poll remains unsupported.
                    if name == REQUEST {
                        return Ok(Value::Scalar);
                    }
                    return Ok(Value::OtherPoll);
                }
                if !matches!(self.locals.get(root), Some(Value::Root)) {
                    return Err(self.reject(argument.span));
                }
                self.epoch = self
                    .epoch
                    .checked_add(1)
                    .ok_or_else(|| self.reject(e.span))?;
                // These are the resolved, unshadowed sealed primitives: module
                // declarations are checked before this walk and locals above.
                self.phase = (if self.phase & (OPEN | CLOSING) != 0 {
                    CLOSING
                } else {
                    0
                }) | (self.phase & CLOSED);
                if name == REQUEST {
                    return Ok(Value::Scalar);
                }
                let pending = self.phase & CLOSING;
                self.phase = pending | CLOSED;
                Ok(Value::Poll {
                    epoch: self.epoch,
                    pending,
                })
            }
            _ => Err(self.reject(e.span)),
        }
    }
    fn exit(&mut self, exit: ProtocolExit, actual: u8, span: Span) -> R<()> {
        if actual == 0 {
            return Ok(());
        }
        (self.pay)(self.contract.exits.len())?;
        let Some(row) = self
            .contract
            .exits
            .iter()
            .find(|r| r.input == self.input && r.exit == exit)
        else {
            return Err(error(
                "E-PROTOCOL-DEFINITION",
                span,
                "reachable protocol result is missing from the declared upper bound",
            ));
        };
        if actual & !mask(&row.output) != 0 {
            return Err(error(
                "E-PROTOCOL-DEFINITION",
                span,
                "protocol definition exceeds its declared per-result phase upper bound",
            ));
        }
        Ok(())
    }
    fn run(&mut self) -> R<()> {
        for statement in &self.function.body.statements {
            (self.pay)(1)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    initializer: Some(value),
                    ..
                } => {
                    (self.pay)(
                        name.len()
                            .saturating_add(self.locals.len())
                            .saturating_add(1),
                    )?;
                    if matches!(name.as_str(), REQUEST | POLL) || self.locals.contains_key(name) {
                        return Err(self.reject(statement.span));
                    }
                    let value = self.expr(value)?;
                    self.locals.insert(name.clone(), value);
                }
                CoreStatementKind::Expression(e) => {
                    self.expr(e)?;
                }
                CoreStatementKind::Return(value) => {
                    let value = match value {
                        Some(e) => self.expr(e)?,
                        None => Value::Scalar,
                    };
                    match value {
                        Value::Poll { epoch, pending }
                            if epoch == self.epoch
                                && self.function.return_type == tasks::poll_type(&Type::Unit) =>
                        {
                            self.exit(ProtocolExit::PollPending, pending, statement.span)?;
                            self.exit(ProtocolExit::PollReady, CLOSED, statement.span)?;
                        }
                        Value::OtherPoll
                            if self.function.return_type == tasks::poll_type(&Type::Unit) =>
                        {
                            self.exit(ProtocolExit::PollPending, self.phase, statement.span)?;
                            self.exit(ProtocolExit::PollReady, self.phase, statement.span)?;
                        }
                        Value::Scalar
                            if !matches!(self.function.return_type, Type::ErrorUnion { .. })
                                && self.function.return_type != tasks::poll_type(&Type::Unit) =>
                        {
                            self.exit(ProtocolExit::Return, self.phase, statement.span)?;
                        }
                        _ => return Err(self.reject(statement.span)),
                    }
                    return Ok(());
                }
                _ => return Err(self.reject(statement.span)),
            }
        }
        if self.function.return_type == Type::Unit {
            self.exit(ProtocolExit::Return, self.phase, self.function.span)
        } else {
            Err(self.reject(self.function.span))
        }
    }
}

pub(crate) fn check(
    f: &CoreFunction,
    module: &CoreModule,
    imports: &BTreeMap<String, zil_base::ModuleId>,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<()> {
    if !borrowed_template(f) {
        return Ok(());
    }
    if f.protocol_contracts.is_empty() {
        return Err(unsupported(
            f.span,
            "borrowed protocol definition has no contract",
        ));
    }
    if f.protocol_contracts
        .iter()
        .any(|c| !f.callable_parameters.iter().any(|b| b.index == c.parameter))
    {
        return Err(unsupported(
            f.span,
            "protocol role is not an actual registration parameter",
        ));
    }
    for c in &f.protocol_contracts {
        pay(module
            .functions
            .len()
            .saturating_add(imports.len())
            .saturating_add(c.entry.len())
            .saturating_add(c.exits.len())
            .saturating_add(1))?;
        // Resolve bare primitive identity against the actual defining module. A
        // user declaration with this spelling is never a transition certificate.
        if imports.contains_key(REQUEST)
            || imports.contains_key(POLL)
            || module
                .functions
                .iter()
                .any(|function| matches!(function.name.as_str(), REQUEST | POLL))
        {
            return Err(error(
                "E-PROTOCOL-DEFINITION",
                f.span,
                "protocol primitive name is shadowed by an actual function declaration",
            ));
        }
        if c.entry.is_empty()
            || c.entry.windows(2).any(|w| w[0] >= w[1])
            || c.parameter as usize >= f.parameters.len()
            || c.exits
                .windows(2)
                .any(|w| (w[0].input, w[0].exit) >= (w[1].input, w[1].exit))
        {
            return Err(error(
                "E-PROTOCOL-DEFINITION",
                f.span,
                "invalid protocol definition relation",
            ));
        }
        for row in &c.exits {
            pay(row.output.len().saturating_add(1))?;
            let allowed = match &f.return_type {
                Type::ErrorUnion { .. } => {
                    matches!(row.exit, ProtocolExit::ErrorOk | ProtocolExit::ErrorErr)
                }
                ty if *ty == tasks::poll_type(&Type::Unit) => matches!(
                    row.exit,
                    ProtocolExit::PollPending | ProtocolExit::PollReady
                ),
                _ => row.exit == ProtocolExit::Return,
            };
            if !allowed
                || row.output.is_empty()
                || row.output.windows(2).any(|w| w[0] >= w[1])
                || !c.entry.contains(&row.input)
            {
                return Err(error(
                    "E-PROTOCOL-DEFINITION",
                    f.span,
                    "invalid protocol definition output relation",
                ));
            }
        }
        for input in &c.entry {
            let name = &f.parameters[c.parameter as usize].name;
            pay(name.len().saturating_add(1))?;
            if matches!(name.as_str(), REQUEST | POLL) {
                return Err(error(
                    "E-PROTOCOL-DEFINITION",
                    f.span,
                    "protocol root shadows a primitive",
                ));
            }
            let mut locals = BTreeMap::new();
            for role in &f.protocol_contracts {
                let other = &f.parameters[role.parameter as usize].name;
                pay(other.len().saturating_add(1))?;
                locals.insert(other.clone(), Value::OtherRoot);
            }
            locals.insert(name.clone(), Value::Root);
            Proof {
                function: f,
                contract: c,
                input: *input,
                phase: bit(*input),
                epoch: 0,
                locals,
                pay: &mut pay,
            }
            .run()?;
        }
    }
    Ok(())
}
