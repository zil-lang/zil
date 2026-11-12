//! Ordinary Sum facts reconstructed from executable instructions. Branch maps
//! describe possible alternatives, never simultaneous resource debts.
use super::*;
use std::sync::Arc;
mod direct_index;
#[cfg(test)]
mod direct_index_reference;
mod partial_places;
#[cfg(test)]
mod sorted_join_tests;
mod ssa_values;
#[cfg(test)]
mod tag_forgetting_tests;

pub(crate) fn owned(ty: &Type) -> bool {
    match ty {
        Type::Managed(_) => true,
        Type::Tuple(fields) => fields.iter().any(owned),
        Type::Package { fields, .. } => fields.iter().any(|(_, t)| owned(t)),
        Type::Enum { variants, .. } => variants.iter().any(|(_, fs)| fs.iter().any(owned)),
        Type::ErrorUnion { error, result } => owned(error) || owned(result),
        _ => false,
    }
}

pub(crate) fn admit(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(ty, true, "ordinary Sum admission")?;
    fn walk(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<(bool, bool)> {
        budget.spend(1, "ordinary Sum shape")?;
        if !ty.is_copy_value()
            && (super::super::stored_flow::pure_inline(module, ty, budget)?
                || super::super::stored_flow::pure_read(module, ty, budget)?)
        {
            return Ok((false, false));
        }
        match ty {
            Type::Managed(_) => {
                if dynamic_store::is_owned_carrier(ty) {
                    crate::ownership_store::ordinary_owned(module, ty, budget)?;
                    Ok((true, true))
                } else {
                    crate::budget::legacy_value(
                        ty,
                        module
                            .functions
                            .first()
                            .map_or(zil_base::Span::default(), |f| f.span),
                        module,
                    )?;
                    Ok((false, true))
                }
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len(), "ordinary Tuple fields")?;
                let mut store = false;
                let mut owner = false;
                for field in fields {
                    let (s, o) = walk(module, field, budget)?;
                    store |= s;
                    owner |= o;
                }
                Ok((store, owner))
            }
            Type::Package { name, fields } => {
                if name.is_empty() {
                    return Err(error("ordinary Product identity or owner is absent"));
                }
                budget.spend(fields.len(), "ordinary Sum Product fields")?;
                let mut names = BTreeSet::new();
                let mut store = false;
                let mut owner = false;
                for (n, t) in fields {
                    budget.lookup(names.len(), "ordinary Sum field identity")?;
                    budget.product(
                        n.len(),
                        names.len().saturating_add(1).ilog2() as usize + 2,
                        1,
                        "ordinary Sum field name comparisons",
                    )?;
                    if n.is_empty() || !names.insert(n) {
                        return Err(error("duplicate Product field"));
                    }
                    let (has_store, has_owner) = walk(module, t, budget)?;
                    store |= has_store;
                    owner |= has_owner;
                }
                if !owner {
                    return Err(error("ordinary Product has no transitive Managed leaf"));
                }
                Ok((store, owner))
            }
            Type::Enum { name, variants } => {
                if name.is_empty() || variants.is_empty() {
                    return Err(error("ordinary Enum identity or variants absent"));
                }
                let mut names = BTreeSet::new();
                let mut store = false;
                let mut owner = false;
                budget.spend(variants.len(), "ordinary Sum variants")?;
                for (n, fields) in variants {
                    budget.lookup(names.len(), "ordinary Sum variant identity")?;
                    budget.product(
                        n.len(),
                        names.len().saturating_add(1).ilog2() as usize + 2,
                        1,
                        "ordinary Sum variant name comparisons",
                    )?;
                    budget.spend(fields.len(), "ordinary Sum variant fields")?;
                    if n.is_empty() || !names.insert(n) {
                        return Err(error("duplicate Enum variant"));
                    }
                    for t in fields {
                        let (has_store, has_owner) = walk(module, t, budget)?;
                        store |= has_store;
                        owner |= has_owner;
                    }
                }
                Ok((store, owner))
            }
            Type::ErrorUnion { error: e, result } => {
                let owning_failure = matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}))
                    && matches!(e.as_ref(),Type::Package{name,fields} if name.starts_with("@std.task:v1:TaskCreateFailure:") && matches!(fields.as_slice(),[(reason,_),(inputs,Type::Tuple(_))] if reason=="reason" && inputs=="inputs"));
                if owning_failure {
                    let (a, ao) = walk(module, e, budget)?;
                    let (b, bo) = walk(module, result, budget)?;
                    return Ok((a || b, ao || bo));
                }
                // Direct Managed success keeps its existing Copy-error rule.
                // Other owning slots are complete Products/Enums; an owning
                // ErrorUnion is not itself a permitted ErrorUnion slot.
                if matches!(result.as_ref(), Type::Managed(_))
                    && !crate::ownership_store::pod_owned(e)
                {
                    budget.type_work(e, true, "Managed success Copy-error qualification")?;
                    if !e.is_copy_value() {
                        return Err(error("Managed success requires a Copy error payload"));
                    }
                } else {
                    for slot in [e.as_ref(), result.as_ref()] {
                        if !matches!(slot, Type::Package { .. } | Type::Enum { .. })
                            && !crate::ownership_store::pod_owned(slot)
                        {
                            budget.type_work(
                                slot,
                                true,
                                "ordinary ErrorUnion Copy-slot qualification",
                            )?;
                            if !slot.is_copy_value() {
                                return Err(error(
                                    "ordinary ErrorUnion slots require Copy, Product or Enum",
                                ));
                            }
                        }
                    }
                }
                let (a, b) = walk(module, e, budget)?;
                let (c, d) = walk(module, result, budget)?;
                Ok((a | c, b | d))
            }
            t if t.is_copy_value() => Ok((false, false)),
            _ => Err(error(
                "ordinary Sum contains an unsupported borrowed or owning shape",
            )),
        }
    }
    walk(module, ty, budget).map(|(store, _)| store)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Tree {
    missing: bool,
    node: Node,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Copy,
    Leaf(BTreeSet<Birth>),
    Product(Vec<Arc<Tree>>),
    Sum(BTreeMap<u32, Vec<Arc<Tree>>>),
}
impl Tree {
    fn live_async(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        b.spend(1, "actual asynchronous aggregate cleanup leaf")?;
        if self.missing {
            return Ok(false);
        }
        match (&self.node, ty) {
            (Node::Leaf(_), Type::Managed(t)) => {
                Ok(matches!(t.as_ref(), Type::AsyncRegistration { .. }))
            }
            (Node::Product(children), Type::Tuple(fields)) => {
                for (child, ty) in children.iter().zip(fields) {
                    if child.live_async(ty, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (Node::Product(children), Type::Package { fields, .. }) => {
                for (child, (_, ty)) in children.iter().zip(fields) {
                    if child.live_async(ty, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (
                Node::Sum(variants),
                Type::Enum {
                    variants: types, ..
                },
            ) => {
                for (index, children) in variants {
                    for (child, ty) in children.iter().zip(&types[*index as usize].1) {
                        if child.live_async(ty, b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            (Node::Sum(variants), Type::ErrorUnion { error, result }) => {
                for (index, children) in variants {
                    if let Some(child) = children.first()
                        && child.live_async(if *index == 0 { result } else { error }, b)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn live_task(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        b.spend(1, "actual Task aggregate cleanup leaf")?;
        if self.missing {
            return Ok(false);
        }
        match (&self.node, ty) {
            (Node::Leaf(_), Type::Managed(_)) => Ok(true),
            (Node::Product(children), Type::Tuple(fields)) => {
                for (child, ty) in children.iter().zip(fields) {
                    if child.live_task(ty, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (Node::Product(children), Type::Package { fields, .. }) => {
                for (child, (_, ty)) in children.iter().zip(fields) {
                    if child.live_task(ty, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (
                Node::Sum(variants),
                Type::Enum {
                    variants: types, ..
                },
            ) => {
                for (index, children) in variants {
                    for (child, ty) in children.iter().zip(&types[*index as usize].1) {
                        if child.live_task(ty, b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            (Node::Sum(variants), Type::ErrorUnion { error, result }) => {
                for (index, children) in variants {
                    if let Some(child) = children.first()
                        && child.live_task(if *index == 0 { result } else { error }, b)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // Opaque owner/closure storage is outside the selected Sum proof.
            // Preserve the existing full actual-origin cleanup obligation.
            _ => Ok(!ty.is_copy_value()),
        }
    }
    fn unknown(ty: &Type, budget: &mut Budget) -> CompileResult<Arc<Self>> {
        budget.spend(2, "Sum tree allocation")?;
        let node = match ty {
            Type::Managed(_) => Node::Leaf(BTreeSet::from([Birth::Incoming])),
            Type::Tuple(fields) => {
                budget.spend(fields.len(), "Tuple tree children")?;
                Node::Product(
                    fields
                        .iter()
                        .map(|t| Self::unknown(t, budget))
                        .collect::<CompileResult<_>>()?,
                )
            }
            Type::Package { fields, .. } => {
                budget.spend(fields.len(), "Sum Product children")?;
                Node::Product(
                    fields
                        .iter()
                        .map(|(_, t)| Self::unknown(t, budget))
                        .collect::<CompileResult<_>>()?,
                )
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "Sum alternatives allocation")?;
                let mut map = BTreeMap::new();
                for (i, (_, fs)) in variants.iter().enumerate() {
                    budget.spend(fs.len(), "Sum fields allocation")?;
                    map.insert(
                        i as u32,
                        fs.iter()
                            .map(|t| Self::unknown(t, budget))
                            .collect::<CompileResult<_>>()?,
                    );
                }
                Node::Sum(map)
            }
            Type::ErrorUnion { error, result } => Node::Sum(BTreeMap::from([
                (0, vec![Self::unknown(result, budget)?]),
                (1, vec![Self::unknown(error, budget)?]),
            ])),
            _ => Node::Copy,
        };
        Ok(Arc::new(Self {
            missing: false,
            node,
        }))
    }
    fn cost(&self, budget: &mut Budget) -> CompileResult<()> {
        // Arc::make_mut copies only this node and its immediate child refs.
        budget.spend(2, "Sum COW node copy")?;
        match &self.node {
            Node::Leaf(s) => budget.spend(s.len(), "Sum COW leaf origins")?,
            Node::Product(fs) => budget.spend(fs.len(), "Sum COW Product refs")?,
            Node::Sum(vs) => {
                budget.spend(vs.len(), "Sum COW tag map")?;
                for fs in vs.values() {
                    budget.spend(fs.len(), "Sum COW field refs")?;
                }
            }
            Node::Copy => {}
        }
        Ok(())
    }
    fn copied(self: &Arc<Self>, budget: &mut Budget) -> CompileResult<Arc<Self>> {
        budget.spend(1, "Sum immutable fact share")?;
        Ok(Arc::clone(self))
    }
    /// A readonly proof that the ORIGINAL merge would keep every fact. A
    /// false result is inconclusive (new information or a shape error): callers
    /// must use the complete original merge, including its error mutations.
    fn kept_by_join(
        self: &Arc<Self>,
        other: &Arc<Self>,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        budget.spend(1, "Sum readonly join identity")?;
        if Arc::ptr_eq(self, other) {
            return Ok(true);
        }
        budget.spend(1, "Sum readonly join node")?;
        if !self.missing && other.missing {
            return Ok(false);
        }
        match (&self.node, &other.node) {
            (Node::Copy, Node::Copy) => Ok(true),
            (Node::Leaf(old), Node::Leaf(incoming)) => {
                // Sorted subset uses each iterator at most once. A contains
                // loop would need logarithmic charges for every incoming atom.
                budget.spend(
                    old.len().saturating_add(incoming.len()),
                    "Sum readonly origin subset",
                )?;
                let mut old = old.iter().peekable();
                for origin in incoming {
                    while old.peek().is_some_and(|at| *at < origin) {
                        old.next();
                    }
                    if old.next() != Some(origin) {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Node::Product(old), Node::Product(incoming)) => {
                if old.len() != incoming.len() {
                    return Ok(false);
                }
                for (old, incoming) in old.iter().zip(incoming) {
                    if !old.kept_by_join(incoming, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Node::Sum(old), Node::Sum(incoming)) => {
                for (tag, incoming) in incoming {
                    budget.lookup(old.len(), "Sum readonly alternative lookup")?;
                    let Some(old) = old.get(tag) else {
                        return Ok(false);
                    };
                    if old.len() != incoming.len() {
                        return Ok(false);
                    }
                    for (old, incoming) in old.iter().zip(incoming) {
                        if !old.kept_by_join(incoming, budget)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn merge(self: &mut Arc<Self>, other: &Arc<Self>, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum shared join identity")?;
        if Arc::ptr_eq(self, other) {
            return Ok(false);
        }
        edit(self, budget)?.merge_body(other, budget)
    }
    fn narrow(
        self: &mut Arc<Self>,
        path: &[VariantProjection],
        tag: u32,
        equal: bool,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        edit(self, budget)?.narrow_body(path, tag, equal, budget)
    }
    fn preserve_missing(self: &mut Arc<Self>, old: &Tree, b: &mut Budget) -> CompileResult<()> {
        b.spend(1, "retain moved subtree markers across tag invalidation")?;
        let at = edit(self, b)?;
        at.missing |= old.missing;
        match (&mut at.node, &old.node) {
            (Node::Product(a), Node::Product(c)) => {
                for (a, c) in a.iter_mut().zip(c) {
                    a.preserve_missing(c, b)?;
                }
            }
            (Node::Sum(a), Node::Sum(c)) => {
                for (tag, fs) in c {
                    b.lookup(a.len(), "invalidation variant lookup")?;
                    if let Some(target) = a.get_mut(tag) {
                        for (a, c) in target.iter_mut().zip(fs) {
                            a.preserve_missing(c, b)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn forget_addressed_copy_tags(
        self: &mut Arc<Self>,
        ty: &Type,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        // The ordinary owning-error/borrowed-Option composition is found by
        // independent paid raw-type and actual error-place walks. It only
        // schedules this equivalent tag operation; it supplies no tag fact.
        let mixed = budget.store_option_results
            && (budget.task_borrowed_results || budget.ordinary_error_places);
        if budget.pod_stores || mixed {
            budget.spend(1, "Sum terminal tag classification")?;
            // Actual private facts already represent ordinary Record payloads
            // as Copy and managed headers as Leaf. Neither stores tag facts.
            // Do not rescan their complete types after every unrelated write.
            if matches!(self.node, Node::Copy | Node::Leaf(_)) {
                return Ok(());
            }
        }
        // This method starts at an immutable local type and descends only
        // through that type's actual children. Product nodes have no tag to
        // classify; the recursive walk below pays for their real children.
        let copy_sum = match &self.node {
            Node::Sum(_)
                if (budget.has_owned_store_package() || mixed)
                    && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) =>
            {
                budget.addressed_copy_sum(ty)?
            }
            Node::Sum(_) if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) => {
                budget.type_work(ty, true, "mutable local Enum tag knowledge")?;
                ty.is_copy_value()
            }
            Node::Product(_) if budget.has_owned_store_package() || mixed => {
                budget.spend(1, "POD addressed Product tag descent")?;
                false
            }
            _ => {
                // Preserve the established fee path outside the newer POD
                // Store profile; its terminal facts still have no tag.
                budget.type_work(ty, true, "mutable local Enum tag knowledge")?;
                false
            }
        };
        if copy_sum {
            // The exact existing unknown-tag shape is a fixed point of this
            // transfer, independently of which ordinary profile scheduled it.
            // Preserve every moved marker; no completeness fact is inferred.
            if self.tags_unknown(ty, budget)? {
                return Ok(());
            }
            let old = self.copied(budget)?;
            let mut unknown = Self::unknown(ty, budget)?;
            unknown.preserve_missing(&old, budget)?;
            *self = unknown;
            return Ok(());
        }
        match (&mut edit(self, budget)?.node, ty) {
            (Node::Sum(vs), Type::Enum { variants, .. }) => {
                for (tag, fields) in vs {
                    for (child, t) in fields.iter_mut().zip(&variants[*tag as usize].1) {
                        child.forget_addressed_copy_tags(t, budget)?;
                    }
                }
            }
            (Node::Sum(vs), Type::ErrorUnion { error, result }) => {
                for (tag, fields) in vs {
                    if let Some(child) = fields.first_mut() {
                        child.forget_addressed_copy_tags(
                            if *tag == 1 { error } else { result },
                            budget,
                        )?;
                    }
                }
            }
            (Node::Product(children), Type::Tuple(fields)) => {
                for (child, ty) in children.iter_mut().zip(fields) {
                    child.forget_addressed_copy_tags(ty, budget)?;
                }
            }
            (Node::Product(children), Type::Package { fields, .. }) => {
                for (child, (_, t)) in children.iter_mut().zip(fields) {
                    child.forget_addressed_copy_tags(t, budget)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn tags_unknown(&self, ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum existing unknown tag shape")?;
        match (&self.node, ty) {
            // Match Tree::unknown's actual terminal representation. A Leaf
            // may carry different births, so conservatively rebuild it even
            // for Managed rather than treating its node kind as equality.
            (Node::Copy, ty) => Ok(!matches!(
                ty,
                Type::Managed(_)
                    | Type::Tuple(_)
                    | Type::Package { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
            )),
            (Node::Leaf(_), _) => Ok(false),
            (Node::Product(children), Type::Tuple(fields)) => {
                if children.len() != fields.len() {
                    return Ok(false);
                }
                for (child, ty) in children.iter().zip(fields) {
                    if !child.tags_unknown(ty, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Node::Product(children), Type::Package { fields, .. }) => {
                if children.len() != fields.len() {
                    return Ok(false);
                }
                for (child, (_, ty)) in children.iter().zip(fields) {
                    if !child.tags_unknown(ty, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Node::Sum(choices), Type::Enum { variants, .. }) => {
                if choices.len() != variants.len() {
                    return Ok(false);
                }
                for ((tag, children), (index, (_, fields))) in
                    choices.iter().zip(variants.iter().enumerate())
                {
                    budget.spend(1, "Sum existing alternative identity")?;
                    if *tag as usize != index || children.len() != fields.len() {
                        return Ok(false);
                    }
                    for (child, ty) in children.iter().zip(fields) {
                        if !child.tags_unknown(ty, budget)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            (Node::Sum(choices), Type::ErrorUnion { error, result }) => {
                if choices.len() != 2 {
                    return Ok(false);
                }
                for ((tag, children), (index, ty)) in choices
                    .iter()
                    .zip([result.as_ref(), error.as_ref()].into_iter().enumerate())
                {
                    budget.spend(1, "Sum existing union alternative identity")?;
                    if *tag as usize != index
                        || children.len() != 1
                        || !children[0].tags_unknown(ty, budget)?
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn forget_copy_endpoint(
        self: &mut Arc<Self>,
        path: &[VariantProjection],
        ty: &Type,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        if path.is_empty() {
            *self = Self::unknown(ty, budget)?;
            return Ok(());
        }
        let Node::Sum(vs) = &mut edit(self, budget)?.node else {
            return Err(error("mutable payload path crosses non-Enum"));
        };
        if let Some(fs) = vs.get_mut(&path[0].variant) {
            let field = fs
                .get_mut(path[0].index as usize)
                .ok_or_else(|| error("mutable payload field absent"))?;
            field.forget_copy_endpoint(&path[1..], ty, budget)?;
        }
        Ok(())
    }
    fn complete(&self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum completeness")?;
        if self.missing {
            return Ok(false);
        }
        match &self.node {
            Node::Product(fs) => {
                for f in fs {
                    if !f.complete(budget)? {
                        return Ok(false);
                    }
                }
            }
            Node::Sum(vs) => {
                if vs.is_empty() {
                    return Ok(false);
                }
                for fs in vs.values() {
                    for f in fs {
                        if !f.complete(budget)? {
                            return Ok(false);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(true)
    }
    fn no_live_owner(&self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum discharged owner tree")?;
        if self.missing {
            return Ok(true);
        }
        match &self.node {
            Node::Copy => Ok(true),
            Node::Leaf(_) => Ok(false),
            Node::Product(fields) => {
                budget.spend(fields.len(), "Sum discharged Product children")?;
                for field in fields {
                    if !field.no_live_owner(budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Node::Sum(variants) => {
                if variants.is_empty() {
                    return Ok(false);
                }
                budget.spend(variants.len(), "Sum discharged active alternatives")?;
                for fields in variants.values() {
                    budget.spend(fields.len(), "Sum discharged active fields")?;
                    for field in fields {
                        if !field.no_live_owner(budget)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
        }
    }
    fn merge_body(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum fact join")?;
        let mut changed = !self.missing && other.missing;
        self.missing |= other.missing;
        match (&mut self.node, &other.node) {
            (Node::Leaf(a), Node::Leaf(b)) => {
                budget.spend(a.len() + b.len(), "Sum leaf join")?;
                let n = a.len();
                a.extend(b);
                changed |= n != a.len();
            }
            (Node::Product(a), Node::Product(b)) => {
                if a.len() != b.len() {
                    return Err(error("Sum Product shape mismatch"));
                }
                for (a, b) in a.iter_mut().zip(b) {
                    changed |= a.merge(b, budget)?;
                }
            }
            (Node::Sum(a), Node::Sum(b)) => {
                for (tag, fs) in b {
                    budget.lookup(a.len(), "Sum tag join lookup")?;
                    if let Some(old) = a.get_mut(tag) {
                        if old.len() != fs.len() {
                            return Err(error("Sum variant shape mismatch"));
                        }
                        for (o, n) in old.iter_mut().zip(fs) {
                            changed |= o.merge(n, budget)?;
                        }
                    } else {
                        budget.spend(fs.len() + 1, "Sum alternative join allocation")?;
                        a.insert(*tag, fs.clone());
                        changed = true;
                    }
                }
            }
            (Node::Copy, Node::Copy) => {}
            _ => return Err(error("Sum fact has inconsistent shape")),
        }
        Ok(changed)
    }
    fn projected<'a>(
        &'a self,
        path: &[VariantProjection],
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<&'a Self> {
        let mut at = self;
        for p in path {
            budget.spend(1, "Sum projection prefix")?;
            if check && !at.complete(budget)? {
                return Err(error("Sum projection requires a complete live prefix"));
            }
            let Node::Sum(vs) = &at.node else {
                return Err(error("Sum path crosses a non-Enum"));
            };
            if check && (vs.len() != 1 || !vs.contains_key(&p.variant)) {
                return Err(error("Sum projection lacks the current selected tag"));
            }
            at = vs
                .get(&p.variant)
                .and_then(|fs| fs.get(p.index as usize))
                .ok_or_else(|| error("Sum path variant or field is absent"))?;
        }
        if check && !at.complete(budget)? {
            return Err(error("Sum projection endpoint is unavailable"));
        }
        Ok(at)
    }
    fn narrow_body(
        &mut self,
        path: &[VariantProjection],
        tag: u32,
        equal: bool,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        budget.spend(1, "Sum edge refinement")?;
        if let Some((p, rest)) = path.split_first() {
            let Node::Sum(vs) = &mut self.node else {
                return Err(error("tag prefix is not a Sum"));
            };
            let Some(fs) = vs.get_mut(&p.variant) else {
                return Ok(false);
            };
            let Some(child) = fs.get_mut(p.index as usize) else {
                return Err(error("tag field missing"));
            };
            return child.narrow(rest, tag, equal, budget);
        }
        let Node::Sum(vs) = &mut self.node else {
            return Err(error("tag endpoint is not a Sum"));
        };
        budget.spend(vs.len(), "Sum edge alternatives")?;
        vs.retain(|t, _| (*t == tag) == equal);
        Ok(!vs.is_empty())
    }
    fn birth(&self) -> Option<&BTreeSet<Birth>> {
        if let Node::Leaf(s) = &self.node {
            Some(s)
        } else {
            None
        }
    }
}

fn join_tables<K: Ord>(
    target: &mut BTreeMap<K, Arc<Tree>>,
    incoming: &BTreeMap<K, Arc<Tree>>,
    budget: &mut Budget,
) -> CompileResult<bool> {
    budget.spend(1, "Sum sorted inventory size comparison")?;
    if target.len() != incoming.len() {
        return Err(error("Sum joined state inventory differs"));
    }
    budget.spend(target.len(), "Sum sorted target inventory walk")?;
    budget.spend(incoming.len(), "Sum sorted incoming inventory walk")?;
    let mut changed = false;
    for ((key, tree), (other_key, other_tree)) in target.iter_mut().zip(incoming) {
        budget.spend(1, "Sum sorted inventory key comparison")?;
        if key != other_key {
            return Err(error("Sum joined state key differs"));
        }
        changed |= tree.merge(other_tree, budget)?;
    }
    Ok(changed)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Facts {
    owned_creation: bool,
    locals: direct_index::Locals,
    values: ssa_values::Values,
    captures: BTreeMap<u32, Arc<Tree>>,
    // Only observations of this current activation survive. A write kills all
    // its tokens, including old SSA snapshots reused at the same loop site.
    valid: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}
impl Facts {
    fn has_capture(&self, index: u32, budget: &mut Budget) -> CompileResult<bool> {
        if self.captures.is_empty() {
            return Ok(false);
        }
        budget.lookup(self.captures.len(), "Sum capture root membership")?;
        Ok(self.captures.contains_key(&index))
    }
    pub(super) fn require_capture_complete(
        &self,
        index: u32,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        if !self.capture(index, budget)?.complete(budget)? {
            return Err(error("repeatable Sum capture is incomplete"));
        }
        Ok(())
    }
    pub(super) fn may_live_task(
        &self,
        local: LocalId,
        ty: &Type,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        if !self.owned_creation {
            return Ok(true);
        }
        budget.lookup(self.locals.len(), "actual Task cleanup local lookup")?;
        match self.locals.get(&local) {
            Some(tree) => tree.live_task(ty, budget),
            None => Ok(true),
        }
    }
    pub(super) fn no_live_owner(&self, local: LocalId, budget: &mut Budget) -> CompileResult<bool> {
        budget.lookup(self.locals.len(), "Sum discharged local lookup")?;
        self.locals
            .get(&local)
            .ok_or_else(|| error("Sum discharged local absent"))?
            .no_live_owner(budget)
    }
    pub(super) fn cost(&self, budget: &mut Budget) -> CompileResult<()> {
        if self.locals.is_empty() && self.captures.is_empty() {
            return Ok(());
        }
        budget.spend(
            self.locals.len() + self.values.page_count() + self.captures.len() + self.valid.len(),
            "Sum state tables",
        )?;
        if let Some(p) = &self.partial {
            p.cost(budget)?;
        }
        // Map/page cloning shares immutable nodes; it never walks descendants.
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        if self.locals.is_empty() && self.captures.is_empty() {
            return Ok(false);
        }
        let mut change = false;
        if let (Some(a), Some(b)) = (&mut self.partial, &other.partial) {
            change |= a.join(b, budget)?;
        }
        // Same-function states have complete immutable inventories. Walk the
        // sorted maps together and check every key before merging its Tree.
        change |= self.locals.join(&other.locals, budget)?;
        change |= self.values.join(&other.values, budget)?;
        change |= join_tables(&mut self.captures, &other.captures, budget)?;
        budget.spend(
            self.valid.len() + other.valid.len(),
            "Sum observations must join",
        )?;
        let n = self.valid.len();
        self.valid.retain(|v| other.valid.contains(v));
        change |= n != self.valid.len();
        Ok(change)
    }
    fn value(&self, v: ValueId, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.values.len(), "Sum value source")?;
        self.values
            .get(&v)
            .ok_or_else(|| error("Sum value absent"))?
            .copied(budget)
    }
    fn local(&self, l: LocalId, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.locals.len(), "Sum local source")?;
        self.locals
            .get(&l)
            .ok_or_else(|| error("Sum local absent"))?
            .copied(budget)
    }
    fn capture(&self, index: u32, budget: &mut Budget) -> CompileResult<Arc<Tree>> {
        budget.lookup(self.captures.len(), "Sum capture source")?;
        self.captures
            .get(&index)
            .ok_or_else(|| error("Sum capture absent"))?
            .copied(budget)
    }
    fn invalidate(&mut self, l: LocalId, plan: &Plan, budget: &mut Budget) -> CompileResult<()> {
        budget.spend(self.valid.len(), "Sum observation invalidation")?;
        self.valid.retain(|v| {
            plan.observations
                .get(v)
                .is_none_or(|(local, _)| *local != l)
        });
        Ok(())
    }
    pub(super) fn require_place(
        &self,
        local: LocalId,
        path: &[zil_ir::PlaceProjection],
        budget: &mut Budget,
    ) -> CompileResult<()> {
        budget.lookup(self.locals.len(), "replacement initialized owner tree")?;
        if let Some(tree) = self.locals.get(&local) {
            tree.require_replacement(path, budget)?;
        }
        Ok(())
    }
    /// Synchronize only a complete direct Managed field of the new whole
    /// Package local operation. General tagged/Managed summaries keep their
    /// conservative union; no parent shell age or activity is reconstructed.
    pub(super) fn replace_package_birth(
        &mut self,
        local: LocalId,
        index: u32,
        birth: &BTreeSet<Birth>,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        budget.lookup(self.locals.len(), "complete Package exact origin tree")?;
        if let Some(tree) = self.locals.get_mut(&local) {
            budget.spend(1, "complete Package origin root")?;
            if check && tree.missing {
                return Err(error("Package replacement origin root is missing"));
            }
            let Node::Product(fields) = &mut edit(tree, budget)?.node else {
                return Err(error("Package replacement origin root is not a Product"));
            };
            let child = fields
                .get_mut(index as usize)
                .ok_or_else(|| error("Package replacement origin field is absent"))?;
            budget.spend(1, "complete Package origin child")?;
            if check && child.missing {
                return Err(error("Package replacement origin child is missing"));
            }
            let Node::Leaf(old) = &mut edit(child, budget)?.node else {
                return Err(error(
                    "Package replacement origin child is not a Managed leaf",
                ));
            };
            budget.spend(
                old.len().saturating_add(birth.len()),
                "complete Package exact leaf origin copy",
            )?;
            *old = birth.clone();
        }
        Ok(())
    }
    pub(super) fn replace_birth(
        &mut self,
        local: LocalId,
        path: &[zil_ir::PlaceProjection],
        birth: &BTreeSet<Birth>,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        budget.lookup(self.locals.len(), "replacement actual owner tree update")?;
        if let Some(tree) = self.locals.get_mut(&local) {
            tree.replace_leaf_birth(path, birth, check, budget)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn replace_product(
        &mut self,
        local: LocalId,
        rhs: ValueId,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        budget.lookup(
            self.locals.len(),
            "nested replacement optional Product tree",
        )?;
        if !self.locals.contains_key(&local) {
            return Ok(());
        }
        let old = self.local(local, budget)?;
        let incoming = self.value(rhs, budget)?;
        if !matches!(old.node, Node::Product(_)) || !matches!(incoming.node, Node::Product(_)) {
            return Err(error(
                "complete Product replacement requires actual Product contents",
            ));
        }
        if check && (!old.complete(budget)? || !incoming.complete(budget)?) {
            return Err(error(
                "complete Product replacement cannot repair missing child contents",
            ));
        }
        self.locals.set(local, incoming)?;
        Ok(())
    }
    /// Replace a singleton complete local with the actual prepared RHS tree.
    /// Copying the tagged tree preserves each real leaf's origin alternatives.
    pub(super) fn replace_enum(
        &mut self,
        local: LocalId,
        rhs: ValueId,
        plan: &Plan,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let incoming = self.value(rhs, budget)?;
        let old = self.local(local, budget)?;
        if !matches!(
            (&old.node, &incoming.node),
            (Node::Sum(_), Node::Sum(_)) | (Node::Product(_), Node::Product(_))
        ) {
            return Err(error(
                "whole Enum replacement requires actual tagged contents",
            ));
        }
        if check && (!old.complete(budget)? || !incoming.complete(budget)?) {
            return Err(error(
                "whole Enum replacement cannot repair missing old or RHS contents",
            ));
        }
        self.invalidate(local, plan, budget)?;
        budget.lookup(self.locals.len(), "whole Enum definite local publication")?;
        self.locals.set(local, incoming)?;
        Ok(())
    }
    pub(super) fn unpack_birth(
        &self,
        l: LocalId,
        variant: u32,
        index: usize,
        budget: &mut Budget,
    ) -> CompileResult<BTreeSet<Birth>> {
        let t = self.local(l, budget)?;
        let child = match &t.node {
            Node::Sum(vs) if variant != u32::MAX => vs.get(&variant).and_then(|fs| fs.get(index)),
            Node::Product(fs) if variant == u32::MAX => fs.get(index),
            _ => None,
        }
        .ok_or_else(|| error("Unpack field absent"))?;
        Ok(child
            .birth()
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([Birth::Incoming])))
    }
}

/// Private actual staged contents for nonlocal whole-Sum may effects. This is
/// separate from the flat leaf-origin ledger and never grants a must fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Replacement(Arc<Tree>);
impl Replacement {
    pub(super) fn merge(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        self.0.merge(&other.0, budget)
    }
    pub(super) fn copied(&self, budget: &mut Budget) -> CompileResult<Self> {
        Ok(Self(self.0.copied(budget)?))
    }
    pub(super) fn instantiate(
        &self,
        replace: &mut impl FnMut(&BTreeSet<Birth>, &mut Budget) -> CompileResult<BTreeSet<Birth>>,
        budget: &mut Budget,
    ) -> CompileResult<Self> {
        fn node(
            old: &Arc<Tree>,
            replace: &mut impl FnMut(&BTreeSet<Birth>, &mut Budget) -> CompileResult<BTreeSet<Birth>>,
            budget: &mut Budget,
        ) -> CompileResult<Arc<Tree>> {
            old.cost(budget)?;
            let node = match &old.node {
                Node::Leaf(birth) => Node::Leaf(replace(birth, budget)?),
                Node::Copy => Node::Copy,
                Node::Product(fields) => Node::Product(
                    fields
                        .iter()
                        .map(|child| node(child, replace, budget))
                        .collect::<CompileResult<_>>()?,
                ),
                Node::Sum(arms) => {
                    let mut result = BTreeMap::new();
                    for (tag, fields) in arms {
                        budget.spend(
                            fields.len().saturating_add(1),
                            "tagged replacement actual children",
                        )?;
                        result.insert(
                            *tag,
                            fields
                                .iter()
                                .map(|child| node(child, replace, budget))
                                .collect::<CompileResult<_>>()?,
                        );
                    }
                    Node::Sum(result)
                }
            };
            Ok(Arc::new(Tree {
                missing: old.missing,
                node,
            }))
        }
        Ok(Self(node(&self.0, replace, budget)?))
    }
}
impl Facts {
    pub(super) fn replacement_value(
        &self,
        value: ValueId,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<Replacement> {
        let tree = self.value(value, budget)?;
        if check && !tree.complete(budget)? {
            return Err(error(
                "tagged replacement RHS has incomplete actual contents",
            ));
        }
        Ok(Replacement(tree))
    }
    pub(super) fn merge_replacement(
        &mut self,
        local: LocalId,
        incoming: &Replacement,
        plan: &Plan,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let old = self.local(local, budget)?;
        if !matches!(
            (&old.node, &incoming.0.node),
            (Node::Sum(_), Node::Sum(_)) | (Node::Product(_), Node::Product(_))
        ) {
            return Err(error(
                "tagged replacement may effect requires actual Sum contents",
            ));
        }
        if check && (!old.complete(budget)? || !incoming.0.complete(budget)?) {
            return Err(error(
                "tagged replacement cannot repair partial old or staged contents",
            ));
        }
        self.invalidate(local, plan, budget)?;
        budget.lookup(
            self.locals.len(),
            "tagged replacement actual local publication",
        )?;
        self.locals
            .get_mut(&local)
            .ok_or_else(|| error("tagged replacement local absent"))?
            .merge(&incoming.0, budget)?;
        Ok(())
    }
}

pub(super) struct Plan {
    owned_creation: bool,
    // Immutable local indices whose exact Tree constructor contains tag state.
    // None preserves the established scheduling/fees outside mixed consumers.
    tagged_locals: Option<Vec<LocalId>>,
    partial: Option<partial_places::Meta>,
    observations: direct_index::Observations,
    tests: BTreeMap<ValueId, (ValueId, ValueId, u32, bool)>,
}
impl Plan {
    pub(super) fn new(f: &TypedFunction, budget: &mut Budget) -> CompileResult<Option<Self>> {
        // Paid raw preflight marks only functions with actual new shapes/ops.
        if !budget
            .sum_functions
            .get(f.id.0 as usize)
            .copied()
            .unwrap_or(false)
        {
            if !budget.replacement_error_sums {
                return Ok(None);
            }
            // A complete ErrorUnion replacement also needs this kernel's
            // actual old/RHS tree on legacy ordinary-error callers. Preserve
            // their old lane unless such an endpoint really exists.
            budget.spend(
                f.locals
                    .len()
                    .saturating_add(f.values.len())
                    .saturating_add(1),
                "replacement ErrorUnion facts qualification",
            )?;
            let mut needs = false;
            for ty in std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(&f.values)
            {
                let inner = if let Type::Address { inner, .. } = ty {
                    inner.as_ref()
                } else {
                    ty
                };
                if matches!(inner, Type::ErrorUnion { .. }) {
                    budget.type_work(inner, true, "replacement ErrorUnion owning tree")?;
                    needs |= owned(inner);
                }
            }
            if !needs {
                return Ok(None);
            }
        }
        budget.spend(f.values.len() + f.locals.len(), "Sum plan inventory")?;
        let mut partial = false;
        let mut address_tag = false;
        let mut address_replacement = false;
        let mut owned_creation = false;
        let mut constants = BTreeMap::new();
        let mut observations = direct_index::Observations::new(f.values.len(), budget)?;
        let mut tests = BTreeMap::new();
        let mut definitions = BTreeSet::new();
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            budget.spend(1, "Sum instruction inventory")?;
            address_tag |= matches!(i.kind, InstructionKind::PlaceTagAddress { .. });
            owned_creation |= matches!(i.kind, InstructionKind::TaskCreateOwned { .. });
            address_replacement |= matches!(i.kind, InstructionKind::ReservePlaceAddress { .. });
            partial |= matches!(
                i.kind,
                InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::TakeVariantField { .. }
            );
            if let Some(v) = i.result {
                budget.lookup(definitions.len(), "Sum unique result identity")?;
                if v.0 as usize >= f.values.len() || !definitions.insert(v) {
                    return Err(error("Sum observer requires unique actual SSA definitions"));
                }
            }
            match (&i.kind, i.result) {
                (InstructionKind::Constant(Constant::I32(n)), Some(v))
                    if f.values[v.0 as usize] == Type::I32 =>
                {
                    budget.lookup(constants.len(), "Sum constant insertion")?;
                    budget.spend(1, "Sum constant entry")?;
                    constants.insert(v, *n);
                }
                (InstructionKind::VariantTagLocal { local, path }, Some(v)) => {
                    budget.lookup(observations.len(), "Sum observation insertion")?;
                    budget.spend(path.len() + 1, "Sum observation identity")?;
                    observations.insert(v, (*local, path.clone()))?;
                }
                _ => {}
            }
        }
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            budget.spend(1, "Sum condition inventory")?;
            if let (InstructionKind::Binary { op, left, right }, Some(out)) = (&i.kind, i.result)
                && matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
                && f.values.get(left.0 as usize) == Some(&Type::I32)
                && f.values.get(right.0 as usize) == Some(&Type::I32)
                && f.values.get(out.0 as usize) == Some(&Type::Bool)
            {
                for (tag, c) in [(*left, *right), (*right, *left)] {
                    budget.lookup(
                        observations.len() + constants.len(),
                        "Sum condition operands",
                    )?;
                    if observations.contains_key(&tag)
                        && let Some(n) = constants.get(&c)
                    {
                        budget.lookup(tests.len(), "Sum condition insertion")?;
                        tests.insert(out, (tag, c, *n as u32, *op == BinaryOp::Equal));
                    }
                }
            }
        }
        let partial = partial || (address_tag && address_replacement);
        let tagged_locals = if partial
            && budget.store_option_results
            && (budget.task_borrowed_results || budget.ordinary_error_places)
        {
            budget.spend(
                f.locals.len().saturating_mul(2).saturating_add(1),
                "mixed tag local index allocation",
            )?;
            let mut tagged = Vec::with_capacity(f.locals.len());
            for local in &f.locals {
                budget.spend(1, "mixed tag local classification")?;
                let mut pending = vec![&local.ty];
                while let Some(ty) = pending.pop() {
                    budget.spend(1, "mixed tag tree node classification")?;
                    match ty {
                        Type::Enum { .. } | Type::ErrorUnion { .. } => {
                            tagged.push(local.id);
                            break;
                        }
                        Type::Tuple(fields) => {
                            budget.spend(fields.len(), "mixed tag Tuple child inventory")?;
                            pending.extend(fields);
                        }
                        Type::Package { fields, .. } => {
                            budget.spend(fields.len(), "mixed tag Package child inventory")?;
                            pending.extend(fields.iter().map(|(_, child)| child));
                        }
                        // Tree::unknown makes every other form Copy or Leaf;
                        // neither stores tag knowledge or mutable route state.
                        _ => {}
                    }
                }
            }
            Some(tagged)
        } else {
            None
        };
        Ok(Some(Self {
            owned_creation,
            tagged_locals,
            partial: if partial {
                Some(partial_places::Meta::new(f, budget)?)
            } else {
                None
            },
            observations,
            tests,
        }))
    }
    pub(super) fn initial(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        budget: &mut Budget,
    ) -> CompileResult<Facts> {
        let mut facts = Facts {
            owned_creation: self.owned_creation,
            ..Facts::default()
        };
        if self.partial.is_some() {
            facts.partial = Some(partial_places::State::default());
        }
        budget.product(
            f.locals.len(),
            f.parameters.len(),
            1,
            "Sum parameter membership",
        )?;
        facts.locals = direct_index::Locals::build(f, budget, |l, budget| {
            let mut t = Tree::unknown(&l.ty, budget)?;
            edit(&mut t, budget)?.missing = !f.parameters.contains(&l.id);
            Ok(t)
        })?;
        facts.values = ssa_values::Values::build(f.values.len(), budget, |v, budget| {
            let mut t = Tree::unknown(&f.values[v], budget)?;
            edit(&mut t, budget)?.missing = true;
            Ok(t)
        })?;
        if let Some(id) = f.environment
            && budget.has_ordinary_store_sum_capture(id)?
        {
            let closure = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| error("Sum entry environment absent"))?;
            for (index, ty) in closure.captures.iter().enumerate() {
                if !budget.ordinary_store_sum_capture(id, ty)? {
                    continue;
                }
                budget.spend(1, "actual Sum capture root inventory")?;
                budget.lookup(facts.captures.len(), "Sum capture fact allocation")?;
                facts
                    .captures
                    .insert(index as u32, Tree::unknown(ty, budget)?);
            }
        }
        Ok(facts)
    }
    pub(super) fn edge(
        &self,
        f: &TypedFunction,
        b: usize,
        successor: usize,
        facts: &mut Facts,
        budget: &mut Budget,
    ) -> CompileResult<bool> {
        if let (Some(meta), Some(state)) = (&self.partial, &mut facts.partial)
            && let Some((local, path, tag, equal)) =
                meta.edge(state, &f.blocks[b].terminator, successor, budget)?
            && !facts
                .locals
                .get_mut(&local)
                .ok_or_else(|| error("partial edge root absent"))?
                .narrow_place(&path, tag, equal, budget)?
        {
            return Ok(false);
        }
        if let Terminator::Branch {
            condition,
            then_block,
            else_block,
        } = f.blocks[b].terminator
            && then_block != else_block
            && let Some((tag, constant, n, equal)) = self.tests.get(&condition)
        {
            budget.lookup(facts.valid.len(), "Sum edge observation lifetime")?;
            if facts.valid.contains(tag)
                && facts.value(*constant, budget)?.complete(budget)?
                && facts.value(condition, budget)?.complete(budget)?
            {
                let (l, path) = &self.observations[tag];
                let want = if then_block.0 as usize == successor {
                    *equal
                } else {
                    !*equal
                };
                return facts
                    .locals
                    .get_mut(l)
                    .ok_or_else(|| error("Sum edge local absent"))?
                    .narrow(path, *n, want, budget);
            }
        }
        Ok(true)
    }
    pub(super) fn validate_unpack(
        &self,
        f: &TypedFunction,
        i: &Instruction,
        facts: &Facts,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } = &i.kind
        else {
            return Ok(());
        };
        let Type::Enum { variants, .. } = &f.locals[local.0 as usize].ty else {
            return Err(error("Unpack requires an Enum local"));
        };
        if !budget.sum_type(&f.locals[local.0 as usize].ty)? || i.result.is_some() {
            return Err(error("Unpack requires an owning Enum and no result"));
        }
        let fs = variants
            .get(*variant as usize)
            .ok_or_else(|| error("Unpack variant absent"))?;
        if fields.len() != fs.1.len() {
            return Err(error("Unpack field count differs"));
        }
        let mut seen = BTreeSet::new();
        for (l, t) in fields.iter().zip(&fs.1) {
            budget.lookup(seen.len(), "Unpack target uniqueness")?;
            budget.type_work(t, true, "Unpack exact field type")?;
            if l == local
                || !seen.insert(*l)
                || f.locals.get(l.0 as usize).map(|l| &l.ty) != Some(t)
            {
                return Err(error("Unpack targets alias or have wrong types"));
            }
            if check
                && !facts
                    .locals
                    .get(l)
                    .ok_or_else(|| error("Unpack target absent"))?
                    .missing
            {
                return Err(error("Unpack target is not vacant"));
            }
        }
        let t = &facts.locals[local];
        let Node::Sum(vs) = &t.node else {
            return Err(error("Unpack source is not a Sum"));
        };
        if check && (!t.complete(budget)? || vs.len() != 1 || !vs.contains_key(variant)) {
            return Err(error("Unpack lacks a complete current selected source"));
        }
        Ok(())
    }
    pub(super) fn apply(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        facts: &mut Facts,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<Option<BTreeSet<Birth>>> {
        budget.spend(1, "Sum instruction facts")?;
        if crate::stored_flow::failure_unpack(f, i, budget)? {
            let InstructionKind::UnpackTuple { local, fields } = &i.kind else {
                unreachable!()
            };
            if check && !facts.local(*local, budget)?.complete(budget)? {
                return Err(error("borrow-only input view is not initialized"));
            }
            for target in fields {
                if check && facts.locals.get(target).is_some_and(|t| !t.missing) {
                    return Err(error("input view unpack destination is initialized"));
                }
                let tree = Tree::unknown(&f.locals[target.0 as usize].ty, budget)?;
                facts.invalidate(*target, self, budget)?;
                facts.locals.set(*target, tree)?;
            }
            edit(
                facts
                    .locals
                    .get_mut(local)
                    .ok_or_else(|| error("input view source absent"))?,
                budget,
            )?
            .missing = true;
            facts.invalidate(*local, self, budget)?;
            return Ok(None);
        }
        // Only this bounded set has no remaining implicit tree search. In
        // particular Meta::step's result observer and managed-success Sum
        // extraction retain the complete old reserve through the fallback.
        budget.spend(1, "Sum direct transfer qualification")?;
        let direct = (self.partial.is_none() || i.result.is_none())
            && !i
                .result
                .and_then(|v| f.values.get(v.0 as usize))
                .is_some_and(managed_success)
            && matches!(
                i.kind,
                InstructionKind::Constant(_)
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::ByteBufferNew
                    | InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Store { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::StorePackageField { .. }
                    | InstructionKind::DropPackageField { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::DropManaged { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::DropErrorUnion { .. }
                    | InstructionKind::DropClosure { .. }
                    | InstructionKind::DropOwnedArray { .. }
                    | InstructionKind::DropEnum { .. }
                    | InstructionKind::DropTuple { .. }
            );
        for _ in 0..8 {
            if direct {
                budget.spend(1, "Sum instruction direct index reserve")?;
            } else {
                budget.lookup(
                    facts.locals.len() + facts.values.len() + self.observations.len(),
                    "Sum instruction index reserve",
                )?;
            }
        }
        if check && f.call_kind == CallKind::Direct {
            let dropped = match i.kind {
                InstructionKind::DropPackage { local }
                | InstructionKind::DropTuple { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropErrorUnion { local } => Some(local),
                _ => None,
            };
            if let Some(local) = dropped
                && let Some(tree) = facts.locals.get(&local)
                && tree.live_async(&f.locals[local.0 as usize].ty, budget)?
            {
                return Err(error(
                    "Direct aggregate cleanup retains a live asynchronous owner; unpack and explicitly close it",
                ));
            }
        }
        let mut output = None;
        if check
            && self.partial.is_some()
            && let InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local } = i.kind
            && !facts.local(local, budget)?.complete(budget)?
        {
            return Err(error(
                "whole borrow requires every selected descendant initialized",
            ));
        }
        if let (Some(meta), Some(state)) = (&self.partial, &mut facts.partial) {
            meta.step(state, i, budget)?;
            if partial_places::invalidates(&i.kind) {
                if let Some(locals) = &self.tagged_locals {
                    budget.spend(locals.len(), "mixed tagged local invalidation inventory")?;
                    for local in locals {
                        budget.lookup(facts.locals.len(), "mixed tagged local fact lookup")?;
                        facts
                            .locals
                            .get_mut(local)
                            .ok_or_else(|| error("mixed tagged local fact is absent"))?
                            .forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                    }
                } else {
                    for (local, tree) in facts.locals.iter_mut() {
                        tree.forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                    }
                }
            }
        }
        match &i.kind {
            InstructionKind::BindPackageCapture { local, index }
                if facts.has_capture(*index, budget)? =>
            {
                let capture_type = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize));
                if capture_type != Some(&f.locals[local.0 as usize].ty) {
                    return Err(error("Package Sum capture binding type differs"));
                }
                let capture = facts.capture(*index, budget)?;
                if check && !capture.complete(budget)? {
                    return Err(error("Package Sum capture binding is incomplete"));
                }
                budget.lookup(facts.locals.len(), "Package Sum capture local transfer")?;
                facts.locals.set(*local, capture)?;
                edit(
                    facts
                        .captures
                        .get_mut(index)
                        .ok_or_else(|| error("Package Sum capture absent"))?,
                    budget,
                )?
                .missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::EndPackageCapture { local, index }
                if facts.has_capture(*index, budget)? =>
            {
                let local_tree = facts.local(*local, budget)?;
                if check && !facts.capture(*index, budget)?.missing {
                    return Err(error("Package Sum capture binding is still occupied"));
                }
                budget.lookup(facts.captures.len(), "Package Sum capture restoration")?;
                facts.captures.insert(*index, local_tree);
                edit(
                    facts
                        .locals
                        .get_mut(local)
                        .ok_or_else(|| error("Package Sum local absent"))?,
                    budget,
                )?
                .missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeCapture { index } => {
                let ty = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize))
                    .ok_or_else(|| error("TakeCapture Sum environment absent"))?;
                if matches!(
                    ty,
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) && budget.ordinary_store_sum_capture(f.environment.unwrap(), ty)?
                {
                    if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(ty) {
                        return Err(error("TakeCapture Sum result type differs"));
                    }
                    let capture = facts.capture(*index, budget)?;
                    if check && !capture.complete(budget)? {
                        return Err(error("TakeCapture requires a complete Sum capture"));
                    }
                    output = Some(capture);
                    edit(
                        facts
                            .captures
                            .get_mut(index)
                            .ok_or_else(|| error("TakeCapture Sum capture absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
            }
            InstructionKind::WriteCapture { index, value } => {
                let ty = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|closure| closure.captures.get(*index as usize))
                    .ok_or_else(|| error("WriteCapture Sum environment absent"))?;
                if matches!(
                    ty,
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) && budget.ordinary_store_sum_capture(f.environment.unwrap(), ty)?
                {
                    if f.values.get(value.0 as usize) != Some(ty) {
                        return Err(error("WriteCapture Sum source type differs"));
                    }
                    let incoming = facts.value(*value, budget)?;
                    if check
                        && (!facts.capture(*index, budget)?.missing
                            || !incoming.complete(budget)?)
                    {
                        return Err(error(
                            "WriteCapture requires an empty slot and complete Sum",
                        ));
                    }
                    budget.lookup(facts.captures.len(), "Sum capture refill")?;
                    facts.captures.insert(*index, incoming);
                    edit(
                        facts
                            .values
                            .get_mut(value, budget)?
                            .ok_or_else(|| error("Sum capture refill value absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
            }
            InstructionKind::BorrowLocalMut { local }
                if matches!(f.locals[local.0 as usize].ty, Type::Enum { .. }) =>
            {
                facts
                    .locals
                    .get_mut(local)
                    .ok_or_else(|| error("mutable Sum local absent"))?
                    .forget_addressed_copy_tags(&f.locals[local.0 as usize].ty, budget)?;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::Constant(_) | InstructionKind::MakeAggregate { .. } => {}
            InstructionKind::MakeManaged { .. }
            | InstructionKind::ByteBufferNew
            | InstructionKind::OwnedStoreNew { .. } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Leaf(BTreeSet::from([Birth::Created])),
                }));
            }
            InstructionKind::TaskCreateOwned { arguments, .. } => {
                budget.spend(
                    arguments.len().saturating_add(12),
                    "Task allocation failure original input tree",
                )?;
                let inputs = Arc::new(Tree {
                    missing: false,
                    node: Node::Product(
                        arguments
                            .iter()
                            .map(|v| facts.value(*v, budget))
                            .collect::<CompileResult<_>>()?,
                    ),
                });
                let reason = Arc::new(Tree {
                    missing: false,
                    node: Node::Copy,
                });
                let failure = Arc::new(Tree {
                    missing: false,
                    node: Node::Product(vec![reason, inputs]),
                });
                let head = Arc::new(Tree {
                    missing: false,
                    node: Node::Leaf(BTreeSet::from([Birth::Created])),
                });
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(0, vec![head]), (1, vec![failure])])),
                }));
            }
            InstructionKind::WithScopedThread { argument, .. }
                if crate::tasks::transfer::task_owner(&f.values[argument.0 as usize]) =>
            {
                budget.spend(8, "Task thread outcome actual source")?;
                let source = facts.value(*argument, budget)?;
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([
                        (0, vec![source]),
                        (
                            1,
                            vec![
                                Arc::new(Tree {
                                    missing: false,
                                    node: Node::Copy,
                                }),
                                Arc::new(Tree {
                                    missing: false,
                                    node: Node::Copy,
                                }),
                            ],
                        ),
                    ])),
                }));
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let Some(Type::Enum { variants, .. }) =
                    i.result.and_then(|v| f.values.get(v.0 as usize))
                else {
                    return Err(error("MakeVariant result is not Enum"));
                };
                let fs = variants
                    .get(*variant as usize)
                    .ok_or_else(|| error("MakeVariant variant absent"))?;
                if fs.1.len() != fields.len() {
                    return Err(error("MakeVariant field count mismatch"));
                }
                for (v, t) in fields.iter().zip(&fs.1) {
                    budget.type_work(t, true, "MakeVariant exact field type")?;
                    if f.values.get(v.0 as usize) != Some(t) {
                        return Err(error("MakeVariant field type mismatch"));
                    }
                }
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(
                        *variant,
                        fields
                            .iter()
                            .map(|v| facts.value(*v, budget))
                            .collect::<CompileResult<_>>()?,
                    )])),
                }));
            }
            InstructionKind::MakeErrorUnion { value, is_error } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(
                        *is_error as u32,
                        vec![facts.value(*value, budget)?],
                    )])),
                }))
            }
            InstructionKind::MakePackage { fields }
            | InstructionKind::MakeOwnedTuple { fields } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Product(
                        fields
                            .iter()
                            .map(|v| facts.value(*v, budget))
                            .collect::<CompileResult<_>>()?,
                    ),
                }))
            }
            InstructionKind::BorrowPlaceLocal { local, path, .. }
            | InstructionKind::PlaceTagLocal { local, path } => {
                let root = &f.locals[local.0 as usize].ty;
                let ty = if path.is_empty() {
                    root
                } else {
                    place_type(f, *local, path, budget)?
                };
                let borrowed = matches!(i.kind, InstructionKind::BorrowPlaceLocal { .. });
                if borrowed {
                    let InstructionKind::BorrowPlaceLocal { mutable, .. } = i.kind else {
                        unreachable!()
                    };
                    if path.is_empty()
                        || (mutable && !f.locals[local.0 as usize].mutable)
                        || (mutable && matches!(ty, Type::Managed(_)))
                    {
                        return Err(error("mixed borrow has no exact actual storage authority"));
                    }
                    let expected = Type::Address {
                        mutable,
                        inner: Box::new(ty.clone()),
                    };
                    if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&expected) {
                        return Err(error("mixed borrow endpoint Type differs"));
                    }
                } else if !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                    || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::I32)
                {
                    return Err(error("mixed tag must observe an actual Sum header as i32"));
                }
                if check {
                    facts
                        .partial
                        .as_ref()
                        .ok_or_else(|| error("mixed observation proof absent"))?
                        .require(*local, path, budget)?;
                    facts.locals[local].viewed_place(path, borrowed, budget)?;
                }
                if matches!(
                    i.kind,
                    InstructionKind::BorrowPlaceLocal { mutable: true, .. }
                ) {
                    facts
                        .locals
                        .get_mut(local)
                        .unwrap()
                        .forget_place_copy_tags(path, root, budget)?;
                }
            }
            InstructionKind::TakeTupleField { local, path }
            | InstructionKind::TakeVariantField { local, path } => {
                if check {
                    facts
                        .partial
                        .as_ref()
                        .ok_or_else(|| error("partial source proof absent"))?
                        .require(*local, path, budget)?;
                }
                let root = &f.locals[local.0 as usize].ty;
                budget.type_work(root, true, "partial transfer exact root")?;
                if root.is_copy_value()
                    || !match i.kind {
                        InstructionKind::TakeTupleField { .. } => matches!(root, Type::Tuple(_)),
                        _ => matches!(root, Type::Enum { .. } | Type::ErrorUnion { .. }),
                    }
                {
                    return Err(error(
                        "partial transfer requires exact owning Tuple or Enum root",
                    ));
                }
                let endpoint = place_type(f, *local, path, budget)?;
                budget.type_work(endpoint, true, "partial result exact Type")?;
                if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(endpoint) {
                    return Err(error(
                        "partial transfer result differs from actual subtree Type",
                    ));
                }
                output = Some(
                    facts
                        .locals
                        .get_mut(local)
                        .ok_or_else(|| error("partial source absent"))?
                        .take_place(path, check, budget)?,
                );
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeLocal { local } | InstructionKind::Load(local) => {
                output = Some(facts.local(*local, budget)?);
                if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                    edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                    facts.invalidate(*local, self, budget)?;
                }
            }
            InstructionKind::Store { local, value } => {
                let t = facts.value(*value, budget)?;
                facts.invalidate(*local, self, budget)?;
                facts.locals.set(*local, t)?;
            }
            InstructionKind::TakePackageField { local, index }
            | InstructionKind::ReadPackageField { local, index } => {
                let t = facts.local(*local, budget)?;
                let Node::Product(fs) = &t.node else {
                    return Err(error("Sum field source is not Product"));
                };
                output = Some(
                    fs.get(*index as usize)
                        .ok_or_else(|| error("Sum Product field absent"))?
                        .copied(budget)?,
                );
                if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                    if let Node::Product(fs) =
                        &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                    {
                        edit(&mut fs[*index as usize], budget)?.missing = true;
                    }
                    facts.invalidate(*local, self, budget)?;
                }
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => {
                let t = facts.value(*value, budget)?;
                if let Node::Product(fs) =
                    &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                {
                    *fs.get_mut(*index as usize)
                        .ok_or_else(|| error("Sum Product store field absent"))? = t;
                }
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropPackageField { local, index } => {
                if let Node::Product(fs) =
                    &mut edit(facts.locals.get_mut(local).unwrap(), budget)?.node
                {
                    edit(
                        fs.get_mut(*index as usize)
                            .ok_or_else(|| error("Sum Product drop field absent"))?,
                        budget,
                    )?
                    .missing = true;
                }
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::TakeErrorUnion { value, is_error }
            | InstructionKind::ReadErrorUnion { value, is_error } => {
                let t = facts.value(*value, budget)?;
                let Node::Sum(vs) = &t.node else {
                    return Err(error("Sum union source absent"));
                };
                output = Some(
                    vs.get(&(*is_error as u32))
                        .and_then(|fs| fs.first())
                        .map(|t| t.copied(budget))
                        .transpose()?
                        .unwrap_or(Tree::unknown(
                            i.result
                                .and_then(|v| f.values.get(v.0 as usize))
                                .ok_or_else(|| error("union result absent"))?,
                            budget,
                        )?),
                );
            }
            InstructionKind::VariantTagLocal { local, path }
            | InstructionKind::ReadVariantLocal { local, path }
            | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                let ty = projection(&f.locals[local.0 as usize].ty, path, budget)?;
                let tag = matches!(i.kind, InstructionKind::VariantTagLocal { .. });
                let borrowed = matches!(i.kind, InstructionKind::BorrowVariantLocal { .. });
                let borrowed_ty;
                let expected = if borrowed {
                    let InstructionKind::BorrowVariantLocal { mutable, .. } = i.kind else {
                        unreachable!()
                    };
                    if path.is_empty()
                        || (!ty.is_copy_value()
                            && !matches!(
                                ty,
                                Type::Tuple(_)
                                    | Type::Package { .. }
                                    | Type::Enum { .. }
                                    | Type::ErrorUnion { .. }
                            ))
                        || (mutable && !f.locals[local.0 as usize].mutable)
                    {
                        return Err(error(
                            "payload borrow requires finite Copy and actual mutable authority",
                        ));
                    }
                    budget.type_work(ty, true, "Sum payload address type")?;
                    borrowed_ty = Type::Address {
                        mutable,
                        inner: Box::new(ty.clone()),
                    };
                    &borrowed_ty
                } else if tag {
                    if !matches!(ty, Type::Enum { .. }) {
                        return Err(error("tag endpoint is not Enum"));
                    }
                    &Type::I32
                } else {
                    if path.is_empty() || !ty.is_copy_value() {
                        return Err(error("local read endpoint is not strict Copy"));
                    }
                    ty
                };
                if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(expected) {
                    return Err(error("local Sum observation result type differs"));
                }
                facts.locals[local].projected(path, check, budget)?;
                if matches!(
                    i.kind,
                    InstructionKind::BorrowVariantLocal { mutable: true, .. }
                ) && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                    && ty.is_copy_value()
                {
                    facts
                        .locals
                        .get_mut(local)
                        .unwrap()
                        .forget_copy_endpoint(path, ty, budget)?;
                    budget.spend(facts.valid.len(), "mutable payload tag epoch")?;
                    facts.valid.retain(|v| {
                        self.observations.get(v).is_none_or(|(root, observed)| {
                            *root != *local || !observed.starts_with(path)
                        })
                    });
                }
                if tag {
                    budget.lookup(facts.valid.len(), "Sum live observation insertion")?;
                    facts.valid.insert(i.result.unwrap());
                }
            }
            InstructionKind::UnpackVariant {
                local,
                variant,
                fields,
            } => {
                self.validate_unpack(f, i, facts, check, budget)?;
                let tree = facts.local(*local, budget)?;
                let Node::Sum(vs) = &tree.node else {
                    unreachable!()
                };
                if let Some(fs) = vs.get(variant) {
                    for (l, t) in fields.iter().zip(fs) {
                        budget.lookup(facts.locals.len(), "Unpack fact publication")?;
                        facts.invalidate(*l, self, budget)?;
                        facts.locals.set(*l, t.copied(budget)?)?;
                    }
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::UnpackTuple { local, fields } => {
                let tree = facts.local(*local, budget)?;
                let Type::Tuple(types) = &f.locals[local.0 as usize].ty else {
                    return Err(error("UnpackTuple source requires Tuple"));
                };
                if types.len() != fields.len() || i.result.is_some() {
                    return Err(error("UnpackTuple arity or result mismatch"));
                }
                budget.spend(
                    fields.len().saturating_mul(fields.len() + 2),
                    "Tuple atomic destination validation",
                )?;
                let mut targets = BTreeSet::new();
                for (l, t) in fields.iter().zip(types) {
                    if *l == *local || !targets.insert(*l) || f.locals[l.0 as usize].ty != *t {
                        return Err(error("Tuple destination type or alias mismatch"));
                    }
                    if check && facts.locals.get(l).is_some_and(|t| !t.missing) {
                        return Err(error("Tuple destination is initialized"));
                    }
                }
                let Node::Product(children) = &tree.node else {
                    return Err(error("Tuple source tree requires product"));
                };
                for (l, t) in fields.iter().zip(children) {
                    facts.invalidate(*l, self, budget)?;
                    facts.locals.set(*l, t.copied(budget)?)?;
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropTuple { local } => {
                if !matches!(f.locals[local.0 as usize].ty, Type::Tuple(_))
                    || !budget.sum_type(&f.locals[local.0 as usize].ty)?
                    || i.result.is_some()
                {
                    return Err(error("DropTuple requires owning Tuple and no result"));
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::DropEnum { local } => {
                if !matches!(f.locals[local.0 as usize].ty, Type::Enum { .. })
                    || !budget.sum_type(&f.locals[local.0 as usize].ty)?
                    || i.result.is_some()
                {
                    return Err(error("DropEnum requires owning Enum and no result"));
                }
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::DropManaged { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropErrorUnion { local }
            | InstructionKind::DropClosure { local }
            | InstructionKind::DropOwnedArray { local } => {
                edit(facts.locals.get_mut(local).unwrap(), budget)?.missing = true;
                facts.invalidate(*local, self, budget)?;
            }
            InstructionKind::VariantTag { value } | InstructionKind::ReadVariant { value, .. }
                if !f.values[value.0 as usize].is_copy_value()
                    && !crate::stored_flow::shared_enum_observation(module, f, i, budget)? =>
            {
                return Err(error("legacy Enum SSA observation requires Copy"));
            }
            _ => {}
        }
        let mut birth = None;
        if let Some(v) = i.result {
            let mut tree = match output {
                Some(t) => t,
                None => Tree::unknown(&f.values[v.0 as usize], budget)?,
            };
            if matches!(
                i.kind,
                InstructionKind::TaskCreate { .. } | InstructionKind::TaskCreateOwned { .. }
            ) || (matches!(i.kind,InstructionKind::Call{function,..} if module.functions.get(function.0 as usize).is_some_and(|f|f.result_mode==PassingMode::Create)))
            {
                match &mut edit(&mut tree, budget)?.node {
                    Node::Leaf(s) => *s = BTreeSet::from([Birth::Created]),
                    Node::Sum(vs) if managed_success(&f.values[v.0 as usize]) => {
                        if let Some(fs) = vs.get_mut(&0)
                            && let Some(t) = fs.first_mut()
                            && let Node::Leaf(s) = &mut edit(t, budget)?.node
                        {
                            *s = BTreeSet::from([Birth::Created]);
                        }
                    }
                    _ => {}
                }
            }
            if check && !tree.complete(budget)? {
                return Err(error("Sum result is incomplete"));
            }
            if let Some(s) = tree.birth() {
                birth = Some(s.clone());
            } else if managed_success(&f.values[v.0 as usize])
                && let Node::Sum(vs) = &tree.node
            {
                birth = Some(
                    vs.get(&0)
                        .and_then(|fs| fs.first())
                        .and_then(|t| t.birth())
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            facts.values.set(v, tree, budget)?;
        }
        Ok(birth)
    }
}
fn place_type<'a>(
    f: &'a TypedFunction,
    local: LocalId,
    path: &[PlaceProjection],
    budget: &mut Budget,
) -> CompileResult<&'a Type> {
    let mut at = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| error("partial root absent"))?
        .ty;
    budget.spend(path.len().saturating_add(1), "actual partial path shape")?;
    if path.is_empty() {
        return Err(error("partial path must be nonempty"));
    }
    for p in path {
        at = match (p, at) {
            (PlaceProjection::Field(i), Type::Tuple(fs)) => fs.get(*i as usize),
            (PlaceProjection::Field(i), Type::Package { fields, .. }) => {
                fields.get(*i as usize).map(|(_, t)| t)
            }
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| error("partial path is not an actual inline field or tagged payload"))?;
    }
    Ok(at)
}
impl Tree {
    fn narrow_place(
        self: &mut Arc<Self>,
        path: &[PlaceProjection],
        tag: u32,
        equal: bool,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        b.spend(1, "partial mixed tag refinement")?;
        let Some((first, rest)) = path.split_first() else {
            return self.narrow(&[], tag, equal, b);
        };
        match (first, &mut edit(self, b)?.node) {
            (PlaceProjection::Field(i), Node::Product(fs)) => fs
                .get_mut(*i as usize)
                .ok_or_else(|| error("partial tag field absent"))?
                .narrow_place(rest, tag, equal, b),
            (PlaceProjection::Variant(p), Node::Sum(vs)) => {
                let Some(fs) = vs.get_mut(&p.variant) else {
                    return Ok(false);
                };
                fs.get_mut(p.index as usize)
                    .ok_or_else(|| error("partial tag payload absent"))?
                    .narrow_place(rest, tag, equal, b)
            }
            (PlaceProjection::ErrorArm(e), Node::Sum(vs)) => {
                let Some(fs) = vs.get_mut(&u32::from(*e)) else {
                    return Ok(false);
                };
                fs.first_mut()
                    .ok_or_else(|| error("partial error arm absent"))?
                    .narrow_place(rest, tag, equal, b)
            }
            _ => Err(error("partial tag route differs from actual storage")),
        }
    }
    fn forget_place_copy_tags(
        self: &mut Arc<Self>,
        path: &[PlaceProjection],
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(1, "mutable mixed subtree tag invalidation")?;
        let Some((first, rest)) = path.split_first() else {
            return self.forget_addressed_copy_tags(ty, b);
        };
        match (first, ty, &mut edit(self, b)?.node) {
            (PlaceProjection::Field(i), Type::Tuple(ts), Node::Product(fs)) => {
                fs[*i as usize].forget_place_copy_tags(rest, &ts[*i as usize], b)
            }
            (PlaceProjection::Field(i), Type::Package { fields, .. }, Node::Product(fs)) => {
                fs[*i as usize].forget_place_copy_tags(rest, &fields[*i as usize].1, b)
            }
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }, Node::Sum(vs)) => {
                if let Some(fs) = vs.get_mut(&p.variant) {
                    fs[p.index as usize].forget_place_copy_tags(
                        rest,
                        &variants[p.variant as usize].1[p.index as usize],
                        b,
                    )?;
                }
                Ok(())
            }
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }, Node::Sum(vs)) => {
                if let Some(fs) = vs.get_mut(&u32::from(*e)) {
                    fs[0].forget_place_copy_tags(rest, if *e { error } else { result }, b)?;
                }
                Ok(())
            }
            _ => Err(error(
                "mutable mixed invalidation path differs from actual storage",
            )),
        }
    }
    fn viewed_place<'a>(
        &'a self,
        path: &[PlaceProjection],
        complete: bool,
        b: &mut Budget,
    ) -> CompileResult<&'a Tree> {
        b.spend(1, "mixed live subtree observation")?;
        if self.missing {
            return Err(error("mixed place traverses a moved subtree"));
        }
        let Some((first, rest)) = path.split_first() else {
            if complete && !self.complete(b)? {
                return Err(error(
                    "mixed borrow needs every selected descendant initialized",
                ));
            }
            return Ok(self);
        };
        let child = match (first, &self.node) {
            (PlaceProjection::Field(i), Node::Product(fs)) => fs.get(*i as usize),
            (PlaceProjection::Variant(p), Node::Sum(vs)) => {
                vs.get(&p.variant).and_then(|fs| fs.get(p.index as usize))
            }
            (PlaceProjection::ErrorArm(e), Node::Sum(vs)) => {
                vs.get(&u32::from(*e)).and_then(|fs| fs.first())
            }
            _ => None,
        }
        .ok_or_else(|| error("mixed place initialization route differs from actual data"))?;
        child.viewed_place(rest, complete, b)
    }
    fn require_replacement(
        &self,
        path: &[zil_ir::PlaceProjection],
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(1, "replacement initialized path observation")?;
        if self.missing {
            return Err(error("replacement traverses an unavailable owning subtree"));
        }
        let Some((step, rest)) = path.split_first() else {
            if !self.complete(b)? {
                return Err(error("replacement endpoint is not completely initialized"));
            }
            return Ok(());
        };
        let next = match (step, &self.node) {
            (zil_ir::PlaceProjection::Field(i), Node::Product(fs)) => fs.get(*i as usize),
            (zil_ir::PlaceProjection::Variant(p), Node::Sum(vs)) => {
                if vs.len() != 1 {
                    return Err(error("reservation lacks a current exact active variant"));
                }
                vs.get(&p.variant).and_then(|fs| fs.get(p.index as usize))
            }
            (zil_ir::PlaceProjection::ErrorArm(e), Node::Sum(vs)) => {
                if vs.len() != 1 {
                    return Err(error("reservation lacks a current exact active error arm"));
                }
                vs.get(&u32::from(*e)).and_then(|fs| fs.first())
            }
            _ => None,
        }
        .ok_or_else(|| error("reservation initialization route differs from actual owner"))?;
        next.require_replacement(rest, b)
    }
    fn replace_leaf_birth(
        self: &mut Arc<Self>,
        path: &[zil_ir::PlaceProjection],
        birth: &BTreeSet<Birth>,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        budget.spend(1, "replacement initialized owning subtree")?;
        if check && self.missing {
            return Err(error("replacement traverses a moved subtree"));
        }
        let at = edit(self, budget)?;
        let Some((step, rest)) = path.split_first() else {
            let Node::Leaf(old) = &mut at.node else {
                return Err(error(
                    "replacement endpoint is not a managed responsibility leaf",
                ));
            };
            budget.spend(
                old.len().saturating_add(birth.len()),
                "replacement owning leaf birth join",
            )?;
            old.extend(birth);
            return Ok(());
        };
        let child = match (step, &mut at.node) {
            (zil_ir::PlaceProjection::Field(i), Node::Product(fs)) => fs.get_mut(*i as usize),
            (zil_ir::PlaceProjection::Variant(p), Node::Sum(vs)) => {
                if check && (vs.len() != 1 || !vs.contains_key(&p.variant)) {
                    return Err(error("replacement needs an actual active variant"));
                }
                vs.get_mut(&p.variant)
                    .and_then(|fs| fs.get_mut(p.index as usize))
            }
            (zil_ir::PlaceProjection::ErrorArm(e), Node::Sum(vs)) => {
                if check && (vs.len() != 1 || !vs.contains_key(&u32::from(*e))) {
                    return Err(error("replacement needs the actual active error arm"));
                }
                vs.get_mut(&u32::from(*e)).and_then(|fs| fs.first_mut())
            }
            _ => None,
        }
        .ok_or_else(|| error("replacement initialization path differs from actual owner"))?;
        child.replace_leaf_birth(rest, birth, check, budget)
    }
    fn take_place(
        self: &mut Arc<Self>,
        path: &[PlaceProjection],
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<Arc<Self>> {
        budget.spend(1, "partial initialization path step")?;
        if check && self.missing {
            return Err(error("partial transfer prefix is unavailable"));
        }
        let Some((first, rest)) = path.split_first() else {
            if check && !self.complete(budget)? {
                return Err(error(
                    "partial transfer requires the entire selected subtree initialized",
                ));
            }
            let out = self.copied(budget)?;
            edit(self, budget)?.missing = true;
            return Ok(out);
        };
        let at = edit(self, budget)?;
        match (first, &mut at.node) {
            (PlaceProjection::Field(index), Node::Product(fs)) => fs
                .get_mut(*index as usize)
                .ok_or_else(|| error("partial field absent"))?
                .take_place(rest, check, budget),
            (PlaceProjection::Variant(p), Node::Sum(vs)) => {
                if check && (vs.len() != 1 || !vs.contains_key(&p.variant)) {
                    return Err(error("partial transfer has no current exact active tag"));
                }
                vs.get_mut(&p.variant)
                    .and_then(|fs| fs.get_mut(p.index as usize))
                    .ok_or_else(|| error("partial payload absent"))?
                    .take_place(rest, check, budget)
            }
            (PlaceProjection::ErrorArm(e), Node::Sum(vs)) => {
                if check && (vs.len() != 1 || !vs.contains_key(&u32::from(*e))) {
                    return Err(error("partial error arm is not currently selected"));
                }
                vs.get_mut(&u32::from(*e))
                    .and_then(|fs| fs.first_mut())
                    .ok_or_else(|| error("partial error/success content absent"))?
                    .take_place(rest, check, budget)
            }
            _ => Err(error(
                "partial initialization path differs from actual data shape",
            )),
        }
    }
}

fn projection<'a>(
    mut ty: &'a Type,
    path: &[VariantProjection],
    budget: &mut Budget,
) -> CompileResult<&'a Type> {
    for p in path {
        budget.spend(1, "Sum projection type")?;
        let Type::Enum { variants, .. } = ty else {
            return Err(error("Sum projection crosses non-Enum"));
        };
        ty = variants
            .get(p.variant as usize)
            .and_then(|(_, fs)| fs.get(p.index as usize))
            .ok_or_else(|| error("Sum projection type index absent"))?;
    }
    Ok(ty)
}

fn edit<'a>(tree: &'a mut Arc<Tree>, budget: &mut Budget) -> CompileResult<&'a mut Tree> {
    if Arc::strong_count(tree) > 1 {
        tree.cost(budget)?;
    }
    Ok(Arc::make_mut(tree))
}
