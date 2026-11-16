//! Borrow the exact parser-owned scalar AST throughout materialization. Source
//! maps, owner lookups, scope changes and their ordered ledger operations are
//! the same as the ordinary walker; no mutable output tree is needed here.
use super::*;

pub(super) fn execute(
    materializer: &mut Materializer<'_, '_>,
    module: &s::Module,
) -> CompileResult<()> {
    materializer.bind_readonly_source_items(module)?;
    if materializer.self_constants_needed(module)? {
        return Err(category(
            "scalar materialization requires a rewriting self constant",
            module.span,
        ));
    }
    items(materializer, module)?;
    materializer.release_bound_source_sites();
    Ok(())
}

fn items(m: &mut Materializer<'_, '_>, module: &s::Module) -> CompileResult<()> {
    m.depth(
        0,
        module
            .items
            .first()
            .map_or(Span::default(), |item| item.span),
    )?;
    // Keep the original row allocation charge even when current bind accounting
    // can replace same-address transport. Insufficient evidence keeps the walk.
    m.environment.budget.reserve_temporary_bytes(
        module.items.len() as u64
            * std::mem::size_of::<Vec<Option<compiler::source_sites::SourceSite>>>() as u64,
    )?;
    let readonly = m.take_readonly_source_sites(module)?;
    let mut source_rows = Vec::new();
    if !readonly {
        source_rows = Vec::with_capacity(module.items.len());
        for item in &module.items {
            source_rows.push(m.take_item_sites(item)?);
        }
    }
    let row_capacity = if readonly {
        module.items.len()
    } else {
        source_rows.capacity()
    };
    let row_bytes = row_capacity as u64
        * std::mem::size_of::<Vec<Option<compiler::source_sites::SourceSite>>>() as u64;
    let mut source_rows = source_rows.into_iter();
    for (ordinal, item) in module.items.iter().enumerate() {
        if readonly {
            m.restore_readonly_source_sites(item, ordinal)?;
        } else {
            m.restore_item_sites(item, source_rows.next().expect("one source row per item"))?;
        }
        let path_len = m.item_path.len();
        m.environment.budget.reserve_analysis_work(1)?;
        if path_len == m.item_path.capacity() {
            let old = m.item_path.capacity();
            m.environment
                .budget
                .reserve_temporary_bytes((path_len as u64 + 1) * 4)?;
            m.item_path.reserve_exact(1);
            m.environment.budget.release_temporary_bytes(old as u64 * 4);
        }
        m.item_path.push(
            u32::try_from(ordinal)
                .map_err(|_| category("item structural ordinal overflow", item.span))?,
        );
        // Resolve from this request's complete frozen environment, just like
        // the ordinary untracked, original-coordinate item path.
        let _owner = m.owner(module.id, item.span, None)?;
        m.depth(1, item.span)?;
        match &item.kind {
            s::ItemKind::Import(_) => {}
            s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                if runtime_phase::deferred(function, &mut m.environment.budget, m.max_depth)? {
                    return Err(category(
                        "scalar materialization has a deferred runtime body",
                        item.span,
                    ));
                }
                for parameter in &function.params {
                    ty(m, &parameter.ty, 2)?;
                }
                if let Some(result) = &function.return_type {
                    ty(m, result, 2)?;
                }
                m.runtime_scopes
                    .push(function.params.iter().map(|p| p.name.clone()).collect());
                let result = match &function.body {
                    s::FunctionBody::Block(body) => block(m, body, 2),
                    s::FunctionBody::Expression(value) => expression(m, value, 2),
                    s::FunctionBody::Declaration => Ok(()),
                };
                m.runtime_scopes.pop();
                result?;
            }
            _ => {
                return Err(category(
                    "non-scalar item in sealed scalar materialization",
                    item.span,
                ));
            }
        }
        m.discard_item_sites(item)?;
        m.item_path.truncate(path_len);
    }
    m.environment.budget.release_temporary_bytes(row_bytes);
    Ok(())
}

fn ty(m: &mut Materializer<'_, '_>, value: &s::TypeRef, depth: usize) -> CompileResult<()> {
    m.depth(depth, value.span)?;
    match &value.kind {
        s::TypeKind::Named { args, .. } if args.is_empty() => Ok(()),
        s::TypeKind::Group(inner) => ty(m, inner, depth + 1),
        _ => Err(category(
            "non-scalar type in sealed scalar materialization",
            value.span,
        )),
    }
}

fn block(m: &mut Materializer<'_, '_>, body: &s::Block, depth: usize) -> CompileResult<()> {
    m.runtime_scopes.push(BTreeSet::new());
    let result = (|| {
        m.depth(
            depth + 1,
            body.statements.first().map_or(Span::default(), |s| s.span),
        )?;
        for stmt in &body.statements {
            statement(m, stmt, depth + 2)?;
            m.clear_statement_source_sites(stmt)?;
        }
        Ok(())
    })();
    m.runtime_scopes.pop();
    result
}

fn statement(m: &mut Materializer<'_, '_>, stmt: &s::Stmt, depth: usize) -> CompileResult<()> {
    m.depth(depth, stmt.span)?;
    match &stmt.kind {
        s::StmtKind::Binding {
            pattern,
            ty: annotation,
            value,
            ..
        } => {
            if let Some(annotation) = annotation {
                ty(m, annotation, depth + 1)?;
            }
            expression(m, value, depth + 1)?;
            m.bind_runtime_pattern(pattern);
        }
        s::StmtKind::Return(Some(value)) | s::StmtKind::Expression(value) => {
            expression(m, value, depth + 1)?;
        }
        s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => {}
        s::StmtKind::Loop(body) | s::StmtKind::Unsafe(body) => block(m, body, depth + 1)?,
        s::StmtKind::If {
            condition: s::Condition::Expression(condition),
            then_body,
            else_body,
        } => {
            expression(m, condition, depth + 1)?;
            block(m, then_body, depth + 1)?;
            if let Some(branch) = else_body {
                match branch {
                    s::ElseBranch::Block(body) => block(m, body, depth + 1)?,
                    s::ElseBranch::If(stmt) => statement(m, stmt, depth + 1)?,
                }
            }
        }
        s::StmtKind::While {
            condition: s::Condition::Expression(condition),
            body,
        } => {
            expression(m, condition, depth + 1)?;
            block(m, body, depth + 1)?;
        }
        s::StmtKind::Assign { target, value, .. } => {
            expression(m, target, depth + 1)?;
            expression(m, value, depth + 1)?;
        }
        _ => {
            return Err(category(
                "non-scalar statement in sealed scalar materialization",
                stmt.span,
            ));
        }
    }
    Ok(())
}

fn expression(m: &mut Materializer<'_, '_>, value: &s::Expr, depth: usize) -> CompileResult<()> {
    m.depth(depth, value.span)?;
    match &value.kind {
        s::ExprKind::Literal(_) | s::ExprKind::Name(_) => {}
        s::ExprKind::Group(inner) | s::ExprKind::Unary { operand: inner, .. } => {
            expression(m, inner, depth + 1)?;
        }
        s::ExprKind::Binary { left, right, .. }
        | s::ExprKind::Pipeline {
            input: left,
            target: right,
        } => {
            expression(m, left, depth + 1)?;
            expression(m, right, depth + 1)?;
        }
        s::ExprKind::Member { object, .. } => expression(m, object, depth + 1)?,
        s::ExprKind::Call { callee, arguments } => {
            expression(m, callee, depth + 1)?;
            for argument in arguments {
                expression(m, &argument.value, depth + 1)?;
            }
        }
        s::ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            expression(m, condition, depth + 1)?;
            expression(m, then_value, depth + 1)?;
            expression(m, else_value, depth + 1)?;
        }
        _ => {
            return Err(category(
                "non-scalar expression in sealed scalar materialization",
                value.span,
            ));
        }
    }
    Ok(())
}
