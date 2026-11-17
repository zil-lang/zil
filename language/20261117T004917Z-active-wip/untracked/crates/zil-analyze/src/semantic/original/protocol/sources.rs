//! Owning source occurrences. The representation is private to this module:
//! users must handle both a whole value and every projected owning subplace.
use super::*;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct OwnerSources {
    whole: Option<u32>,
    projected: BTreeMap<Vec<PlaceProjection>, u32>,
    opaque: bool,
}
impl OwnerSources {
    pub(super) fn exact(role: u32) -> Self {
        Self { whole: Some(role), projected: BTreeMap::new(), opaque: false }
    }
    pub(super) fn has_any(&self) -> bool {
        self.opaque || self.has_exact()
    }
    pub(super) fn has_exact(&self) -> bool { self.whole.is_some() || !self.projected.is_empty() }
    pub(super) fn opaque(exposed: bool) -> Self { Self { opaque: exposed, ..Self::default() } }
    pub(super) fn opaque_exposure(&self) -> bool { self.opaque }
    pub(super) fn expose(&self) -> Self { Self::opaque(self.has_any()) }
    pub(super) fn include_opaque(&mut self, exposed: bool) { self.opaque |= exposed; }
    pub(super) fn only_exact_root(&self) -> Option<u32> {
        if self.projected.is_empty() { self.whole } else { None }
    }
    pub(super) fn charge(&self, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, WORK)?;
        for path in self.projected.keys() {
            b.spend(path.len().saturating_add(1), WORK)?;
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
        Ok(())
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
