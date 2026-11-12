//! Lexical receiver-name rules do not depend on resource or aggregate layout.
use super::*;

pub(super) fn check(module: &CoreModule, limits: &Limits) -> R<()> {
    check_modules(std::iter::once(module), limits)
}
pub(super) fn check_modules<'a>(
    modules: impl IntoIterator<Item = &'a CoreModule>,
    limits: &Limits,
) -> R<()> {
    let mut checker = Names { limits, visited: 0 };
    for module in modules {
        for f in &module.functions {
            if primitive_name(&f.name) {
                return Err(primitive_reserved(&f.name, f.span));
            }
            for (index, p) in f.parameters.iter().enumerate() {
                if primitive_name(&p.name) {
                    return Err(primitive_reserved(&p.name, p.span));
                }
                if p.name == "self" && (index != 0 || f.receiver.is_none() || !f.name.contains('.'))
                {
                    return Err(reserved(p.span));
                }
            }
            checker.block(&f.body, f.receiver.is_some(), 0)?;
        }
    }
    Ok(())
}

fn reserved(span: Span) -> Diagnostic {
    error(
        "E-RECEIVER-RESERVED-NAME",
        span,
        "self is reserved for the method receiver and cannot be rebound",
    )
}
fn primitive_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u64"
            | "usize"
            | "f32"
            | "f64"
            | "i32"
            | "i64"
            | "c_null"
            | "c_wstr_null"
            | "c_wstr"
            | "c_callback"
            | "Store"
            | "StoreElement"
            | "Value"
            | "SyncDrop"
            | "store_new"
            | "store_reserve"
            | "store_push"
            | "store_pop"
            | "store_len"
            | "store_capacity"
            | "store_at"
            | "store_at_mut"
            | "error_ok"
            | "error_err"
            | "ByteBuffer"
            | "bytes_new"
            | "bytes_reserve"
            | "bytes_truncate"
            | "bytes_push"
            | "bytes_len"
            | "bytes_capacity"
            | "bytes_view"
            | "bytes_view_mut"
    )
}
fn primitive_reserved(name: &str, span: Span) -> Diagnostic {
    if matches!(name, "error_ok" | "error_err") {
        return error(
            "E-ERROR-CONSTRUCTOR-NAME",
            span,
            "bare error-union constructors cannot be rebound",
        );
    }
    if matches!(name, "Store" | "StoreElement") || name.starts_with("store_") {
        return error(
            "E-OWNED-STORE-NAME",
            span,
            "Store and bare Store primitives cannot be rebound",
        );
    }
    if name == "ByteBuffer" || name.starts_with("bytes_") {
        return error(
            "E-BYTE-BUFFER-NAME",
            span,
            "ByteBuffer and bare byte storage primitives cannot be rebound",
        );
    }
    if name.starts_with("c_") {
        return error(
            "E-FFI-CONSTRUCTOR-NAME",
            span,
            "bare static foreign constructor names cannot be rebound",
        );
    }
    error(
        "E-INTEGER-CONVERSION-NAME",
        span,
        "u8, u16, i32 and i64 are reserved bare integer conversion names",
    )
}
struct Names<'a> {
    limits: &'a Limits,
    visited: usize,
}
impl Names<'_> {
    fn charge(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth || self.visited >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "name resolution traversal budget exceeded",
                Some(span),
            ));
        }
        self.visited += 1;
        Ok(())
    }
    fn block(&mut self, b: &CoreBlock, receiver: bool, depth: usize) -> R<()> {
        self.charge(depth, b.span)?;
        for s in &b.statements {
            self.charge(depth + 1, s.span)?;
            match &s.kind {
                CoreStatementKind::TupleBinding {
                    pattern,
                    initializer,
                    ..
                } => {
                    self.expr(initializer, receiver, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                }
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    if primitive_name(name) {
                        return Err(primitive_reserved(name, s.span));
                    }
                    if name == "self" {
                        return Err(reserved(s.span));
                    }
                    if let Some(e) = initializer {
                        self.expr(e, receiver, depth + 1)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, receiver, depth + 1)?;
                    self.expr(value, receiver, depth + 1)?;
                }
                CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                    self.expr(e, receiver, depth + 1)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, receiver, depth + 1)?;
                    self.block(then_body, receiver, depth + 1)?;
                    if let Some(b) = else_body {
                        self.block(b, receiver, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, receiver, depth + 1)?;
                    self.block(body, receiver, depth + 1)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.expr(value, receiver, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                    self.block(then_body, receiver, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, receiver, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    self.expr(value, receiver, depth + 1)?;
                    self.pattern(pattern, depth + 1)?;
                    self.block(body, receiver, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    if primitive_name(name) {
                        return Err(primitive_reserved(name, s.span));
                    }
                    if name == "self" {
                        return Err(reserved(s.span));
                    }
                    self.expr(start, receiver, depth + 1)?;
                    self.expr(end, receiver, depth + 1)?;
                    self.block(body, receiver, depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, receiver, depth + 1)?;
                    for arm in arms {
                        self.pattern(&arm.pattern, depth + 1)?;
                        self.block(&arm.body, receiver, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(b)
                | CoreStatementKind::Unsafe(b)
                | CoreStatementKind::Defer(b) => self.block(b, receiver, depth + 1)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn pattern(&mut self, p: &CorePattern, depth: usize) -> R<()> {
        self.charge(depth, p.span)?;
        match &p.kind {
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => self.pattern(pattern, depth + 1)?,
            CorePatternKind::Name(name) if primitive_name(name) => {
                return Err(primitive_reserved(name, p.span));
            }
            CorePatternKind::Name(name) if name == "self" => return Err(reserved(p.span)),
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Record { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn expr(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        self.charge(depth, e.span)?;
        match &e.kind {
            CoreExprKind::ReceiverMember(_) if !receiver => Err(error(
                "E-RECEIVER-NONE",
                e.span,
                "leading-dot member requires a lexical method receiver",
            )),
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand)
            | CoreExprKind::IntegerConvert { value: operand, .. }
            | CoreExprKind::NumericConvert { value: operand, .. }
            | CoreExprKind::Ascribed { value: operand, .. } => {
                self.expr(operand, receiver, depth + 1)
            }
            CoreExprKind::Binary { left, right, .. } => {
                self.expr_pair(left, right, receiver, depth + 1)
            }
            CoreExprKind::Call { .. } => self.expr_call(e, receiver, depth),
            CoreExprKind::Index { object, index } => {
                self.expr_pair(object, index, receiver, depth + 1)
            }
            CoreExprKind::Slice { .. } => self.expr_slice(e, receiver, depth),
            CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                self.expr_list(items, receiver, depth + 1)
            }
            CoreExprKind::Member { object, .. } => self.expr(object, receiver, depth + 1),
            CoreExprKind::Record { fields, .. } => self.expr_fields(fields, receiver, depth + 1),
            CoreExprKind::RecordUpdate { .. } => self.expr_record_update(e, receiver, depth),
            CoreExprKind::Sequence { .. } => self.expr_sequence(e, receiver, depth),
            CoreExprKind::Closure { .. } => self.expr_closure(e, receiver, depth),
            CoreExprKind::If { .. } => self.expr_if(e, receiver, depth),
            _ => Ok(()),
        }
    }

    #[inline(never)]
    fn expr_pair(
        &mut self,
        first: &CoreExpr,
        second: &CoreExpr,
        receiver: bool,
        depth: usize,
    ) -> R<()> {
        self.expr(first, receiver, depth)?;
        self.expr(second, receiver, depth)
    }

    #[inline(never)]
    fn expr_list(&mut self, items: &[CoreExpr], receiver: bool, depth: usize) -> R<()> {
        for item in items {
            self.expr(item, receiver, depth)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_fields(
        &mut self,
        fields: &[(String, CoreExpr)],
        receiver: bool,
        depth: usize,
    ) -> R<()> {
        for (_, value) in fields {
            self.expr(value, receiver, depth)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_call(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::Call { callee, arguments } = &e.kind else {
            unreachable!("name call dispatcher")
        };
        self.expr(callee, receiver, depth + 1)?;
        self.expr_list(arguments, receiver, depth + 1)
    }

    #[inline(never)]
    fn expr_slice(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::Slice { object, start, end } = &e.kind else {
            unreachable!("name slice dispatcher")
        };
        self.expr(object, receiver, depth + 1)?;
        for endpoint in [start, end].into_iter().flatten() {
            self.expr(endpoint, receiver, depth + 1)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn expr_record_update(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::RecordUpdate { base, fields, .. } = &e.kind else {
            unreachable!("name record dispatcher")
        };
        self.expr(base, receiver, depth + 1)?;
        self.expr_fields(fields, receiver, depth + 1)
    }

    #[inline(never)]
    fn expr_sequence(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::Sequence { bindings, value } = &e.kind else {
            unreachable!("name sequence dispatcher")
        };
        for (name, e) in bindings {
            if primitive_name(name) {
                return Err(primitive_reserved(name, e.span));
            }
            self.expr(e, receiver, depth + 1)?;
        }
        self.expr(value, receiver, depth + 1)
    }

    #[inline(never)]
    fn expr_closure(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::Closure {
            parameters, body, ..
        } = &e.kind
        else {
            unreachable!("name closure dispatcher")
        };
        if let Some(p) = parameters.iter().find(|p| primitive_name(&p.name)) {
            return Err(primitive_reserved(&p.name, p.span));
        }
        if let Some(p) = parameters.iter().find(|p| p.name == "self") {
            return Err(reserved(p.span));
        }
        self.block(body, receiver, depth + 1)
    }

    #[inline(never)]
    fn expr_if(&mut self, e: &CoreExpr, receiver: bool, depth: usize) -> R<()> {
        let CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } = &e.kind
        else {
            unreachable!("name if dispatcher")
        };
        self.expr(condition, receiver, depth + 1)?;
        self.expr(then_value, receiver, depth + 1)?;
        self.expr(else_value, receiver, depth + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, NodeId};
    fn span() -> Span {
        Span::new(FileId(0), 0, 5)
    }
    fn module(kind: CoreStatementKind) -> CoreModule {
        CoreModule {
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            closure_life_contexts: Vec::new(),
            aggregate_templates: Vec::new(),
            aggregate_applications: Vec::new(),
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            aggregates: vec![],
            enums: vec![],
            unsupported: vec![],
            functions: vec![CoreFunction {
                storage_life: None,
                call_kind: None,
                life: None,
                callable_return: None,
                state_parameters: Vec::new(),
                protocol_contract: None,
                type_parameters: Vec::new(),
                callable_parameters: Vec::new(),
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: NodeId(0),
                name: "example".into(),
                parameters: vec![],
                return_type: Type::Unit,
                receiver: None,
                span: span(),
                body: CoreBlock {
                    span: span(),
                    statements: vec![CoreStatement {
                        id: NodeId(1),
                        kind,
                        span: span(),
                    }],
                },
            }],
        }
    }
    #[test]
    fn local_self_cannot_shadow_receiver() {
        let m = module(CoreStatementKind::Binding {
            name: "self".into(),
            mutable: false,
            ty: None,
            initializer: None,
        });
        assert_eq!(
            check(&m, &Limits::default()).unwrap_err().code,
            "E-RECEIVER-RESERVED-NAME"
        );
    }
    #[test]
    fn leading_dot_does_not_fallback_to_other_names() {
        let m = module(CoreStatementKind::Return(Some(CoreExpr {
            id: NodeId(2),
            kind: CoreExprKind::ReceiverMember("Center".into()),
            span: span(),
        })));
        assert_eq!(
            check(&m, &Limits::default()).unwrap_err().code,
            "E-RECEIVER-NONE"
        );
    }
}
