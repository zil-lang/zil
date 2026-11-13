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
            classes.push(self.life(key, *term, b)?);
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
        // Static metadata is an actual-source obligation, not a removable
        // class. Until the shared request can carry each occurrence, reject.
        if !value.statics.is_empty() {
            return Err(bad(
                "semantic value retains pending static occurrences requiring exact CFG requests",
            ));
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
