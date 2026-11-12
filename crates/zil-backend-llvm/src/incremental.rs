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
        self.summary = BackendSessionSummary::default();
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
    functions: Vec<zil_ir::TypedFunction>,
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
                functions: lir.functions.to_vec(),
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
