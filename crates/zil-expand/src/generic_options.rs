//! Option syntax enters the existing enum specialization path with a sealed
//! compiler-only template. The generated enum remains ordinary untrusted Core.
use super::*;
use crate::generated_spans::{self, Visitor as _};
pub(super) const TEMPLATE: &str = "$builtinOption340";
pub(super) const TASK_POLL: &str = "$stdTaskPoll343";
pub(super) const TASK_CREATE_ERROR: &str = "$stdTaskCreateError343";
pub(super) const TASK_POLL_STATE: &str = "$stdTaskPollState343";
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

pub(super) fn prepare(module: &Module, limits: &Limits) -> CompileResult<(Option<Module>, Limits)> {
    let mut probe = Probe {
        budget: Budget::new(limits),
        present: false,
        tasks: false,
    };
    for item in &module.items {
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
        crate::ct_selection::Visitor::item(&mut probe, item, 0)?;
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
    if options {
        normal.budget.item(&item).map_err(|e| vec![e])?;
        output.items.insert(0, item);
    }
    if tasks {
        // These distributed standard declarations use the same ordinary enum
        // specialization/type/ownership route as source declarations.
        for item in task_declarations(span)
            .into_iter()
            .chain(task_aliases(module, span))
        {
            normal.budget.item(&item).map_err(|e| vec![e])?;
            output.items.insert(0, item);
        }
    }
    Ok((Some(output), normal.budget.remaining_limits()))
}

fn task_declarations(span: Span) -> Vec<Item> {
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
    .map(|(name, variants, generic)| Item {
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
    })
    .collect()
}

// Prelude aliases are ordinary bindings. Existing user declarations win, and
// the ordinary alias visitor handles local value/type shadowing at each use.
fn task_aliases(module: &Module, span: Span) -> Vec<Item> {
    let occupied = module
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::TypeAlias(a) => Some(a.name.as_str()),
            ItemKind::Import(i) => i
                .alias
                .as_deref()
                .or_else(|| i.path.last().map(String::as_str)),
            _ => name(item),
        })
        .collect::<BTreeSet<_>>();
    let mut aliases = vec![];
    for public in ["Poll", "TaskCreateError", "TaskPollState"] {
        let target = task_declaration(public).unwrap();
        for qualified in [true, false] {
            if occupied.contains(if qualified { "std" } else { public }) {
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
    aliases
}
