//! Nominal obligations in a real caller Gamma, borrowing the same immutable
//! whole-program proof. Runtime type equality only locates an application.
use super::*;

pub(in crate::semantics) struct NominalRequest {
    pub caller: SemanticDefinitionId,
    pub recipe: SemanticTypeRecipe,
}
pub(in crate::semantics) struct NominalWhere<'a> {
    program: &'a SemanticModule,
    proof: &'a Proof,
}
pub(super) fn borrowed_check<'a, 'b>(
    program: &'a SemanticModule,
    proof: &'a Proof,
    owner: SemanticDefinitionId,
    limits: &'b Limits,
    left: &'b mut usize,
) -> R<Check<'a, 'b>> {
    if !proof.complete.contains(&owner) {
        return Err(bad("original where caller has no complete original body"));
    }
    *left = left.checked_sub(1).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original where context work exceeded", None)
    })?;
    Ok(Check {
        program,
        limits,
        left,
        proof: ProofAccess::Borrowed(proof),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: vec![owner],
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        escape_active: Vec::new(),
    })
}
impl<'a> NominalWhere<'a> {
    pub(in crate::semantics) fn new(program: &'a SemanticModule, proof: &'a Proof) -> Self {
        Self { program, proof }
    }
    pub(in crate::semantics) fn requests(
        &self,
        definition: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Vec<NominalRequest>> {
        *left = left
            .checked_sub(self.proof.types.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original nominal request lookup exceeded",
                    None,
                )
            })?;
        let mut result: Vec<NominalRequest> = Vec::new();
        for ((caller, _), value) in &self.proof.types {
            if !self.proof.complete.contains(caller) {
                continue;
            }
            for recipe in substitute::nominal_types(value, definition, limits, left)? {
                let mut duplicate = false;
                for previous in &result {
                    if previous.caller != *caller {
                        continue;
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &previous.recipe,
                        limits,
                        left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(&recipe, limits, left, 1)?;
                    duplicate |= previous.recipe == recipe;
                }
                if !duplicate {
                    *left = left.checked_sub(1).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "original nominal request allocation exceeded",
                            None,
                        )
                    })?;
                    result.push(NominalRequest {
                        caller: *caller,
                        recipe,
                    });
                }
            }
        }
        Ok(result)
    }
    pub(in crate::semantics) fn prove(
        &self,
        caller: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = recipe
        else {
            return Err(bad(
                "original nominal obligation changes its source recipe kind",
            ));
        };
        let mut check = borrowed_check(self.program, self.proof, caller, limits, left)?;
        let declaration = check.definition(*definition)?;
        if !matches!(declaration.body, SemanticDefinitionBody::Nominal { .. }) {
            return Err(bad(
                "original nominal obligation has no true nominal declaration",
            ));
        }
        let result = (|| {
            for requirement in
                declaration
                    .requirements
                    .iter()
                    .chain(
                        declaration
                            .parameters
                            .iter()
                            .flat_map(|formal| match formal {
                                SemanticFormal::Type { requirements } => requirements.as_slice(),
                                _ => &[],
                            }),
                    )
            {
                let mapped = super::super::instantiate::requirement_with_self(
                    self.program,
                    self.proof,
                    requirement,
                    super::super::instantiate::DomainMapping {
                        source: *definition,
                        use_definition: caller,
                        arguments,
                        parent_call: None,
                    },
                    recipe,
                    limits,
                    check.left,
                )?;
                check.satisfy_requirement(caller, &mapped, 0)?;
            }
            Ok(())
        })();
        if !matches!(check.proof, ProofAccess::Borrowed(_)) {
            return Err(bad(
                "read-only original where unexpectedly changed its proof context",
            ));
        }
        result
    }
}
