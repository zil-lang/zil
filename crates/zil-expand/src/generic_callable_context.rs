//! Original selected Fn bound and actual header inverse, before type erasure.
use super::*;
use zil_ir::*;

pub(crate) struct Produced {
    pub(crate) origin: origins::Origin,
    pub(crate) parameter: u32,
    pub(crate) generic_index: u32,
    pub(crate) kind: CallableKind,
    pub(crate) context: CoreCallableLifeContext,
    pub(crate) life: StorageLifeContract,
    pub(crate) parameters: Vec<Type>,
    pub(crate) result: Type,
}
#[derive(Default)]
pub(crate) struct Ledger {
    pub(crate) rows: Vec<Option<Produced>>,
    consumed: Vec<(origins::Origin, u32, u32, u32)>,
}
fn unavailable(span: Span, message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Expand, message, Some(span))
}
pub(super) fn scalar_syntax(ty: &TypeRef) -> bool {
    matches!(&ty.kind,TypeKind::Named {path,args} if args.is_empty() && matches!(path.as_slice(),[name] if matches!(name.as_str(),"unit"|"void"|"bool"|"i32"|"i64"|"u8"|"u16" | "u32" | "i8" | "i16" | "isize"|"u64"|"usize"|"f32"|"f64")))
}
pub(super) fn original_header_capability(item: &Item, budget: &mut Budget<'_>) -> R<()> {
    use crate::ct_selection::Visitor;
    struct Header<'a, 'l> {
        budget: &'a mut Budget<'l>,
    }
    impl Visitor for Header<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(
                ty.kind,
                TypeKind::MetaEval(_) | TypeKind::MetaSplice(_) | TypeKind::MetaBlock(_)
            ) {
                return Err(vec![unavailable(
                    ty.span,
                    "source helper header rewrite requires original bound lineage",
                )]);
            }
            crate::ct_selection::walk_type(self, ty, depth)
        }
    }
    if !item.attributes.is_empty() {
        return Err(unavailable(
            item.span,
            "source helper header attributes require original signature lineage",
        ));
    }
    let ItemKind::Function(function) = &item.kind else {
        return Err(unavailable(
            item.span,
            "source helper header is not a free function",
        ));
    };
    let mut header = Header { budget };
    for parameter in &function.generics {
        header
            .node(parameter.span, 0)
            .map_err(|mut e| e.remove(0))?;
        if let GenericParamKind::Type(bounds) = &parameter.kind {
            for bound in bounds {
                header.ty(bound, 0).map_err(|mut e| e.remove(0))?;
            }
        }
    }
    for parameter in &function.params {
        header.ty(&parameter.ty, 0).map_err(|mut e| e.remove(0))?;
    }
    if let Some(result) = &function.return_type {
        header.ty(result, 0).map_err(|mut e| e.remove(0))?;
    }
    Ok(())
}
impl Ledger {
    pub(crate) fn consume(
        &mut self,
        origin: origins::Origin,
        parameter: u32,
        generic: u32,
        bound: u32,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<Produced>> {
        budget.identity_work(
            self.rows
                .len()
                .saturating_add(self.consumed.len())
                .saturating_add(1),
            span,
        )?;
        let key = (origin, parameter, generic, bound);
        if self.consumed.contains(&key) {
            return Err(unavailable(
                span,
                "source helper original selected bound is lowered more than once",
            ));
        }
        let mut selected = None;
        for (index, row) in self.rows.iter().enumerate() {
            if row.as_ref().is_some_and(|r| {
                r.origin == origin
                    && r.parameter == parameter
                    && r.generic_index == generic
                    && r.context.bound_ordinal == bound
            }) && selected.replace(index).is_some()
            {
                return Err(unavailable(
                    span,
                    "source helper selected bound is ambiguous",
                ));
            }
        }
        if selected.is_some() {
            budget.identity_work(1, span)?;
            self.consumed.push(key);
        }
        Ok(selected.and_then(|index| self.rows[index].take()))
    }
    pub(crate) fn finish(&self, budget: &mut Budget<'_>, span: Span) -> R<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        budget.identity_work(self.rows.len(), span)?;
        if self.rows.iter().any(Option::is_some) {
            return Err(unavailable(
                span,
                "source helper bound has no actual lowered role",
            ));
        }
        Ok(())
    }
}
impl Engine<'_> {
    /// Called only at the first real stored-header preparation, while both the
    /// actual source bound and the complete declaration order remain available.
    pub(super) fn prepare_callable_context(
        &mut self,
        function: &Function,
        header: &StorageLifeContract,
        fixed: &[crate::stored_types::LexicalLife],
        invocation: &[crate::stored_types::LexicalLife],
        generic_parameter: usize,
        runtime_parameter: Option<usize>,
    ) -> R<bool> {
        if self.life_scope.is_empty() {
            return Ok(false);
        }
        let selected = callable::template(function, &mut self.budget)?;
        let Some(selected) = selected else {
            return Ok(false);
        };
        self.budget
            .identity_work(selected.bindings().count(), function.span)?;
        let mut roles = selected.bindings().filter(|role| {
            role.source_generic_parameter == generic_parameter as u32
                && runtime_parameter.is_none_or(|parameter| role.index == parameter)
        });
        let Some(role) = roles.next() else {
            return Ok(false);
        };
        if roles.next().is_some() {
            return Err(unavailable(
                function.span,
                "source helper selected role is ambiguous",
            ));
        }
        let parameter = function.generics.get(generic_parameter).ok_or_else(|| {
            unavailable(
                function.span,
                "source helper original generic parameter is absent",
            )
        })?;
        let GenericParamKind::Type(bounds) = &parameter.kind else {
            return Err(unavailable(
                function.span,
                "source helper original generic is not a Type",
            ));
        };
        let bound_ordinal = role.source_bound_ordinal;
        let bound = bounds
            .get(bound_ordinal as usize)
            .ok_or_else(|| unavailable(function.span, "source helper original bound is absent"))?;
        // A higher-rank binder belongs to its own prepared Code signature.
        // Only free declared classes require this original-Definition bridge.
        if matches!(bound.kind, TypeKind::ForLifetime { .. })
            || !callable_return_context::named_life(bound, &mut self.budget)?
        {
            return Ok(false);
        }
        let kind = role.kind;
        let role_parameter = role.index as u32;
        let role_generic = role.generic_index;
        let arguments = &role.parameters;
        let result = &role.result;
        let lending_result = kind != CallableKind::FnOnce
            && matches!(&result.kind, TypeKind::Reference { mutable:false, lifetime:None, inner }
                if scalar_syntax(inner));
        let origin = self
            .current_origin
            .ok_or_else(|| unavailable(function.span, "source helper owner lineage is absent"))?;
        if origin.member.is_some() || origin.provider || origin.compiler {
            return Err(unavailable(
                function.span,
                "source helper requires an original free Definition",
            ));
        }
        let origins = self
            .origins
            .ok_or_else(|| unavailable(function.span, "source helper identity is unavailable"))?;
        self.budget
            .identity_work(origins.entries.len().saturating_add(1), function.span)?;
        let original_index = origins
            .entries
            .iter()
            .position(|entry| *entry == origin)
            .ok_or_else(|| {
                unavailable(
                    function.span,
                    "source helper original declaration is absent",
                )
            })?;
        let original = self
            .original_module
            .items
            .get(original_index)
            .ok_or_else(|| unavailable(function.span, "source helper original item is absent"))?;
        if !matches!(original.kind, ItemKind::Function(_)) {
            return Err(unavailable(
                function.span,
                "source helper original owner is not a free function",
            ));
        }
        original_header_capability(original, &mut self.budget)?;
        let source = origins.identity(origin.declaration, &mut self.budget, function.span)?;
        if source.closure_sites != crate::source_context::ClosureSiteLineage::Original
            || !source.identity.generation_path.is_empty()
        {
            return Err(unavailable(
                function.span,
                "source helper requires original unreplaced declaration capability",
            ));
        }
        origins::pay_identity(&source.identity, &mut self.budget, function.span)?;
        let identity = source.identity.clone();
        let declaration = LifeDeclarationId {
            module: origin.declaration.module,
            declaration: source.declaration,
            compiler: false,
        };
        let mut parameters = Vec::new();
        let mut life_ordinals = Vec::new();
        for (index, parameter) in function.generics.iter().enumerate() {
            self.budget.node(parameter.span, 0)?;
            parameters.push(match &parameter.kind {
                GenericParamKind::Lifetime => {
                    life_ordinals.push(index as u32);
                    LifeParameterKind::Lifetime
                }
                GenericParamKind::Type(_) => LifeParameterKind::Type,
                GenericParamKind::Const(ty) => {
                    LifeParameterKind::Const(self.products.storage.pointer_parameter_type(
                        ty,
                        &self.nominals,
                        &mut self.budget,
                        0,
                    )?)
                }
            });
        }
        if life_ordinals.len() != self.life_scope.len() {
            return Err(unavailable(
                function.span,
                "source helper lexical scope lost its full ordinal inverse",
            ));
        }
        let source_of =
            |life: crate::stored_types::LexicalLife| life_ordinals.get(life.0 as usize).copied();
        let mut inputs = Vec::new();
        self.budget.identity_work(
            header.bindings.len().saturating_add(header.classes.len()),
            function.span,
        )?;
        for (binder, position) in header.bindings.iter().map(|r| (r.binder, &r.position)) {
            self.budget
                .identity_work(position.path.len().saturating_add(1), function.span)?;
            let StorageBase::Parameter(index) = position.base else {
                // Result rows belong to the complete header and are retained
                // there; an input anchor must always name a real parameter.
                continue;
            };
            let life = match binder {
                StorageBinder::Fixed(i) => fixed.get(i as usize),
                StorageBinder::Invocation(i) => invocation.get(i as usize),
            }
            .ok_or_else(|| {
                unavailable(function.span, "source helper header inverse is incomplete")
            })?;
            inputs.push(CoreClosureInputAnchor {
                parameter: index,
                source_parameter: source_of(*life),
                header_binder: binder,
                position: CoreClosureHeaderPosition::Borrow(position.clone()),
            });
        }
        for (binder, position) in header.classes.iter().map(|r| (r.binder, &r.position)) {
            self.budget
                .identity_work(position.path.len().saturating_add(1), function.span)?;
            let StorageBase::Parameter(index) = position.base else {
                // Result rows belong to the complete header and are retained
                // there; an input anchor must always name a real parameter.
                continue;
            };
            let life = match binder {
                StorageBinder::Fixed(i) => fixed.get(i as usize),
                StorageBinder::Invocation(i) => invocation.get(i as usize),
            }
            .ok_or_else(|| {
                unavailable(function.span, "source helper header inverse is incomplete")
            })?;
            inputs.push(CoreClosureInputAnchor {
                parameter: index,
                source_parameter: source_of(*life),
                header_binder: binder,
                position: CoreClosureHeaderPosition::Class(position.clone()),
            });
        }
        let mut occurrences = Vec::new();
        let mut stored = false;
        let mut next = self.life_scope.len() as u32;
        let mut actual_parameters = Vec::new();
        for (index, ty) in arguments.iter().enumerate() {
            self.budget.ty(ty)?;
            let mut ty = ty.clone();
            self.storage_header(
                &mut ty,
                StorageBase::Parameter(index as u32),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
            actual_parameters.push(self.products.storage.pointer_parameter_type(
                &ty,
                &self.nominals,
                &mut self.budget,
                0,
            )?);
        }
        self.budget.ty(result)?;
        let mut result = result.clone();
        if !lending_result {
            self.storage_header(
                &mut result,
                StorageBase::Result,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
        }
        let actual_result = self.products.storage.pointer_parameter_type(
            &result,
            &self.nominals,
            &mut self.budget,
            0,
        )?;
        // Only actual declared outer lives are Fixed. An anonymous callback
        // input remains Invocation even inside a qualified nominal boundary.
        for occurrence in &mut occurrences {
            occurrence.fixed_input = false;
        }
        let (life, bound_fixed, _) = crate::stored_types::contract_with_sources(
            occurrences,
            (0..self.life_scope.len())
                .map(|index| crate::stored_types::LexicalLife(index as u32))
                .collect(),
            &mut self.budget,
            bound.span,
        )?;
        let mut named_fixed = Vec::new();
        self.budget.identity_work(
            bound_fixed
                .len()
                .saturating_mul(inputs.len().saturating_add(1)),
            function.span,
        )?;
        for (fixed, lexical) in bound_fixed.iter().enumerate() {
            let parameter = source_of(*lexical).ok_or_else(|| {
                unavailable(
                    bound.span,
                    "source helper bound Fixed needs a real declared life",
                )
            })?;
            named_fixed.push(CoreClosureNamedFixed {
                fixed: fixed as u32,
                parameter,
            });
        }
        self.budget.identity_work(
            self.products.callable_contexts.rows.len().saturating_add(1),
            function.span,
        )?;
        if self
            .products
            .callable_contexts
            .rows
            .iter()
            .flatten()
            .any(|r| {
                r.origin == origin
                    && r.parameter == role_parameter
                    && r.generic_index == role_generic
                    && r.context.bound_ordinal == bound_ordinal
            })
        {
            return Err(unavailable(
                function.span,
                "source helper declaration is produced more than once",
            ));
        }
        self.products.callable_contexts.rows.push(Some(Produced {
            origin,
            parameter: role_parameter,
            generic_index: role_generic,
            kind,
            context: CoreCallableLifeContext {
                declaration,
                identity,
                parameters,
                generic_parameter: generic_parameter as u32,
                bound_ordinal,
                named_fixed,
                inputs,
            },
            life,
            parameters: actual_parameters,
            result: actual_result,
        }));
        self.products.storage.source_helpers = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    fn expanded(text: &str) -> zil_ir::CoreModule {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "bound.zil", text);
        let module = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let expanded = crate::expand_with_map_limits_target_context(
            &source,
            &module,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        assert!(
            expanded.core.unsupported.is_empty(),
            "{:?}",
            expanded.core.unsupported
        );
        expanded.core
    }

    #[test]
    fn owned_callable_bound_keeps_named_life_without_inventing_data_anchor() {
        let module = expanded(
            "package State<F>{manages storage:ByteBuffer;callback:F;count:i32}fn pack<life a,F:FnMut(&'a i32)->i32>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback,count:0}}",
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "pack")
            .unwrap();
        let [bound] = function.callable_parameters.as_slice() else {
            panic!("one actual role");
        };
        assert_eq!(
            bound.parameters,
            [Type::Address {
                mutable: false,
                inner: Box::new(Type::I32)
            }]
        );
        let life = bound.life.as_ref().unwrap();
        assert_eq!(
            (life.fixed_binder_count, life.invocation_binder_count),
            (1, 0)
        );
        assert_eq!(life.bindings[0].binder, StorageBinder::Fixed(0));
        let source = bound.source_life.as_ref().unwrap();
        assert_eq!(
            source.parameters,
            [LifeParameterKind::Lifetime, LifeParameterKind::Type]
        );
        assert_eq!(source.generic_parameter, 1);
        assert_eq!(
            source.named_fixed,
            [CoreClosureNamedFixed {
                fixed: 0,
                parameter: 0
            }]
        );
        assert!(source.inputs.is_empty());
        assert_eq!(
            function.storage_life.as_ref().unwrap().fixed_binder_count,
            0
        );
    }

    #[test]
    fn source_bound_preserves_all_parameter_and_result_occurrences() {
        let module = expanded(
            "fn apply<life a,life b,F:Fn(&'a i32,&'b i32)->&'a i32>(callback:&F,a:&'a i32,b:&'b i32)->i32{return *callback(a,b)}",
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "apply")
            .unwrap();
        let [bound] = function.callable_parameters.as_slice() else {
            panic!("one actual role");
        };
        let life = bound.life.as_ref().unwrap();
        assert_eq!(
            (life.fixed_binder_count, life.invocation_binder_count),
            (2, 0)
        );
        assert_eq!(
            life.bindings
                .iter()
                .map(|row| (row.position.base, row.binder))
                .collect::<Vec<_>>(),
            [
                (StorageBase::Parameter(0), StorageBinder::Fixed(0)),
                (StorageBase::Parameter(1), StorageBinder::Fixed(1)),
                (StorageBase::Result, StorageBinder::Fixed(0)),
            ]
        );
        let context = bound.source_life.as_ref().unwrap();
        assert_eq!(
            context.named_fixed,
            [
                CoreClosureNamedFixed {
                    fixed: 0,
                    parameter: 0
                },
                CoreClosureNamedFixed {
                    fixed: 1,
                    parameter: 1
                }
            ]
        );
        assert_eq!(context.generic_parameter, 2);
        assert_eq!(context.inputs.len(), 3);
    }
    #[test]
    fn stored_roles_preserve_the_same_complete_source_bound_as_direct_inputs() {
        let module = expanded(
            "package Pair<F,G>{manages storage:ByteBuffer;left:F;right:G}fn read<life a,life b,F:Fn(&'a i32)->i32,G:Fn(&'b i32)->i32>(state:Pair<F,G>,a:&'a i32,b:&'b i32)->i32{return state.left(a)+state.right(b)}",
        );
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "read")
            .unwrap();
        let [state] = function.state_parameters.as_slice() else {
            panic!("one real State parameter");
        };
        assert_eq!(state.bindings.len(), 2);
        for (ordinal, binding) in state.bindings.iter().enumerate() {
            let life = binding.life.as_ref().unwrap();
            let context = binding.source_life.as_ref().unwrap();
            assert_eq!(context.generic_parameter, ordinal as u32 + 2);
            assert_eq!(
                context.named_fixed,
                [CoreClosureNamedFixed {
                    fixed: 0,
                    parameter: ordinal as u32
                }]
            );
            assert_eq!(
                (life.fixed_binder_count, life.invocation_binder_count),
                (1, 0)
            );
            assert_eq!(life.bindings.len(), 1);
            assert_eq!(life.bindings[0].binder, StorageBinder::Fixed(0));
            assert_eq!(binding.fields, [ordinal as u32 + 1]);
        }
    }
}
