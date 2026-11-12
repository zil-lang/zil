//! Session-owned raw native work products. Every entry is bound to immutable
//! input values; current Verify and LIR binding remain outside this cache.
use crate::BuildOptions;
use std::sync::Arc;
use zil_base::Fingerprint;
use zil_ir::{FunctionId, LirView};
use zil_llvm_api::model::Module;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendSession {
    pub(crate) lower: crate::emit::incremental::Session,
    pub(crate) partition: Option<Arc<PartitionProduct>>,
    pub(crate) transformed: Option<Arc<TransformProduct>>,
    pub(crate) keys: Vec<Arc<KeyProduct>>,
    pub(crate) key_partition: Option<Arc<PartitionProduct>>,
    pub(crate) preflights: Vec<Arc<PreflightProduct>>,
    pub(crate) native_products: NativeProductHistory,
    pub(crate) summary: BackendSessionSummary,
    pub(crate) link: Option<Arc<crate::link_cache::Product>>,
    pub(crate) link_tools: crate::object_cache::InstalledToolIdentities,
    diagnostic_items: std::collections::BTreeMap<String, Arc<DiagnosticItem>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendSessionSummary {
    pub reused_machine_module: bool,
    pub whole_machine_input_functions_compared: usize,
    pub lowered_functions: usize,
    pub reused_functions: usize,
    pub reused_partition: bool,
    pub reused_partition_recipe: bool,
    pub reused_partition_units: usize,
    pub rebuilt_partition_units: usize,
    pub reused_machine_transforms: bool,
    pub reused_object_keys: usize,
    pub reused_link: bool,
    pub rendered_diagnostic_items: usize,
    pub reused_diagnostic_items: usize,
    pub source_projection_traversals: usize,
    pub source_projection_instruction_visits: usize,
    pub source_projection_reuses: usize,
    pub native_history_restored_units: usize,
    pub native_history_retained_epochs: usize,
    /// Conservative sum of encoded native payloads, not process RSS.
    pub native_history_payload_bytes: u64,
}
impl BackendSession {
    pub fn summary(&self) -> &BackendSessionSummary {
        &self.summary
    }
    pub(crate) fn render(&mut self, module: &Module) -> DiagnosticText {
        let header = Module {
            triple: module.triple.clone(),
            fingerprint: module.fingerprint.clone(),
            items: (Vec::new()).into(),
        }
        .to_text();
        let mut fragments = Vec::with_capacity(module.items.len());
        let mut next = std::collections::BTreeMap::new();
        for item in &module.items {
            let name = match item {
                zil_llvm_api::model::Item::Function(function) => {
                    format!("function:{}", function.name)
                }
                zil_llvm_api::model::Item::Global(global) => format!("global:{}", global.name),
                zil_llvm_api::model::Item::NamedType(name, _) => format!("type:{name}"),
            };
            let product = if let Some(old) = self
                .diagnostic_items
                .get(&name)
                .filter(|old| old.item == *item)
            {
                self.summary.reused_diagnostic_items += 1;
                old.clone()
            } else {
                self.summary.rendered_diagnostic_items += 1;
                Arc::new(DiagnosticItem {
                    item: item.clone(),
                    text: Arc::new(zil_llvm_api::model::item_to_text(item)),
                })
            };
            fragments.push(product.text.clone());
            next.insert(name, product);
        }
        self.diagnostic_items = next;
        DiagnosticText {
            text: Arc::new(header),
            fragments: fragments.into(),
            fingerprint: None,
        }
    }
    pub(crate) fn begin(&mut self) {
        self.summary = BackendSessionSummary {
            native_history_retained_epochs: self.native_products.retained_epochs(),
            native_history_payload_bytes: self.native_products.payload_bytes(),
            ..BackendSessionSummary::default()
        };
        self.link_tools.begin();
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticItem {
    item: zil_llvm_api::model::Item,
    text: Arc<String>,
}
pub(crate) type ScalarIdentity = (Arc<()>, Vec<(Arc<()>, Arc<()>)>);
pub(crate) fn same_identity(left: &ScalarIdentity, right: &ScalarIdentity) -> bool {
    Arc::ptr_eq(&left.0, &right.0)
        && left.1.len() == right.1.len()
        && left
            .1
            .iter()
            .zip(&right.1)
            .all(|((lm, ls), (rm, rs))| Arc::ptr_eq(lm, rm) && Arc::ptr_eq(ls, rs))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransformProduct {
    pub(crate) input: ScalarIdentity,
    pub(crate) machine: Module,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionInput {
    options: BuildOptions,
    names: Option<Vec<(FunctionId, String)>>,
    collected: bool,
    closures: Vec<FunctionId>,
    raw: Option<RawInput>,
    machine: Option<Module>,
    scalar: Option<ScalarIdentity>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct RawInput {
    functions: zil_ir::PersistentVec<zil_ir::TypedFunction>,
    closures: Vec<zil_ir::ClosureDefinition>,
    resources: Vec<zil_ir::ResourceDefinition>,
    layouts: Vec<zil_ir::ScalarLayout>,
}
impl PartitionInput {
    pub(crate) fn new(
        options: &BuildOptions,
        names: Option<&[(FunctionId, String)]>,
        collected: bool,
        lir: &LirView<'_>,
        machine: &Module,
        scalar: Option<ScalarIdentity>,
    ) -> Self {
        let mut options = options.clone();
        options.output.clear();
        let machine = scalar.is_none().then(|| {
            let mut machine = machine.clone();
            machine.fingerprint.clear(); // diagnostic only, never native Builder input
            machine
        });
        Self {
            options,
            names: names.map(<[_]>::to_vec),
            collected,
            closures: lir
                .closures
                .iter()
                .map(|closure| closure.function)
                .collect(),
            raw: scalar.is_none().then(|| RawInput {
                functions: lir.functions.clone(),
                closures: lir.closures.to_vec(),
                resources: lir.resources.to_vec(),
                layouts: lir.layouts.to_vec(),
            }),
            machine,
            scalar,
        }
    }
}
impl PartitionInput {
    pub(crate) fn same_recipe_environment(&self, other: &Self) -> bool {
        self.options == other.options
            && self.names == other.names
            && self.collected == other.collected
            && self.closures == other.closures
            && matches!((&self.scalar, &other.scalar), (Some((left, _)), Some((right, _))) if Arc::ptr_eq(left, right))
    }
}
impl PartialEq for PartitionInput {
    fn eq(&self, other: &Self) -> bool {
        if self.options != other.options
            || self.names != other.names
            || self.collected != other.collected
            || self.closures != other.closures
        {
            return false;
        }
        match (&self.scalar, &other.scalar) {
            (Some((left_context, left)), Some((right_context, right))) => {
                Arc::ptr_eq(left_context, right_context)
                    && left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|((lm, ls), (rm, rs))| Arc::ptr_eq(lm, rm) && Arc::ptr_eq(ls, rs))
            }
            (None, None) => self.raw == other.raw && self.machine == other.machine,
            _ => false,
        }
    }
}
impl Eq for PartitionInput {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PartitionProduct {
    pub(crate) input: PartitionInput,
    pub(crate) units: Vec<Module>,
    pub(crate) fallback: Option<&'static str>,
    pub(crate) diagnostic: DiagnosticText,
    pub(crate) private_closures: usize,
    pub(crate) recipe: Option<crate::machine_cgu::Recipe>,
}
impl PartitionProduct {
    pub(crate) fn diagnostic(&self, fingerprint: &str) -> DiagnosticText {
        self.diagnostic.current(fingerprint)
    }
}
/// Shared exact diagnostic bytes. A current verification fingerprint can be
/// streamed into the header without cloning the retained function text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticText {
    text: Arc<String>,
    fragments: zil_llvm_api::model::SharedVec<Arc<String>>,
    fingerprint: Option<String>,
}
impl From<String> for DiagnosticText {
    fn from(text: String) -> Self {
        Self {
            text: Arc::new(text),
            fragments: Vec::new().into(),
            fingerprint: None,
        }
    }
}
impl DiagnosticText {
    fn current(&self, fingerprint: &str) -> Self {
        Self {
            text: self.text.clone(),
            fragments: self.fragments.clone(),
            fingerprint: Some(fingerprint.to_owned()),
        }
    }
    pub(crate) fn write(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let prefix = "; ZIL verified LLVM module\n; verified-fingerprint: ";
        if let Some(fingerprint) = &self.fingerprint
            && let Some(rest) = self.text.strip_prefix(prefix)
            && let Some(end) = rest.find('\n')
        {
            file.write_all(prefix.as_bytes())?;
            file.write_all(fingerprint.as_bytes())?;
            file.write_all(&rest.as_bytes()[end..])?;
        } else {
            file.write_all(self.text.as_bytes())?;
        }
        for fragment in &self.fragments {
            file.write_all(fragment.as_bytes())?;
        }
        file.flush()
    }
    #[cfg(test)]
    pub(crate) fn to_text(&self) -> String {
        let mut text = self.text.as_ref().clone();
        if let Some(fingerprint) = &self.fingerprint {
            let prefix = "; ZIL verified LLVM module\n; verified-fingerprint: ";
            if let Some(rest) = text.strip_prefix(prefix)
                && let Some(end) = rest.find('\n')
            {
                text.replace_range(prefix.len()..prefix.len() + end, fingerprint);
            }
        }
        for fragment in &self.fragments {
            text.push_str(fragment);
        }
        text
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyProduct {
    pub(crate) directory: std::path::PathBuf,
    pub(crate) module: Module,
    pub(crate) key: Fingerprint,
}

/// Raw work-product bookkeeping only. Equality here is snapshot identity, never
/// checker authority: every use goes back through the LLVM preflight API.
#[derive(Clone, Debug)]
pub(crate) struct PreflightProduct {
    pub(crate) module: Module,
    pub(crate) receipt: Arc<zil_llvm_api::PreflightReceipt>,
}
impl PartialEq for PreflightProduct {
    fn eq(&self, other: &Self) -> bool {
        self.module == other.module && Arc::ptr_eq(&self.receipt, &other.receipt)
    }
}
impl Eq for PreflightProduct {}

/// A stable ordinal candidate slot, not a reuse authorization. A partition
/// change can put another unit in this slot; the native consumer must still
/// bind its complete current configuration and observed input dependencies.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NativeUnitOwner {
    pub(crate) triple: String,
    pub(crate) requested_units: usize,
    pub(crate) ordinal: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeUnitProduct {
    pub(crate) module: Module,
    pub(crate) products: zil_llvm_api::NativeWorkProducts,
}

// Optional raw-native candidates only. Four successful owner epochs are enough
// to retain named cold/private parents across intervening broad edits without
// retaining arbitrary session history. The independent checker never lives here.
const NATIVE_HISTORY_EPOCHS: usize = 4;
const NATIVE_HISTORY_PAYLOAD_BYTES: u64 = 128 * 1024 * 1024;
type NativeUnits = std::collections::BTreeMap<NativeUnitOwner, Arc<NativeUnitProduct>>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeProductHistory {
    epochs: std::collections::VecDeque<Arc<NativeEpoch>>,
}
#[derive(Debug, PartialEq, Eq)]
struct NativeEpoch {
    units: NativeUnits,
    payload_bytes: u64,
}
impl NativeProductHistory {
    pub(crate) fn candidates(
        &self,
        owner: &NativeUnitOwner,
    ) -> (Option<Arc<NativeUnitProduct>>, Vec<Arc<NativeUnitProduct>>) {
        let parent = self
            .epochs
            .front()
            .and_then(|epoch| epoch.units.get(owner))
            .cloned();
        let older = self
            .epochs
            .iter()
            .skip(1)
            .filter_map(|epoch| epoch.units.get(owner).cloned())
            .collect();
        (parent, older)
    }
    pub(crate) fn retained_epochs(&self) -> usize {
        self.epochs.len()
    }
    pub(crate) fn payload_bytes(&self) -> u64 {
        self.epochs.iter().map(|epoch| epoch.payload_bytes).sum()
    }
    pub(crate) fn commit(&mut self, enabled: bool, next: NativeUnits) {
        self.commit_with_cap(enabled, next, NATIVE_HISTORY_PAYLOAD_BYTES);
    }
    fn commit_with_cap(&mut self, enabled: bool, mut next: NativeUnits, cap: u64) {
        if !enabled {
            self.epochs.clear();
            return;
        }
        // Bound even a single new epoch. Oversize optional products are dropped
        // in deterministic owner order; compilation/verification are unaffected.
        let mut payload_bytes = 0_u64;
        next.retain(|_, product| {
            let bytes = product.products.retained_payload_bytes();
            if bytes > cap.saturating_sub(payload_bytes) {
                return false;
            }
            payload_bytes += bytes;
            true
        });
        // Dedup uses existing opaque product identity only, never a content
        // digest or checker authority. A product is paired with another raw unit
        // only after complete current triple/items equality on an object hit.
        let found = self.epochs.iter().position(|epoch| {
            epoch.units.len() == next.len()
                && epoch
                    .units
                    .iter()
                    .zip(&next)
                    .all(|((old_owner, old), (new_owner, new))| {
                        old_owner == new_owner && old.products == new.products
                    })
        });
        let current = found
            .and_then(|index| self.epochs.remove(index))
            .unwrap_or_else(|| {
                Arc::new(NativeEpoch {
                    units: next,
                    payload_bytes,
                })
            });
        self.epochs.push_front(current);
        while self.epochs.len() > NATIVE_HISTORY_EPOCHS || self.payload_bytes() > cap {
            self.epochs.pop_back();
        }
    }
}

#[cfg(all(test, feature = "native-products"))]
pub(crate) mod native_history_tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Inst, Item, Linkage, Op, Signature, Ty};

    pub(crate) fn module(answer: i64, sibling: i64) -> Module {
        let function = |name: &str, value| {
            Item::Function(Function {
                name: name.into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), value))),
                        )],
                    }]
                    .into(),
                ),
            })
        };
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "diagnostic-only".into(),
            items: vec![function("answer", answer), function("sibling", sibling)].into(),
        }
    }
    pub(crate) fn owner(ordinal: usize) -> NativeUnitOwner {
        NativeUnitOwner {
            triple: "x86_64-unknown-linux-gnu".into(),
            requested_units: 16,
            ordinal,
        }
    }
    fn product(value: i64) -> Arc<NativeUnitProduct> {
        let module = module(value, value + 1);
        let (object, _, product) = zil_llvm_api::compile_module_with_native_products(
            &module,
            &zil_llvm_api::Config {
                triple: &module.triple,
                optimization: zil_llvm_api::Optimization::O2,
            },
            crate::FunctionMerging::Disabled,
            &|| false,
            None,
        )
        .unwrap();
        assert_eq!(
            object.receipt.native_products.unwrap().available,
            1,
            "native history acceptance requires the installed extension"
        );
        Arc::new(NativeUnitProduct {
            module,
            products: product.expect("native product"),
        })
    }
    fn units(product: &Arc<NativeUnitProduct>) -> NativeUnits {
        [(owner(0), product.clone())].into()
    }

    #[test]
    fn native_history_is_bounded_lru_and_checkpoint_owners_remain_immutable() {
        let values: Vec<_> = (0..5).map(product).collect();
        let mut history = NativeProductHistory::default();
        for value in &values[..4] {
            history.commit(true, units(value));
        }
        assert_eq!(history.retained_epochs(), 4);
        let checkpoint = history.clone();
        let oldest = history.epochs.back().unwrap().clone();
        history.commit(true, units(&values[0]));
        assert_eq!(history.retained_epochs(), 4);
        assert!(Arc::ptr_eq(history.epochs.front().unwrap(), &oldest));
        history.commit(true, units(&values[4]));
        let (active, previous) = history.candidates(&owner(0));
        assert_eq!(active.unwrap().products, values[4].products);
        assert_eq!(
            previous.iter().map(|p| &p.products).collect::<Vec<_>>(),
            vec![
                &values[0].products,
                &values[3].products,
                &values[2].products
            ]
        );
        assert_eq!(
            checkpoint.candidates(&owner(0)).0.unwrap().products,
            values[3].products
        );
        assert_eq!(checkpoint.retained_epochs(), 4);
        // A failed caller discards its cloned staging history. A successful
        // disabled build releases all retained epochs, including historical ones.
        history.commit(false, NativeUnits::new());
        assert_eq!(history.retained_epochs(), 0);
        assert_eq!(history.payload_bytes(), 0);
        assert_eq!(checkpoint.retained_epochs(), 4);
    }

    #[test]
    fn native_history_payload_cap_drops_only_optional_candidates() {
        let first = product(42);
        let second = product(43);
        let cap = first.products.retained_payload_bytes();
        assert!(cap > 0 && second.products.retained_payload_bytes() > 0);
        let mut history = NativeProductHistory::default();
        history.commit_with_cap(
            true,
            [(owner(0), first.clone()), (owner(1), second)].into(),
            cap,
        );
        assert_eq!(history.payload_bytes(), cap);
        assert_eq!(
            history.candidates(&owner(0)).0.unwrap().products,
            first.products
        );
        assert!(history.candidates(&owner(1)).0.is_none());
        history.commit_with_cap(true, units(&first), cap);
        assert_eq!(
            history.retained_epochs(),
            1,
            "same opaque owners deduplicate the epoch"
        );
        history.commit_with_cap(true, units(&first), 0);
        assert_eq!(history.payload_bytes(), 0);
        assert!(history.candidates(&owner(0)).0.is_none());
        assert!(history.candidates(&owner(0)).1.is_empty());
    }
}
