//! Private substitution identities anchored in complete actual type domains.
//! An outer class image is never an origin or an equality certificate.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    Function(FunctionId),
    Witness(DynVtableId, u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Root {
    Receiver,
    DeclarationType(u32),
    MethodType(u32),
    ApplicationType(u32),
    Lexical(StorageBinder),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Origin {
    pub owner: Owner,
    pub root: Root,
    pub class: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LifeTerm {
    pub origin: Origin,
    pub image: StorageBinder,
}
#[derive(Clone, Debug)]
pub(super) struct Value {
    pub raw: DynTypeValue,
    pub terms: Vec<LifeTerm>,
}
impl std::ops::Deref for Value {
    type Target = DynTypeValue;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl Value {
    pub(super) fn anchored(
        raw: &DynTypeValue,
        owner: Owner,
        root: Root,
        b: &mut Budget,
    ) -> Check<Self> {
        dynamic_metadata::recipe(raw, b)?;
        if raw.classes.len() != raw.life.binder_count as usize {
            return Err(bad("intrinsic term scope omits a class occurrence"));
        }
        b.spend(raw.classes.len())?;
        let canonical = external::canonical_indices(raw, b)?;
        let terms = raw
            .classes
            .iter()
            .enumerate()
            .map(|(class, image)| LifeTerm {
                origin: Origin {
                    owner,
                    root,
                    class: canonical[class],
                },
                image: *image,
            })
            .collect();
        Ok(Self {
            raw: raw.clone(),
            terms,
        })
    }
    pub(super) fn term(&self, class: u32, b: &mut Budget) -> Check<LifeTerm> {
        b.spend(1)?;
        self.terms
            .get(class as usize)
            .copied()
            .ok_or_else(|| bad("actual intrinsic occurrence has no complete origin term"))
    }
}
#[derive(Clone, Default, Debug)]
pub(super) struct Substitution {
    lives: Vec<(DynLifePattern, LifeTerm)>,
    types: Vec<(bool, u32, Value)>,
}
impl Substitution {
    pub(super) fn bind(
        &mut self,
        formal: DynLifePattern,
        term: LifeTerm,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.lives.len().saturating_add(1))?;
        if let Some((_, previous)) = self.lives.iter().find(|(f, _)| *f == formal) {
            if previous != &term {
                return Err(bad(
                    "one original lifetime formal has different intrinsic origins",
                ));
            }
        } else {
            self.lives.push((formal, term));
        }
        Ok(())
    }
    /// Change only outside images after the complete original scoped match.
    /// Intrinsic origins, whole types and their class partitions are immutable.
    pub(super) fn map_images(
        &mut self,
        mut map: impl FnMut(StorageBinder, &mut Budget) -> Check<StorageBinder>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.lives.len().saturating_add(self.types.len()))?;
        for (_, term) in &mut self.lives {
            term.image = map(term.image, b)?;
        }
        for (_, _, value) in &mut self.types {
            dynamic_metadata::recipe(&value.raw, b)?;
            b.spend(value.terms.len())?;
            for image in &mut value.raw.classes {
                *image = map(*image, b)?;
            }
            for term in &mut value.terms {
                term.image = map(term.image, b)?;
            }
        }
        Ok(())
    }
    pub(super) fn optional_life(
        &self,
        formal: DynLifePattern,
        b: &mut Budget,
    ) -> Check<Option<LifeTerm>> {
        b.spend(self.lives.len().saturating_add(1))?;
        Ok(self
            .lives
            .iter()
            .find(|(f, _)| *f == formal)
            .map(|(_, t)| *t))
    }
    pub(super) fn life(&self, formal: DynLifePattern, b: &mut Budget) -> Check<LifeTerm> {
        b.spend(self.lives.len().saturating_add(1))?;
        self.lives
            .iter()
            .find(|(f, _)| *f == formal)
            .map(|(_, t)| *t)
            .ok_or_else(|| bad("lifetime substitution has no actual intrinsic occurrence anchor"))
    }
    pub(super) fn type_actual(
        &self,
        method: bool,
        index: u32,
        b: &mut Budget,
    ) -> Check<Option<&Value>> {
        b.spend(self.types.len().saturating_add(1))?;
        Ok(self
            .types
            .iter()
            .find(|(m, i, _)| *m == method && *i == index)
            .map(|(_, _, v)| v))
    }
    pub(super) fn bind_type(
        &mut self,
        method: bool,
        index: u32,
        value: &Value,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(
            self.types
                .len()
                .saturating_add(value.terms.len())
                .saturating_add(1),
        )?;
        if let Some((_, _, previous)) = self
            .types
            .iter()
            .find(|(m, i, _)| *m == method && *i == index)
        {
            dynamic_metadata::recipe(&previous.raw, b)?;
            dynamic_metadata::recipe(&value.raw, b)?;
            if !external::terms_equal(previous, value, b)? {
                return Err(bad(
                    "one original Type formal has different complete intrinsic substitutions",
                ));
            }
        } else {
            dynamic_metadata::recipe(&value.raw, b)?;
            self.types.push((method, index, value.clone()));
        }
        Ok(())
    }
}
/// Compute a fresh local partition from established terms, never their images.
/// This renumbers a newly constructed value, not any existing complete Type.
pub(super) fn partition(terms: &[LifeTerm], b: &mut Budget) -> Check<(Vec<u32>, Vec<LifeTerm>)> {
    b.spend(terms.len())?;
    let mut unique: Vec<LifeTerm> = vec![];
    let mut classes = Vec::with_capacity(terms.len());
    for term in terms {
        b.spend(unique.len().saturating_add(1))?;
        if let Some(i) = unique.iter().position(|old| old.origin == term.origin) {
            if unique[i].image != term.image {
                return Err(bad("one intrinsic origin has inconsistent lexical images"));
            }
            classes.push(i as u32);
        } else {
            classes.push(unique.len() as u32);
            unique.push(*term);
        }
    }
    Ok((classes, unique))
}
