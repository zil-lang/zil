//! Bind an original abstract-call subject to its exact actual receiver slot and
//! invocation. Type/role facts never grant its data or receiver-head loans.
use super::*;
use crate::semantic::value::Value;

impl Check<'_, '_> {
    pub(super) fn abstract_operation(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction, edge: SemanticSiteEdge::After } if actual == block && instruction as usize == index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            let Some(boundary) = self.original.abstract_boundary(
                instance,
                site.recipe,
                self.program,
                self.inventory,
                b,
            )?
            else {
                continue;
            };
            if !actuals.is_empty() {
                return Err(fail("abstract invocation invents mixed target actuals"));
            }
            if selected.is_some() {
                return Err(fail(
                    "abstract opcode has competing original invocation roles",
                ));
            }
            let instruction = function
                .blocks
                .get(block.0 as usize)
                .filter(|b| b.id == block)
                .and_then(|b| b.instructions.get(index))
                .ok_or_else(|| fail("abstract invocation has no actual instruction"))?;
            let InstructionKind::CallClosureBorrowed {
                callee,
                kind,
                arguments,
            } = &instruction.kind
            else {
                return Err(unknown(
                    "abstract original borrowed receiver needs its exact actual borrowed-call opcode",
                ));
            };
            if *kind != boundary.kind || boundary.signature.call_kind != CallKind::Direct {
                return Err(unknown(
                    "abstract callable needs its complete requested invocation ABI and receiver mode",
                ));
            }
            let parameter = self.anchored_local(
                instance,
                function,
                original::LocalAnchor::Parameter(boundary.parameter),
                b,
            )?;
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *callee,
                b,
            )?;
            if self.receiver_local(function, *callee, b, 0)? != parameter {
                return Err(fail(
                    "abstract call selects another original receiver parameter",
                ));
            }
            let receiver = self.ssa_value(instance, function, *callee, b, 0)?;
            let Type::Address { mutable, inner } = &receiver.ty else {
                return Err(fail(
                    "abstract call receiver is not an actual environment address",
                ));
            };
            let Type::Address {
                mutable: expected_mutable,
                ..
            } = &boundary.receiver.ty
            else {
                return Err(fail(
                    "original abstract receiver lost its reference carrier",
                ));
            };
            if (*mutable && !*expected_mutable)
                || (*kind == CallableKind::FnMut && !*mutable)
                || inner.as_ref() != &boundary.subject.ty
            {
                return Err(fail(
                    "abstract call changes its original complete receiver type or mode",
                ));
            }
            let expected_mutable = *expected_mutable;
            let mut expected_receiver = boundary.receiver;
            if !*mutable && expected_mutable {
                let Type::Address { inner, .. } = &expected_receiver.ty else {
                    unreachable!()
                };
                crate::dyn_metadata::ty(inner, b)?;
                expected_receiver.ty = Type::Address {
                    mutable: false,
                    inner: inner.clone(),
                };
                if expected_receiver.life.slots.len() != 1 {
                    return Err(fail("original receiver omits its outer occurrence"));
                }
                expected_receiver.life.slots[0].mode = StoredBorrowMode::SharedAddress;
            }
            if !self.outer_role(instance, function, &receiver, &expected_receiver, b, 0)? {
                return Err(fail(
                    "abstract receiver does not satisfy its original head role",
                ));
            }
            let Type::Closure(id) = boundary.subject.ty else {
                return Err(unknown(
                    "abstract function pointer requires its complete pointer invocation proof",
                ));
            };
            b.spend(self.module.closures.len().saturating_add(1), WORK)?;
            let definition = self
                .module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == id)
                .ok_or_else(|| {
                    fail("abstract receiver has no matching actual environment definition")
                })?;
            let mut context = instance;
            let mut found = false;
            for _ in 0..=b.partial_depth {
                b.spend(1, WORK)?;
                if context.definition == boundary.signature_owner {
                    found = true;
                    break;
                }
                context = self.inventory.instance(
                    context.parent.ok_or_else(|| {
                        fail("abstract role leaves its actual lexical parent chain")
                    })?,
                    b,
                )?;
            }
            if !found {
                return Err(unknown(
                    "abstract role lexical chain exceeds its finite depth",
                ));
            }
            self.capture_directory(id, b)?;
            self.callable_signature(
                context,
                &boundary.subject,
                definition,
                &boundary.signature,
                boundary.kind,
                b,
            )?;
            if arguments.len() != boundary.parameters.len()
                || arguments.len() != definition.parameters.len()
            {
                return Err(fail(
                    "abstract actual invocation changes its complete argument arity",
                ));
            }
            for ((argument, expected), actual_type) in arguments
                .iter()
                .zip(&boundary.parameters)
                .zip(&definition.parameters)
            {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *argument,
                    b,
                )?;
                if function.values.get(argument.0 as usize) != Some(&expected.ty)
                    || actual_type != &expected.ty
                {
                    return Err(fail(
                        "abstract invocation changes an original argument whole type",
                    ));
                }
                if self.uses_full_value(&expected.ty, b)? {
                    let actual = self.ssa_value(instance, function, *argument, b, 0)?;
                    let valid = if matches!(expected.ty, Type::Address { .. }) {
                        self.outer_role(instance, function, &actual, expected, b, 0)?
                    } else {
                        actual == *expected
                    };
                    if !valid {
                        return Err(fail(
                            "abstract invocation changes a complete original argument role",
                        ));
                    }
                }
            }
            if definition.result != boundary.result.ty {
                return Err(fail(
                    "abstract invocation changes its promised whole result type",
                ));
            }
            match instruction.result {
                Some(value)
                    if function.values.get(value.0 as usize) == Some(&boundary.result.ty) => {}
                None if boundary.result.ty == Type::Unit && boundary.result.classes.is_empty() => {}
                _ => {
                    return Err(fail(
                        "abstract invocation has another actual result SSA type",
                    ));
                }
            }
            // This is a semantic result-role substitution after proving the
            // actual receiver/entry/full signature. Actual roots, retained
            // parents, loans, effects and cleanup are still independently
            // reconstructed by the memory/callable kernels before publication.
            selected = Some(boundary.result);
        }
        Ok(selected)
    }
}
