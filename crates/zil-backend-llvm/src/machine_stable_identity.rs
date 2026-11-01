//! Private closed scalar experiment. Names, owners and ordering change in the
//! actual model; object keys and current verification are never normalized.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;
use zil_base::Fingerprint;
use zil_ir::{FunctionId, LirView};
use zil_llvm_api::model::{Function, Item, Linkage, Module, Op, Ty, Val};

const MAX_FUNCTIONS: usize = 10_000;
const MAX_KEY_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORK: usize = 1_000_000;

/// Research-only diagnostics must be absent in every timing experiment.
pub(crate) const DIAGNOSTIC_ENV: &str = "ZIL_RESEARCH_CLOSED_STABLE_DCE_TRACE";

pub(crate) struct Diagnostic {
    target: PathBuf,
    buckets: Vec<usize>,
    renames: BTreeMap<String, String>,
}

impl Diagnostic {
    /// Record actual post-internalization units and their real object keys.
    /// Failure affects only this optional observation, never compilation.
    pub(crate) fn write(
        &self,
        units: &[Module],
        cache: Option<&crate::object_cache::PreparedCache>,
    ) {
        if self.buckets.len() != units.len() {
            return;
        }
        let q = zil_base::json_string;
        let renames = self
            .renames
            .iter()
            .map(|(old, new)| format!("[{},{}]", q(old), q(new)))
            .collect::<Vec<_>>()
            .join(",");
        let records = self.buckets.iter().zip(units).enumerate().map(|(index, (bucket, module))| {
            let items = module.items.iter().map(|item| {
                let Item::Function(function) = item else { unreachable!() };
                format!("{{\"name\":{},\"definition\":{},\"linkage\":{}}}", q(&function.name), function.blocks.is_some(), q(&format!("{:?}", function.linkage)))
            }).collect::<Vec<_>>().join(",");
            let key = cache.map_or_else(|| "null".to_owned(), |cache| q(&cache.key(module).to_hex()));
            format!("{{\"bucket\":{bucket},\"unit_index\":{index},\"action_key\":{key},\"actual_item_order\":[{items}]}}")
        }).collect::<Vec<_>>().join(",");
        let text = format!(
            "{{\"kind\":\"closed-stable-dce-v2\",\"phase\":\"post-partition-before-codegen\",\"pid\":{},\"renames\":[{renames}],\"units\":[{records}]}}\n",
            std::process::id()
        );
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.target)
            .and_then(|mut file| file.write_all(text.as_bytes()));
        if let Err(error) = written {
            let _ = writeln!(
                std::io::stderr(),
                "closed stable research trace unavailable: {error}"
            );
        }
    }
}

struct Work<'a> {
    remaining: usize,
    cancelled: &'a (dyn Fn() -> bool + Sync),
}
impl Work<'_> {
    fn spend(&mut self, amount: usize) -> Option<()> {
        self.remaining = self.remaining.checked_sub(amount)?;
        if self.remaining.is_multiple_of(256) && (self.cancelled)() {
            return None;
        }
        Some(())
    }
}

pub(crate) struct Prepared {
    names: BTreeMap<String, String>,
    owners: BTreeMap<usize, usize>,
    declarations: BTreeMap<usize, BTreeSet<usize>>,
    source_names: BTreeSet<String>,
    entry_name: String,
    entry_owner: usize,
}

fn integer(ty: &Ty) -> bool {
    matches!(ty, Ty::Int(1 | 8 | 16 | 32 | 64))
}
fn scalar_signature(function: &Function) -> bool {
    (integer(&function.signature.result) || function.signature.result == Ty::Void)
        && function.signature.result_attrs.is_empty()
        && !function.signature.variadic
        && function
            .signature
            .params
            .iter()
            .all(|parameter| integer(&parameter.ty) && parameter.attrs.is_empty())
        && function.attrs.is_empty()
}

// Check the whole admitted operand domain before preflight; no recursive
// local_uses/symbols walk can run on unvalidated deep arrays or GEPs here.
fn simple_value(value: &Val, work: &mut Work<'_>) -> Option<()> {
    work.spend(1)?;
    match value {
        Val::Local(_) | Val::Int(_) | Val::Bool(_) | Val::Zero | Val::Undef | Val::Poison => {
            Some(())
        }
        Val::Global(_)
        | Val::Null
        | Val::Array(_)
        | Val::Bytes(_)
        | Val::Gep { .. }
        | Val::Invalid(_) => None,
    }
}

fn source_symbol(id: FunctionId) -> String {
    format!("zil.fn.{}", id.0)
}

/// Every miss leaves the borrowed original untouched. The caller checks
/// cancellation again before using either this plan or the original policy.
pub(crate) fn prepare(
    module: &Module,
    lir: &LirView<'_>,
    hints: &[(FunctionId, String)],
    count: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Prepared> {
    prepare_inner(module, lir, hints, count, cancelled, false)
}

/// The closed collector has already selected the current emitted definitions.
/// Rebind their original source identities without cloning the removed bodies.
/// The original model admission and complete preflight still run in inner.
pub(crate) fn prepare_collected(
    module: &Module,
    lir: &LirView<'_>,
    hints: &[(FunctionId, String)],
    count: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<Prepared> {
    if count < 2
        || !lir.closures.is_empty()
        || !lir.resources.is_empty()
        || hints.is_empty()
        || hints.len() > MAX_FUNCTIONS
        || module.items.len() > MAX_WORK
        || (cancelled)()
    {
        return None;
    }
    let mut work = Work {
        remaining: MAX_WORK,
        cancelled,
    };
    let mut ids = BTreeSet::new();
    let mut foreign_names = BTreeSet::new();
    for item in &module.items {
        work.spend(1)?;
        let Item::Function(function) = item else {
            return None;
        };
        if function.blocks.is_none() {
            if !function.name.starts_with("llvm.") {
                foreign_names.insert(function.name.as_str());
            }
        } else if function.name != "main" {
            let id = FunctionId(function.name.strip_prefix("zil.fn.")?.parse().ok()?);
            if function.name != source_symbol(id) || !ids.insert(id) || ids.len() > 8 {
                return None;
            }
        }
    }
    if ids.is_empty() {
        return None;
    }
    let mut functions = Vec::new();
    for &id in &ids {
        work.spend(1)?;
        let function = lir.functions.get(id.0 as usize)?;
        if function.id != id || function.foreign.is_some() {
            return None;
        }
        functions.push(function.clone());
    }
    // Existing admission recognizes external declarations through the current
    // LIR foreign symbols. Keep exactly those still present in the small model.
    if !foreign_names.is_empty() {
        for function in lir.functions {
            work.spend(1)?;
            if let Some(foreign) = &function.foreign
                && foreign_names.remove(foreign.symbol.as_str())
            {
                functions.push(function.clone());
                if foreign_names.is_empty() {
                    break;
                }
            }
        }
        if !foreign_names.is_empty() {
            return None;
        }
    }
    let mut selected_hints = Vec::new();
    let mut key_bytes = 0usize;
    for (id, key) in hints {
        work.spend(1)?;
        if ids.contains(id) {
            key_bytes = key_bytes.checked_add(key.len())?;
            if key_bytes > MAX_KEY_BYTES {
                return None;
            }
            selected_hints.push((*id, key.clone()));
        }
    }
    let selected = LirView {
        schema_version: lir.schema_version,
        module_id: lir.module_id,
        verified_fingerprint: lir.verified_fingerprint,
        target: lir.target,
        functions: &functions,
        closures: lir.closures,
        resources: lir.resources,
        layouts: std::borrow::Cow::Borrowed(lir.layouts.as_ref()),
    };
    prepare_inner(module, &selected, &selected_hints, count, cancelled, true)
}

fn prepare_inner(
    module: &Module,
    lir: &LirView<'_>,
    hints: &[(FunctionId, String)],
    count: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
    allow_single_owner: bool,
) -> Option<Prepared> {
    if count < 2
        || !lir.closures.is_empty()
        || !lir.resources.is_empty()
        || hints.is_empty()
        || hints.len() > MAX_FUNCTIONS
        || module.items.len() > MAX_WORK
    {
        return None;
    }
    let mut work = Work {
        remaining: MAX_WORK,
        cancelled,
    };
    let mut by_id = BTreeMap::new();
    let mut logical = BTreeSet::new();
    let mut key_bytes = 0usize;
    for (id, key) in hints {
        work.spend(1)?;
        key_bytes = key_bytes.checked_add(key.len())?;
        if key.is_empty()
            || key_bytes > MAX_KEY_BYTES
            || !logical.insert(key.as_str())
            || by_id.insert(*id, key.as_str()).is_some()
        {
            return None;
        }
    }
    let mut names = BTreeMap::new();
    let mut source_names = BTreeSet::new();
    let mut stable_owners = BTreeMap::new();
    let mut foreign_names = BTreeSet::new();
    let mut entry = None;
    let mut source_count = 0usize;
    for function in lir.functions {
        work.spend(1)?;
        if let Some(foreign) = &function.foreign {
            foreign_names.insert(foreign.symbol.as_str());
            continue;
        }
        source_count = source_count.checked_add(1)?;
        let key = *by_id.get(&function.id)?;
        if function.environment.is_some() {
            return None;
        }
        let mut framed = Vec::with_capacity(32 + key.len());
        framed.extend_from_slice(b"ZIL-CLOSED-STABLE-SYMBOL-v1\0");
        framed.extend_from_slice(key.as_bytes());
        let stable = format!("zil.stable.{}", Fingerprint::of(&framed).to_hex());
        if !source_names.insert(stable.clone()) {
            return None;
        }
        let mut owner_key = Vec::with_capacity(32 + key.len());
        owner_key.extend_from_slice(b"ZIL-CLOSED-STABLE-OWNER-v1\0");
        owner_key.extend_from_slice(key.as_bytes());
        let hash = Fingerprint::of(&owner_key).0;
        let owner = u64::from_le_bytes(hash[..8].try_into().ok()?) as usize % count;
        let original = source_symbol(function.id);
        if function.name == "main" && entry.replace(original.clone()).is_some() {
            return None;
        }
        stable_owners.insert(original.clone(), owner);
        if names.insert(original, stable).is_some() {
            return None;
        }
    }
    if source_count != hints.len() {
        return None;
    }
    let entry = entry?;
    let entry_owner = *stable_owners.get(&entry)?;
    let mut symbols = BTreeSet::new();
    let mut declaration_ids = BTreeMap::new();
    let mut definitions = BTreeSet::new();
    let mut owners = BTreeMap::new();
    let mut definition_refs = Vec::new();
    let mut wrapper_seen = false;
    for (index, item) in module.items.iter().enumerate() {
        work.spend(1)?;
        let Item::Function(function) = item else {
            return None;
        };
        if !symbols.insert(function.name.as_str()) {
            return None;
        }
        if source_names.contains(&function.name) {
            return None;
        }
        let Some(blocks) = &function.blocks else {
            if !(function.name.starts_with("llvm.")
                || foreign_names.contains(function.name.as_str()))
            {
                return None;
            }
            declaration_ids.insert(function.name.as_str(), index);
            continue;
        };
        work.spend(function.signature.params.len())?;
        if !scalar_signature(function) || function.linkage == Linkage::AvailableExternally {
            return None;
        }
        let wrapper = function.name == "main";
        let owner = if wrapper {
            if wrapper_seen || function.linkage != Linkage::External {
                return None;
            }
            wrapper_seen = true;
            entry_owner
        } else {
            if !definitions.insert(function.name.as_str()) {
                return None;
            }
            *stable_owners.get(&function.name)?
        };
        owners.insert(index, owner);
        let mut references = BTreeSet::new();
        for block in blocks {
            work.spend(1)?;
            for instruction in &block.instructions {
                work.spend(1)?;
                match &instruction.op {
                    Op::Call {
                        result,
                        callee: Val::Global(callee),
                        args,
                        ..
                    } => {
                        work.spend(args.len())?;
                        if !callee.starts_with("llvm.")
                            && (!(integer(result) || *result == Ty::Void)
                                || args.iter().any(|argument| !integer(&argument.ty)))
                        {
                            return None;
                        }
                        // The only admitted definition-to-definition edge is
                        // the native main wrapper to its colocated source entry.
                        if names.contains_key(callee) {
                            if !wrapper || callee != &entry {
                                return None;
                            }
                        } else {
                            if !(callee.starts_with("llvm.")
                                || foreign_names.contains(callee.as_str()))
                            {
                                return None;
                            }
                            references.insert(callee.as_str());
                        }
                        for argument in args {
                            simple_value(&argument.value, &mut work)?;
                        }
                    }
                    Op::Call { .. }
                    | Op::Alloca { .. }
                    | Op::Load { .. }
                    | Op::Store { .. }
                    | Op::Gep { .. }
                    | Op::Insert { .. } => return None,
                    Op::Binary { ty, .. } | Op::Compare { ty, .. } | Op::Phi { ty, .. } => {
                        if !integer(ty) {
                            return None;
                        }
                        for value in instruction.op.values() {
                            simple_value(value, &mut work)?;
                        }
                    }
                    Op::Cast { arg, to, .. } => {
                        if !integer(&arg.ty) || !integer(to) {
                            return None;
                        }
                        simple_value(&arg.value, &mut work)?;
                    }
                    Op::Freeze(argument) | Op::Return(Some(argument)) => {
                        if !integer(&argument.ty) {
                            return None;
                        }
                        simple_value(&argument.value, &mut work)?;
                    }
                    Op::Extract { arg, .. } => {
                        // Checked integer intrinsics use a flat integer pair.
                        let Ty::Struct(fields) = &arg.ty else {
                            return None;
                        };
                        if fields.len() != 2 || !fields.iter().all(integer) {
                            return None;
                        }
                        simple_value(&arg.value, &mut work)?;
                    }
                    Op::CondBranch { condition, .. } => simple_value(condition, &mut work)?,
                    Op::Branch(_) | Op::Return(None) | Op::Unreachable => {}
                }
            }
        }
        definition_refs.push((owner, references));
    }
    if !wrapper_seen
        || definitions.len() != names.len()
        || names
            .keys()
            .any(|name| !definitions.contains(name.as_str()))
    {
        return None;
    }
    let mut declarations = BTreeMap::<usize, BTreeSet<usize>>::new();
    for (owner, references) in definition_refs {
        for reference in references {
            work.spend(1)?;
            declarations
                .entry(owner)
                .or_default()
                .insert(*declaration_ids.get(reference)?);
        }
    }
    if !allow_single_owner && owners.values().copied().collect::<BTreeSet<_>>().len() < 2 {
        return None;
    }
    work.spend(1)?;
    // No renamed/filtered/sorted model has existed before this full check.
    zil_llvm_api::validate_module(module).ok()?;
    work.spend(1)?;
    Some(Prepared {
        names,
        owners,
        declarations,
        source_names,
        entry_name: entry,
        entry_owner,
    })
}

fn rename_value(value: &mut Val, names: &BTreeMap<String, String>) {
    match value {
        Val::Global(name) => {
            if let Some(replacement) = names.get(name) {
                *name = replacement.clone();
            }
            return;
        }
        Val::Array(_) | Val::Gep { .. } => {}
        Val::Local(_)
        | Val::Int(_)
        | Val::Bool(_)
        | Val::Null
        | Val::Zero
        | Val::Undef
        | Val::Poison
        | Val::Bytes(_)
        | Val::Invalid(_) => return,
    }
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Val::Global(name) => {
                if let Some(replacement) = names.get(name) {
                    *name = replacement.clone();
                }
            }
            Val::Array(arguments) => {
                pending.extend(arguments.iter_mut().map(|argument| &mut argument.value))
            }
            Val::Gep { ptr, indices, .. } => {
                pending.push(ptr.as_mut());
                pending.extend(indices.iter_mut().map(|argument| &mut argument.value));
            }
            Val::Local(_)
            | Val::Int(_)
            | Val::Bool(_)
            | Val::Null
            | Val::Zero
            | Val::Undef
            | Val::Poison
            | Val::Bytes(_)
            | Val::Invalid(_) => {}
        }
    }
}

fn rename_op(op: &mut Op, names: &BTreeMap<String, String>) {
    match op {
        Op::Alloca { .. } | Op::Branch(_) | Op::Return(None) | Op::Unreachable => {}
        Op::Load { ptr, .. } => rename_value(ptr, names),
        Op::Store { arg, ptr, .. } => {
            rename_value(&mut arg.value, names);
            rename_value(ptr, names);
        }
        Op::Binary { left, right, .. } | Op::Compare { left, right, .. } => {
            rename_value(left, names);
            rename_value(right, names);
        }
        Op::Cast { arg, .. }
        | Op::Extract { arg, .. }
        | Op::Freeze(arg)
        | Op::Return(Some(arg)) => rename_value(&mut arg.value, names),
        Op::Gep { ptr, indices, .. } => {
            rename_value(ptr, names);
            for argument in indices {
                rename_value(&mut argument.value, names);
            }
        }
        Op::Insert { arg, field, .. } => {
            rename_value(&mut arg.value, names);
            rename_value(&mut field.value, names);
        }
        Op::Call { callee, args, .. } => {
            rename_value(callee, names);
            for argument in args {
                rename_value(&mut argument.value, names);
            }
        }
        Op::Phi { incoming, .. } => {
            for (value, _) in incoming {
                rename_value(value, names);
            }
        }
        Op::CondBranch { condition, .. } => rename_value(condition, names),
    }
}

impl Prepared {
    /// The original complete plan and preflight have already established that
    /// only native main can reference the source entry, with no other source
    /// reference/address/global. This method never runs on a rejected model.
    pub(crate) fn prune_unreachable_closed_definitions(&mut self, module: &mut Module) {
        let mut required = BTreeSet::new();
        for item in &module.items {
            let Item::Function(function) = item else {
                unreachable!()
            };
            if function.name != "main" && function.name != self.entry_name {
                continue;
            }
            for instruction in function
                .blocks
                .iter()
                .flatten()
                .flat_map(|block| &block.instructions)
            {
                if let Op::Call {
                    callee: Val::Global(name),
                    ..
                } = &instruction.op
                    && !self.names.contains_key(name)
                {
                    required.insert(name.as_str());
                }
            }
        }
        // Decide every retained original position before moving any item.
        let retained = module
            .items
            .iter()
            .map(|item| {
                let Item::Function(function) = item else {
                    unreachable!()
                };
                if function.blocks.is_some() {
                    function.name == "main" || function.name == self.entry_name
                } else {
                    required.contains(function.name.as_str())
                }
            })
            .collect::<Vec<_>>();
        let entry_stable_name = self.names[&self.entry_name].clone();
        let original = std::mem::take(&mut module.items);
        let mut owners = BTreeMap::new();
        let mut declarations = BTreeSet::new();
        for (index, item) in original.into_iter().enumerate() {
            if !retained[index] {
                continue;
            }
            let new_index = module.items.len();
            if self.owners.contains_key(&index) {
                owners.insert(new_index, self.entry_owner);
            } else {
                declarations.insert(new_index);
            }
            module.items.push(item);
        }
        self.owners = owners;
        self.declarations = BTreeMap::from([(self.entry_owner, declarations)]);
        self.names.retain(|name, _| *name == self.entry_name);
        self.source_names = BTreeSet::from([entry_stable_name]);
    }

    pub(crate) fn rename(&self, module: &mut Module) {
        for item in &mut module.items {
            match item {
                Item::Function(function) => {
                    if let Some(name) = self.names.get(&function.name) {
                        function.name = name.clone();
                    }
                    for block in function.blocks.iter_mut().flatten() {
                        for instruction in &mut block.instructions {
                            rename_op(&mut instruction.op, &self.names);
                        }
                    }
                }
                Item::Global(global) => {
                    if let Some(value) = &mut global.initializer {
                        rename_value(value, &self.names);
                    }
                }
                Item::NamedType(..) => {}
            }
        }
    }

    pub(crate) fn partition(
        self,
        module: Module,
        trace: Option<PathBuf>,
    ) -> (
        crate::machine_cgu::Plan,
        BTreeSet<String>,
        Option<Diagnostic>,
    ) {
        let mut declarations = BTreeMap::new();
        let mut members = BTreeMap::<usize, Vec<Item>>::new();
        for (index, item) in module.items.into_iter().enumerate() {
            if let Some(&owner) = self.owners.get(&index) {
                members.entry(owner).or_default().push(item);
            } else {
                declarations.insert(index, item);
            }
        }
        let mut units = Vec::with_capacity(members.len());
        let mut diagnostic = trace.map(|target| Diagnostic {
            target,
            buckets: Vec::with_capacity(members.len()),
            renames: self.names,
        });
        for (owner, mut definitions) in members {
            if let Some(diagnostic) = &mut diagnostic {
                diagnostic.buckets.push(owner);
            }
            definitions.sort_by(|left, right| {
                let (Item::Function(left), Item::Function(right)) = (left, right) else {
                    unreachable!()
                };
                left.name.cmp(&right.name)
            });
            let mut prelude = self
                .declarations
                .get(&owner)
                .into_iter()
                .flatten()
                .map(|index| declarations[index].clone())
                .collect::<Vec<_>>();
            prelude.sort_by(|left, right| {
                let (Item::Function(left), Item::Function(right)) = (left, right) else {
                    unreachable!()
                };
                left.name.cmp(&right.name)
            });
            prelude.extend(definitions);
            units.push(Module {
                triple: module.triple.clone(),
                fingerprint: module.fingerprint.clone(),
                items: prelude,
            });
        }
        (
            crate::machine_cgu::Plan {
                units,
                fallback: None,
            },
            self.source_names,
            diagnostic,
        )
    }
}

#[cfg(test)]
#[path = "machine_stable_identity_tests.rs"]
mod tests;
