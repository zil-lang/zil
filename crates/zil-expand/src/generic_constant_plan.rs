//! Only existing committed impl proof selects concrete equation work.
use super::*;
use crate::associated_constants::{Equation, Instance, Plan, Source};
use crate::ct_selection::{Visitor, walk_expression, walk_type};

pub(super) fn scalar(ty: &TypeRef) -> bool {
    matches!(&grouped(ty).kind, TypeKind::Named { path, args }
        if args.is_empty() && path.len() == 1 && matches!(path[0].as_str(),
        "bool"|"u8"|"u16"|"i32"|"i64"|"u64"|"usize"|"f32"|"f64"))
}
fn self_object(mut value: &Expr) -> bool {
    while let ExprKind::Group(inner) = &value.kind {
        value = inner;
    }
    matches!(&value.kind, ExprKind::Name(name) if name == "Self")
}
struct Dependencies<'a, 'b, 'l> {
    declaration: &'a TraitDecl,
    budget: &'b mut Budget<'l>,
    members: BTreeSet<usize>,
}
impl Visitor for Dependencies<'_, '_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
        if matches!(&ty.kind, TypeKind::Named { path,.. } if path.first().is_some_and(|p|p=="Self"))
        {
            return Err(vec![unsupported(
                ty.span,
                "Self-dependent constant receiving/type expressions need a later CT bridge",
            )]);
        }
        walk_type(self, ty, depth)
    }
    fn expression(&mut self, value: &Expr, depth: usize) -> CompileResult<()> {
        let member = match &value.kind {
            ExprKind::Member {
                object,
                member: MemberName::Name(name),
            } if self_object(object) => Some(name.as_str()),
            _ => trait_bounds::constants::parts(value).and_then(|(name, host, _)| {
                (trait_bounds::plain(host) == Some("Self")).then_some(name)
            }),
        };
        if let Some(name) = member {
            self.node(value.span, depth)?;
            self.budget.const_expression(value).map_err(|e| vec![e])?;
            for (ordinal, item) in self.declaration.members.iter().enumerate() {
                self.node(item.span, depth)?;
                if let ItemKind::Const(c) = &item.kind {
                    self.text(&c.name, item.span)?;
                    self.text(name, value.span)?;
                    if c.name == name {
                        let ty = c.ty.as_ref().ok_or_else(|| {
                            vec![error(
                                item.span,
                                "associated constant requires a declared type",
                            )]
                        })?;
                        self.budget.ty(ty).map_err(|e| vec![e])?;
                        if !scalar(ty) {
                            return Err(vec![unsupported(
                                ty.span,
                                "Self-dependent constants require existing explicit scalar types",
                            )]);
                        }
                        self.budget
                            .identity_work(self.members.len().saturating_add(1), value.span)
                            .map_err(|e| vec![e])?;
                        self.members.insert(ordinal);
                        return Ok(());
                    }
                }
            }
            return Err(vec![error(
                value.span,
                "Self constant member is not declared by its own trait",
            )]);
        }
        if matches!(&value.kind, ExprKind::Name(name) if name=="Self") {
            return Err(vec![unsupported(
                value.span,
                "Self constant expressions require an own-trait scalar member",
            )]);
        }
        walk_expression(self, value, depth)
    }
}
pub(super) fn dependencies(
    value: &Expr,
    declaration: &TraitDecl,
    budget: &mut Budget<'_>,
) -> R<Vec<usize>> {
    let mut scan = Dependencies {
        declaration,
        budget,
        members: BTreeSet::new(),
    };
    scan.expression(value, 0).map_err(|mut e| e.remove(0))?;
    scan.budget.identity_work(scan.members.len(), value.span)?;
    Ok(scan.members.into_iter().collect())
}

pub(crate) fn prepare(module: &Module, width: u16, limits: &Limits) -> CompileResult<Plan> {
    prepare_inner(module, width, limits).map_err(|e| vec![e])
}
fn prepare_inner(module: &Module, width: u16, limits: &Limits) -> R<Plan> {
    let mut budget = Budget::new(limits);
    budget.target_pointer_width = width;
    for item in &module.items {
        budget.item(item)?;
    }
    let patterns = BTreeMap::new();
    let checked = trait_bounds::associated::prepare(module, true, None, None, &mut budget)?
        .expect("associated checking requested");
    let mut engine = Engine::with_budget(&checked, limits, &patterns, None, budget)?;
    let index = trait_bounds::Index::new(module, &mut engine.budget)?;
    let mut definitions = Vec::new();
    let mut relevant = BTreeSet::new();
    for (item_index, item) in module.items.iter().enumerate() {
        engine.budget.node(item.span, 0)?;
        let (members, declaration, trait_index) = match &item.kind {
            ItemKind::Trait(t) => (&t.members, t, item_index),
            ItemKind::Impl(i) if i.trait_type.is_some() => {
                let reference = index.pattern_reference(
                    i.trait_type.as_ref().expect("guarded"),
                    &mut engine.budget,
                )?;
                let (owner, t) = index.declaration_item(&reference);
                let mut selected = None;
                for (n, candidate) in module.items.iter().enumerate() {
                    engine.budget.node(candidate.span, 0)?;
                    if std::ptr::eq(candidate, owner) {
                        selected = Some(n);
                        break;
                    }
                }
                (
                    &i.members,
                    t,
                    selected.ok_or_else(|| {
                        unsupported(
                            item.span,
                            "constant trait owner is outside the original module",
                        )
                    })?,
                )
            }
            _ => continue,
        };
        for (member_index, member) in members.iter().enumerate() {
            engine.budget.node(member.span, 0)?;
            let ItemKind::Const(c) = &member.kind else {
                continue;
            };
            let Some(value) = &c.value else { continue };
            let deps = dependencies(value, declaration, &mut engine.budget)?;
            if deps.is_empty() {
                continue;
            }
            if !declaration.generics.is_empty() || !declaration.bounds.is_empty() {
                return Err(unsupported(
                    member.span,
                    "Self-dependent constants in generic or inherited traits require a later instance bridge",
                ));
            }
            let mut required = None;
            for candidate in &declaration.members {
                engine.budget.node(candidate.span, 0)?;
                if let ItemKind::Const(declared) = &candidate.kind {
                    engine.budget.text(&declared.name, candidate.span)?;
                    engine.budget.text(&c.name, member.span)?;
                    if declared.name == c.name {
                        required = declared.ty.as_ref();
                        break;
                    }
                }
            }
            let required =
                required.ok_or_else(|| error(member.span, "constant declaration is absent"))?;
            engine.budget.ty(required)?;
            if !scalar(required) {
                return Err(unsupported(
                    member.span,
                    "Self-dependent constants require existing explicit scalar types",
                ));
            }
            engine
                .budget
                .identity_work(relevant.len().saturating_add(1), member.span)?;
            relevant.insert(trait_index);
            definitions.push(Equation {
                source: Source {
                    item: item_index,
                    member: member_index,
                },
                trait_item: trait_index,
                receiving: required.clone(),
                dependencies: deps,
            });
        }
    }
    let mut instances = Vec::new();
    for (item_index, item) in module.items.iter().enumerate() {
        engine.budget.node(item.span, 0)?;
        let ItemKind::Impl(i) = &item.kind else {
            continue;
        };
        let Some(application) = &i.trait_type else {
            continue;
        };
        let reference = index.pattern_reference(application, &mut engine.budget)?;
        let (owner, declaration) = index.declaration_item(&reference);
        let mut trait_index = None;
        for candidate in &relevant {
            engine.budget.node(item.span, 0)?;
            if std::ptr::eq(&module.items[*candidate], owner) {
                trait_index = Some(*candidate);
                break;
            }
        }
        let Some(trait_index) = trait_index else {
            continue;
        };
        if !i.generics.is_empty()
            || !i.where_clause.is_empty()
            || trait_bounds::plain(&i.target).is_none()
        {
            return Err(unsupported(
                item.span,
                "Self-dependent constants need a concrete nongeneric nominal impl",
            ));
        }
        let witness = index
            .solve(
                &reference,
                &i.target,
                &trait_bounds::Bounds::new(),
                &BTreeSet::new(),
                &mut engine.budget,
            )?
            .ok_or_else(|| {
                error(
                    item.span,
                    "constant job requires an actual committed impl witness",
                )
            })?;
        if !std::ptr::eq(witness.item, item) {
            return Err(error(
                item.span,
                "constant job selected a different impl witness",
            ));
        }
        let mut effective = Vec::new();
        let mut default_checks = Vec::new();
        for (ordinal, member) in declaration.members.iter().enumerate() {
            engine.budget.node(member.span, 0)?;
            let ItemKind::Const(c) = &member.kind else {
                continue;
            };
            let ty =
                c.ty.as_ref()
                    .ok_or_else(|| error(member.span, "constant declared type absent"))?;
            engine.budget.ty(ty)?;
            if !scalar(ty) {
                return Err(unsupported(
                    member.span,
                    "a Self-dependent instance requires existing scalar equations",
                ));
            }
            let mut supplied = None;
            for (n, supply) in i.members.iter().enumerate() {
                engine.budget.node(supply.span, 0)?;
                if let ItemKind::Const(s) = &supply.kind {
                    engine.budget.text(&s.name, supply.span)?;
                    engine.budget.text(&c.name, member.span)?;
                    if s.name == c.name {
                        supplied = Some(Source {
                            item: item_index,
                            member: n,
                        });
                        break;
                    }
                }
            }
            let default = Source {
                item: trait_index,
                member: ordinal,
            };
            if let Some(supply) = supplied {
                effective.push(supply);
                if c.value.is_some()
                    && !dependencies(
                        c.value.as_ref().expect("checked"),
                        declaration,
                        &mut engine.budget,
                    )?
                    .is_empty()
                {
                    default_checks.push(default);
                }
            } else if c.value.is_some() {
                effective.push(default);
            } else {
                return Err(error(
                    item.span,
                    "impl is missing its associated constant value",
                ));
            }
        }
        engine.budget.identity_work(
            effective
                .len()
                .saturating_add(default_checks.len())
                .saturating_add(1),
            item.span,
        )?;
        instances.push(Instance {
            item: item_index,
            trait_item: trait_index,
            effective,
            default_checks,
        });
    }
    Ok(Plan {
        definitions,
        instances,
        remaining_limits: engine.budget.remaining_limits(),
    })
}
