//! Pure callable-metadata reservation products, never validation authority.
//! The original visitor is the only cost producer and the only failure path.
use super::*;
use zil_base::Limits;
use zil_ir::{PersistentVec, TypedModule};

const HISTORY: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq)]
struct FunctionProduct {
    input: Arc<TypedFunction>,
    cost: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Product {
    // Own the immutable root: pointer equality cannot suffer allocator ABA.
    functions: PersistentVec<TypedFunction>,
    limits: Limits,
    cost: usize,
    per_function: Vec<Arc<FunctionProduct>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Cache {
    history: std::collections::VecDeque<Arc<Product>>,
}
#[derive(Clone, Debug)]
pub(in crate::emit) struct Pending(Arc<Product>);

fn functions(module: &TypedModule) -> Option<&PersistentVec<TypedFunction>> {
    // Exact dependency audit of stored_borrows::reserve_callable_metadata:
    // functions.len; each function's protocol entry/exits/output lengths,
    // storage_life, callable roles/lives, return_type, locals' types and values'
    // types. type_work/contract/shape read only those recursively owned values
    // and Limits.max_call_depth; no nominal/closure/resource lookup occurs.
    // Bind ALL function fields via their owning immutable root, and ALL Limits.
    // Closures/resources have separate visitor loops, so refuse either domain.
    // Other module fields below are not read by that visitor. They remain
    // independently checked by current Verify, bind_lir and later consumers.
    let TypedModule {
        functions,
        closures,
        resources,
        dyn_declarations: _,
        dyn_interfaces: _,
        dyn_vtables: _,
        life_declarations: _,
        life_instances: _,
        schema_version: _,
        module_id: _,
        source_fingerprint: _,
        target: _,
        unsupported: _,
    } = module;
    (closures.is_empty() && resources.is_empty()).then_some(functions)
}
impl Cache {
    fn reserve(
        &self,
        module: &TypedModule,
        limits: &Limits,
        work: &mut usize,
    ) -> CompileResult<Option<Pending>> {
        let Some(owner) = functions(module) else {
            stored_borrows::reserve_callable_metadata(module, limits, work)?;
            return Ok(None);
        };
        if let Some(product) = self
            .history
            .iter()
            .find(|p| p.functions.ptr_eq(owner) && p.limits == *limits)
        {
            if *work >= product.cost {
                *work -= product.cost;
                return Ok(Some(Pending(product.clone())));
            }
            // Preserve every original partial deduction and first error when
            // the complete known cost does not fit. Never report it ourselves.
            stored_borrows::reserve_callable_metadata(module, limits, work)?;
            return Ok(None);
        }
        // A root edit is not a whole-project query hit: replay each unchanged
        // owning function in ORIGINAL ORDER, visiting only changed owners.
        // Every key remains backed by a strong immutable owner (no ABA).
        let before = *work;
        stored_borrows::reserve_callable_metadata_header(module, work)?;
        let mut previous = std::collections::BTreeMap::new();
        for root in &self.history {
            if root.limits == *limits {
                for product in &root.per_function {
                    previous
                        .entry(Arc::as_ptr(&product.input))
                        .or_insert(product);
                }
            }
        }
        let mut per_function = Vec::with_capacity(owner.len());
        for (index, function) in owner.iter().enumerate() {
            let input = owner
                .owner(index)
                .expect("iterated persistent function owner");
            if let Some(product) = previous.get(&Arc::as_ptr(input))
                && Arc::ptr_eq(&product.input, input)
                && *work >= product.cost
            {
                *work -= product.cost;
                per_function.push(Arc::clone(*product));
            } else {
                let old = *work;
                stored_borrows::reserve_callable_function_metadata(function, limits, work)?;
                per_function.push(Arc::new(FunctionProduct {
                    input: input.clone(),
                    cost: old - *work,
                }));
            }
        }
        Ok(Some(Pending(Arc::new(Product {
            functions: owner.clone(),
            limits: limits.clone(),
            cost: before - *work,
            per_function,
        }))))
    }
    fn remember(&mut self, pending: Option<Pending>) {
        let Some(Pending(product)) = pending else {
            return;
        };
        // Promotions too are committed only after successful complete lowering.
        // A failed metadata, stored-binding, LIR bind or emitter request leaves
        // the successful history (including its LRU order) untouched.
        if let Some(index) = self
            .history
            .iter()
            .position(|p| p.functions.ptr_eq(&product.functions) && p.limits == product.limits)
        {
            self.history.remove(index);
        }
        self.history.push_front(product);
        self.history.truncate(HISTORY);
    }
}
impl Session {
    pub(in crate::emit) fn reserve_callable_metadata(
        &self,
        verified: &VerifiedModule,
        work: &mut usize,
    ) -> CompileResult<Option<Pending>> {
        self.metadata_budget
            .reserve(verified.module(), verified.limits(), work)
    }
    pub(in crate::emit) fn remember_callable_metadata(&mut self, pending: Option<Pending>) {
        self.metadata_budget.remember(pending);
    }
}

#[cfg(test)]
pub(crate) fn test_contract(original: &TypedModule, limits: &Limits) {
    // Raw cost-visitor probes only; no cached value grants a language capability.
    assert!(original.closures.is_empty() && original.resources.is_empty());
    let mut cache = Cache::default();
    let mut work = usize::MAX;
    let pending = cache.reserve(original, limits, &mut work).unwrap();
    let cost = usize::MAX - work;
    assert!(cost > 0);
    assert!(cache.history.is_empty(), "reservation alone cannot publish");
    cache.remember(pending);
    let first = cache.history[0].clone();
    for budget in [cost.saturating_add(7), cost, cost - 1, 0] {
        let mut expected = budget;
        let fresh = stored_borrows::reserve_callable_metadata(original, limits, &mut expected);
        let mut actual = budget;
        let got = cache.reserve(original, limits, &mut actual);
        assert_eq!(got.as_ref().map(|_| ()), fresh.as_ref().map(|_| ()));
        assert_eq!(actual, expected, "partial ledger at {budget}");
        if let Ok(Some(Pending(product))) = got {
            assert!(
                Arc::ptr_eq(&first, &product),
                "successful matching owner must actually replay"
            );
        }
        assert!(Arc::ptr_eq(&first, &cache.history[0]));
    }
    let mut other_limits = limits.clone();
    other_limits.max_nodes = other_limits.max_nodes.saturating_add(1);
    let mut work = usize::MAX;
    let pending = cache.reserve(original, &other_limits, &mut work).unwrap();
    assert!(
        !Arc::ptr_eq(&first, &pending.as_ref().unwrap().0),
        "ALL Limits fields bind even when visitor does not read that field"
    );
    // Equal values in independently allocated roots do not match an owner.
    let mut equal = original.clone();
    equal.functions = original.functions.iter().cloned().collect();
    assert!(!equal.functions.ptr_eq(&original.functions));
    let mut work = usize::MAX;
    let pending = cache.reserve(&equal, limits, &mut work).unwrap();
    assert!(!Arc::ptr_eq(&first, &pending.as_ref().unwrap().0));
    // Changes beneath an owned persistent root force COW and the original scan.
    let mut changed = original.clone();
    changed.functions[0].values.push(Type::Array {
        element: Box::new(Type::I64),
        length: 1,
    });
    assert!(!changed.functions.ptr_eq(&original.functions));
    let mut work = usize::MAX;
    let pending = cache.reserve(&changed, limits, &mut work).unwrap();
    assert!(!Arc::ptr_eq(&first, &pending.as_ref().unwrap().0));
    assert!(usize::MAX - work > cost);
    let edited = &pending.as_ref().unwrap().0;
    assert!(!Arc::ptr_eq(
        &first.per_function[0],
        &edited.per_function[0]
    ));
    for i in 1..original.functions.len() {
        assert!(
            Arc::ptr_eq(&first.per_function[i], &edited.per_function[i]),
            "unchanged function {i} must replay by actual owner"
        );
    }
    // Below/at edited total, per-function replay preserves the original ledger
    // and first error in sequence, including the changed nested type visitor.
    let edited_cost = usize::MAX - work;
    for budget in [edited_cost, edited_cost - 1, cost / 2] {
        let mut expected = budget;
        let fresh = stored_borrows::reserve_callable_metadata(&changed, limits, &mut expected);
        let mut actual = budget;
        let got = cache.reserve(&changed, limits, &mut actual);
        assert_eq!(got.as_ref().map(|_| ()), fresh.as_ref().map(|_| ()));
        assert_eq!(actual, expected);
    }
    // A newly invalid depth still gets exactly the original first error/ledger;
    // a failed visitor supplies no pending product and cannot evict history.
    let mut shallow = limits.clone();
    shallow.max_call_depth = 0;
    let mut expected = usize::MAX;
    let fresh = stored_borrows::reserve_callable_metadata(&changed, &shallow, &mut expected);
    assert!(fresh.is_err());
    let mut actual = usize::MAX;
    assert_eq!(
        cache.reserve(&changed, &shallow, &mut actual).unwrap_err(),
        fresh.unwrap_err()
    );
    assert_eq!(actual, expected);
    assert!(Arc::ptr_eq(&first, &cache.history[0]));
    // Unread module identity fields may vary; the cached object is ONLY a cost.
    let mut identity = original.clone();
    identity.source_fingerprint = zil_base::Fingerprint::of(b"other diagnostic owner");
    identity.module_id = zil_base::ModuleId(99);
    identity.target = TargetProfile::windows_x64_llvm();
    let mut work = usize::MAX;
    assert!(Arc::ptr_eq(
        &first,
        &cache
            .reserve(&identity, limits, &mut work)
            .unwrap()
            .unwrap()
            .0
    ));
    // Separate metadata inventories always use the original complete visitor.
    let mut resource = original.clone();
    resource.resources.push(ResourceDefinition {
        name: "unused-budget-probe".into(),
        payload: Type::I64,
        teardown: None,
    });
    let mut expected = usize::MAX;
    stored_borrows::reserve_callable_metadata(&resource, limits, &mut expected).unwrap();
    let mut actual = usize::MAX;
    assert!(
        cache
            .reserve(&resource, limits, &mut actual)
            .unwrap()
            .is_none()
    );
    assert_eq!(actual, expected);
    let mut closure = original.clone();
    closure.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(99),
        function: FunctionId(0),
        captures: vec![Type::I32],
        parameters: vec![Type::I32],
        result: Type::I32,
        kind: zil_ir::CallableKind::Shared,
        lending_captures: vec![],
    });
    let mut expected = usize::MAX;
    stored_borrows::reserve_callable_metadata(&closure, limits, &mut expected).unwrap();
    let mut actual = usize::MAX;
    assert!(
        cache
            .reserve(&closure, limits, &mut actual)
            .unwrap()
            .is_none()
    );
    assert_eq!(actual, expected);
    // Four successful roots, restore promotion and independent checkpoint roots.
    let roots = (0..5)
        .map(|i| {
            let mut m = original.clone();
            m.functions[0].name = format!("metadata-root-{i}");
            m
        })
        .collect::<Vec<_>>();
    let mut history = Cache::default();
    for root in &roots[..4] {
        let mut work = usize::MAX;
        let p = history.reserve(root, limits, &mut work).unwrap();
        history.remember(p);
    }
    let checkpoint = history.clone();
    let mut work = usize::MAX;
    let restored = history.reserve(&roots[0], limits, &mut work).unwrap();
    assert!(Arc::ptr_eq(
        &checkpoint.history[3],
        &restored.as_ref().unwrap().0
    ));
    history.remember(restored);
    let mut work = usize::MAX;
    let p = history.reserve(&roots[4], limits, &mut work).unwrap();
    history.remember(p);
    assert_eq!(history.history.len(), 4);
    let mut work = usize::MAX;
    let evicted = history.reserve(&roots[1], limits, &mut work).unwrap();
    assert!(!Arc::ptr_eq(&checkpoint.history[2], &evicted.unwrap().0));
    assert_eq!(checkpoint.history.len(), 4);
}
