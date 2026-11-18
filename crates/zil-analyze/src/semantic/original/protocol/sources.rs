//! Owning source occurrences. The representation is private to this module:
//! users must handle both a whole value and every projected owning subplace.
use super::*;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct OwnerSources {
    whole: Option<u32>,
    projected: BTreeMap<Vec<PlaceProjection>, u32>,
    variants: BTreeMap<Vec<PlaceProjection>, ActiveVariant>,
    opaque: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ActiveVariant {
    pub(super) definition: SemanticDefinitionId,
    pub(super) tag: u32,
    pub(super) name: String,
}

impl OwnerSources {
    pub(super) fn exact(role: u32) -> Self {
        Self { whole: Some(role), ..Self::default() }
    }
    pub(super) fn has_any(&self) -> bool {
        self.opaque || self.has_exact()
    }
    pub(super) fn has_exact(&self) -> bool { self.whole.is_some() || !self.projected.is_empty() }
    pub(super) fn has_projected(&self) -> bool { !self.projected.is_empty() }
    pub(super) fn opaque(exposed: bool) -> Self { Self { opaque: exposed, ..Self::default() } }
    pub(super) fn opaque_exposure(&self) -> bool { self.opaque }
    pub(super) fn expose(&self) -> Self { Self::opaque(self.has_any()) }
    pub(super) fn include_opaque(&mut self, exposed: bool) { self.opaque |= exposed; }
    pub(super) fn with_opaque(mut self, exposed: bool) -> Self { self.include_opaque(exposed); self }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<()> {
        self.charge(b)?;
        other.charge(b)?;
        if self.whole != other.whole || self.projected != other.projected {
            return Err(unknown("original protocol alias join needs its complete possible-head relation"));
        }
        for path in self.variants.keys() {
            b.spend(path.len().saturating_add(1), WORK)?;
            b.lookup(other.variants.len(), WORK)?;
        }
        self.variants.retain(|path, tag| other.variants.get(path) == Some(tag));
        self.opaque |= other.opaque;
        Ok(())
    }
    pub(super) fn only_exact_root(&self) -> Option<u32> {
        if !self.opaque && self.projected.is_empty() { self.whole } else { None }
    }
    pub(super) fn receiver_with_effect(&self, state: &mut State, b: &mut Budget) -> CompileResult<u32> {
        let root = self.whole.filter(|_| self.projected.is_empty())
            .ok_or_else(|| unknown("original protocol receiver needs one independently established whole head"))?;
        state.invalidate(b)?;
        if self.opaque {
            for (role, phase) in &mut state.phases {
                b.spend(1, WORK)?;
                // The current receiver's affine identity and irreversible
                // private Closed gate were independently established. Opaque
                // access to other heads cannot certify their unchanged state.
                if *role != root && *phase != 0 { *phase = ANY; }
            }
        }
        Ok(root)
    }
    pub(super) fn retire_absent_success(&self, state: &mut State, b: &mut Budget) -> CompileResult<()> {
        // Bookkeeping on a selected error arm only removes permission; this is
        // not an exact-root capability query on the unselected whole carrier.
        state.invalidate(b)?;
        if let Some(root) = self.whole {
            b.lookup(state.phases.len(), WORK)?;
            state.phases.insert(root, 0);
        }
        Ok(())
    }
    pub(super) fn retire_consumed(&self, state: &mut State, b: &mut Budget) -> CompileResult<()> {
        // Removing the one actually consumed whole owner grants no access or
        // Closed fact. Projected aggregates owe their own complete contract.
        if self.projected.is_empty() { self.retire_absent_success(state, b)?; }
        Ok(())
    }
    pub(super) fn charge(&self, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        for path in self.projected.keys() {
            b.spend(path.len().saturating_add(1), WORK)?;
        }
        for (path, active) in &self.variants {
            b.spend(path.len().saturating_add(active.name.len()).saturating_add(1), WORK)?;
        }
        Ok(())
    }
    pub(super) fn contains(&self, role: u32, b: &mut Budget) -> CompileResult<bool> {
        self.charge(b)?;
        Ok(self.whole == Some(role) || self.projected.values().any(|source| *source == role))
    }
    fn insert(&mut self, path: Vec<PlaceProjection>, role: u32, b: &mut Budget) -> CompileResult<()> {
        b.spend(path.len().saturating_add(1), WORK)?;
        let previous = if path.is_empty() { self.whole.replace(role) } else {
            b.lookup(self.projected.len(), WORK)?;
            self.projected.insert(path, role)
        };
        if previous.is_some_and(|previous| previous != role) {
            return Err(fail("original owning place has competing source identities"));
        }
        Ok(())
    }
    pub(super) fn append_under(&mut self, prefix: &[PlaceProjection],
        input: &Self, b: &mut Budget) -> CompileResult<()> {
        self.opaque |= input.opaque;
        if let Some(role) = input.whole {
            b.spend(prefix.len(), WORK)?;
            self.insert(prefix.to_vec(), role, b)?;
        }
        for (path, role) in &input.projected {
            b.spend(prefix.len().saturating_add(path.len()), WORK)?;
            let mut full = prefix.to_vec();
            full.extend_from_slice(path);
            self.insert(full, *role, b)?;
        }
        for (path, active) in &input.variants {
            b.spend(prefix.len().saturating_add(path.len()).saturating_add(active.name.len()), WORK)?;
            let mut full = prefix.to_vec();
            full.extend_from_slice(path);
            self.insert_variant(full, active.clone(), b)?;
        }
        Ok(())
    }
    pub(super) fn set_variant(&mut self, active: ActiveVariant, b: &mut Budget) -> CompileResult<()> {
        self.insert_variant(Vec::new(), active, b)
    }
    fn insert_variant(&mut self, path: Vec<PlaceProjection>, active: ActiveVariant, b: &mut Budget) -> CompileResult<()> {
        b.spend(path.len().saturating_add(active.name.len()).saturating_add(1), WORK)?;
        b.lookup(self.variants.len(), WORK)?;
        if self.variants.insert(path, active.clone()).is_some_and(|previous| previous != active) {
            return Err(fail("original owning place has competing active variants"));
        }
        Ok(())
    }
    pub(super) fn active_variant(&self, b: &mut Budget) -> CompileResult<Option<&ActiveVariant>> {
        b.lookup(self.variants.len(), WORK)?;
        Ok(self.variants.get(&Vec::new()))
    }
    /// Only a stored subplace is projected. A whole owner cannot be guessed
    /// to contain another head, so its unmodelled exposure is returned too.
    pub(super) fn project(&self, step: PlaceProjection,
        b: &mut Budget) -> CompileResult<Self> {
        let mut output = Self::opaque(self.opaque || self.whole.is_some());
        for (path, role) in &self.projected {
            b.spend(path.len().saturating_add(1), WORK)?;
            if path.first() == Some(&step) {
                output.insert(path[1..].to_vec(), *role, b)?;
            }
        }
        for (path, active) in &self.variants {
            b.spend(path.len().saturating_add(active.name.len()).saturating_add(1), WORK)?;
            if path.first() == Some(&step) {
                output.insert_variant(path[1..].to_vec(), active.clone(), b)?;
            }
        }
        Ok(output)
    }
    /// Success's whole owner and failure's projected inputs coexist only as
    /// a tag-sensitive carrier; selecting either arm retains exactly its map.
    pub(super) fn error_payload(&self, is_error: bool, b: &mut Budget) -> CompileResult<Self> {
        let mut output = self.project(PlaceProjection::ErrorArm(is_error), b)?;
        // The success whole is absent on failure, not an opaque failure input.
        output.opaque = self.opaque;
        if !is_error && let Some(role) = self.whole {
            output.insert(Vec::new(), role, b)?;
        }
        Ok(output)
    }
}
