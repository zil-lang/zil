//! Private exact-input work products. These recipes are not validation evidence:
//! current declaration and complete native-unit preflight remain in the caller.
use super::{distribute_item, rewrite_direct_calls};
use crate::EntryPoint;
use crate::object_cache::{BufferedHasher, installed_compiler_identity};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::hash::Hash;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::Fingerprint;
use zil_ir::{InstructionKind, LirView};
use zil_llvm_api::model::{Item, Linkage, Module};

const MAGIC: &[u8; 8] = b"ZILCGU01";
const DIRECTORY: &str = "partition-layout-v1";
const MAX_ITEMS: usize = 262_144;
const MAX_UNITS: usize = 256;
const MAX_REFS: usize = 1_048_576;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const HEADER: usize = 8 + 32 + 32;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Unit {
    pub owner: usize,
    pub prelude: Vec<usize>,
    pub declarations: Vec<usize>,
    pub imports: Vec<usize>,
    pub definitions: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub units: Vec<Unit>,
}

impl Layout {
    /// Both the original planner and a checked receipt use this exact emitter.
    pub(super) fn replay(&self, module: Module) -> Vec<Module> {
        let mut remaining = vec![0usize; module.items.len()];
        let mut prototypes = BTreeMap::new();
        for unit in &self.units {
            for &index in unit.definitions.iter().chain(&unit.imports) {
                remaining[index] += 1;
            }
            for &index in unit.declarations.iter().chain(&unit.imports) {
                prototypes
                    .entry(index)
                    .or_insert_with(|| match &module.items[index] {
                        Item::Function(f) => Item::Function(f.declaration()),
                        Item::Global(g) => Item::Global(g.declaration()),
                        Item::NamedType(..) => unreachable!("a definition has a declaration"),
                    });
            }
        }
        let mut original = module.items.into_iter().map(Some).collect::<Vec<_>>();
        let mut units = Vec::with_capacity(self.units.len());
        for unit in &self.units {
            let clone_names = unit
                .imports
                .iter()
                .map(|&index| {
                    let Item::Function(function) =
                        original[index].as_ref().expect("remaining import")
                    else {
                        unreachable!("an imported definition is a function")
                    };
                    (
                        function.name.clone(),
                        format!("zil.import.{}.{}", unit.owner, function.name),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let mut items = Vec::with_capacity(
                unit.prelude.len()
                    + unit.declarations.len()
                    + 2 * unit.imports.len()
                    + unit.definitions.len(),
            );
            for &index in &unit.prelude {
                items.push(original[index].as_ref().expect("retained prelude").clone());
            }
            for &index in &unit.declarations {
                items.push(prototypes[&index].clone());
            }
            for &index in &unit.imports {
                // Canonical address operands keep this prototype. Only direct
                // calls are rewritten to the private implementation.
                items.push(prototypes[&index].clone());
                let Item::Function(mut function) =
                    distribute_item(&mut original[index], &mut remaining[index])
                else {
                    unreachable!("an imported definition is a function")
                };
                function.linkage = Linkage::Internal;
                function.name = clone_names[&function.name].clone();
                rewrite_direct_calls(&mut function, &clone_names);
                items.push(Item::Function(function));
            }
            for &index in &unit.definitions {
                let mut item = distribute_item(&mut original[index], &mut remaining[index]);
                if let Item::Function(function) = &mut item {
                    rewrite_direct_calls(function, &clone_names);
                }
                items.push(item);
            }
            units.push(Module {
                triple: module.triple.clone(),
                fingerprint: module.fingerprint.clone(),
                items,
            });
        }
        units
    }

    fn encode(&self) -> Option<Vec<u8>> {
        if !(2..=MAX_UNITS).contains(&self.units.len()) {
            return None;
        }
        let refs = self.units.iter().try_fold(0usize, |sum, unit| {
            sum.checked_add(unit.prelude.len())?
                .checked_add(unit.declarations.len())?
                .checked_add(unit.imports.len())?
                .checked_add(unit.definitions.len())
        })?;
        if refs > MAX_REFS {
            return None;
        }
        let length = 4usize
            .checked_add(self.units.len().checked_mul(20)?)?
            .checked_add(refs.checked_mul(4)?)?;
        if length > MAX_BYTES {
            return None;
        }
        let mut bytes = Vec::with_capacity(length);
        put(&mut bytes, self.units.len())?;
        for unit in &self.units {
            put(&mut bytes, unit.owner)?;
            for indices in [
                &unit.prelude,
                &unit.declarations,
                &unit.imports,
                &unit.definitions,
            ] {
                put(&mut bytes, indices.len())?;
                for &index in indices {
                    put(&mut bytes, index)?;
                }
            }
        }
        Some(bytes)
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let mut reader = Reader(bytes);
        let count = reader.word()?;
        if !(2..=MAX_UNITS).contains(&count) {
            return None;
        }
        let mut refs = 0usize;
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            units.push(Unit {
                owner: reader.word()?,
                prelude: reader.indices(&mut refs)?,
                declarations: reader.indices(&mut refs)?,
                imports: reader.indices(&mut refs)?,
                definitions: reader.indices(&mut refs)?,
            });
        }
        reader.0.is_empty().then_some(Self { units })
    }

    /// Structural checks protect replay's indices, allocations and linkage. The
    /// checksum/current-input key protects the original graph decision itself,
    /// with the same trusted-directory boundary as the existing object cache.
    fn valid(&self, module: &Module, lir: &LirView<'_>, count: usize) -> bool {
        if module.items.len() > MAX_ITEMS || self.units.len() > count {
            return false;
        }
        let mut assigned = vec![false; module.items.len()];
        let named = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| matches!(item, Item::NamedType(..)).then_some(index))
            .collect::<Vec<_>>();
        let has_imports = self.units.iter().any(|unit| !unit.imports.is_empty());
        let source_names = if has_imports {
            lir.functions
                .iter()
                .filter(|f| f.foreign.is_none())
                .map(|f| format!("zil.fn.{}", f.id.0))
                .collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        let mut source_cost = 0usize;
        let mut symbols = BTreeSet::new();
        for item in module.items.iter().filter(|_| has_imports) {
            match item {
                Item::Function(function) => {
                    if !symbols.insert(function.name.as_str()) {
                        return false;
                    }
                    if function.blocks.is_some() && source_names.contains(&function.name) {
                        let Some(cost) = source_cost.checked_add(function.cost()) else {
                            return false;
                        };
                        source_cost = cost;
                    }
                }
                Item::Global(global) => {
                    if !symbols.insert(global.name.as_str()) {
                        return false;
                    }
                }
                Item::NamedType(..) => {}
            }
        }
        let mut copied_cost = 0usize;
        let mut previous = None;
        for unit in &self.units {
            if unit.owner >= count
                || previous.is_some_and(|owner| owner >= unit.owner)
                || unit.definitions.is_empty()
            {
                return false;
            }
            previous = Some(unit.owner);
            for indices in [
                &unit.prelude,
                &unit.declarations,
                &unit.imports,
                &unit.definitions,
            ] {
                if indices.last().is_some_and(|&i| i >= module.items.len())
                    || indices.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return false;
                }
            }
            let mut named_here = Vec::new();
            for &index in &unit.prelude {
                if definition(&module.items[index]) {
                    return false;
                }
                if matches!(module.items[index], Item::NamedType(..)) {
                    named_here.push(index);
                }
            }
            if named_here != named {
                return false;
            }
            for &index in &unit.definitions {
                if !definition(&module.items[index]) || assigned[index] {
                    return false;
                }
                assigned[index] = true;
            }
            for &index in &unit.declarations {
                if !definition(&module.items[index])
                    || local(&module.items[index])
                    || unit.definitions.binary_search(&index).is_ok()
                    || unit.imports.binary_search(&index).is_ok()
                {
                    return false;
                }
            }
            for &index in &unit.imports {
                let Item::Function(function) = &module.items[index] else {
                    return false;
                };
                if function.blocks.is_none()
                    || function.linkage != Linkage::External
                    || !source_names.contains(&function.name)
                    || unit.definitions.binary_search(&index).is_ok()
                    || symbols
                        .contains(format!("zil.import.{}.{}", unit.owner, function.name).as_str())
                {
                    return false;
                }
                let Some(cost) = copied_cost.checked_add(function.cost()) else {
                    return false;
                };
                copied_cost = cost;
                if copied_cost > source_cost {
                    return false;
                }
            }
        }
        module
            .items
            .iter()
            .enumerate()
            .all(|(i, item)| assigned[i] == definition(item))
    }
}

fn definition(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.blocks.is_some(),
        Item::Global(g) => g.initializer.is_some(),
        Item::NamedType(..) => false,
    }
}
fn local(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.linkage.is_local(),
        Item::Global(g) => g.linkage.is_local(),
        Item::NamedType(..) => false,
    }
}
fn put(bytes: &mut Vec<u8>, value: usize) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(value).ok()?.to_le_bytes());
    Some(())
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn word(&mut self) -> Option<usize> {
        let (word, tail) = self.0.split_at_checked(4)?;
        self.0 = tail;
        Some(u32::from_le_bytes(word.try_into().ok()?) as usize)
    }
    fn indices(&mut self, total: &mut usize) -> Option<Vec<usize>> {
        let count = self.word()?;
        *total = total.checked_add(count)?;
        if *total > MAX_REFS || count > self.0.len() / 4 {
            return None;
        }
        (0..count).map(|_| self.word()).collect()
    }
}

pub(super) struct Cache {
    root: PathBuf,
    key: Fingerprint,
}
impl Cache {
    pub(super) fn new(
        root: &Path,
        module: &Module,
        lir: &LirView<'_>,
        entry: EntryPoint,
        count: usize,
        split: bool,
        imports: bool,
    ) -> Option<Self> {
        if ["LD_PRELOAD", "LD_AUDIT"]
            .into_iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
        {
            return None;
        }
        let key = action_key(
            module,
            lir,
            entry,
            count,
            split,
            imports,
            installed_compiler_identity().ok()?,
        )?;
        Some(Self {
            root: root.into(),
            key,
        })
    }
    pub(super) fn load(&self, module: &Module, lir: &LirView<'_>, count: usize) -> Option<Layout> {
        private_existing(&self.root).ok()?;
        let directory = self.root.join(DIRECTORY);
        private_existing(&directory).ok()?;
        let path = entry_path(&directory, self.key);
        let meta = fs::symlink_metadata(&path).ok()?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() < HEADER as u64
            || meta.len() > (HEADER + MAX_BYTES) as u64
        {
            return None;
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        File::open(path)
            .ok()?
            .take((HEADER + MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() != meta.len() as usize
            || &bytes[..8] != MAGIC
            || bytes[8..40] != self.key.0
            || Fingerprint::of(&bytes[HEADER..]).0 != bytes[40..HEADER]
        {
            return None;
        }
        let layout = Layout::decode(&bytes[HEADER..])?;
        layout.valid(module, lir, count).then_some(layout)
    }
    pub(super) fn pending(self, layout: &Layout) -> Option<Pending> {
        let path = entry_path(&self.root.join(DIRECTORY), self.key);
        Some(Pending {
            root: self.root,
            path,
            key: self.key,
            bytes: layout.encode()?,
        })
    }
}

fn action_key(
    module: &Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split: bool,
    imports: bool,
    compiler: Fingerprint,
) -> Option<Fingerprint> {
    if !(2..=MAX_UNITS).contains(&count) || module.items.len() > MAX_ITEMS {
        return None;
    }
    let mut remaining = MAX_REFS.checked_sub(lir.functions.len())?;
    let mut state = BufferedHasher::new();
    MAGIC.hash(&mut state);
    compiler.hash(&mut state);
    count.hash(&mut state);
    split.hash(&mut state);
    imports.hash(&mut state);
    match entry {
        EntryPoint::None => 0u8.hash(&mut state),
        EntryPoint::Main => 1u8.hash(&mut state),
        EntryPoint::ReturnValue(id) => {
            2u8.hash(&mut state);
            id.hash(&mut state);
        }
    }
    lir.target.hash(&mut state);
    lir.schema_version.hash(&mut state);
    lir.module_id.hash(&mut state);
    lir.functions.len().hash(&mut state);
    // Exact projection of every LIR field read by the partition planner. The
    // complete machine model below binds bodies/types/linkage independently.
    for function in lir.functions {
        function.id.hash(&mut state);
        function.name.hash(&mut state);
        function.foreign.is_some().hash(&mut state);
        function.span.file.hash(&mut state);
        function.blocks.len().hash(&mut state);
        remaining = remaining.checked_sub(function.blocks.len())?;
        for block in &function.blocks {
            block.instructions.len().hash(&mut state);
            remaining = remaining.checked_sub(block.instructions.len())?;
            for instruction in &block.instructions {
                let callee = match &instruction.kind {
                    InstructionKind::Call { function, .. } => Some(function),
                    _ => None,
                };
                callee.hash(&mut state);
            }
        }
    }
    // Exhaustive destructuring requires future machine fields to be considered.
    // The diagnostic fingerprint is freshly preserved by replay, never authority.
    let Module {
        triple,
        fingerprint: _,
        items,
    } = module;
    triple.hash(&mut state);
    items.hash(&mut state);
    Some(state.digest())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    root: PathBuf,
    path: PathBuf,
    key: Fingerprint,
    bytes: Vec<u8>,
}
impl Pending {
    /// Pin the real root only after PreparedCache has created/validated it.
    /// Driver publication guards register this exact optional destination.
    pub(crate) fn pin(mut self) -> Option<Self> {
        private_existing(&self.root).ok()?;
        self.root = fs::canonicalize(&self.root).ok()?;
        let directory = self.root.join(DIRECTORY);
        match fs::symlink_metadata(&directory) {
            Ok(_) => private_existing(&directory).ok()?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        self.path = entry_path(&directory, self.key);
        Some(self)
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn commit(self) {
        // Optional metadata cannot revoke an already published compilation.
        let _ = self.store();
    }
    fn store(&self) -> io::Result<()> {
        private_existing(&self.root)?;
        let directory = self.root.join(DIRECTORY);
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        private_existing(&directory)?;
        let temp = directory.join(format!(
            ".pending-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temp)?;
            file.write_all(MAGIC)?;
            file.write_all(&self.key.0)?;
            file.write_all(&Fingerprint::of(&self.bytes).0)?;
            file.write_all(&self.bytes)?;
            file.flush()?;
            drop(file);
            fs::rename(&temp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}
fn entry_path(directory: &Path, key: Fingerprint) -> PathBuf {
    directory.join(format!("{key}.layout"))
}
fn private_existing(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "layout cache is not a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "layout cache is not private",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
