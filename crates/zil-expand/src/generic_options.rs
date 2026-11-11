//! Option syntax enters the existing enum specialization path with a sealed
//! compiler-only template. The generated enum remains ordinary untrusted Core.
use super::*;
use crate::generated_spans::{self, Visitor as _};
pub(crate) const TEMPLATE: &str = "$builtinOption340";
pub(super) const TASK_POLL: &str = "$stdTaskPoll343";
pub(super) const TASK_CREATE_ERROR: &str = "$stdTaskCreateError343";
pub(super) const TASK_POLL_STATE: &str = "$stdTaskPollState343";

pub(super) fn storage_identity(
    item: &Item,
    origin: origins::Origin,
    budget: &mut Budget<'_>,
) -> R<(zil_ir::LifeDeclarationId, zil_ir::LifeDeclarationIdentity)> {
    let (role, template, empty, full, ordinal, module_path, public_name) = match origin.role {
        Some(origins::CompilerRole::Option) => (
            origins::CompilerRole::Option,
            TEMPLATE,
            "None",
            "Some",
            0,
            "option",
            "Option",
        ),
        Some(origins::CompilerRole::Poll) => (
            origins::CompilerRole::Poll,
            TASK_POLL,
            "Pending",
            "Ready",
            1,
            "task",
            "Poll",
        ),
        None => {
            return Err(error(
                item.span,
                "stored compiler nominal lacks actual prelude role provenance",
            ));
        }
    };
    budget.item(item)?;
    let ItemKind::Enum(declaration) = &item.kind else {
        return Err(error(
            item.span,
            format!("builtin {public_name} storage declaration is not an enum"),
        ));
    };
    let [formal] = declaration.generics.as_slice() else {
        return Err(error(
            item.span,
            format!("builtin {public_name} storage declaration has wrong arity"),
        ));
    };
    let [none, some] = declaration.variants.as_slice() else {
        return Err(error(
            item.span,
            format!("builtin {public_name} storage declaration has wrong variants"),
        ));
    };
    let VariantFields::Tuple(fields) = &some.fields else {
        return Err(error(
            item.span,
            format!("builtin {public_name} {full} payload is not a tuple"),
        ));
    };
    let [payload] = fields.as_slice() else {
        return Err(error(
            item.span,
            format!("builtin {public_name} {full} payload has wrong arity"),
        ));
    };
    if !origin.compiler
        || !origin.declaration.compiler
        || (role == origins::CompilerRole::Option && origin.declaration.ordinal != 0)
        || origin.member.is_some()
        || item.public
        || !item.attributes.is_empty()
        || declaration.name != template
        // Alias normalization alpha-renames Type binders. Authenticate the
        // actual binder reference, not its pre-normalization spelling.
        || formal.name.is_empty()
        || !matches!(&formal.kind, GenericParamKind::Type(bounds) if bounds.is_empty())
        || none.name != empty
        || !matches!(none.fields, VariantFields::Unit)
        || !none.attributes.is_empty()
        || some.name != full
        || !some.attributes.is_empty()
        || payload.manages
        || !matches!(&payload.ty.kind, TypeKind::Named {path,args} if path.as_slice()==[formal.name.as_str()] && args.is_empty())
    {
        return Err(error(
            item.span,
            format!("builtin {public_name} storage declaration is not authentic"),
        ));
    }
    budget.identity_work(64, item.span)?;
    budget.lineage_bytes(
        std::mem::size_of::<zil_ir::LifeDeclarationIdentity>()
            .saturating_add(3usize.saturating_mul(std::mem::size_of::<String>()))
            .saturating_add(64),
        item.span,
    )?;
    Ok((
        zil_ir::LifeDeclarationId {
            module: zil_base::ModuleId(u32::MAX),
            declaration: zil_base::NodeId(ordinal),
            compiler: true,
        },
        zil_ir::LifeDeclarationIdentity {
            package: "@zil.builtin".into(),
            module_path: vec![module_path.into()],
            declaration_path: vec!["enum".into(), public_name.into()],
            generation_path: Vec::new(),
        },
    ))
}

fn task_declaration(name: &str) -> Option<&'static str> {
    match name {
        "Poll" => Some(TASK_POLL),
        "TaskCreateError" => Some(TASK_CREATE_ERROR),
        "TaskPollState" => Some(TASK_POLL_STATE),
        _ => None,
    }
}

struct Probe<'a> {
    budget: Budget<'a>,
    present: bool,
    tasks: bool,
}
impl crate::ct_selection::Visitor for Probe<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn constructor_pattern(&mut self, path: &[String], _: Span) -> CompileResult<()> {
        // Only an actual constructor path requests standard declarations.
        // A literal string or an unrelated binding named Poll keeps its old
        // path and fee; ordinary alias resolution still handles shadowing.
        self.tasks |= path.len() >= 2 && task_declaration(&path[path.len() - 2]).is_some();
        Ok(())
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
        if matches!(ty.kind, TypeKind::Optional(_))
            || matches!(&ty.kind, TypeKind::Named{path,..} if path.as_slice()==["Option"])
        {
            self.present = true;
        }
        self.tasks |= matches!(&ty.kind, TypeKind::Named { path, .. } if path.last().is_some_and(|n| task_declaration(n).is_some() || n=="Task"));
        crate::ct_selection::walk_type(self, ty, depth)
    }
    fn expression(&mut self, e: &Expr, depth: usize) -> CompileResult<()> {
        self.present |= matches!(&e.kind, ExprKind::Name(name) if name=="Option");
        self.tasks |= matches!(&e.kind, ExprKind::Name(name) if task_declaration(name).is_some() || name=="task_create");
        self.tasks |= matches!(&e.kind,ExprKind::Member{member:MemberName::Name(name),..} if task_declaration(name).is_some());
        crate::ct_selection::walk_expression(self, e, depth)
    }
}
struct Normalize<'a> {
    budget: Budget<'a>,
}
impl generated_spans::Visitor for Normalize<'_> {
    fn node(&mut self, span: &mut Span, depth: usize) -> CompileResult<()> {
        self.budget.node(*span, depth).map_err(|e| vec![e])
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span).map_err(|e| vec![e])
    }
    fn ty(&mut self, ty: &mut TypeRef, depth: usize) -> CompileResult<()> {
        if let TypeKind::Optional(inner) = &mut ty.kind {
            self.budget.ty(inner).map_err(|e| vec![e])?;
            ty.kind = TypeKind::Named {
                path: vec![TEMPLATE.into()],
                args: vec![GenericArg::Type(*inner.clone())],
            };
        } else if let TypeKind::Named { path, .. } = &mut ty.kind
            && path.as_slice() == ["Option"]
        {
            path[0] = TEMPLATE.into();
        }
        generated_spans::walk_type(self, ty, depth)
    }
    fn expression(&mut self, e: &mut Expr, depth: usize) -> CompileResult<()> {
        if let ExprKind::Name(name) = &mut e.kind
            && name == "Option"
        {
            *name = TEMPLATE.into();
        }
        generated_spans::walk_expression(self, e, depth)
    }
}

#[derive(Debug)]
pub(super) struct Prepared {
    pub(super) module: Module,
    pub(super) prelude: origins::CompilerPrelude,
}
#[cfg(test)]
pub(super) fn prepare(
    module: &Module,
    limits: &Limits,
) -> CompileResult<(Option<Prepared>, Limits)> {
    prepare_cached(module, limits, None)
}
pub(super) fn prepare_cached(
    module: &Module,
    limits: &Limits,
    mut reuse: Option<&mut crate::incremental::Reuse<'_>>,
) -> CompileResult<(Option<Prepared>, Limits)> {
    let mut probe = Probe {
        budget: Budget::new(limits),
        present: false,
        tasks: false,
    };
    for item in &module.items {
        if reuse.as_deref_mut().is_some_and(|reuse| {
            reuse.replay(
                crate::incremental::Stage::Prepare,
                item.span,
                &mut probe.budget,
            )
        }) {
            continue;
        }
        let before = probe.budget.remaining_limits();
        let clock = reuse.as_deref().and_then(|reuse| reuse.clock());
        let reserved = match &item.kind {
            ItemKind::Aggregate(a) => a.name == "Option",
            ItemKind::Enum(e) => e.name == "Option",
            ItemKind::TypeAlias(a) => a.name == "Option",
            ItemKind::Trait(t) => t.name == "Option",
            _ => false,
        };
        if reserved {
            return Err(vec![error(
                item.span,
                "Option is a reserved built-in type family",
            )]);
        }
        let result = crate::ct_selection::Visitor::item(&mut probe, item, 0);
        if let Some(reuse) = reuse.as_deref_mut() {
            reuse.production(clock);
        }
        result?;
        if let Some(reuse) = reuse.as_deref_mut() {
            reuse.record(
                crate::incremental::Stage::Prepare,
                item.span,
                before,
                &probe.budget,
            );
        }
    }
    let remaining = probe.budget.remaining_limits();
    if !probe.present && !probe.tasks {
        return Ok((None, remaining));
    }
    // Clone only after a complete prepaid borrowed scan; normalize with the
    // same remaining ledger, then ordinary specialization pays its own work.
    let options = probe.present;
    let tasks = probe.tasks;
    let mut output = module.clone();
    let mut normal = Normalize {
        budget: probe.budget,
    };
    for item in &mut output.items {
        normal.item(item, 0)?;
    }
    let span = module.span;
    let item = Item {
        public: false,
        attributes: vec![],
        span,
        kind: ItemKind::Enum(EnumDecl {
            name: TEMPLATE.into(),
            generics: vec![GenericParam {
                name: "T".into(),
                kind: GenericParamKind::Type(vec![]),
                span,
            }],
            variants: vec![
                Variant {
                    name: "None".into(),
                    fields: VariantFields::Unit,
                    attributes: vec![],
                    span,
                },
                Variant {
                    name: "Some".into(),
                    fields: VariantFields::Tuple(vec![VariantTupleField {
                        ty: named("T", span),
                        manages: false,
                        span,
                    }]),
                    attributes: vec![],
                    span,
                },
            ],
        }),
    };
    let mut prelude = origins::CompilerPrelude::default();
    if options {
        prelude.len = 1;
        prelude.option = Some(0);
        normal.budget.item(&item).map_err(|e| vec![e])?;
        output.items.insert(0, item);
    }
    if tasks {
        // These distributed standard declarations use the same ordinary enum
        // specialization/type/ownership route as source declarations.
        // Fixed distributed declarations/aliases are bounded before creation;
        // the variable user binding inventory is paid separately below.
        normal
            .budget
            .identity_work(4096, span)
            .map_err(|e| vec![e])?;
        let aliases = task_aliases(module, span, &mut normal.budget).map_err(|e| vec![e])?;
        for (role, item) in task_declarations(span)
            .into_iter()
            .chain(aliases.into_iter().map(|item| (None, item)))
        {
            normal.budget.item(&item).map_err(|e| vec![e])?;
            output.items.insert(0, item);
            prelude.len += 1;
            prelude.option = prelude.option.map(|index| index + 1);
            prelude.poll = prelude.poll.map(|index| index + 1);
            if role == Some(origins::CompilerRole::Poll) {
                prelude.poll = Some(0);
            }
        }
    }
    Ok((
        Some(Prepared {
            module: output,
            prelude,
        }),
        normal.budget.remaining_limits(),
    ))
}

fn task_declarations(span: Span) -> Vec<(Option<origins::CompilerRole>, Item)> {
    [
        (TASK_CREATE_ERROR, vec!["Allocation"], false),
        (
            TASK_POLL_STATE,
            vec!["Complete", "Cancelled", "Invalid"],
            false,
        ),
        (TASK_POLL, vec!["Pending", "Ready"], true),
    ]
    .into_iter()
    .map(|(name, variants, generic)| {
        (
            generic.then_some(origins::CompilerRole::Poll),
            Item {
                public: false,
                attributes: vec![],
                span,
                kind: ItemKind::Enum(EnumDecl {
                    name: name.into(),
                    generics: if generic {
                        vec![GenericParam {
                            name: "T".into(),
                            kind: GenericParamKind::Type(vec![]),
                            span,
                        }]
                    } else {
                        vec![]
                    },
                    variants: variants
                        .into_iter()
                        .map(|name| Variant {
                            name: name.into(),
                            attributes: vec![],
                            span,
                            fields: if generic && name == "Ready" {
                                VariantFields::Tuple(vec![VariantTupleField {
                                    ty: named("T", span),
                                    manages: false,
                                    span,
                                }])
                            } else {
                                VariantFields::Unit
                            },
                        })
                        .collect(),
                }),
            },
        )
    })
    .collect()
}

// Prelude aliases are ordinary bindings. Existing user declarations win, and
// the ordinary alias visitor handles local value/type shadowing at each use.
fn task_aliases(module: &Module, span: Span, budget: &mut Budget<'_>) -> R<Vec<Item>> {
    budget.identity_work(module.items.len(), span)?;
    let mut occupied = BTreeSet::new();
    for item in &module.items {
        let declared = match &item.kind {
            ItemKind::TypeAlias(a) => Some(a.name.as_str()),
            ItemKind::Extern { function, .. } => Some(function.name.as_str()),
            ItemKind::Const(c) => Some(c.name.as_str()),
            ItemKind::Trait(t) => Some(t.name.as_str()),
            ItemKind::Import(i) => i
                .alias
                .as_deref()
                .or_else(|| i.path.last().map(String::as_str)),
            _ => name(item),
        };
        if let Some(name) = declared {
            budget.text(name, item.span)?;
            let comparisons = occupied
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
                .saturating_mul(12);
            budget.identity_work(
                name.len().saturating_add(1).saturating_mul(comparisons),
                item.span,
            )?;
            occupied.insert(name);
        }
    }
    let mut aliases = vec![];
    for public in ["Poll", "TaskCreateError", "TaskPollState"] {
        let target = task_declaration(public).unwrap();
        for qualified in [true, false] {
            let key = if qualified { "std" } else { public };
            let comparisons = occupied
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
                .saturating_mul(12);
            budget.identity_work(
                key.len().saturating_add(1).saturating_mul(comparisons),
                span,
            )?;
            if occupied.contains(key) {
                continue;
            }
            let generic = public == "Poll";
            aliases.push(Item {
                public: false,
                attributes: vec![],
                span,
                kind: ItemKind::TypeAlias(TypeAlias {
                    name: if qualified {
                        format!("std.task.{public}")
                    } else {
                        public.into()
                    },
                    generics: if generic {
                        vec![GenericParam {
                            name: "T".into(),
                            kind: GenericParamKind::Type(vec![]),
                            span,
                        }]
                    } else {
                        vec![]
                    },
                    bounds: vec![],
                    value: Some(TypeRef {
                        span,
                        kind: TypeKind::Named {
                            path: vec![target.into()],
                            args: if generic {
                                vec![GenericArg::Type(named("T", span))]
                            } else {
                                vec![]
                            },
                        },
                    }),
                }),
            });
        }
    }
    Ok(aliases)
}

#[cfg(test)]
mod namespace_budget {
    use super::*;
    #[test]
    fn standard_type_spelling_in_a_string_keeps_the_plain_path_and_budget() {
        let prepare_literal = |literal: &str| {
            let source = zil_base::SourceUnit::new(
                zil_base::FileId(0),
                zil_base::ModuleId(0),
                "literal.zil",
                format!("fn main(){{let text=\"{literal}\";}}"),
            );
            let module = zil_syntax::parse(&source).into_result().unwrap();
            prepare(&module, &Limits::default()).unwrap()
        };
        let (ordinary, ordinary_budget) = prepare_literal("Moll");
        let (poll, poll_budget) = prepare_literal("Poll");
        assert!(ordinary.is_none() && poll.is_none());
        assert_eq!(
            ordinary_budget.max_analysis_steps,
            poll_budget.max_analysis_steps
        );
    }
    #[test]
    fn common_prefix_standard_namespace_inventory_is_prepaid_at_exact_boundary() {
        fn input(prefix: usize) -> Module {
            let mut text = String::new();
            for i in 0..16 {
                text.push_str(&format!(
                    "struct {}_{i} {{x:i32}}\n",
                    "Namespace".repeat(prefix)
                ));
            }
            text.push_str("fn worker(x:i32)->i32=>x;fn main(){let t=task_create(worker,42);}");
            let source = zil_base::SourceUnit::new(
                zil_base::FileId(0),
                zil_base::ModuleId(0),
                "namespace.zil",
                text,
            );
            zil_syntax::parse(&source).into_result().unwrap()
        }
        let limits = Limits::default();
        let mut boundaries = vec![];
        for prefix in [1, 32] {
            let module = input(prefix);
            let accepts = |n| {
                prepare(
                    &module,
                    &Limits {
                        max_analysis_steps: n,
                        ..limits.clone()
                    },
                )
            };
            let (mut lo, mut hi) = (1, limits.max_analysis_steps);
            accepts(hi).unwrap();
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if accepts(mid).is_ok() {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            accepts(lo).unwrap();
            assert_eq!(
                accepts(lo - 1).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            accepts(lo).unwrap();
            boundaries.push(lo);
        }
        assert!(boundaries[1] > boundaries[0] * 2, "{boundaries:?}");
        eprintln!(
            "namespace exact source boundaries short={} long={}",
            boundaries[0], boundaries[1]
        );
    }
}
