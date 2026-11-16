//! Infer only the callee's fresh mixed variables through Task's full input
//! Context. Rigid classes, projection paths and nominal identities stay exact.
use super::*;

impl Check<'_, '_> {
    fn unify_task_life(
        &mut self,
        a: &SemanticLifeRecipe,
        b: &SemanticLifeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        self.unify_outer(
            &SemanticOuterLifeRecipe::Class(a.clone()),
            &SemanticOuterLifeRecipe::Class(b.clone()),
            formals,
            solutions,
            implicit,
            invocation,
            depth + 1,
        )
    }
    fn unify_task_sequence(
        &mut self,
        a: &SemanticClassSequenceRecipe,
        b: &SemanticClassSequenceRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(a.parts.len().saturating_add(b.parts.len()), depth)?;
        if a.parts.len() != b.parts.len() {
            return Err(bad(
                "original Task call changes its complete Context domain",
            ));
        }
        for (a, b) in a.parts.iter().zip(&b.parts) {
            match (a, b) {
                (SemanticClassSequencePart::One(a), SemanticClassSequencePart::One(b)) => {
                    self.unify_task_life(a, b, formals, solutions, implicit, invocation, depth + 1)?
                }
                (
                    SemanticClassSequencePart::AllClasses { ty: a, path: ap },
                    SemanticClassSequencePart::AllClasses { ty: b, path: bp },
                ) => {
                    self.pay(ap.len().saturating_add(bp.len()), depth)?;
                    if ap != bp {
                        return Err(bad(
                            "original Task call changes a complete-domain projection",
                        ));
                    }
                    self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                }
                _ => {
                    return Err(bad(
                        "original Task call changes a complete-domain expansion kind",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn unify_task_shape(
        &mut self,
        a: &SemanticRetainedShapeRecipe,
        b: &SemanticRetainedShapeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        let owner = self
            .contexts
            .last()
            .copied()
            .ok_or_else(|| bad("original Task inference has no lexical Gamma"))?;
        let a = self.normalized_task_shape(owner, a, depth + 1)?;
        let b = self.normalized_task_shape(owner, b, depth + 1)?;
        self.unify_task_sequence(
            &a.domain,
            &b.domain,
            formals,
            solutions,
            implicit,
            invocation,
            depth + 1,
        )?;
        self.pay(
            a.ports
                .len()
                .saturating_add(b.ports.len())
                .saturating_add(a.retained.len())
                .saturating_add(b.retained.len())
                .saturating_add(a.nominal.len())
                .saturating_add(b.nominal.len()),
            depth,
        )?;
        if a.ports.len() != b.ports.len()
            || a.retained.len() != b.retained.len()
            || a.nominal.len() != b.nominal.len()
        {
            return Err(bad(
                "original Task call changes its complete input Context inventory",
            ));
        }
        for (a, b) in a.ports.iter().zip(&b.ports) {
            match (a, b) {
                (
                    SemanticPortPart::One {
                        leaf: a,
                        mode: am,
                        class: ac,
                    },
                    SemanticPortPart::One {
                        leaf: b,
                        mode: bm,
                        class: bc,
                    },
                ) if am == bm => {
                    self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                    self.unify_task_life(
                        ac,
                        bc,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                (
                    SemanticPortPart::FromType { ty: a, classes: ac },
                    SemanticPortPart::FromType { ty: b, classes: bc },
                ) => {
                    self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                    self.unify_task_sequence(
                        ac,
                        bc,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                _ => {
                    return Err(bad(
                        "original Task call changes an input port kind or borrowing mode",
                    ));
                }
            }
        }
        for (a, b) in a.retained.iter().zip(&b.retained) {
            match (a, b) {
                (
                    SemanticRetainedPart::One {
                        path: ap,
                        child_class: ai,
                        parent: a,
                    },
                    SemanticRetainedPart::One {
                        path: bp,
                        child_class: bi,
                        parent: b,
                    },
                ) => {
                    self.pay(ap.len().saturating_add(bp.len()), depth)?;
                    if ap != bp || ai != bi {
                        return Err(bad(
                            "original Task call changes a retained child projection",
                        ));
                    }
                    self.unify_task_life(
                        a,
                        b,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                (
                    SemanticRetainedPart::All {
                        path: ap,
                        parents: a,
                    },
                    SemanticRetainedPart::All {
                        path: bp,
                        parents: b,
                    },
                ) => {
                    self.pay(ap.len().saturating_add(bp.len()), depth)?;
                    if ap != bp {
                        return Err(bad(
                            "original Task call changes a retained whole-domain projection",
                        ));
                    }
                    self.unify_task_sequence(
                        a,
                        b,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                _ => {
                    return Err(bad(
                        "original Task call changes a retained relation expansion kind",
                    ));
                }
            }
        }
        for (a, b) in a.nominal.iter().zip(&b.nominal) {
            match (a, b) {
                (
                    SemanticNominalPart::One {
                        path: ap,
                        declaration: ad,
                        argument: aa,
                        class: a,
                    },
                    SemanticNominalPart::One {
                        path: bp,
                        declaration: bd,
                        argument: ba,
                        class: b,
                    },
                ) => {
                    self.pay(ap.len().saturating_add(bp.len()), depth)?;
                    if ap != bp || ad != bd || aa != ba {
                        return Err(bad(
                            "original Task call changes a nominal identity or full ordinal",
                        ));
                    }
                    self.unify_task_life(
                        a,
                        b,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                (
                    SemanticNominalPart::FromType {
                        path: ap,
                        ty: a,
                        classes: ac,
                    },
                    SemanticNominalPart::FromType {
                        path: bp,
                        ty: b,
                        classes: bc,
                    },
                ) => {
                    self.pay(ap.len().saturating_add(bp.len()), depth)?;
                    if ap != bp {
                        return Err(bad("original Task call changes a nominal projection"));
                    }
                    self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                    self.unify_task_sequence(
                        ac,
                        bc,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
                _ => return Err(bad("original Task call changes a nominal expansion kind")),
            }
        }
        Ok(())
    }
}
