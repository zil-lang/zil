//! One reversible prefix/suffix level, below all semantic consumers. A suffix
//! is an ordinary checked pack item, never another dictionary or a certificate.
use super::*;

pub(super) const MAGIC: &[u8; 8] = b"ZILPKD03";
pub(super) const HEADER: usize = 24;
pub(super) const ROW: usize = 72;

#[derive(Clone)]
pub(super) struct Recipe {
    pub(super) body: Fingerprint,
    pub(super) prefix: Vec<u8>,
}

pub(super) struct Prepared<'a> {
    pub(super) parts: Vec<(Fingerprint, &'a [u8])>,
    pub(super) recipes: BTreeMap<Fingerprint, Recipe>,
}

// Wire v47's scalar selector pins the six flags and the outer ID layout.
// Validate the length and UTF-8 label, then preserve EVERY prefix byte. No
// function name, semantic decode, or guessed position in the body is used.
fn prefix_length(bytes: &[u8]) -> Option<usize> {
    zil_ir::wire::raw_scalar_function_id(bytes)?;
    let end = 14usize.checked_add(count(bytes, 10)?)?;
    std::str::from_utf8(bytes.get(14..end)?).ok()?;
    Some(end)
}

pub(super) fn prepare<'a>(
    items: &'a [(Fingerprint, Arc<[u8]>)],
    bounds: Bounds,
    cancellation: &CancellationToken,
) -> Option<Prepared<'a>> {
    if items.is_empty() || items.len() > bounds.max_items {
        return None;
    }
    let mut originals = BTreeMap::<Fingerprint, &'a [u8]>::new();
    let mut total = 0usize;
    for (key, bytes) in items {
        if cancellation.is_cancelled() || bytes.is_empty() || bytes.len() > bounds.maximum {
            return None;
        }
        if let Some(previous) = originals.get(key) {
            if *previous != bytes.as_ref() {
                return None;
            }
        } else {
            // Charge complete original bytes before factoring or writing files.
            total = total
                .checked_add(bytes.len())
                .filter(|sum| *sum <= bounds.maximum)?;
            if Fingerprint::of(bytes) != *key {
                return None;
            }
            originals.insert(*key, bytes.as_ref());
        }
    }
    let mut parts = BTreeMap::new();
    let mut recipes = BTreeMap::new();
    let mut stored = 0usize;
    let mut prefixes = 0usize;
    for (&key, &bytes) in &originals {
        if cancellation.is_cancelled() {
            return None;
        }
        let split = prefix_length(bytes)
            .filter(|end| *end < bytes.len())
            .unwrap_or(0);
        let (prefix, body) = bytes.split_at(split);
        let body_key = if split == 0 {
            key
        } else {
            Fingerprint::of(body)
        };
        if let Some(previous) = parts.insert(body_key, body) {
            if previous != body {
                return None;
            }
        } else {
            stored = stored.checked_add(body.len())?;
        }
        prefixes = prefixes.checked_add(prefix.len())?;
        recipes.insert(
            key,
            Recipe {
                body: body_key,
                prefix: prefix.to_vec(),
            },
        );
    }
    // Small/unshared/unknown inputs keep the old complete-item representation.
    // This conservative comparison includes the entire extra recipe table;
    // fewer physical locator rows can only make the dictionary smaller still.
    let overhead = HEADER.checked_add(recipes.len().checked_mul(ROW)?)?;
    if stored.checked_add(prefixes)?.checked_add(overhead)? >= total {
        return Some(Prepared {
            parts: originals.into_iter().collect(),
            recipes: BTreeMap::new(),
        });
    }
    Some(Prepared {
        parts: parts.into_iter().collect(),
        recipes,
    })
}

fn totals(
    recipes: &BTreeMap<Fingerprint, Recipe>,
    locations: &BTreeMap<Fingerprint, Location>,
    bounds: Bounds,
) -> Option<(usize, usize)> {
    if recipes.is_empty() || recipes.len() > bounds.max_items {
        return None;
    }
    let mut total = 0usize;
    let mut prefixes = 0usize;
    let mut used = BTreeSet::new();
    for (key, recipe) in recipes {
        let item = locations.get(&recipe.body)?;
        if recipe.prefix.is_empty() {
            if *key != recipe.body {
                return None;
            }
        } else if prefix_length(&recipe.prefix)? != recipe.prefix.len() {
            return None;
        }
        if item.length == 0 {
            return None;
        }
        total = total
            .checked_add(recipe.prefix.len())?
            .checked_add(item.length)?;
        if total > bounds.maximum {
            return None;
        }
        prefixes = prefixes.checked_add(recipe.prefix.len())?;
        used.insert(recipe.body);
    }
    if used.len() != locations.len() {
        return None;
    }
    Some((total, prefixes))
}

pub(super) fn encode(
    logical: &[u8],
    locations: &BTreeMap<Fingerprint, Location>,
    packs: &BTreeMap<Fingerprint, usize>,
    recipes: &BTreeMap<Fingerprint, Recipe>,
    bounds: Bounds,
) -> Option<Vec<u8>> {
    if recipes.is_empty() {
        return Manifest::encode_parts(logical, locations, packs, bounds);
    }
    let (total, prefixes) = totals(recipes, locations, bounds)?;
    let inner = Manifest::encode_parts(logical, locations, packs, bounds)?;
    let table_size = recipes.len().checked_mul(ROW)?.checked_add(prefixes)?;
    let size = HEADER.checked_add(table_size)?.checked_add(inner.len())?;
    if size > bounds.manifest_maximum()? {
        return None;
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(MAGIC);
    push_number(&mut bytes, total)?;
    push_count(&mut bytes, recipes.len())?;
    push_count(&mut bytes, table_size)?;
    for (key, recipe) in recipes {
        bytes.extend_from_slice(&key.0);
        bytes.extend_from_slice(&recipe.body.0);
        push_number(&mut bytes, recipe.prefix.len())?;
        bytes.extend_from_slice(&recipe.prefix);
    }
    bytes.extend_from_slice(&inner);
    Some(bytes)
}

pub(super) fn decode(bytes: &[u8], bounds: Bounds) -> Option<Manifest> {
    if bytes.get(..8) != Some(MAGIC) {
        return Manifest::decode(bytes, bounds);
    }
    if bytes.len() > bounds.manifest_maximum()? {
        return None;
    }
    let total = number(bytes, 8)?;
    let items = count(bytes, 16)?;
    let table_size = count(bytes, 20)?;
    if total > bounds.maximum || items == 0 || items > bounds.max_items {
        return None;
    }
    let prefixes = table_size.checked_sub(items.checked_mul(ROW)?)?;
    if prefixes > total {
        return None;
    }
    let end = HEADER.checked_add(table_size)?;
    let table = bytes.get(HEADER..end)?;
    // Intentionally only the legacy flat locator table here. Neither shared
    // metadata nor a second dictionary can recurse through this boundary.
    let mut manifest = Manifest::decode(bytes.get(end..)?, bounds)?;
    let mut position = 0usize;
    let mut previous = None;
    for _ in 0..items {
        let row_end = position.checked_add(ROW)?;
        let row = table.get(position..row_end)?;
        let key = Fingerprint(row[..32].try_into().ok()?);
        if previous.is_some_and(|previous| previous >= key) {
            return None;
        }
        previous = Some(key);
        let body = Fingerprint(row[32..64].try_into().ok()?);
        let prefix_end = row_end.checked_add(number(row, 64)?)?;
        let prefix = table.get(row_end..prefix_end)?.to_vec();
        manifest.recipes.insert(key, Recipe { body, prefix });
        position = prefix_end;
    }
    if position != table.len() || totals(&manifest.recipes, &manifest.locations, bounds)?.0 != total
    {
        return None;
    }
    Some(manifest)
}
