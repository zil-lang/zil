//! Original opaque return signature and the containing actual input directory.
//! These are untrusted proposals; Types proves every selector against the real
//! creator and the returned Closure's complete capture/fixed domain.
use super::*;
use crate::stored_types::LexicalLife;
use zil_ir::*;

pub(crate) struct Produced {
    pub(crate) origin: origins::Origin,
    pub(crate) scope: CoreFunctionSourceScope,
    pub(crate) returned: CoreCallableReturn,
}
#[derive(Default)]
pub(crate) struct Ledger {
    pub(crate) rows: Vec<Option<Produced>>,
    consumed: Vec<origins::Origin>,
}
impl Ledger {
    pub(crate) fn consume(
        &mut self,
        origin: origins::Origin,
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
        if self.consumed.contains(&origin) {
            return Err(unsupported(
                span,
                "opaque return original owner was lowered twice",
            ));
        }
        let mut selected = None;
        for (index, row) in self.rows.iter().enumerate() {
            if row.as_ref().is_some_and(|row| row.origin == origin)
                && selected.replace(index).is_some()
            {
                return Err(unsupported(
                    span,
                    "opaque return original owner is ambiguous",
                ));
            }
        }
        if selected.is_some() {
            self.consumed.push(origin);
        }
        Ok(selected.and_then(|index| self.rows[index].take()))
    }
    pub(crate) fn finish(&self, budget: &mut Budget<'_>, span: Span) -> R<()> {
        budget.identity_work(self.rows.len(), span)?;
        if self.rows.iter().any(Option::is_some) {
            return Err(unsupported(
                span,
                "opaque return has no actual owner lowering event",
            ));
        }
        Ok(())
    }
}
impl Engine<'_> {
    pub(super) fn prepare_callable_return_context(
        &mut self,
        function: &Function,
        header: &StorageLifeContract,
        header_fixed: &[LexicalLife],
        header_invocation: &[LexicalLife],
    ) -> R<()> {
        let Some(ty) = &function.return_type else {
            return Ok(());
        };
        let Some((kind, arguments, result)) = callable::opaque_signature(ty) else {
            return Ok(());
        };
        if self.life_scope.is_empty() {
            return Ok(());
        }
        let origin = self.current_origin.ok_or_else(|| {
            unsupported(
                function.span,
                "opaque return original owner lineage is absent",
            )
        })?;
        if origin.member.is_some() || origin.provider || origin.compiler {
            return Err(unsupported(
                function.span,
                "opaque return scope requires an original free Definition",
            ));
        }
        let origins = self.origins.ok_or_else(|| {
            unsupported(function.span, "opaque return original identity is absent")
        })?;
        self.budget
            .identity_work(origins.entries.len().saturating_add(1), function.span)?;
        let original_index = origins
            .entries
            .iter()
            .position(|entry| *entry == origin)
            .ok_or_else(|| {
                unsupported(
                    function.span,
                    "opaque return owner is absent from its original ledger",
                )
            })?;
        let original = self
            .original_module
            .items
            .get(original_index)
            .ok_or_else(|| unsupported(function.span, "opaque return original item is absent"))?;
        callable_context::original_header_capability(original, &mut self.budget)?;
        let ItemKind::Function(original_function) = &original.kind else {
            unreachable!()
        };
        let source = origins.identity(origin.declaration, &mut self.budget, function.span)?;
        if source.closure_sites != crate::source_context::ClosureSiteLineage::Original
            || !source.identity.generation_path.is_empty()
        {
            return Err(unsupported(
                function.span,
                "opaque return requires an original unreplaced declaration",
            ));
        }
        origins::pay_identity(&source.identity, &mut self.budget, function.span)?;
        let declaration = LifeDeclarationId {
            module: origin.declaration.module,
            declaration: source.declaration,
            compiler: false,
        };
        let mut parameters = Vec::new();
        let mut names = Vec::new();
        for (ordinal, parameter) in original_function.generics.iter().enumerate() {
            self.budget.node(parameter.span, 0)?;
            parameters.push(match &parameter.kind {
                GenericParamKind::Lifetime => {
                    self.budget.text(&parameter.name, parameter.span)?;
                    names.push((parameter.name.clone(), ordinal as u32));
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
        let mut lexical_sources = Vec::new();
        for name in &self.life_scope {
            self.budget
                .identity_work(names.len().saturating_add(1), function.span)?;
            lexical_sources.push(
                names
                    .iter()
                    .find(|(source, _)| source == name)
                    .map(|(_, ordinal)| *ordinal),
            );
        }
        let mut inputs = Vec::new();
        let mut input_lives = Vec::new();
        self.budget.identity_work(
            header.bindings.len().saturating_add(header.classes.len()),
            function.span,
        )?;
        for row in &header.bindings {
            let StorageBase::Parameter(parameter) = row.position.base else {
                continue;
            };
            self.budget
                .identity_work(row.position.path.len().saturating_add(1), function.span)?;
            let life = match row.binder {
                StorageBinder::Fixed(i) => header_fixed.get(i as usize),
                StorageBinder::Invocation(i) => header_invocation.get(i as usize),
            }
            .ok_or_else(|| {
                unsupported(
                    function.span,
                    "opaque return actual input inverse is incomplete",
                )
            })?;
            inputs.push(CoreClosureInputAnchor {
                parameter,
                source_parameter: lexical_sources.get(life.0 as usize).copied().flatten(),
                header_binder: row.binder,
                position: CoreClosureHeaderPosition::Borrow(row.position.clone()),
            });
            input_lives.push(*life);
        }
        for row in &header.classes {
            let StorageBase::Parameter(parameter) = row.position.base else {
                continue;
            };
            self.budget
                .identity_work(row.position.path.len().saturating_add(1), function.span)?;
            let life = match row.binder {
                StorageBinder::Fixed(i) => header_fixed.get(i as usize),
                StorageBinder::Invocation(i) => header_invocation.get(i as usize),
            }
            .ok_or_else(|| {
                unsupported(
                    function.span,
                    "opaque return actual input class inverse is incomplete",
                )
            })?;
            inputs.push(CoreClosureInputAnchor {
                parameter,
                source_parameter: lexical_sources.get(life.0 as usize).copied().flatten(),
                header_binder: row.binder,
                position: CoreClosureHeaderPosition::Class(row.position.clone()),
            });
            input_lives.push(*life);
        }
        // This complete input directory is built before inspecting which
        // classes the returned bound mentions, and includes unused inputs.
        let scope = CoreFunctionSourceScope {
            declaration,
            identity: source.identity.clone(),
            parameters,
            inputs,
        };
        let mut occurrences = Vec::new();
        let mut stored = false;
        let mut next = u32::try_from(self.life_scope.len())
            .map_err(|_| error(ty.span, "opaque return life scope overflow"))?;
        let mut actual_parameters = Vec::new();
        for (index, parameter) in arguments.iter().enumerate() {
            self.budget.ty(parameter)?;
            let mut parameter = parameter.clone();
            self.storage_header(
                &mut parameter,
                StorageBase::Parameter(index as u32),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
            actual_parameters.push(self.products.storage.pointer_parameter_type(
                &parameter,
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
        for occurrence in &mut occurrences {
            occurrence.fixed_input = false;
        }
        let grounded = (0..self.life_scope.len())
            .map(|i| LexicalLife(i as u32))
            .collect();
        let (life, bound_fixed, _) = crate::stored_types::contract_with_sources(
            occurrences,
            grounded,
            &mut self.budget,
            ty.span,
        )?;
        let mut fixed = Vec::new();
        for (ordinal, lexical) in bound_fixed.into_iter().enumerate() {
            self.budget
                .identity_work(input_lives.len().saturating_add(1), ty.span)?;
            let selector = input_lives
                .iter()
                .position(|input| *input == lexical)
                .ok_or_else(|| {
                    unsupported(
                        ty.span,
                        "opaque return Fixed has no actual creator input anchor",
                    )
                })?;
            self.budget.identity_work(1, ty.span)?;
            fixed.push(CoreCallableReturnFixed {
                fixed: ordinal as u32,
                input: scope.inputs[selector].clone(),
            });
        }
        origins::pay_identity(&scope.identity, &mut self.budget, ty.span)?;
        self.budget.identity_work(scope.parameters.len(), ty.span)?;
        for parameter in &scope.parameters {
            if let LifeParameterKind::Const(ty) = parameter {
                crate::stored_types::pay_type(ty, &mut self.budget, function.span, 0)?;
            }
        }
        let context = CoreCallableReturnLifeContext {
            declaration: scope.declaration,
            identity: scope.identity.clone(),
            parameters: scope.parameters.clone(),
            fixed,
        };
        self.budget.identity_work(
            self.products.callable_returns.rows.len().saturating_add(1),
            function.span,
        )?;
        if self
            .products
            .callable_returns
            .rows
            .iter()
            .flatten()
            .any(|row| row.origin == origin)
        {
            return Err(unsupported(
                function.span,
                "opaque return scope was produced twice",
            ));
        }
        self.products.callable_returns.rows.push(Some(Produced {
            origin,
            scope,
            returned: CoreCallableReturn {
                life: Some(life),
                source_life: Some(context),
                kind,
                parameters: actual_parameters,
                result: actual_result,
            },
        }));
        self.products.storage.source_helpers = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_base::{FileId, ModuleId, SourceUnit, TargetProfile};

    fn core(text: &str) -> CompileResult<CoreModule> {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "callable-return-source.zil", text);
        let syntax = zil_syntax::parse(&source).into_result()?;
        let aliases = BTreeMap::new();
        crate::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &Limits::default(),
            &TargetProfile::default(),
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
        )
        .map(|expanded| expanded.core)
    }
    #[test]
    fn explicit_once_result_keeps_the_real_creator_input_anchor() {
        let module=core("fn make<life a>(value:&'a mut i32)->impl FnOnce()->&'a mut i32{return move()=>give value} fn main(){}").unwrap();
        let function = module.functions.iter().find(|f| f.name == "make").unwrap();
        let scope = function.source_scope.as_ref().unwrap();
        let bound = function.callable_return.as_ref().unwrap();
        assert_eq!(bound.kind, CallableKind::FnOnce);
        assert_eq!(
            bound.result,
            Type::Address {
                mutable: true,
                inner: Box::new(Type::I32)
            }
        );
        let context = bound.source_life.as_ref().unwrap();
        assert_eq!(context.declaration, scope.declaration);
        assert_eq!(context.identity, scope.identity);
        assert_eq!(scope.parameters, [LifeParameterKind::Lifetime]);
        assert_eq!(scope.inputs.len(), 1);
        assert_eq!(context.fixed.len(), 1);
        assert_eq!(context.fixed[0].input, scope.inputs[0]);
        assert_eq!(scope.inputs[0].source_parameter, Some(0));
        assert_eq!(
            bound.life.as_ref().unwrap().bindings,
            [StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: vec![]
                },
                binder: StorageBinder::Fixed(0)
            }]
        );
        assert!(function.life.is_none());
        assert!(function.storage_life.is_some());
    }
    #[test]
    fn return_selection_does_not_erase_unused_input_or_swap_same_abi_lives() {
        for (returned, ordinal) in [("a", 0), ("b", 1)] {
            let module=core(&format!("fn make<life a,life b>(left:&'a mut i32,right:&'b mut i32)->impl FnOnce()->&'{returned} mut i32{{return move()=>give left}} fn main(){{}}")).unwrap();
            let function = module.functions.iter().find(|f| f.name == "make").unwrap();
            let scope = function.source_scope.as_ref().unwrap();
            assert_eq!(scope.inputs.len(), 2);
            assert_eq!(scope.inputs[0].source_parameter, Some(0));
            assert_eq!(scope.inputs[1].source_parameter, Some(1));
            let context = function
                .callable_return
                .as_ref()
                .unwrap()
                .source_life
                .as_ref()
                .unwrap();
            assert_eq!(context.fixed[0].input, scope.inputs[ordinal]);
        }
    }
}
