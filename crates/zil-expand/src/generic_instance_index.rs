//! Request-local lookup coordinates for generic templates and actual vectors.
//! Hashes select buckets only; every hit compares the complete original key.
use super::*;
use std::cmp::Ordering;

type Key = (String, Vec<String>);
type Row = (Key, String);

fn compare(left: &str, right: &str, budget: &mut Budget<'_>, span: Span) -> R<Ordering> {
    for (a, b) in left.bytes().zip(right.bytes()) {
        budget.identity_work(1, span)?;
        match a.cmp(&b) {
            Ordering::Equal => {}
            order => return Ok(order),
        }
    }
    budget.identity_work(1, span)?;
    Ok(left.len().cmp(&right.len()))
}

#[derive(Default)]
pub(super) struct Templates<'a> {
    items: Vec<&'a Item>,
}
impl<'a> Templates<'a> {
    pub(super) fn new(
        source: &BTreeMap<String, &'a Item>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Self> {
        let bytes = source
            .len()
            .checked_mul(std::mem::size_of::<&Item>())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "template directory allocation overflow",
                    Some(span),
                )
            })?;
        budget.lineage_bytes(bytes, span)?;
        let mut items = Vec::new();
        items.try_reserve_exact(source.len()).map_err(|_| {
            Diagnostic::limit(
                Phase::Expand,
                "template directory allocation failed",
                Some(span),
            )
        })?;
        for (key, item) in source {
            budget.identity_work(1, span)?;
            if compare(
                key,
                name(item).ok_or_else(|| error(span, "template name absent"))?,
                budget,
                span,
            )? != Ordering::Equal
            {
                return Err(error(
                    span,
                    "template directory key differs from declaration",
                ));
            }
            items.push(*item);
        }
        Ok(Self { items })
    }
    pub(super) fn get(
        &self,
        key: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<&'a Item>> {
        let (mut lo, mut hi) = (0, self.items.len());
        while lo < hi {
            budget.identity_work(1, span)?;
            let mid = lo + (hi - lo) / 2;
            let item = self.items[mid];
            match compare(
                name(item).expect("checked template directory"),
                key,
                budget,
                span,
            )? {
                Ordering::Less => lo = mid + 1,
                Ordering::Greater => hi = mid,
                Ordering::Equal => return Ok(Some(item)),
            }
        }
        Ok(None)
    }
}

#[derive(Default)]
pub(super) struct Instances {
    buckets: BTreeMap<u64, Vec<Row>>,
    len: usize,
}
fn fingerprint(key: &Key, budget: &mut Budget<'_>, span: Span) -> R<u64> {
    let mut hash = 0xcbf29ce484222325u64;
    for part in std::iter::once(&key.0).chain(&key.1) {
        // Boundaries and ordinal order participate; the exact comparison below
        // remains decisive even if any two complete vectors collide.
        for byte in (part.len() as u64)
            .to_le_bytes()
            .into_iter()
            .chain(part.bytes())
        {
            budget.identity_work(2, span)?;
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    Ok(hash)
}
fn same(left: &Key, right: &Key, budget: &mut Budget<'_>, span: Span) -> R<bool> {
    budget.identity_work(1, span)?;
    if left.1.len() != right.1.len() || compare(&left.0, &right.0, budget, span)? != Ordering::Equal
    {
        return Ok(false);
    }
    for (a, b) in left.1.iter().zip(&right.1) {
        if compare(a, b, budget, span)? != Ordering::Equal {
            return Ok(false);
        }
    }
    Ok(true)
}
impl Instances {
    pub(super) fn len(&self) -> usize {
        self.len
    }
    fn bucket_fee(&self, budget: &mut Budget<'_>, span: Span) -> R<()> {
        budget.identity_work(
            self.buckets
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2)
                .saturating_mul(12),
            span,
        )
    }
    pub(super) fn get(&self, key: &Key, budget: &mut Budget<'_>, span: Span) -> R<Option<&String>> {
        let hash = fingerprint(key, budget, span)?;
        self.bucket_fee(budget, span)?;
        if let Some(rows) = self.buckets.get(&hash) {
            for (candidate, value) in rows {
                if same(candidate, key, budget, span)? {
                    return Ok(Some(value));
                }
            }
        }
        Ok(None)
    }
    pub(super) fn insert(
        &mut self,
        key: Key,
        value: String,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<String>> {
        let hash = fingerprint(&key, budget, span)?;
        self.bucket_fee(budget, span)?;
        if let Some(rows) = self.buckets.get_mut(&hash) {
            for (candidate, existing) in rows.iter_mut() {
                if same(candidate, &key, budget, span)? {
                    return Ok(Some(std::mem::replace(existing, value)));
                }
            }
            budget.lineage_push(rows, (key, value), span)?;
        } else {
            // Amortized retained storage: every nonempty B-tree node owns
            // at least one distinct bucket key. Reserve one full internal
            // node per new bucket, covering all live nodes and root splits.
            // The search and insertion work remain separately metered.
            let bytes =
                12 * (std::mem::size_of::<(u64, Vec<Row>)>() + std::mem::size_of::<usize>()) + 64;
            budget.lineage_bytes(bytes, span)?;
            let mut rows = Vec::new();
            budget.lineage_push(&mut rows, (key, value), span)?;
            self.bucket_fee(budget, span)?;
            self.buckets.insert(hash, rows);
        }
        self.len += 1;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorted_template_lookup_keeps_original_item_identity_and_absence() {
        use zil_base::{FileId, ModuleId, SourceUnit};
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "directory.zil",
            "fn z<T>() {}\nfn a<T>() {}\nfn middle<T>() {}\n",
        );
        let module = zil_syntax::parse(&source).into_result().unwrap();
        let map = module
            .items
            .iter()
            .map(|item| (name(item).unwrap().to_owned(), item))
            .collect();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let directory = Templates::new(&map, &mut budget, module.span).unwrap();
        for item in &module.items {
            let actual = directory
                .get(name(item).unwrap(), &mut budget, module.span)
                .unwrap()
                .unwrap();
            assert!(std::ptr::eq(actual, item));
        }
        for missing in ["", "aa", "middle2", "zz"] {
            assert!(
                directory
                    .get(missing, &mut budget, module.span)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn complete_keys_and_colliding_buckets_keep_distinct_instances() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let key = ("F".to_owned(), vec!["T".to_owned(), "owner:1".to_owned()]);
        let other = ("F".to_owned(), vec!["T".to_owned(), "owner:2".to_owned()]);
        let mut index = Instances::default();
        let hash = fingerprint(&key, &mut budget, Span::default()).unwrap();
        // Force the adversarial collision independently of the hash function.
        index
            .buckets
            .insert(hash, vec![(other.clone(), "other".into())]);
        index.len = 1;
        assert!(
            index
                .get(&key, &mut budget, Span::default())
                .unwrap()
                .is_none()
        );
        index
            .insert(key.clone(), "first".into(), &mut budget, Span::default())
            .unwrap();
        assert_eq!(
            index
                .get(&key, &mut budget, Span::default())
                .unwrap()
                .map(String::as_str),
            Some("first")
        );
        assert_eq!(index.len(), 2);
        assert_eq!(
            index
                .insert(
                    key.clone(),
                    "replacement".into(),
                    &mut budget,
                    Span::default()
                )
                .unwrap(),
            Some("first".into())
        );
        assert_eq!(index.len(), 2);
        let rows = &index.buckets[&hash];
        assert_eq!(rows[0], (other, "other".into()));
        let swapped = ("F".to_owned(), vec!["owner:1".to_owned(), "T".to_owned()]);
        assert!(
            index
                .get(&swapped, &mut budget, Span::default())
                .unwrap()
                .is_none()
        );
        let short = Limits {
            max_analysis_steps: 1,
            ..limits
        };
        assert!(
            index
                .get(&key, &mut Budget::new(&short), Span::default())
                .is_err()
        );
    }
}
