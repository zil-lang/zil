//! Lossless private conversion between the shared V semantic domain and one W
//! lexical domain. Public binder numbers never become semantic identities.
use super::*;
use crate::semantic::api::{
    CheckedType, ClassKey, FrameToken, IntrinsicOrigin, OperandKey, ScopeKey,
};

pub(super) struct Symbols {
    owner: terms::Owner,
    token: FrameToken,
    classes: Vec<(ClassKey, terms::LifeTerm)>,
    origins: Vec<(IntrinsicOrigin, terms::Origin)>,
}
impl Symbols {
    pub(super) fn new(owner: terms::Owner, token: FrameToken) -> Self {
        Self {
            owner,
            token,
            classes: vec![],
            origins: vec![],
        }
    }
    /// Bind a key established by the actual W/lexical input. A shared outside
    /// image does not authorize replacing either source's intrinsic origin.
    pub(super) fn bind(
        &mut self,
        key: ClassKey,
        term: terms::LifeTerm,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.classes.len().saturating_add(1))?;
        if let Some((_, known)) = self.classes.iter().find(|(candidate, _)| candidate == &key) {
            if *known != term {
                return Err(bad("one semantic class has inconsistent actual W bindings"));
            }
        } else {
            self.classes.push((key, term));
        }
        Ok(())
    }
    pub(super) fn class(&self, key: &ClassKey, b: &mut Budget) -> Check<terms::LifeTerm> {
        b.spend(self.classes.len())?;
        self.classes
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, term)| *term)
            .ok_or_else(|| bad("semantic class has no established actual W lexical binding"))
    }
    pub(super) fn life(
        &mut self,
        key: ClassKey,
        term: terms::LifeTerm,
        b: &mut Budget,
    ) -> Check<ClassKey> {
        b.spend(self.classes.len())?;
        if let Some((known, _)) = self.classes.iter().find(|(_, known)| *known == term) {
            return Ok(known.clone());
        }
        self.bind(key.clone(), term, b)?;
        Ok(key)
    }
    pub(super) fn outgoing(
        &mut self,
        value: &terms::Value,
        scope: ScopeKey,
        operand: OperandKey,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        dynamic_metadata::recipe(&value.raw, b)?;
        if value.terms.len() != value.life.binder_count as usize
            || value.classes.len() != value.terms.len()
        {
            return Err(bad(
                "outgoing semantic value loses its complete intrinsic domain",
            ));
        }
        b.spend(value.terms.len().saturating_add(value.terms.len()))?;
        let mut classes = Vec::with_capacity(value.terms.len());
        let mut origins = Vec::with_capacity(value.terms.len());
        for (class, term) in value.terms.iter().enumerate() {
            let origin = IntrinsicOrigin::Existing {
                scope,
                operand: operand.clone(),
                path: vec![],
                class: class as u32,
            };
            let key = ClassKey::Intrinsic {
                scope,
                operand: operand.clone(),
                path: vec![],
                class: class as u32,
            };
            if matches!(operand, OperandKey::TypeArgument { .. }) {
                // A callee type actual anchors each intrinsic class in its
                // own full formal ordinal; it never reads its caller images.
                self.bind(key.clone(), *term, b)?;
                classes.push(key);
            } else {
                classes.push(self.life(key, *term, b)?);
            }
            b.spend(self.origins.len().saturating_add(1))?;
            if let Some((_, known)) = self
                .origins
                .iter()
                .find(|(candidate, _)| candidate == &origin)
            {
                if *known != term.origin {
                    return Err(bad(
                        "one existing semantic occurrence has different intrinsic origins",
                    ));
                }
            } else {
                self.origins.push((origin.clone(), term.origin));
            }
            origins.push(origin);
        }
        Ok(CheckedType {
            ty: value.raw.ty.clone(),
            life: value.life.clone(),
            classes,
            origins,
            statics: vec![],
        })
    }
    pub(super) fn incoming(&mut self, value: &CheckedType, b: &mut Budget) -> Check<terms::Value> {
        if !value.statics.is_empty() {
            return Err(bad(
                "semantic value retains pending static occurrences requiring exact CFG requests",
            ));
        }
        self.incoming_value(value, b)
    }
    /// Preserve static occurrences alongside the term conversion. The caller
    /// must retain these in its executable request; this grants no provenance.
    pub(super) fn incoming_pending(
        &mut self,
        value: &CheckedType,
        b: &mut Budget,
    ) -> Check<(terms::Value, Vec<crate::semantic::StaticOccurrence>)> {
        b.spend(value.statics.len())?;
        for occurrence in &value.statics {
            use crate::semantic::StaticOccurrence;
            match occurrence {
                StaticOccurrence::Direct { path, leaf, .. } => {
                    b.spend(path.len())?;
                    dynamic_metadata::ty(leaf, b)?;
                }
                StaticOccurrence::Logical { position, leaf, .. } => {
                    b.spend(position.path.len())?;
                    dynamic_metadata::ty(leaf, b)?;
                }
            }
        }
        Ok((self.incoming_value(value, b)?, value.statics.clone()))
    }
    fn incoming_value(&mut self, value: &CheckedType, b: &mut Budget) -> Check<terms::Value> {
        dynamic_metadata::ty(&value.ty, b)?;
        let count = value.life.binder_count as usize;
        if count > b.limits.max_nodes
            || value.classes.len() != count
            || value.origins.len() != count
        {
            return Err(bad(
                "incoming semantic value omits an intrinsic origin or class",
            ));
        }
        b.spend(
            count
                .saturating_add(value.life.slots.len())
                .saturating_add(value.life.retained.len())
                .saturating_add(value.life.nominal_arguments.len()),
        )?;
        for row in &value.life.slots {
            b.spend(row.path.len())?;
        }
        for row in &value.life.retained {
            b.spend(row.path.len())?;
        }
        for row in &value.life.nominal_arguments {
            b.spend(row.path.len())?;
        }
        let mut terms = Vec::with_capacity(count);
        for (class, origin) in value.classes.iter().zip(&value.origins) {
            let image = self.class(class, b)?.image;
            let origin = match origin {
                IntrinsicOrigin::DerivedClass { source } => self.class(source, b)?.origin,
                IntrinsicOrigin::Existing { .. } => {
                    b.spend(self.origins.len().saturating_add(1))?;
                    if let Some((_, known)) = self
                        .origins
                        .iter()
                        .find(|(candidate, _)| candidate == origin)
                    {
                        *known
                    } else {
                        let index = u32::try_from(self.origins.len())
                            .map_err(|_| limit("semantic intrinsic origin table exceeded"))?;
                        let result = terms::Origin {
                            owner: self.owner,
                            root: terms::Root::Semantic(self.token.0, index),
                            class: 0,
                        };
                        self.origins.push((origin.clone(), result));
                        result
                    }
                }
            };
            terms.push(terms::LifeTerm { origin, image });
        }
        Ok(terms::Value {
            raw: DynTypeValue {
                ty: value.ty.clone(),
                life: value.life.clone(),
                classes: terms.iter().map(|term| term.image).collect(),
            },
            terms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_two_class_type_survives_equal_images_while_a_derived_type_can_have_one_class() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let id = LifeDeclarationId {
            module: zil_base::ModuleId(0),
            declaration: zil_base::NodeId(950),
            compiler: false,
        };
        let life = StorageLifeShape {
            binder_count: 2,
            slots: vec![],
            retained: vec![],
            nominal_arguments: (0..2)
                .map(|class| NominalLifeArgument {
                    path: vec![],
                    declaration: id,
                    argument: class,
                    binder: class,
                })
                .collect(),
        };
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::BorrowedStorage {
                    storage: Box::new(Type::Record {
                        name: "symbols.Pair".into(),
                        fields: vec![("code".into(), Type::I32)],
                    }),
                    life: life.clone(),
                },
                life,
                classes: vec![StorageBinder::Fixed(0); 2],
            },
            terms::Owner::Function(FunctionId(0)),
            terms::Root::Receiver,
            &mut budget,
        )
        .unwrap();
        let scope = ScopeKey::Witness {
            definition: SemanticDefinitionId {
                module: zil_base::ModuleId(0),
                index: 1,
            },
            frame: FrameToken(7),
        };
        let operand = OperandKey::TypeArgument { scope, ordinal: 3 };
        let mut symbols = Symbols::new(terms::Owner::Function(FunctionId(0)), FrameToken(8));
        let mut checked = symbols
            .outgoing(&value, scope, operand, &mut budget)
            .unwrap();
        assert_ne!(checked.classes[0], checked.classes[1]);
        checked.classes[1] = checked.classes[0].clone();
        let roundtrip = symbols.incoming(&checked, &mut budget).unwrap();
        assert_eq!(roundtrip.raw.ty, value.raw.ty);
        assert_eq!(roundtrip.life.binder_count, 2);
        assert_ne!(roundtrip.terms[0].origin, roundtrip.terms[1].origin);
        let mut derived = checked.clone();
        derived.life.binder_count = 1;
        for row in &mut derived.life.nominal_arguments {
            row.binder = 0;
        }
        if let Type::BorrowedStorage { life, .. } = &mut derived.ty {
            *life = derived.life.clone();
        }
        derived.classes.truncate(1);
        derived.origins = vec![IntrinsicOrigin::DerivedClass {
            source: derived.classes[0].clone(),
        }];
        assert_eq!(
            symbols
                .incoming(&derived, &mut budget)
                .unwrap()
                .life
                .binder_count,
            1
        );
        let mut unknown = checked;
        unknown.classes[0] = ClassKey::Formal { scope, ordinal: 99 };
        assert!(symbols.incoming(&unknown, &mut budget).is_err());
    }
}
