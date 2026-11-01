//! Enum declarations and patterns remain ordinary untrusted source data.
use super::*;

impl Expander<'_> {
    pub(super) fn enum_declaration(&mut self, declaration: &s::EnumDecl, span: Span) {
        if !declaration.generics.is_empty() {
            self.unsupported("generic enum declarations", span);
        }
        for variant in &declaration.variants {
            if let s::VariantFields::Tuple(fields) = &variant.fields {
                for field in fields {
                    if field.manages
                        && !matches!(&field.ty.kind, s::TypeKind::Named { path, .. } if path.as_slice() != ["Ref"])
                    {
                        self.unsupported("manages enum field requires a resource nominal type, not a borrowed type", field.span);
                    }
                }
            }
        }
        let mut variants = Vec::new();
        for variant in &declaration.variants {
            self.unexecuted_attributes(&variant.attributes, "enum variant");
            let fields = match &variant.fields {
                s::VariantFields::Unit => Vec::new(),
                s::VariantFields::Tuple(fields) => fields
                    .iter()
                    .map(|field| {
                        let ty = type_ref(&field.ty);
                        if field.manages {
                            Type::Managed(Box::new(ty))
                        } else {
                            ty
                        }
                    })
                    .collect(),
                s::VariantFields::Record(_) => {
                    self.unsupported("record enum variants", variant.span);
                    Vec::new()
                }
            };
            variants.push((variant.name.clone(), fields));
        }
        let id = self.id();
        self.map_node(id, span, ExpansionRole::Source, Vec::new());
        self.enums.push(CoreEnum {
            id,
            name: declaration.name.clone(),
            variants,
            span,
        });
    }

    pub(super) fn match_statement(
        &mut self,
        value: &s::Expr,
        arms: &[s::MatchArm],
    ) -> CoreStatementKind {
        let value = self.expr(value);
        let arms = arms
            .iter()
            .map(|arm| {
                let pattern = self.pattern(&arm.pattern);
                let body = match &arm.body {
                    s::ArrowBody::Block(block) => self.block(block),
                    s::ArrowBody::Expression(expression) => {
                        let expression = self.expr(expression);
                        CoreBlock {
                            statements: vec![
                                self.statement(CoreStatementKind::Expression(expression), arm.span),
                            ],
                            span: arm.span,
                        }
                    }
                };
                CoreMatchArm {
                    pattern,
                    body,
                    span: arm.span,
                }
            })
            .collect();
        CoreStatementKind::Match { value, arms }
    }

    pub(super) fn pattern(&mut self, pattern: &s::Pattern) -> CorePattern {
        let kind = match &pattern.kind {
            s::PatternKind::Name(name) => CorePatternKind::Name(name.clone()),
            s::PatternKind::Wildcard => CorePatternKind::Wildcard,
            s::PatternKind::Literal(s::Literal::Integer(value)) => {
                CorePatternKind::Integer(value.clone())
            }
            s::PatternKind::Literal(s::Literal::Bool(value)) => CorePatternKind::Bool(*value),
            s::PatternKind::Tuple(fields) => {
                CorePatternKind::Tuple(fields.iter().map(|p| self.pattern(p)).collect())
            }
            s::PatternKind::Constructor { path, arguments } if path.len() >= 2 => {
                CorePatternKind::Variant {
                    ty: Type::Named(path[..path.len() - 1].join(".")),
                    variant: path.last().expect("nonempty path").clone(),
                    fields: arguments.iter().map(|p| self.pattern(p)).collect(),
                }
            }
            _ => {
                self.unsupported(
                    "reference, non-scalar or unqualified constructor pattern",
                    pattern.span,
                );
                CorePatternKind::Wildcard
            }
        };
        CorePattern {
            kind,
            span: pattern.span,
        }
    }
}
