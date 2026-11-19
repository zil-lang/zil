//! A static shape match permits the boundary to compare origins and classes
//! of the same two borrowed frames. Candidate incompatibility is decided before
//! installing any such map, so candidate order supplies no semantic evidence.
use super::*;

pub(super) struct Frames<'a> {
    actual: &'a EvalEnv,
    expected: &'a EvalEnv,
}

pub(in crate::semantic::flow) struct ValuePair<'a> {
    actual: &'a CheckedType,
    expected: &'a CheckedType,
}
impl<'a> ValuePair<'a> {
    pub(in crate::semantic::flow) fn into_parts(self) -> (&'a CheckedType, &'a CheckedType) {
        (self.actual, self.expected)
    }
}

fn same_shape(a: &CheckedType, z: &CheckedType, b: &mut Budget) -> Check<bool> {
    if !a.same_raw_type(z, b)? {
        return Ok(false);
    }
    b.spend(1)?;
    if a.classes.len() != z.classes.len()
        || a.origins.len() != z.origins.len()
        || a.statics.len() != z.statics.len()
        || a.life.binder_count != z.life.binder_count
        || a.life.slots.len() != z.life.slots.len()
        || a.life.retained.len() != z.life.retained.len()
        || a.life.nominal_arguments.len() != z.life.nominal_arguments.len()
    {
        return Ok(false);
    }
    if !a.life.slots.is_empty()
        || !a.life.retained.is_empty()
        || !a.life.nominal_arguments.is_empty()
    {
        dynamic_metadata::life_shape(&a.life, b)?;
        dynamic_metadata::life_shape(&z.life, b)?;
        if a.life != z.life {
            return Ok(false);
        }
    }
    for s in a.statics.iter().chain(&z.statics) {
        b.spend(1)?;
        match s {
            StaticOccurrence::Direct { path, leaf, .. } => {
                b.spend(path.len())?;
                dynamic_metadata::ty(leaf, b)?;
            }
            StaticOccurrence::Logical { position, leaf, .. } => {
                dynamic_metadata::position(position, b)?;
                dynamic_metadata::ty(leaf, b)?;
            }
        }
    }
    Ok(a.statics == z.statics)
}

impl<'a> Frames<'a> {
    pub(super) fn checked(
        actual: &'a EvalEnv,
        expected: &'a EvalEnv,
        b: &mut Budget,
    ) -> Check<Option<Self>> {
        if actual.lexical.len() != expected.lexical.len() {
            return Ok(None);
        }
        for (a, z) in actual.lexical.iter().zip(&expected.lexical) {
            b.spend(
                a.actuals
                    .len()
                    .saturating_add(a.environment.len())
                    .saturating_add(1),
            )?;
            if a.definition != z.definition
                || a.actuals.len() != z.actuals.len()
                || a.environment.len() != z.environment.len()
            {
                return Ok(None);
            }
            for (a, z) in a
                .actuals
                .iter()
                .zip(&z.actuals)
                .chain(a.environment.iter().zip(&z.environment))
            {
                let same = match (a, z) {
                    (CheckedActual::Life(_), CheckedActual::Life(_)) => true,
                    (CheckedActual::Type(a), CheckedActual::Type(z)) => same_shape(a, z, b)?,
                    (
                        CheckedActual::Const {
                            carrier: a,
                            value: x,
                        },
                        CheckedActual::Const {
                            carrier: z,
                            value: y,
                        },
                    ) => {
                        dynamic_metadata::constant(x, b)?;
                        dynamic_metadata::constant(y, b)?;
                        x == y && same_shape(a, z, b)?
                    }
                    _ => false,
                };
                if !same {
                    return Ok(None);
                }
            }
            match (&a.self_type, &z.self_type) {
                (None, None) => {}
                (Some(a), Some(z)) if same_shape(a, z, b)? => {}
                _ => return Ok(None),
            }
        }
        Ok(Some(Self { actual, expected }))
    }

    pub(super) fn boundary(
        self,
        context: &CheckedContext,
        caller: &EvalEnv,
        b: &mut Budget,
    ) -> Check<trait_members::Boundary> {
        let mut result = trait_members::Boundary::default();
        for (a, z) in self.actual.lexical.iter().zip(&self.expected.lexical) {
            result.scope(a.key, z.key, b)?;
            for (a, z) in a
                .actuals
                .iter()
                .zip(&z.actuals)
                .chain(a.environment.iter().zip(&z.environment))
            {
                match (a, z) {
                    (CheckedActual::Life(a), CheckedActual::Life(z)) => {
                        result.class(context, caller, a, z, b)?
                    }
                    (CheckedActual::Type(actual), CheckedActual::Type(expected))
                    | (
                        CheckedActual::Const {
                            carrier: actual, ..
                        },
                        CheckedActual::Const {
                            carrier: expected, ..
                        },
                    ) => {
                        result.matched_value(context, caller, ValuePair { actual, expected }, b)?
                    }
                    _ => unreachable!("borrowed formal kinds were matched before binding"),
                }
            }
            if let (Some(actual), Some(expected)) = (&a.self_type, &z.self_type) {
                result.matched_value(context, caller, ValuePair { actual, expected }, b)?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn candidate_shape_and_joint_origin_mapping_are_checked_separately() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../fixtures/ordinary-identity-complete-call-sites.wire"),
            &limits,
        )
        .unwrap();
        let definition = module.semantic.as_ref().unwrap().instances[0].definition;
        let env = |index, origins: [u32; 2], wrong_type| {
            let key = ScopeKey::Instance(SemanticInstanceId {
                module: zil_base::ModuleId(1),
                index,
            });
            let actuals = origins
                .iter()
                .map(|id| {
                    let inner = if wrong_type { Type::U64 } else { Type::I32 };
                    let ty = Type::Address {
                        mutable: false,
                        inner: Box::new(inner),
                    };
                    let head = ClassKey::Implicit {
                        scope: key,
                        ordinal: 0,
                    };
                    CheckedActual::Type(CheckedType {
                        type_cell: CheckedTypeData::new(ty),
                        life: StorageLifeShape {
                            binder_count: 1,
                            slots: vec![StorageLifeSlot {
                                path: vec![],
                                binder: 0,
                                mode: StoredBorrowMode::SharedAddress,
                            }],
                            retained: vec![],
                            nominal_arguments: vec![],
                        },
                        classes: vec![head],
                        origins: vec![IntrinsicOrigin::Existing {
                            scope: key,
                            operand: OperandKey::Value(ValueId(*id)),
                            path: vec![],
                            class: 0,
                        }],
                        statics: vec![],
                    })
                })
                .collect();
            EvalEnv {
                lexical: vec![Rc::new(LexicalFrame {
                    key,
                    definition,
                    actuals,
                    environment: vec![],
                    self_type: None,
                })],
                assumptions: vec![key],
                images: vec![],
                bounds: vec![],
                invocations: vec![],
                at: None,
            }
        };
        let a = env(1, [3, 3], false);
        let same = env(2, [4, 4], false);
        let permuted = env(2, [4, 5], false);
        let other = env(2, [4, 4], true);
        let mut b = Budget::new(&limits);
        let context = CheckedContext::catalogue(&module, &mut b).unwrap();
        assert!(Frames::checked(&a, &other, &mut b).unwrap().is_none());
        Frames::checked(&a, &same, &mut b)
            .unwrap()
            .unwrap()
            .boundary(&context, &same, &mut b)
            .unwrap();
        let error = Frames::checked(&a, &permuted, &mut b)
            .unwrap()
            .unwrap()
            .boundary(&context, &permuted, &mut b)
            .err()
            .unwrap();
        assert!(
            error
                .message
                .contains("permutes a repeated intrinsic origin")
        );
        b.limits.max_nodes = 1;
        let error = Frames::checked(&a, &other, &mut b).err().unwrap();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }
}
