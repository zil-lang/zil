//! Original selected Fn bound and actual header inverse, before type erasure.
use super::*;
use zil_ir::*;

pub(crate) struct Produced {
    pub(crate) origin: origins::Origin,
    pub(crate) parameter: u32,
    pub(crate) generic_index: u32,
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
    matches!(&ty.kind,TypeKind::Named {path,args} if args.is_empty() && matches!(path.as_slice(),[name] if matches!(name.as_str(),"unit"|"void"|"bool"|"i32"|"i64"|"u8"|"u16"|"u64"|"usize"|"f32"|"f64")))
}
fn original_header_capability(item: &Item, budget: &mut Budget<'_>) -> R<()> {
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
    ) -> R<()> {
        if self.life_scope.is_empty() {
            return Ok(());
        }
        let Some(parameter) = function.generics.get(generic_parameter) else {
            return Ok(());
        };
        let GenericParamKind::Type(bounds) = &parameter.kind else {
            unreachable!()
        };
        let [bound] = bounds.as_slice() else {
            return Ok(());
        };
        let TypeKind::Named { path, args } = &bound.kind else {
            return Ok(());
        };
        let kind = match path.as_slice() {
            [name] if name == "Fn" => CallableKind::Fn,
            [name] if name == "FnMut" => CallableKind::FnMut,
            _ => return Ok(()),
        };
        let candidate = matches!(args.as_slice(), [GenericArg::Type(parameters), GenericArg::Type(_)]
            if matches!(&parameters.kind, TypeKind::Tuple(parameters) if matches!(parameters.as_slice(), [parameter]
                if matches!(&parameter.kind, TypeKind::Reference { mutable:false,inner,.. }
                    if matches!(&inner.kind, TypeKind::Named { args,.. } if matches!(args.as_slice(), [GenericArg::Lifetime(_)]))))));
        if !candidate {
            return Ok(());
        }
        let Some(GenericArg::Type(result)) = args.last() else {
            return Ok(());
        };
        if !scalar_syntax(result)
            || function
                .return_type
                .as_ref()
                .is_some_and(|result| !scalar_syntax(result))
        {
            return Ok(());
        }
        let selected = callable::template(function, &mut self.budget)?.ok_or_else(|| {
            unavailable(
                function.span,
                "source helper actual selected role is absent",
            )
        })?;
        let [selected_role] = selected.roles.as_slice() else {
            return Err(unavailable(
                function.span,
                "source helper first profile requires one actual shared Fn role",
            ));
        };
        if selected_role.source_generic_parameter != generic_parameter as u32
            || selected_role.source_bound_ordinal != 0
            || selected_role.kind != kind
        {
            return Err(unavailable(
                function.span,
                "source helper selected bound differs from its actual receiver",
            ));
        }
        if callable::parameter_mode(&function.params[selected_role.index].ty, &parameter.name)
            != Some(Some(kind == CallableKind::FnMut))
        {
            return Ok(());
        }
        let role_parameter = selected_role.index as u32;
        let role_generic = selected_role.generic_index;
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
            if matches!(parameter.kind, GenericParamKind::Lifetime) {
                parameters.push(LifeParameterKind::Lifetime);
                life_ordinals.push(index as u32);
            } else {
                parameters.push(LifeParameterKind::Type);
            }
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
                return Err(unavailable(
                    function.span,
                    "source helper first profile has scalar result",
                ));
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
                return Err(unavailable(
                    function.span,
                    "source helper first profile has scalar result",
                ));
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
        let [GenericArg::Type(arguments), GenericArg::Type(result)] = args.as_slice() else {
            return Err(unavailable(
                bound.span,
                "source helper requires its actual complete Fn P/R",
            ));
        };
        let TypeKind::Tuple(arguments) = &arguments.kind else {
            return Err(unavailable(
                bound.span,
                "source helper Fn parameters require a tuple",
            ));
        };
        if arguments.len() != 1 {
            return Err(unavailable(
                bound.span,
                "source helper first profile has one shared qualified callback input",
            ));
        }
        let mut occurrences = Vec::new();
        let mut stored = false;
        let mut next = self.life_scope.len() as u32;
        let mut actual_parameters = Vec::new();
        for (index, ty) in arguments.iter().enumerate() {
            self.budget.ty(ty)?;
            let mut ty = ty.clone();
            let physical_witness = self.storage_header(
                &mut ty,
                StorageBase::Parameter(index as u32),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
            if !physical_witness {
                return Ok(());
            }
            actual_parameters.push(self.products.storage.pointer_parameter_type(
                &ty,
                &self.nominals,
                &mut self.budget,
                0,
            )?);
        }
        self.budget.ty(result)?;
        let mut result = result.clone();
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
        let actual_result = self.products.storage.pointer_parameter_type(
            &result,
            &self.nominals,
            &mut self.budget,
            0,
        )?;
        let (life, bound_fixed, _) = crate::stored_types::contract_with_sources(
            occurrences,
            Vec::new(),
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
            if !inputs.iter().any(|input| {
                input.source_parameter == Some(parameter)
                    && matches!(input.position, CoreClosureHeaderPosition::Class(_))
            }) {
                return Err(unavailable(
                    bound.span,
                    "source helper bound life has no actual qualified data header anchor",
                ));
            }
            named_fixed.push(CoreClosureNamedFixed {
                fixed: fixed as u32,
                parameter,
            });
        }
        if !stored || named_fixed.is_empty() {
            return Err(unavailable(
                bound.span,
                "source helper bound needs actual qualified Fixed input",
            ));
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
            .any(|r| r.origin == origin)
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
            context: CoreCallableLifeContext {
                declaration,
                identity,
                parameters,
                generic_parameter: generic_parameter as u32,
                bound_ordinal: 0,
                named_fixed,
                inputs,
            },
            life,
            parameters: actual_parameters,
            result: actual_result,
        }));
        self.products.storage.source_helpers = true;
        Ok(())
    }
}
