//! Lower omitted fields through the same function and call machinery as
//! authored operations. The declaration's original field supplies the body;
//! the construction site supplies only the decision to invoke it.
use super::*;
use crate::generated_spans::{self, Visitor};
use std::collections::{BTreeMap, BTreeSet};
use zil_base::NodeId;

struct DefaultField<'a> {
    host: usize,
    ordinal: u32,
    field: &'a s::Field,
    name: String,
    public: bool,
}

pub(super) struct Helper {
    pub(super) module: ModuleId,
    pub(super) placement: u32,
    pub(super) origin: crate::source_context::DeclarationOrigin,
}

pub(super) struct Prepared {
    pub(super) modules: Vec<s::Module>,
    pub(super) helpers: Vec<Helper>,
}

impl Prepared {
    pub(super) fn declarations<'a>(
        &'a self,
        mut originals: SourceDeclarationInventory<'a>,
        budget: &mut Budget<'_>,
    ) -> R<SourceDeclarationInventory<'a>> {
        let modules = self
            .modules
            .iter()
            .map(|module| (module.id, module))
            .collect::<BTreeMap<_, _>>();
        let max_depth = budget.remaining_limits().max_call_depth;
        for row in &mut originals.declarations {
            budget.identity_work(
                modules.len().saturating_add(1).ilog2() as usize + 1,
                row.item.span,
            )?;
            let module = modules.get(&row.module).ok_or_else(|| {
                error(
                    row.item.span,
                    "default preparation lost the original module",
                )
            })?;
            row.item =
                crate::source_context::source_item_at_path(module, &row.path, max_depth, |work| {
                    budget
                        .identity_work(work, module.span)
                        .map_err(|error| vec![error])
                })
                .map_err(|mut errors| errors.remove(0))?
                .ok_or_else(|| {
                    error(
                        row.item.span,
                        "default preparation moved an original declaration",
                    )
                })?;
        }
        for helper in &self.helpers {
            let module = modules.get(&helper.module).ok_or_else(|| {
                error(
                    Span::default(),
                    "default helper lost its declaration module",
                )
            })?;
            let item = module.items.get(helper.placement as usize).ok_or_else(|| {
                error(module.span, "default helper lost its prepared declaration")
            })?;
            budget.lineage_bytes(2 * std::mem::size_of::<u32>(), item.span)?;
            budget.lineage_push(
                &mut originals.declarations,
                SourceDeclaration {
                    module: helper.module,
                    declaration: helper.origin.declaration,
                    origin: &helper.origin,
                    path: vec![helper.placement],
                    source_path: vec![helper.placement],
                    item,
                    parent: None,
                    public: item.public,
                },
                item.span,
            )?;
            let next = originals
                .next_by_module
                .iter_mut()
                .find(|(module, _)| *module == helper.module)
                .ok_or_else(|| {
                    error(
                        item.span,
                        "default helper lacks a finite declaration namespace",
                    )
                })?;
            next.1 = next
                .1
                .max(helper.origin.declaration.0.checked_add(1).ok_or_else(|| {
                    error(item.span, "default helper declaration namespace exhausted")
                })?);
        }
        Ok(originals)
    }
}

struct Rewrite<'a, 'b, 'l> {
    module: ModuleId,
    imports: &'a [CoreImport],
    hosts: &'a BTreeMap<(ModuleId, String), Vec<usize>>,
    defaults: &'a [DefaultField<'a>],
    selected: &'b mut BTreeSet<usize>,
    result: Option<s::TypeRef>,
    default_host: Option<&'a str>,
    local_types: Vec<String>,
    budget: &'b mut Budget<'l>,
}

impl Rewrite<'_, '_, '_> {
    fn record(&mut self, expression: &mut s::Expr, context: Option<&s::TypeRef>) -> R<()> {
        let s::ExprKind::Record {
            ty,
            base: None,
            fields,
        } = &mut expression.kind
        else {
            return Ok(());
        };
        let span = expression.span;
        let written = ty
            .as_deref()
            .map(|ty| super::plan::type_expression(ty, self.budget))
            .transpose()?;
        let Some(s::TypeRef {
            kind: s::TypeKind::Named { path, args },
            ..
        }) = written.as_ref().or(context)
        else {
            return Ok(());
        };
        if !args.is_empty() {
            return Ok(());
        }
        let (module, alias, name) = match path.as_slice() {
            [name] => (
                self.module,
                None,
                if name == "Self" {
                    self.default_host.unwrap_or(name)
                } else {
                    name
                },
            ),
            [alias, name] => {
                self.budget
                    .identity_work(self.imports.len().saturating_mul(alias.len() + 1), span)?;
                let mut targets = self.imports.iter().filter(|edge| edge.alias == *alias);
                let Some(edge) = targets.next() else {
                    return Ok(());
                };
                if targets.next().is_some() {
                    return Ok(());
                }
                (edge.module, Some(alias.as_str()), name.as_str())
            }
            _ => return Ok(()),
        };
        if alias.is_none() {
            self.budget
                .identity_work(self.local_types.len().saturating_mul(name.len() + 1), span)?;
            if self.local_types.iter().any(|local| local == name) {
                return Ok(());
            }
        }
        self.budget.identity_work(
            name.len()
                .saturating_add(self.hosts.len().saturating_add(1).ilog2() as usize),
            span,
        )?;
        let Some(defaults) = self.hosts.get(&(module, name.to_owned())) else {
            return Ok(());
        };
        for &index in defaults {
            let default = &self.defaults[index];
            self.budget.identity_work(
                fields.len().saturating_mul(default.field.name.len() + 1),
                span,
            )?;
            if fields.iter().any(|field| field.name == default.field.name)
                || alias.is_some() && !default.public
            {
                continue;
            }
            self.budget
                .identity_work(self.selected.len().saturating_add(1).ilog2() as usize, span)?;
            self.selected.insert(index);
            self.budget.lineage_bytes(
                default.name.len() + default.field.name.len() + std::mem::size_of::<s::Expr>(),
                span,
            )?;
            let kind = match alias {
                Some(alias) => {
                    self.budget
                        .lineage_bytes(alias.len() + std::mem::size_of::<s::Expr>(), span)?;
                    s::ExprKind::Member {
                        object: Box::new(s::Expr {
                            semantic: Default::default(),
                            kind: s::ExprKind::Name(alias.to_owned()),
                            span,
                        }),
                        member: s::MemberName::Name(default.name.clone()),
                    }
                }
                None => s::ExprKind::Name(default.name.clone()),
            };
            self.budget.lineage_push(
                fields,
                s::RecordField {
                    name: default.field.name.clone(),
                    value: Some(s::Expr {
                        semantic: Default::default(),
                        kind: s::ExprKind::Call {
                            callee: Box::new(s::Expr {
                                semantic: Default::default(),
                                kind,
                                span,
                            }),
                            arguments: Vec::new(),
                        },
                        span,
                    }),
                    span,
                },
                span,
            )?;
        }
        Ok(())
    }
}

impl Visitor for Rewrite<'_, '_, '_> {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
        self.budget.node(*span, depth).map_err(|error| vec![error])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|error| vec![error])
    }
    fn item(&mut self, item: &mut s::Item, depth: usize) -> CompileResult<()> {
        let formals = match &item.kind {
            s::ItemKind::Function(function) => function.generics.as_slice(),
            s::ItemKind::Aggregate(declaration) => declaration.generics.as_slice(),
            s::ItemKind::Impl(implementation) => implementation.generics.as_slice(),
            s::ItemKind::Trait(declaration) => declaration.generics.as_slice(),
            _ => &[],
        };
        let local_count = self.local_types.len();
        for formal in formals {
            if matches!(formal.kind, s::GenericParamKind::Type(_)) {
                self.budget
                    .lineage_bytes(formal.name.len(), formal.span)
                    .map_err(|error| vec![error])?;
                self.budget
                    .lineage_push(&mut self.local_types, formal.name.clone(), formal.span)
                    .map_err(|error| vec![error])?;
            }
        }
        let result = (|| {
            if let s::ItemKind::Aggregate(declaration) = &mut item.kind {
                self.budget
                    .node(item.span, depth)
                    .map_err(|error| vec![error])?;
                for method in &mut declaration.methods {
                    self.item(method, depth + 1)?;
                }
                return Ok(());
            }
            if let s::ItemKind::Function(function) = &mut item.kind {
                if let (Some(ty), s::FunctionBody::Expression(expression)) =
                    (&function.return_type, &mut function.body)
                {
                    self.record(expression, Some(ty))
                        .map_err(|error| vec![error])?;
                }
                if let Some(ty) = &function.return_type {
                    self.budget.ty(ty).map_err(|error| vec![error])?;
                }
                let previous = std::mem::replace(&mut self.result, function.return_type.clone());
                let result = generated_spans::walk_item(self, item, depth);
                self.result = previous;
                result
            } else {
                generated_spans::walk_item(self, item, depth)
            }
        })();
        self.local_types.truncate(local_count);
        result
    }
    fn expression(&mut self, e: &mut s::Expr, depth: usize) -> CompileResult<()> {
        self.record(e, None).map_err(|error| vec![error])?;
        if let s::ExprKind::Ascribed { ty, value } = &mut e.kind {
            self.record(value, Some(ty)).map_err(|error| vec![error])?;
        }
        generated_spans::walk_expression(self, e, depth)
    }
    fn ty(&mut self, ty: &mut s::TypeRef, depth: usize) -> CompileResult<()> {
        if let Some(host) = self.default_host
            && let s::TypeKind::Named { path, args } = &mut ty.kind
            && path.as_slice() == ["Self"]
            && args.is_empty()
        {
            self.budget
                .lineage_bytes(host.len(), ty.span)
                .map_err(|error| vec![error])?;
            path[0] = host.to_owned();
        }
        generated_spans::walk_type(self, ty, depth)
    }
    fn statement(&mut self, statement: &mut s::Stmt, depth: usize) -> CompileResult<()> {
        match &mut statement.kind {
            s::StmtKind::Binding {
                ty: Some(ty),
                value,
                ..
            } => self.record(value, Some(ty)).map_err(|error| vec![error])?,
            s::StmtKind::Return(Some(value)) => {
                let result = self.result.take();
                let prepared = self.record(value, result.as_ref());
                self.result = result;
                prepared.map_err(|error| vec![error])?;
            }
            _ => {}
        }
        generated_spans::walk_statement(self, statement, depth)
    }
}

pub(super) fn prepare(
    syntax: &[&s::Module],
    inventory: &SourceDeclarationInventory<'_>,
    imports: &[Vec<CoreImport>],
    budget: &mut Budget<'_>,
) -> R<Option<Prepared>> {
    if syntax.len() != imports.len() {
        return Err(error(
            Span::default(),
            "original record defaults require complete module imports",
        ));
    }
    let mut fields = Vec::new();
    let mut hosts = BTreeMap::new();
    let mut selected = BTreeSet::new();
    for (host, row) in inventory.declarations.iter().enumerate() {
        let s::ItemKind::Aggregate(declaration) = &row.item.kind else {
            continue;
        };
        if !declaration.generics.is_empty() {
            continue;
        }
        for (ordinal, field) in declaration.fields.iter().enumerate() {
            budget.node(field.span, 0)?;
            if field.manages || field.default.is_none() {
                continue;
            }
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(field.span, "default field ordinal overflow"))?;
            let name = format!("$source_default_{}_{}", row.declaration.0, ordinal);
            budget.lineage_bytes(
                name.len().saturating_add(declaration.name.len()),
                field.span,
            )?;
            let public = row.public && declaration.kind == s::AggregateKind::Struct && field.public;
            let index = fields.len();
            budget.lineage_push(
                hosts
                    .entry((row.module, declaration.name.clone()))
                    .or_insert_with(Vec::new),
                index,
                field.span,
            )?;
            budget.lineage_push(
                &mut fields,
                DefaultField {
                    host,
                    ordinal,
                    field,
                    name,
                    public,
                },
                field.span,
            )?;
            if public {
                selected.insert(index);
            }
        }
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let span = syntax.first().map_or(Span::default(), |module| module.span);
    let mut modules = budget.retained_copy(span, |budget| {
        let mut modules = Vec::new();
        for module in syntax {
            for item in &module.items {
                budget.item(item)?;
            }
            budget.lineage_push(&mut modules, (*module).clone(), module.span)?;
        }
        Ok(modules)
    })?;
    for (module, edges) in modules.iter_mut().zip(imports) {
        let mut rewrite = Rewrite {
            module: module.id,
            imports: edges,
            hosts: &hosts,
            defaults: &fields,
            selected: &mut selected,
            result: None,
            default_host: None,
            local_types: Vec::new(),
            budget,
        };
        for item in &mut module.items {
            rewrite
                .item(item, 0)
                .map_err(|mut errors| errors.remove(0))?;
        }
    }
    let mut helpers = Vec::new();
    let mut completed = BTreeSet::new();
    let mut namespaces = inventory
        .next_by_module
        .iter()
        .copied()
        .collect::<BTreeMap<_, _>>();
    while let Some(index) = selected.pop_first() {
        if !completed.insert(index) {
            continue;
        }
        let default = &fields[index];
        let host = &inventory.declarations[default.host];
        let s::ItemKind::Aggregate(host_declaration) = &host.item.kind else {
            return Err(error(
                host.item.span,
                "default lost its original nominal declaration",
            ));
        };
        let field = default.field;
        let source = field.default.as_ref().expect("selected default body");
        let module_index = modules
            .iter()
            .position(|module| module.id == host.module)
            .ok_or_else(|| {
                error(
                    field.span,
                    "default body lost its actual declaration module",
                )
            })?;
        budget.const_expression(source)?;
        budget.ty(&field.ty)?;
        let mut function = crate::records::default_function(default.name.clone(), field);
        let s::FunctionBody::Expression(body) = &mut function.body else {
            return Err(error(
                field.span,
                "default helper loses its authored expression",
            ));
        };
        crate::modules::bind_default_self(body, host.module, &host_declaration.name, budget)?;
        let mut item = s::Item {
            kind: s::ItemKind::Function(function),
            attributes: Vec::new(),
            public: default.public,
            span: field.span,
        };
        let mut rewrite = Rewrite {
            module: host.module,
            imports: &imports[module_index],
            hosts: &hosts,
            defaults: &fields,
            selected: &mut selected,
            result: None,
            default_host: Some(&host_declaration.name),
            local_types: Vec::new(),
            budget,
        };
        rewrite
            .item(&mut item, 0)
            .map_err(|mut errors| errors.remove(0))?;
        let module = &mut modules[module_index];
        let placement = u32::try_from(module.items.len())
            .map_err(|_| error(field.span, "default helper placement overflow"))?;
        budget.lineage_push(&mut module.items, item, field.span)?;
        let next = namespaces.get_mut(&host.module).ok_or_else(|| {
            error(
                field.span,
                "default helper module has no declaration namespace",
            )
        })?;
        let declaration = NodeId(*next);
        *next = next
            .checked_add(1)
            .ok_or_else(|| error(field.span, "default helper declaration namespace exhausted"))?;
        let mut identity = host.origin.identity.clone();
        identity.declaration_path.extend([
            "field-default".into(),
            default.ordinal.to_string(),
            default.name.clone(),
        ]);
        budget.lineage_push(
            &mut helpers,
            Helper {
                module: host.module,
                placement,
                origin: crate::source_context::DeclarationOrigin {
                    closure_sites: crate::source_context::ClosureSiteLineage::Original,
                    declaration,
                    identity,
                    retained_members: None,
                    generated: None,
                },
            },
            field.span,
        )?;
    }
    Ok((!helpers.is_empty()).then_some(Prepared { modules, helpers }))
}
