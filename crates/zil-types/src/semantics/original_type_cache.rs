//! Request-local memoization of complete original recipes. A digest selects a
//! bucket only: owner and full recipe equality remain mandatory on every hit.
use super::*;
type Key = (SemanticDefinitionId, zil_base::Fingerprint);

pub(super) struct Table<T> {
    buckets: BTreeMap<Key, Vec<(std::rc::Rc<SemanticTypeRecipe>, T)>>,
    entries: usize,
}
impl<T: Copy> Table<T> {
    pub(super) fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
            entries: 0,
        }
    }
    /// Intern a borrowed immutable recipe without first deep-copying a query
    /// result. Only a genuinely new entry owns a charged recipe snapshot.
    pub(super) fn intern(
        &mut self,
        owner: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        next: T,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(T, Option<std::rc::Rc<SemanticTypeRecipe>>)> {
        let (key, found) = self.lookup(owner, recipe, limits, left)?;
        if let Some(index) = found {
            Ok((index, None))
        } else {
            Ok((next, Some(self.insert(key, recipe, next, limits, left)?)))
        }
    }
    pub(super) fn lookup(
        &self,
        owner: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<T>)> {
        let key = (owner, digest(recipe, limits, left)?);
        fee(
            left,
            self.buckets.len().saturating_add(1).ilog2() as usize + 1,
        )?;
        if let Some(bucket) = self.buckets.get(&key) {
            fee(left, bucket.len())?;
            for (candidate, result) in bucket {
                // The query is already bounded by digest(). Equality only
                // visits paired fields, so one candidate walk pays the match.
                crate::dyn_metadata::semantic_operand_snapshot(
                    candidate.as_ref(),
                    limits,
                    left,
                    1,
                )?;
                if candidate.as_ref() == recipe {
                    return Ok((key, Some(*result)));
                }
            }
        }
        Ok((key, None))
    }
    pub(super) fn insert(
        &mut self,
        key: Key,
        recipe: &SemanticTypeRecipe,
        value: T,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::rc::Rc<SemanticTypeRecipe>> {
        if self.entries >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original type cache entry limit exceeded",
                None,
            ));
        }
        fee(
            left,
            (self.buckets.len().saturating_add(1).ilog2() as usize + 1)
                .saturating_add(std::mem::size_of::<Key>())
                .saturating_add(std::mem::size_of::<(std::rc::Rc<SemanticTypeRecipe>, T)>()),
        )?;
        let recipe = std::rc::Rc::new(copy_recipe(recipe, limits, left)?);
        self.buckets
            .entry(key)
            .or_default()
            .push((recipe.clone(), value));
        self.entries += 1;
        Ok(recipe)
    }
}
fn fee(left: &mut usize, amount: usize) -> R<()> {
    *left = left.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original type cache work exceeded", None)
    })?;
    Ok(())
}
// This private, request-local key includes complete derived-Hash structure.
// Scalar values use a self-delimiting integer encoding; byte/string payloads
// remain complete. Work is charged for every actual byte written, in addition
// to the caller's complete grammar/depth/node validation before hashing.
struct Sink<'a> {
    digest: &'a mut zil_base::FingerprintBuilder,
    left: &'a mut usize,
    exhausted: bool,
}
impl Sink<'_> {
    fn integer(&mut self, mut value: u128) {
        let mut bytes = [0u8; 19];
        let mut count = 0;
        loop {
            let mut byte = (value & 127) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 128;
            }
            bytes[count] = byte;
            count += 1;
            if value == 0 {
                break;
            }
        }
        std::hash::Hasher::write(self, &bytes[..count]);
    }
}
impl std::hash::Hasher for Sink<'_> {
    fn finish(&self) -> u64 {
        // Consumers use the full fingerprint, never Hasher's lossy finish.
        u64::from_le_bytes(
            self.digest.clone().digest().0[..8]
                .try_into()
                .expect("eight bytes"),
        )
    }
    fn write(&mut self, bytes: &[u8]) {
        if self.exhausted {
            return;
        }
        if let Some(left) = self.left.checked_sub(bytes.len()) {
            *self.left = left;
            self.digest.update(bytes);
        } else {
            self.exhausted = true;
        }
    }
    fn write_u8(&mut self, value: u8) {
        self.integer(value as u128);
    }
    fn write_u16(&mut self, value: u16) {
        self.integer(value as u128);
    }
    fn write_u32(&mut self, value: u32) {
        self.integer(value as u128);
    }
    fn write_u64(&mut self, value: u64) {
        self.integer(value as u128);
    }
    fn write_u128(&mut self, value: u128) {
        self.integer(value as u128);
    }
    fn write_usize(&mut self, value: usize) {
        self.integer(value as u128);
    }
    fn write_i8(&mut self, value: i8) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
    fn write_i16(&mut self, value: i16) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
    fn write_i32(&mut self, value: i32) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
    fn write_i64(&mut self, value: i64) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
    fn write_i128(&mut self, value: i128) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
    fn write_isize(&mut self, value: isize) {
        let signed = value as i128;
        self.integer(((signed as u128) << 1) ^ ((signed >> 127) as u128));
    }
}
pub(in crate::semantics) fn hash_into(
    digest: &mut zil_base::FingerprintBuilder,
    left: &mut usize,
    value: &impl std::hash::Hash,
) -> R<()> {
    let mut sink = Sink {
        digest,
        left,
        exhausted: false,
    };
    std::hash::Hash::hash(value, &mut sink);
    if sink.exhausted {
        return Err(Diagnostic::limit(
            Phase::Types,
            "original semantic query digest budget exceeded",
            None,
        ));
    }
    Ok(())
}
pub(super) fn digest(
    recipe: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<zil_base::Fingerprint> {
    // Hash every complete recipe field after its actual depth/node bounds
    // have been checked. The digest selects a bucket, never a theorem.
    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
    let mut digest = zil_base::FingerprintBuilder::new();
    hash_into(&mut digest, left, recipe)?;
    Ok(digest.digest())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_integer_keys_keep_high_bits_sign_and_actual_byte_cost() {
        fn check(value: &impl std::hash::Hash, bytes: &[u8]) {
            let mut left = 100;
            let mut digest = zil_base::FingerprintBuilder::new();
            hash_into(&mut digest, &mut left, value).unwrap();
            assert_eq!(digest.digest(), zil_base::Fingerprint::of(bytes));
            assert_eq!(left, 100 - bytes.len());
        }
        check(&0u128, &[0]);
        check(&127u64, &[127]);
        check(&128u64, &[128, 1]);
        check(&-1i128, &[1]);
        let mut largest = [255; 19];
        largest[18] = 3;
        check(&u128::MAX, &largest);
        check(&i128::MIN, &largest);
        largest[0] = 254;
        check(&i128::MAX, &largest);
    }
    fn owner(index: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    #[test]
    fn colliding_bucket_does_not_supply_another_recipes_cleanup_fact() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let expected = SemanticTypeRecipe::Atom(Type::I32);
        let different = SemanticTypeRecipe::Atom(Type::I64);
        let mut table = Table::<bool>::new();
        let (key, _) = table
            .lookup(owner(0), &expected, &limits, &mut left)
            .unwrap();
        table
            .insert(key, &different, true, &limits, &mut left)
            .unwrap();
        assert_eq!(
            table
                .lookup(owner(0), &expected, &limits, &mut left)
                .unwrap()
                .1,
            None
        );
    }
    #[test]
    fn same_ordinal_in_another_definition_cannot_reuse_a_capability() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let recipe = SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        let mut table = Table::<bool>::new();
        let (key, _) = table.lookup(owner(0), &recipe, &limits, &mut left).unwrap();
        table
            .insert(key, &recipe, true, &limits, &mut left)
            .unwrap();
        assert_eq!(
            table
                .lookup(owner(1), &recipe, &limits, &mut left)
                .unwrap()
                .1,
            None
        );
        assert_eq!(
            table
                .lookup(owner(0), &recipe, &limits, &mut left)
                .unwrap()
                .1,
            Some(true)
        );
    }
}
