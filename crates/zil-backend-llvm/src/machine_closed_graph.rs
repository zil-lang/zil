//! Reduce the already classed CLOSED current machine graph before planning.
//! This carries raw values only. All original bodies, including dead bodies,
//! must pass current preflight before reachability can remove any of them.
use std::collections::BTreeSet;
use zil_ir::{LirView, PersistentVec, TypedFunction};
use zil_llvm_api::model::{Item, Module};

#[derive(Clone, Debug)]
pub(crate) struct Reduction {
    removed: BTreeSet<String>,
    pub(crate) copy_allowance: usize,
    pub(crate) witness: std::sync::Arc<String>,
    projection: std::sync::OnceLock<Projection>,
}
#[derive(Clone, Debug)]
struct Projection {
    input: PersistentVec<TypedFunction>,
    definitions: BTreeSet<String>,
    output: PersistentVec<TypedFunction>,
}
impl PartialEq for Reduction {
    fn eq(&self, other: &Self) -> bool {
        self.removed == other.removed
            && self.copy_allowance == other.copy_allowance
            && self.witness == other.witness
    }
}
impl Eq for Reduction {}

#[cfg(test)]
pub(crate) fn prune(
    module: &mut Module,
    lir: &LirView<'_>,
    classes: &mut BTreeSet<String>,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Option<Reduction>, zil_llvm_api::Error> {
    prune_with_preflight(
        module,
        lir,
        classes,
        protected,
        cancelled,
        &mut crate::machine_preflight::CurrentPreflight::default(),
    )
}

pub(crate) fn prune_with_preflight(
    module: &mut Module,
    lir: &LirView<'_>,
    classes: &mut BTreeSet<String>,
    protected: &BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
    preflight: &mut crate::machine_preflight::CurrentPreflight,
) -> Result<Option<Reduction>, zil_llvm_api::Error> {
    // These are actual current source definitions or freshly minted current
    // classes. Unknown/native/foreign symbols and protected roles stay roots.
    let mut source_names = BTreeSet::new();
    for function in lir.functions.iter().filter(|f| f.foreign.is_none()) {
        source_names.insert(format!("zil.fn.{}", function.id.0));
        source_names.insert(format!("zil.fn.{}.lc1", function.id.0));
    }
    let mut candidates = classes.clone();
    candidates.extend(source_names.iter().cloned());
    candidates.retain(|name| !protected.contains(name));
    // The actual full current source graph supplies the original copy cap,
    // including protected sources, before any definition is removed.
    let copy_allowance = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) if f.blocks.is_some() && source_names.contains(&f.name) => {
                Some(f.cost())
            }
            _ => None,
        })
        .fold(0usize, usize::saturating_add);
    let Some(witness) = witness(module, copy_allowance) else {
        return Ok(None);
    };
    // The private closed domain affects reachability only. Validate the actual
    // COMPLETE classed original before deletion; no temporary linkage model or
    // restoration pass is needed, and malformed dead bodies remain visible.
    let Some(stats) = crate::machine_closed_prune::prune_closed_graph_with_preflight(
        module,
        classes,
        &candidates,
        cancelled,
        preflight,
    )?
    else {
        return Ok(None);
    };
    // The pruning consumer performs the final cancellation check before commit.
    let live_classes = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(function) if classes.contains(&function.name) => {
                Some(function.name.clone())
            }
            _ => None,
        })
        .collect();
    *classes = live_classes;
    Ok(Some(Reduction {
        removed: stats.removed_names.into_iter().collect(),
        copy_allowance,
        witness: std::sync::Arc::new(witness),
        projection: std::sync::OnceLock::new(),
    }))
}

// An exact, compact diagnostic snapshot of the actual class-production boundary.
// The qualifier reconstructs every item and checks full source/class body Eq
// before later range/scalar transforms, then binds the final live ABI and roles.
// This is ordinary diagnostic text, never loaded as a permission or machine IR.
fn witness(module: &Module, copy_allowance: usize) -> Option<String> {
    use std::collections::HashMap;
    use zil_base::json_string;
    use zil_llvm_api::model::Function;
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    const OUTER: &str = "__zil_witness_outer__";
    let mut owners: HashMap<usize, Vec<(&Function, usize)>> = HashMap::new();
    let mut templates = Vec::new();
    let mut rows = Vec::with_capacity(module.items.len());
    let mut bytes = 0usize;
    for item in &module.items {
        let (name, selected) = if let Item::Function(f) = item {
            let owner = f.blocks.as_ref().map(|b| std::ptr::from_ref(&**b) as usize);
            let selected = owner.and_then(|owner| owners.get(&owner)).and_then(|rows| {
                rows.iter()
                    .find(|(old, _)| {
                        old.linkage == f.linkage
                            && old.signature == f.signature
                            && old.attrs == f.attrs
                            && old
                                .blocks
                                .as_ref()
                                .unwrap()
                                .ptr_eq(f.blocks.as_ref().unwrap())
                    })
                    .map(|(_, index)| *index)
            });
            let index = if let Some(index) = selected {
                index
            } else {
                let mut template = f.clone();
                template.name = OUTER.into();
                let text = zil_llvm_api::model::item_to_text(&Item::Function(template));
                let encoded = json_string(&text);
                bytes = bytes.checked_add(encoded.len())?;
                if bytes > MAX_BYTES {
                    return None;
                }
                let index = templates.len();
                templates.push(encoded);
                if let Some(owner) = owner {
                    owners.entry(owner).or_default().push((f, index));
                }
                index
            };
            (json_string(&f.name), index)
        } else {
            let encoded = json_string(&zil_llvm_api::model::item_to_text(item));
            bytes = bytes.checked_add(encoded.len())?;
            if bytes > MAX_BYTES {
                return None;
            }
            let index = templates.len();
            templates.push(encoded);
            ("null".into(), index)
        };
        let row = format!("[{name},{selected}]");
        bytes = bytes.checked_add(row.len())?;
        if bytes > MAX_BYTES {
            return None;
        }
        rows.push(row);
    }
    Some(format!(
        "; zil-current-class-input-v1 {{\"templates\":[{}],\"items\":[{}],\"copy_allowance\":{copy_allowance}}}\n",
        templates.join(","),
        rows.join(",")
    ))
}

/// A private partition input projection AFTER current whole-graph validation
/// and pruning. It grants no lowering permission and never replaces the full
/// VerifiedModule/LIR binding or the final native-unit checks.
impl Reduction {
    pub(crate) fn source_functions(
        &self,
        module: &Module,
        lir: &LirView<'_>,
    ) -> Option<PersistentVec<TypedFunction>> {
        let definitions = module
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(f) = item else { return None };
                f.blocks.is_some().then_some(f.name.clone())
            })
            .collect::<BTreeSet<_>>();
        if let Some(projection) = self.projection.get()
            && projection.input.ptr_eq(lir.functions)
            && projection.definitions == definitions
        {
            return Some(projection.output.clone());
        }
        let mut functions = PersistentVec::default();
        for (index, function) in lir.functions.iter().enumerate() {
            if function.foreign.is_some()
                || definitions.contains(format!("zil.fn.{}", function.id.0).as_str())
                || definitions.contains(format!("zil.fn.{}.lc1", function.id.0).as_str())
            {
                functions.push_owner(
                    lir.functions
                        .owner(index)
                        .expect("current function owner")
                        .clone(),
                );
            } else if !self.removed.contains(&format!("zil.fn.{}", function.id.0))
                && !self
                    .removed
                    .contains(&format!("zil.fn.{}.lc1", function.id.0))
            {
                // Missing output is not proof of deadness.
                return None;
            }
        }
        let _ = self.projection.set(Projection {
            input: lir.functions.clone(),
            definitions,
            output: functions.clone(),
        });
        Some(functions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, Span, TargetProfile};
    use zil_ir::{BlockId, CallKind, FunctionId, LirModule, PassingMode, Type};
    use zil_llvm_api::model::{
        Arg, Block, Function, Global, Inst, Linkage, Op, Signature, Ty, Val,
    };

    fn source() -> LirModule {
        LirModule {
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            verified_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            functions: (0..4)
                .map(|id| TypedFunction {
                    storage_life: None,
                    call_kind: CallKind::Direct,
                    reference_return_bound: None,
                    callable_return_contract: None,
                    callable_contract: None,
                    protocol_contract: None,
                    result_mode: PassingMode::Infer,
                    foreign: None,
                    id: FunctionId(id),
                    name: if id == 0 {
                        "main".into()
                    } else {
                        format!("source{id}")
                    },
                    environment: None,
                    parameters: vec![],
                    return_type: Type::I32,
                    locals: vec![].into(),
                    values: vec![].into(),
                    blocks: vec![].into(),
                    entry: BlockId(0),
                    span: Span::default(),
                })
                .collect(),
            closures: vec![],
            resources: vec![],
            layouts: vec![],
        }
    }
    fn function(name: &str, target: Option<&str>) -> Item {
        let mut instructions = vec![];
        let returned = if let Some(target) = target {
            instructions.push(Inst::new(
                Some("value".into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(target.into()),
                    args: vec![],
                    vararg: None,
                },
            ));
            Val::Local("value".into())
        } else {
            Val::Int(7)
        };
        instructions.push(Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), returned))),
        ));
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions,
                }]
                .into(),
            ),
        })
    }
    fn module() -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current".into(),
            items: vec![
                function("main", Some("zil.fn.0")),
                function("zil.fn.0", Some("zil.fn.1")),
                function("zil.fn.1", None),
                function("zil.fn.2", None),
                function("zil.fn.3", None),
            ]
            .into(),
        }
    }
    #[test]
    fn current_closed_reduction_preserves_protected_roles_and_partition_copy_cap() {
        let mut source = source();
        let protected = BTreeSet::from(["main".into(), "zil.fn.0".into()]);
        let mut model = module();
        zil_llvm_api::validate_module(&model).unwrap();
        let (_, mut classes, _) = crate::machine_call_classes::canonicalize_stable_cached(
            &mut model,
            &protected,
            &|| false,
            None,
        )
        .unwrap();
        assert_eq!(classes.len(), 1);
        let reduction = prune(
            &mut model,
            &source.as_view(),
            &mut classes,
            &protected,
            &|| false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(reduction.removed.len(), 3);
        assert!(
            model
                .items
                .iter()
                .all(|item| !matches!(item, Item::Function(f)
            if matches!(f.name.as_str(), "zil.fn.1" | "zil.fn.2" | "zil.fn.3")))
        );
        assert!(
            model
                .items
                .iter()
                .all(|item| !matches!(item, Item::Function(f)
            if f.blocks.is_some() && f.linkage != Linkage::External))
        );
        zil_llvm_api::validate_module(&model).unwrap();
        let name = classes.first().unwrap();
        let hash = Fingerprint::of(name.as_bytes()).0;
        let bucket = u64::from_le_bytes(hash[..8].try_into().unwrap()) as usize % 16;
        source.functions[0].span.file = FileId(((bucket + 1) % 16) as u32);
        let functions = reduction
            .source_functions(&model, &source.as_view())
            .unwrap();
        assert_eq!(
            functions.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![FunctionId(0)]
        );
        let again = reduction
            .source_functions(&model, &source.as_view())
            .unwrap();
        assert!(functions.ptr_eq(&again));
        let mut changed_input = source.clone();
        changed_input.functions[0].id = FunctionId(9999);
        assert!(
            reduction
                .source_functions(&model, &changed_input.as_view())
                .is_none()
        );
        assert!(
            functions.owner(0).unwrap().as_ref() == source.functions.owner(0).unwrap().as_ref()
        );
        let full = source.as_view();
        let mut projected = full.clone();
        projected.functions = &functions;
        let build = |allowance| {
            crate::machine_cgu::partition_classes_reusing_with_policy(
                model.clone(),
                &projected,
                crate::EntryPoint::Main,
                16,
                &classes,
                None,
                None,
                crate::machine_cgu::CallImportPolicy::BoundedClasses(Some(allowance)),
            )
        };
        let accepted = build(reduction.copy_allowance).unwrap();
        assert!(accepted.plan.fallback.is_none());
        assert_eq!(accepted.plan.units.len(), 2);
        assert!(accepted.plan.units.iter().all(|unit| {
            unit.items
                .iter()
                .any(|item| matches!(item, Item::Function(f) if f.blocks.is_some()))
        }));
        assert!(build(0).is_none());
        let identity = (std::sync::Arc::new(()), PersistentVec::default());
        let input = crate::incremental::PartitionInput::new(
            &crate::BuildOptions::new("program"),
            None,
            false,
            &full,
            &model,
            Some(identity),
        )
        .with_closed_graph(Some(&reduction));
        assert!(input.same_completed_output(&input));
        let mut smaller = reduction.clone();
        smaller.copy_allowance = 0;
        let changed = input.clone().with_closed_graph(Some(&smaller));
        assert_ne!(input, changed);
        assert!(!input.same_completed_output(&changed));
        assert!(!input.same_build_options(&changed));
        // A missing source output with no actual deletion is never admitted.
        let mut absent = model.clone();
        absent
            .items
            .retain(|item| !matches!(item, Item::Function(f) if f.name == "zil.fn.0"));
        assert!(reduction.source_functions(&absent, &full).is_none());
    }
    #[test]
    fn addresses_dead_malformed_models_and_cancellation_cannot_lend_deletion_authority() {
        let source = source();
        let protected = BTreeSet::from(["main".into(), "zil.fn.0".into()]);
        let mut raw = module();
        raw.items.push(Item::Global(Global {
            name: "address".into(),
            linkage: Linkage::Private,
            unnamed_addr: false,
            constant: true,
            ty: Ty::Ptr,
            initializer: Some(Val::Global("zil.fn.2".into())),
            alignment: 0,
        }));
        let mut classes = BTreeSet::new();
        let reduction = prune(
            &mut raw,
            &source.as_view(),
            &mut classes,
            &protected,
            &|| false,
        )
        .unwrap()
        .unwrap();
        assert!(!reduction.removed.contains("zil.fn.2"));
        let mut malformed = module();
        let Item::Function(dead) = &mut malformed.items[4] else {
            unreachable!()
        };
        dead.blocks.as_mut().unwrap()[0].instructions[0].op = Op::Return(None);
        let before = malformed.clone();
        assert!(
            prune(
                &mut malformed,
                &source.as_view(),
                &mut classes,
                &protected,
                &|| false
            )
            .is_err()
        );
        assert_eq!(malformed, before);
        let mut cancelled = module();
        let before = cancelled.clone();
        assert!(
            prune(
                &mut cancelled,
                &source.as_view(),
                &mut classes,
                &protected,
                &|| true
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(cancelled, before);
    }
}
