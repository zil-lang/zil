//! Ordinary Sum facts reconstructed from executable instructions. Branch maps
//! describe possible alternatives, never simultaneous resource debts.
use super::*;
use std::sync::Arc;

pub(crate) fn owned(ty: &Type) -> bool {
    match ty {
        Type::Managed(_) => true,
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
        match ty {
            Type::Managed(_) => {
                if dynamic_store::is_owned_carrier(ty) {
                    dynamic_store::ordinary_owned(module, ty, budget)?;
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
                // Direct Managed success keeps its existing Copy-error rule.
                // Other owning slots are complete Products/Enums; an owning
                // ErrorUnion is not itself a permitted ErrorUnion slot.
                if matches!(result.as_ref(), Type::Managed(_)) {
                    budget.type_work(e, true, "Managed success Copy-error qualification")?;
                    if !e.is_copy_value() {
                        return Err(error("Managed success requires a Copy error payload"));
                    }
                } else {
                    for slot in [e.as_ref(), result.as_ref()] {
                        if !matches!(slot, Type::Package { .. } | Type::Enum { .. }) {
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
    fn unknown(ty: &Type, budget: &mut Budget) -> CompileResult<Arc<Self>> {
        budget.spend(2, "Sum tree allocation")?;
        let node = match ty {
            Type::Managed(_) => Node::Leaf(BTreeSet::from([Birth::Incoming])),
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Facts {
    locals: BTreeMap<LocalId, Arc<Tree>>,
    values: BTreeMap<ValueId, Arc<Tree>>,
    // Only observations of this current activation survive. A write kills all
    // its tokens, including old SSA snapshots reused at the same loop site.
    valid: BTreeSet<ValueId>,
}
impl Facts {
    pub(super) fn cost(&self, budget: &mut Budget) -> CompileResult<()> {
        if self.locals.is_empty() {
            return Ok(());
        }
        budget.spend(
            self.locals.len() + self.values.len() + self.valid.len(),
            "Sum state tables",
        )?;
        // Map cloning shares immutable nodes; it never walks their descendants.
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        if self.locals.is_empty() {
            return Ok(false);
        }
        let mut change = false;
        for (k, t) in &other.locals {
            budget.lookup(self.locals.len(), "Sum local join")?;
            change |= self
                .locals
                .get_mut(k)
                .ok_or_else(|| error("Sum local missing"))?
                .merge(t, budget)?;
        }
        for (k, t) in &other.values {
            budget.lookup(self.values.len(), "Sum value join")?;
            change |= self
                .values
                .get_mut(k)
                .ok_or_else(|| error("Sum value missing"))?
                .merge(t, budget)?;
        }
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
    fn invalidate(&mut self, l: LocalId, plan: &Plan, budget: &mut Budget) -> CompileResult<()> {
        budget.spend(self.valid.len(), "Sum observation invalidation")?;
        self.valid.retain(|v| {
            plan.observations
                .get(v)
                .is_none_or(|(local, _)| *local != l)
        });
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
        let Node::Sum(vs) = &t.node else {
            return Err(error("Unpack source is not a Sum"));
        };
        let child = vs
            .get(&variant)
            .and_then(|fs| fs.get(index))
            .ok_or_else(|| error("Unpack field absent"))?;
        Ok(child
            .birth()
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([Birth::Incoming])))
    }
}

pub(super) struct Plan {
    observations: BTreeMap<ValueId, (LocalId, Vec<VariantProjection>)>,
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
            return Ok(None);
        }
        budget.spend(f.values.len() + f.locals.len(), "Sum plan inventory")?;
        let mut constants = BTreeMap::new();
        let mut observations = BTreeMap::new();
        let mut tests = BTreeMap::new();
        let mut definitions = BTreeSet::new();
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            budget.spend(1, "Sum instruction inventory")?;
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
                    observations.insert(v, (*local, path.clone()));
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
        Ok(Some(Self {
            observations,
            tests,
        }))
    }
    pub(super) fn initial(&self, f: &TypedFunction, budget: &mut Budget) -> CompileResult<Facts> {
        let mut facts = Facts::default();
        budget.product(
            f.locals.len(),
            f.parameters.len(),
            1,
            "Sum parameter membership",
        )?;
        for l in &f.locals {
            budget.lookup(facts.locals.len(), "Sum local fact allocation")?;
            let mut t = Tree::unknown(&l.ty, budget)?;
            edit(&mut t, budget)?.missing = !f.parameters.contains(&l.id);
            facts.locals.insert(l.id, t);
        }
        for (v, ty) in f.values.iter().enumerate() {
            budget.lookup(facts.values.len(), "Sum value fact allocation")?;
            let mut t = Tree::unknown(ty, budget)?;
            edit(&mut t, budget)?.missing = true;
            facts.values.insert(ValueId(v as u32), t);
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
        // Reserve the fixed number of indexed lookups used by the transfer;
        // variable-width field and subtree work is charged separately below.
        for _ in 0..8 {
            budget.lookup(
                facts.locals.len() + facts.values.len() + self.observations.len(),
                "Sum instruction index reserve",
            )?;
        }
        let mut output = None;
        match &i.kind {
            InstructionKind::Constant(_) | InstructionKind::MakeAggregate { .. } => {}
            InstructionKind::MakeManaged { .. }
            | InstructionKind::ByteBufferNew
            | InstructionKind::OwnedStoreNew { .. } => {
                output = Some(Arc::new(Tree {
                    missing: false,
                    node: Node::Leaf(BTreeSet::from([Birth::Created])),
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
            InstructionKind::MakePackage { fields } => {
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
                facts.locals.insert(*local, t);
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
            | InstructionKind::ReadVariantLocal { local, path } => {
                let ty = projection(&f.locals[local.0 as usize].ty, path, budget)?;
                let tag = matches!(i.kind, InstructionKind::VariantTagLocal { .. });
                let expected = if tag {
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
                        facts.locals.insert(*l, t.copied(budget)?);
                    }
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
                if !f.values[value.0 as usize].is_copy_value() =>
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
            if let InstructionKind::Call { function, .. } = i.kind
                && module
                    .functions
                    .get(function.0 as usize)
                    .is_some_and(|f| f.result_mode == PassingMode::Create)
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
            facts.values.insert(v, tree);
        }
        Ok(birth)
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
