//! Audit actual output impls after item generation/selection. Completion is a
//! private trace of this request's CT jobs, never an input permission bit.
use super::*;
use zil_expand::ct_selection::Visitor;

#[derive(Clone, Copy)]
struct Row<'a> {
    top: Option<usize>,
    item: &'a s::Item,
}
fn collect<'a>(
    items: &'a [s::Item],
    top: bool,
    rows: &mut Vec<Row<'a>>,
    budget: &mut zil_ct::CtExecutionBudget,
    depth: usize,
    maximum: usize,
) -> CompileResult<()> {
    if depth > maximum {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "associated output declaration depth exhausted",
            items.first().map(|i| i.span),
        )]);
    }
    for (ordinal, item) in items.iter().enumerate() {
        budget.reserve_analysis_work(1)?;
        budget.reserve_temporary_bytes(4 * std::mem::size_of::<Row<'_>>() as u64)?;
        rows.push(Row {
            top: top.then_some(ordinal),
            item,
        });
        match &item.kind {
            s::ItemKind::Meta(s::MetaItem::If {
                then_items,
                else_items,
                ..
            }) => {
                collect(then_items, false, rows, budget, depth + 1, maximum)?;
                collect(else_items, false, rows, budget, depth + 1, maximum)?;
            }
            s::ItemKind::Meta(s::MetaItem::For { items, .. }) => {
                collect(items, false, rows, budget, depth + 1, maximum)?
            }
            _ => {}
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
struct Binding<'a> {
    name: &'a str,
    module: ModuleId,
    ty: &'a s::TypeRef,
    outer: usize,
    active: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selection {
    Closed,
    SelfDefault,
    OtherConsumer,
    Unknown,
}

struct Scope<'a, 'b> {
    modules: &'a [s::Module],
    rows: &'a [Vec<Row<'a>>],
    imports: &'a [(ModuleId, CoreImport)],
    budget: &'b mut zil_ct::CtExecutionBudget,
    maximum: usize,
}
impl<'a> Scope<'a, '_> {
    fn module(&mut self, id: ModuleId, span: Span) -> CompileResult<usize> {
        for (index, module) in self.modules.iter().enumerate() {
            self.budget.reserve_analysis_work(1)?;
            if module.id == id {
                return Ok(index);
            }
        }
        Err(category("associated coverage module is absent", span))
    }
    fn imported(
        &mut self,
        owner: usize,
        alias: &str,
        _span: Span,
    ) -> CompileResult<Option<ModuleId>> {
        let mut declaration = None;
        for row in &self.rows[owner] {
            self.budget.reserve_analysis_work(1)?;
            if let s::ItemKind::Import(import) = &row.item.kind {
                let Some(name) = import.alias.as_ref().or_else(|| import.path.last()) else {
                    continue;
                };
                self.budget
                    .reserve_analysis_work(name.len().min(alias.len()) as u64 + 1)?;
                if name == alias && !import.syntax {
                    if declaration.is_some() || row.top.is_none() {
                        return Ok(None);
                    }
                    declaration = Some(import);
                }
            }
        }
        let mut target = None;
        for (module, edge) in self.imports {
            self.budget
                .reserve_analysis_work(edge.alias.len().min(alias.len()) as u64 + 1)?;
            if *module == self.modules[owner].id && edge.alias == alias {
                if target.is_some_and(|old| old != edge.module) {
                    return Ok(None);
                }
                target = Some(edge.module);
            }
        }
        Ok(target)
    }
    fn self_default(&mut self, declaration: &s::TraitDecl) -> CompileResult<bool> {
        for member in &declaration.members {
            self.budget.reserve_analysis_work(1)?;
            if let s::ItemKind::Const(c) = &member.kind
                && let Some(value) = &c.value
            {
                let mut scan = associated_constants::Dependencies {
                    parameters: &declaration.generics,
                    budget: self.budget,
                    symbolic: false,
                    self_dependent: false,
                };
                scan.expression(value, 0)?;
                if scan.self_dependent {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    // None is an unresolved target. It cannot silently count as a closed
    // default when this request actually contains Self-dependent templates.
    fn resolves(
        &mut self,
        current: ModuleId,
        ty: &'a s::TypeRef,
        bindings: &[Binding<'a>],
        active: &mut Vec<(ModuleId, usize)>,
        depth: usize,
    ) -> CompileResult<Selection> {
        self.budget.reserve_analysis_work(1)?;
        if depth > self.maximum {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "associated coverage alias depth exhausted",
                Some(ty.span),
            )]);
        }
        if let s::TypeKind::Group(inner) = &ty.kind {
            return self.resolves(current, inner, bindings, active, depth + 1);
        }
        let s::TypeKind::Named { path, args } = &ty.kind else {
            return Ok(Selection::Unknown);
        };
        if let [name] = path.as_slice()
            && args.is_empty()
        {
            for binding in bindings.iter().rev() {
                self.budget
                    .reserve_analysis_work(binding.name.len().min(name.len()) as u64 + 1)?;
                if binding.name == name {
                    let Some(prefix) = active.get(..binding.active) else {
                        return Ok(Selection::Unknown);
                    };
                    self.budget.reserve_analysis_work(prefix.len() as u64)?;
                    self.budget
                        .reserve_temporary_bytes(std::mem::size_of_val(prefix) as u64)?;
                    let mut caller = prefix.to_vec();
                    return self.resolves(
                        binding.module,
                        binding.ty,
                        &bindings[..binding.outer],
                        &mut caller,
                        depth + 1,
                    );
                }
            }
        }
        let owner = self.module(current, ty.span)?;
        let (target, name) = match path.as_slice() {
            [name] => (current, name.as_str()),
            [alias, name] => {
                let Some(target) = self.imported(owner, alias, ty.span)? else {
                    return Ok(Selection::Unknown);
                };
                (target, name.as_str())
            }
            _ => return Ok(Selection::Unknown),
        };
        let target_index = self.module(target, ty.span)?;
        let mut candidate = None;
        for (ordinal, row) in self.rows[target_index].iter().enumerate() {
            self.budget.reserve_analysis_work(1)?;
            let symbol = match &row.item.kind {
                s::ItemKind::Trait(t) => Some(t.name.as_str()),
                s::ItemKind::TypeAlias(a) => Some(a.name.as_str()),
                _ => None,
            };
            if let Some(symbol) = symbol {
                self.budget
                    .reserve_analysis_work(symbol.len().min(name.len()) as u64 + 1)?;
                if symbol == name {
                    if candidate.is_some() {
                        return Ok(Selection::Unknown);
                    }
                    candidate = Some(ordinal);
                }
            }
        }
        let Some(ordinal) = candidate else {
            // A complete namespace with no nominal trait/alias leaves this
            // construct to its existing consumer (for example resource Drop).
            // Pending item selection cannot establish that absence.
            for row in &self.rows[target_index] {
                self.budget.reserve_analysis_work(1)?;
                if matches!(row.item.kind, s::ItemKind::Meta(_)) {
                    return Ok(Selection::Unknown);
                }
            }
            return Ok(Selection::OtherConsumer);
        };
        let row = &self.rows[target_index][ordinal];
        // A late selector has not established which declaration exists. Such a
        // row cannot establish a closed target merely by being encountered first.
        if row.top.is_none() {
            return Ok(Selection::Unknown);
        }
        match &row.item.kind {
            s::ItemKind::Trait(t) => {
                self.budget
                    .reserve_analysis_work(t.name.len().min(name.len()) as u64 + 1)?;
                if t.name == name {
                    return self.self_default(t).map(|dependent| {
                        if dependent {
                            Selection::SelfDefault
                        } else {
                            Selection::Closed
                        }
                    });
                }
            }
            s::ItemKind::TypeAlias(alias) => {
                self.budget
                    .reserve_analysis_work(alias.name.len().min(name.len()) as u64 + 1)?;
                let Some(value) = &alias.value else {
                    return Ok(Selection::Unknown);
                };
                if alias.generics.len() != args.len() {
                    return Ok(Selection::Unknown);
                }
                self.budget.reserve_analysis_work(active.len() as u64 + 1)?;
                self.budget.reserve_temporary_bytes(512)?;
                if active.contains(&(target, ordinal)) {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "associated constant coverage cannot finitely resolve a recursive trait alias",
                        Some(ty.span),
                    )]);
                }
                let caller_depth = active.len();
                active.push((target, ordinal));
                self.budget
                    .reserve_analysis_work(bindings.len() as u64 + alias.generics.len() as u64)?;
                self.budget.reserve_temporary_bytes(
                    (bindings.len().saturating_add(alias.generics.len())
                        * std::mem::size_of::<Binding<'_>>()) as u64
                        + 128,
                )?;
                let mut nested = Vec::with_capacity(bindings.len() + alias.generics.len());
                nested.extend_from_slice(bindings);
                for (position, parameter) in alias.generics.iter().enumerate() {
                    self.budget.reserve_analysis_work(1)?;
                    if matches!(parameter.kind, s::GenericParamKind::Type(_)) {
                        let Some(s::GenericArg::Type(actual)) = args.get(position) else {
                            return Ok(Selection::Unknown);
                        };
                        nested.push(Binding {
                            name: &parameter.name,
                            module: current,
                            ty: actual,
                            outer: bindings.len(),
                            active: caller_depth,
                        });
                    }
                }
                let result = self.resolves(target, value, &nested, active, depth + 1);
                active.pop();
                return result;
            }
            _ => {}
        }
        Ok(Selection::Unknown)
    }
}
impl Materializer<'_, '_> {
    pub(super) fn audit_self_constant_coverage(
        &mut self,
        modules: &[s::Module],
        imports: &[Vec<CoreImport>],
    ) -> CompileResult<()> {
        // A definition was prepared only after real frozen-source checks. No
        // input or raw cache can supply these private request-local coordinates.
        if self.self_constant_sources.is_empty() {
            return Ok(());
        }
        let mark = self.temporary_mark();
        self.environment
            .budget
            .reserve_analysis_work(modules.len() as u64)?;
        self.environment.budget.reserve_temporary_bytes(
            (modules.len() * std::mem::size_of::<Vec<Row<'_>>>()) as u64,
        )?;
        let mut rows = Vec::with_capacity(modules.len());
        for module in modules {
            let mut entries = Vec::new();
            collect(
                &module.items,
                true,
                &mut entries,
                &mut self.environment.budget,
                0,
                self.max_depth,
            )?;
            rows.push(entries);
        }
        let mut edges = Vec::new();
        for (module, original) in self.sources.iter().zip(imports) {
            for edge in original {
                self.environment
                    .budget
                    .reserve_analysis_work(edge.alias.len() as u64 + 1)?;
                self.environment.budget.reserve_temporary_bytes(
                    (4 * std::mem::size_of::<(ModuleId, CoreImport)>() + edge.alias.len()) as u64,
                )?;
                edges.push((module.module_id, edge.clone()));
            }
        }
        for (module, edge) in &self.required_imports {
            self.environment
                .budget
                .reserve_analysis_work(edge.alias.len() as u64 + 1)?;
            self.environment.budget.reserve_temporary_bytes(
                (4 * std::mem::size_of::<(ModuleId, CoreImport)>() + edge.alias.len()) as u64,
            )?;
            edges.push((*module, edge.clone()));
        }
        let mut scope = Scope {
            modules,
            rows: &rows,
            imports: &edges,
            budget: &mut self.environment.budget,
            maximum: self.max_depth,
        };
        for (module_index, module) in modules.iter().enumerate() {
            for row in &rows[module_index] {
                scope.budget.reserve_analysis_work(1)?;
                let s::ItemKind::Impl(implementation) = &row.item.kind else {
                    continue;
                };
                let Some(application) = &implementation.trait_type else {
                    continue;
                };
                let selection = scope.resolves(module.id, application, &[], &mut Vec::new(), 0)?;
                if matches!(selection, Selection::Closed | Selection::OtherConsumer) {
                    continue;
                }
                scope
                    .budget
                    .reserve_analysis_work(self.self_constant_impl_outputs.len() as u64 + 1)?;
                if selection == Selection::SelfDefault
                    && row.top.is_some_and(|ordinal| {
                        self.self_constant_impl_outputs
                            .contains(&(module.id, ordinal))
                    })
                {
                    continue;
                }
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "generated, late-selected, or unresolved Self-default impl has no completed Source CT jobs",
                    Some(row.item.span),
                )]);
            }
        }
        self.release_equation_temporary(mark, 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn g231_coverage_import_aliases_require_one_actual_target() {
        let texts = [
            "import api;fn main(){}",
            "pub trait C{const N:i32=1;}",
            "pub trait C{const N:i32;const B:i32=Self.N+1;}",
        ];
        let modules = texts
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let source = SourceUnit::new(
                    zil_base::FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{index}.zil"),
                    text,
                );
                zil_syntax::parse(&source).into_result().unwrap()
            })
            .collect::<Vec<_>>();
        let mut budget = zil_ct::CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: 10000,
            max_call_depth: 64,
            max_live_cells: 10000,
            max_temporary_bytes: 1000000,
            max_generated_nodes: 10000,
            max_instances: 10000,
            max_analysis_work: 1000000,
        });
        let mut rows = Vec::new();
        for module in &modules {
            let mut entries = Vec::new();
            collect(&module.items, true, &mut entries, &mut budget, 0, 64).unwrap();
            rows.push(entries);
        }
        for (target, expected) in [
            (ModuleId(1), Selection::Closed),
            (ModuleId(2), Selection::Unknown),
        ] {
            let edges = vec![
                (
                    ModuleId(0),
                    CoreImport {
                        alias: "api".into(),
                        module: ModuleId(1),
                        span: modules[0].span,
                    },
                ),
                (
                    ModuleId(0),
                    CoreImport {
                        alias: "api".into(),
                        module: target,
                        span: modules[0].span,
                    },
                ),
            ];
            let mut scope = Scope {
                modules: &modules,
                rows: &rows,
                imports: &edges,
                budget: &mut budget,
                maximum: 64,
            };
            let ty = s::TypeRef {
                span: modules[0].span,
                kind: s::TypeKind::Named {
                    path: vec!["api".into(), "C".into()],
                    args: vec![],
                },
            };
            assert_eq!(
                scope
                    .resolves(ModuleId(0), &ty, &[], &mut Vec::new(), 0)
                    .unwrap(),
                expected
            );
        }
    }
}
