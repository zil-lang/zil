//! Lexical dependencies of actual nominal generation requests. This private
//! ledger preserves unresolved original binders; it never closes their domains
//! or chooses a Types helper/closure instance from its physical image.
use super::*;
use crate::semantic_nominal_lower::{ApplicationArguments, Dependency};
use zil_ir::{
    SemanticActualRecipe, SemanticConstRecipe, SemanticLifeRecipe, SemanticOriginalNameBinding,
    SemanticOriginalNameValue, SemanticOriginalNamespace, SemanticTypeRecipe,
};

struct Life<'a> {
    name: &'a str,
    parent: Option<&'a Life<'a>>,
}
struct Scan<'a, 'b, 'l> {
    names: &'a [SemanticOriginalNameBinding],
    bound: Option<&'a Life<'a>>,
    output: &'b mut Vec<Dependency>,
    argument: u32,
    budget: &'b mut Budget<'l>,
}
impl Scan<'_, '_, '_> {
    fn binding(&mut self, name: &str, namespace: SemanticOriginalNamespace, span: Span) -> R<()> {
        if namespace == SemanticOriginalNamespace::Lifetime {
            let mut scope = self.bound;
            while let Some(bound) = scope {
                self.budget
                    .identity_work(name.len().saturating_add(bound.name.len()), span)?;
                if bound.name == name {
                    return Ok(());
                }
                scope = bound.parent;
            }
        }
        self.budget
            .identity_work(self.names.len().saturating_mul(name.len() + 1), span)?;
        for binding in self.names {
            if binding.namespace != namespace || binding.path.as_slice() != [name] {
                continue;
            }
            let formal = match &binding.value {
                SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(
                    SemanticTypeRecipe::Formal(reference),
                ))
                | SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Const {
                    value: SemanticConstRecipe::Formal(reference),
                    ..
                })
                | SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Life(
                    SemanticLifeRecipe::Formal(reference),
                )) => reference,
                _ => continue,
            };
            self.budget.identity_work(self.output.len(), span)?;
            if !self.output.iter().any(|row| {
                row.argument == self.argument && row.namespace == namespace && row.formal == *formal
            }) {
                self.budget.lineage_push(
                    self.output,
                    Dependency {
                        argument: self.argument,
                        namespace,
                        formal: formal.clone(),
                    },
                    span,
                )?;
            }
        }
        Ok(())
    }
    fn lifetimes(&mut self, args: &[GenericArg], span: Span) -> R<()> {
        for argument in args {
            self.budget.node(span, 0)?;
            if let GenericArg::Lifetime(name) = argument {
                self.binding(name, SemanticOriginalNamespace::Lifetime, span)?;
            }
        }
        Ok(())
    }
}
impl crate::ct_selection::Visitor for Scan<'_, '_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth).map_err(|error| vec![error])
    }
    fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
        self.budget.text(value, span).map_err(|error| vec![error])
    }
    fn expression(&mut self, value: &Expr, depth: usize) -> CompileResult<()> {
        if let ExprKind::Name(name) = &value.kind {
            self.binding(name, SemanticOriginalNamespace::Value, value.span)
                .map_err(|error| vec![error])?;
        }
        crate::ct_selection::walk_expression(self, value, depth)
    }
    fn ty(&mut self, value: &TypeRef, depth: usize) -> CompileResult<()> {
        match &value.kind {
            TypeKind::ForLifetime { lifetime, inner } => {
                self.node(value.span, depth)?;
                self.text(lifetime, value.span)?;
                let bound = Life {
                    name: lifetime,
                    parent: self.bound,
                };
                let mut child = Scan {
                    names: self.names,
                    bound: Some(&bound),
                    output: &mut *self.output,
                    argument: self.argument,
                    budget: &mut *self.budget,
                };
                return child.ty(inner, depth + 1);
            }
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args, .. } => {
                if let Some(root) = path.first() {
                    self.binding(root, SemanticOriginalNamespace::Type, value.span)
                        .map_err(|error| vec![error])?;
                    // The parser preserves a bare mixed const actual as a
                    // Type-shaped name. Only its actual original Const binding
                    // classifies that occurrence; no target ABI is consulted.
                    self.binding(root, SemanticOriginalNamespace::Value, value.span)
                        .map_err(|error| vec![error])?;
                }
                self.lifetimes(args, value.span)
                    .map_err(|error| vec![error])?;
            }
            TypeKind::Reference {
                lifetime: Some(name),
                ..
            } => {
                self.binding(name, SemanticOriginalNamespace::Lifetime, value.span)
                    .map_err(|error| vec![error])?;
            }
            _ => {}
        }
        crate::ct_selection::walk_type(self, value, depth)
    }
}
impl Engine<'_> {
    pub(super) fn with_nominal_owner<T>(
        &mut self,
        owner: Option<zil_syntax::ast::SemanticContextMarker>,
        run: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let previous = std::mem::replace(&mut self.nominal_owner, owner);
        let result = run(self);
        self.nominal_owner = previous;
        result
    }
    pub(super) fn item_nominal_owner(
        &mut self,
        item: &Item,
    ) -> R<Option<zil_syntax::ast::SemanticContextMarker>> {
        match &item.kind {
            ItemKind::Function(function) | ItemKind::Extern { function, .. } => {
                Ok(function.semantic.raw_index().map(|_| function.semantic))
            }
            ItemKind::Aggregate(value) => self.aggregate_nominal_owner(&value.name, item.span),
            ItemKind::Enum(value) => self.aggregate_nominal_owner(&value.name, item.span),
            _ => Ok(self.nominal_owner),
        }
    }
    fn aggregate_nominal_owner(
        &mut self,
        name: &str,
        span: Span,
    ) -> R<Option<zil_syntax::ast::SemanticContextMarker>> {
        let mut selected = None;
        for application in &self.products.nominal_applications {
            self.budget.text(&application.generated, span)?;
            self.budget.text(name, span)?;
            if application.generated != name {
                continue;
            }
            if Some(application.origin) != self.current_origin {
                return Err(error(span, "nominal pending generation origin differs"));
            }
            if selected.replace(application.owner).is_some() {
                return Err(error(
                    span,
                    "nominal pending item has multiple generation owners",
                ));
            }
        }
        Ok(selected.flatten())
    }
    pub(super) fn nominal_dependencies(
        &mut self,
        arguments: &[TypeRef],
        span: Span,
    ) -> R<Vec<Dependency>> {
        use crate::ct_selection::Visitor;
        let Some(owner) = self.nominal_owner else {
            return Ok(Vec::new());
        };
        let Some(program) = self.products.semantic.as_ref() else {
            return Ok(Vec::new());
        };
        let names = program.nominal_owner_names(owner, &mut self.budget, span)?;
        let mut output = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let ordinal = u32::try_from(index)
                .map_err(|_| error(span, "nominal actual ordinal exhausted"))?;
            let mut scan = Scan {
                names,
                bound: None,
                output: &mut output,
                argument: ordinal,
                budget: &mut self.budget,
            };
            scan.ty(argument, 0)
                .map_err(|mut errors| errors.remove(0))?;
        }
        Ok(output)
    }
}

impl ApplicationArguments {
    pub(crate) fn complete(&self) -> Option<&[zil_ir::LifeNominalActual]> {
        match self {
            Self::Complete(values) => Some(values),
            Self::Deferred(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ct_selection::Visitor;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, ModuleId, SourceUnit};
    use zil_ir::SemanticScopeRef;

    #[test]
    fn returned_symbolic_nominal_uses_the_actual_callee_owner() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "returned-nominal-owner.zil",
            "package State<F>{pub a:F;pub b:F}fn pack<F:FnMut(i32)->i32>(a:F,b:F)->State<F>{return State::<F>{a:a,b:b}}fn caller()->i32{return 0}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let aliases = BTreeMap::new();
        let (originals, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&syntax],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let (prepared, program) = crate::source_semantics::program::prepare(
            &[&syntax],
            &originals,
            &[Vec::new()],
            &mut limits.clone(),
        )
        .unwrap();
        let module = &prepared[0];
        let ItemKind::Function(pack) = &module.items[1].kind else {
            panic!()
        };
        let ItemKind::Function(caller) = &module.items[2].kind else {
            panic!()
        };
        let mut budget = Budget::new(&limits);
        let context = SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Aligned(&originals),
        }
        .resolve(std::slice::from_ref(&source), &mut budget)
        .unwrap();
        let mut origins = origins::Origins::single(module, false, &mut budget).unwrap();
        origins
            .capture_identities(module, false, &context, &mut budget)
            .unwrap();
        let patterns = BTreeMap::new();
        let mut engine = Engine::with_budget(
            module,
            &limits,
            &patterns,
            Some(&origins),
            &[],
            budget,
            None,
        )
        .unwrap();
        engine.products.semantic = Some(program);
        engine.nominal_owner = Some(caller.semantic);
        let first = engine.signature_result("pack").unwrap().unwrap();
        let second = engine.signature_result("pack").unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(engine.nominal_owner, Some(caller.semantic));
        assert_eq!(engine.products.nominal_applications.len(), 1);
        let application = &engine.products.nominal_applications[0];
        assert_eq!(application.owner, Some(pack.semantic));
        let ApplicationArguments::Deferred(dependencies) = &application.arguments else {
            panic!("unbound callable must not become a complete Named(F) actual")
        };
        assert!(
            dependencies
                .iter()
                .any(|dependency| dependency.argument == 0
                    && dependency.namespace == SemanticOriginalNamespace::Type
                    && dependency.formal.ordinal == 0
                    && dependency.formal.scope == SemanticScopeRef::Ancestor(0))
        );
    }

    #[test]
    fn actual_owner_dependencies_preserve_the_original_mixed_ordinals() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "nominal-owner.zil",
            "fn owner<life a,T,F:FnMut(i32)->i32,#N:i32>(value:T,callback:F,array:[T;N],view:&'a i32)->i32{return N}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&syntax],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let (prepared, program) = crate::source_semantics::program::prepare(
            &[&syntax],
            &origins,
            &[Vec::new()],
            &mut limits.clone(),
        )
        .unwrap();
        let ItemKind::Function(function) = &prepared[0].items[0].kind else {
            panic!()
        };
        let mut budget = Budget::new(&limits);
        let names = program
            .nominal_owner_names(function.semantic, &mut budget, function.span)
            .unwrap();
        let mut output = Vec::new();
        for (argument, parameter) in function.params.iter().enumerate() {
            Scan {
                names,
                bound: None,
                output: &mut output,
                argument: argument as u32,
                budget: &mut budget,
            }
            .ty(&parameter.ty, 0)
            .unwrap();
        }
        let has = |argument, namespace, ordinal| {
            output.iter().any(|row| {
                row.argument == argument
                    && row.namespace == namespace
                    && row.formal.scope == SemanticScopeRef::Ancestor(0)
                    && row.formal.ordinal == ordinal
            })
        };
        assert!(has(0, SemanticOriginalNamespace::Type, 1));
        assert!(has(1, SemanticOriginalNamespace::Type, 2));
        assert!(has(2, SemanticOriginalNamespace::Type, 1));
        assert!(has(2, SemanticOriginalNamespace::Value, 3));
        assert!(has(3, SemanticOriginalNamespace::Lifetime, 0));
        assert_eq!(output.len(), 5);

        // A new quantifier shadows the original lifetime spelling. It must
        // not be recorded as a dependency of the outer owner's Life(0).
        let local = TypeRef {
            kind: TypeKind::ForLifetime {
                lifetime: "a".into(),
                inner: Box::new(function.params[3].ty.clone()),
            },
            span: function.span,
        };
        let mut scoped = Vec::new();
        Scan {
            names,
            bound: None,
            output: &mut scoped,
            argument: 0,
            budget: &mut budget,
        }
        .ty(&local, 0)
        .unwrap();
        assert!(scoped.is_empty());
        assert!(
            program
                .nominal_owner_names(
                    zil_syntax::ast::SemanticContextMarker::untrusted(u32::MAX - 1),
                    &mut budget,
                    function.span
                )
                .is_err()
        );
    }
}
