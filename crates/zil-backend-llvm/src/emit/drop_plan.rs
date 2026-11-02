//! Private machine recipes derived from the currently verified complete type.
//! A plan is never a permission certificate. Only active tags reach payloads.
use super::*;
use std::hash::Hash;

pub(super) fn owned_sum(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) && !ty.is_copy_value()
}
pub(super) fn value_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    if ty.is_copy_value() {
        return true;
    }
    match ty {
        Type::Managed(_) => resources::supported_type(ty, resources),
        Type::Package { fields, .. } => fields.iter().all(|(_, t)| value_type(t, resources)),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fs)| fs.iter().all(|t| value_type(t, resources))),
        Type::ErrorUnion { error, result } => {
            value_type(error, resources) && value_type(result, resources)
        }
        _ => false,
    }
}
fn has_store(ty: &Type) -> bool {
    match ty {
        Type::Managed(p) => matches!(p.as_ref(), Type::OwnedStore { .. }),
        Type::Package { fields, .. } => fields.iter().any(|(_, t)| has_store(t)),
        _ => false,
    }
}
fn legacy_flat(ty: &Type) -> bool {
    matches!(ty,Type::Package{fields,..} if fields.iter().all(|(_,t)| t.is_copy_value() || matches!(t,Type::Managed(_) | Type::Closure(_))))
}
/// Old Copy and already implemented flat error protocols retain their old path.
pub(super) fn new_shape(ty: &Type) -> bool {
    match ty {
        Type::Enum { .. } => !ty.is_copy_value(),
        Type::Package { fields, .. } => fields.iter().any(|(_, t)| owned_sum(t) || new_shape(t)),
        Type::ErrorUnion { error, result } => [error.as_ref(), result.as_ref()].iter().any(|t| {
            owned_sum(t)
                || new_shape(t)
                || (matches!(t, Type::Package { .. }) && has_store(t) && !legacy_flat(t))
        }),
        _ => false,
    }
}
pub(super) fn symbol(ty: &Type) -> String {
    format!("zil.sum.drop.{}", key(ty))
}
fn key(ty: &Type) -> zil_base::Fingerprint {
    let mut hash = zil_base::FingerprintBuilder::new();
    ty.hash(&mut hash);
    hash.digest()
}
struct Budget<'a> {
    limits: &'a zil_base::Limits,
    nodes: usize,
    work: usize,
}
impl<'a> Budget<'a> {
    fn new(limits: &'a zil_base::Limits) -> Self {
        Self {
            limits,
            nodes: 0,
            work: 0,
        }
    }
    fn pay(&mut self, depth: usize, work: usize) -> CompileResult<()> {
        self.nodes = self.nodes.checked_add(1).ok_or_else(limit)?;
        self.work = self.work.checked_add(work).ok_or_else(limit)?;
        if depth > self.limits.max_call_depth
            || self.nodes > self.limits.max_nodes
            || self.work > self.limits.max_analysis_steps
        {
            return Err(limit());
        }
        Ok(())
    }
    fn machine_weight(&mut self, ty: &Type, depth: usize) -> CompileResult<usize> {
        self.pay(depth, 1)?;
        Ok(match ty {
            Type::Managed(t) => self.machine_weight(t, depth + 1)?,
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                let mut n = 1usize;
                for (_, t) in fields {
                    n = n
                        .checked_add(self.machine_weight(t, depth + 1)?)
                        .ok_or_else(limit)?;
                }
                n
            }
            Type::Tuple(fields) => {
                let mut n = 1usize;
                for t in fields {
                    n = n
                        .checked_add(self.machine_weight(t, depth + 1)?)
                        .ok_or_else(limit)?;
                }
                n
            }
            Type::ErrorUnion { error, result } => 2usize
                .checked_add(self.machine_weight(error, depth + 1)?)
                .and_then(|n| n.checked_add(self.machine_weight(result, depth + 1).ok()?))
                .ok_or_else(limit)?,
            Type::Array { element, .. } => 1usize
                .checked_add(self.machine_weight(element, depth + 1)?)
                .ok_or_else(limit)?,
            Type::Enum { .. } | Type::ByteBuffer | Type::OwnedStore { .. } => 4,
            _ => 1,
        })
    }
    fn store_bound(&mut self, ty: &Type, depth: usize) -> CompileResult<usize> {
        self.pay(depth, 1)?;
        let weight = self.machine_weight(ty, depth)?;
        let unit = 16usize
            .checked_mul(weight.saturating_add(1))
            .ok_or_else(limit)?;
        Ok(match ty {
            Type::Unit => 0,
            Type::Managed(t) => self.store_bound(t, depth + 1)?,
            Type::Array { element, length } => self
                .store_bound(element, depth + 1)?
                .checked_add(unit)
                .and_then(|n| n.checked_mul(*length as usize))
                .ok_or_else(limit)?,
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                let mut n = unit;
                for (_, t) in fields {
                    n = n
                        .checked_add(unit)
                        .and_then(|n| n.checked_add(self.store_bound(t, depth + 1).ok()?))
                        .ok_or_else(limit)?;
                }
                n
            }
            Type::Tuple(fields) => {
                let mut n = unit;
                for t in fields {
                    n = n
                        .checked_add(unit)
                        .and_then(|n| n.checked_add(self.store_bound(t, depth + 1).ok()?))
                        .ok_or_else(limit)?;
                }
                n
            }
            Type::ErrorUnion { error, result } => unit
                .checked_mul(3)
                .and_then(|n| n.checked_add(self.store_bound(error, depth + 1).ok()?))
                .and_then(|n| n.checked_add(self.store_bound(result, depth + 1).ok()?))
                .ok_or_else(limit)?,
            Type::Enum { .. } | Type::ByteBuffer | Type::OwnedStore { .. } => 128,
            _ => unit,
        })
    }

    // Prepay the complete subtree before LLVM type construction or Type clones.
    fn tree(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        self.pay(depth, 32)?;
        match ty {
            Type::Package { name, fields } | Type::Record { name, fields } => {
                self.pay(depth, name.len().saturating_add(fields.len()))?;
                for (name, t) in fields {
                    self.pay(depth, name.len())?;
                    self.tree(t, depth + 1)?;
                }
            }
            Type::Enum { name, variants } => {
                self.pay(depth, name.len().saturating_add(variants.len()))?;
                for (name, fields) in variants {
                    self.pay(depth, name.len().saturating_add(fields.len()))?;
                    for t in fields {
                        self.tree(t, depth + 1)?;
                    }
                }
            }
            Type::Tuple(fields) => {
                for t in fields {
                    self.tree(t, depth + 1)?;
                }
            }
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::Slice { element: t }
            | Type::Opaque { hidden: t, .. } => self.tree(t, depth + 1)?,
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for t in parameters {
                    self.tree(t, depth + 1)?;
                }
                self.tree(result, depth + 1)?;
            }
            Type::Named(name) | Type::Unsupported(name) => self.pay(depth, name.len())?,
            Type::OwnedStore { element } => self.tree(element, depth + 1)?,
            Type::Array { element, length } => {
                let _ = length;
                self.tree(element, depth + 1)?;
            }
            Type::ErrorUnion { error, result } => {
                self.tree(error, depth + 1)?;
                self.tree(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "owned Sum layout/drop plan budget exceeded",
        None,
    )]
}

/// Every node owns its exact pointer-relative layout, never a flat union of debts.
#[derive(Clone, Debug)]
enum DropPlan {
    Empty,
    Leaf(String),
    Product(m::Ty, Vec<(u32, DropPlan)>),
    Enum(m::Ty, Vec<DropPlan>),
    Error(m::Ty, Box<DropPlan>, Box<DropPlan>),
}
fn link(
    ty: &Type,
    resources: &[ResourceDefinition],
    budget: &mut Budget<'_>,
    depth: usize,
) -> CompileResult<DropPlan> {
    budget.tree(ty, depth)?;
    if ty.is_copy_value() {
        return Ok(DropPlan::Empty);
    }
    if matches!(
        ty,
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
    ) {
        return Ok(DropPlan::Leaf(symbol(ty)));
    }
    if let Type::Managed(payload) = ty {
        for _ in resources {
            budget.tree(ty, depth)?;
        }
        if matches!(payload.as_ref(), Type::OwnedStore { .. }) {
            return Ok(DropPlan::Leaf(dynamic_store::helper(
                payload, resources, "drop",
            )?));
        }
        let Some(r) = resources.iter().find(|r| r.payload == **payload) else {
            return invalid(
                "E-CODEGEN-CFG",
                "Sum leaf has no actual resource definition",
                None,
            );
        };
        return Ok(if r.payload == Type::ByteBuffer {
            DropPlan::Leaf("zil.bytes.drop".into())
        } else if let Some(f) = r.teardown {
            DropPlan::Leaf(super::symbol(f))
        } else {
            DropPlan::Empty
        });
    }
    unsupported("unsupported complete Sum cleanup leaf", None)
}
fn product_plan(
    ty: &Type,
    fields: &[&Type],
    resources: &[ResourceDefinition],
    budget: &mut Budget<'_>,
    depth: usize,
) -> CompileResult<DropPlan> {
    let mut children = Vec::new();
    for (index, t) in fields.iter().enumerate() {
        // A typed GEP clones the product machine type. Pay its complete size
        // before building it, without re-planning each descendant's cleanup.
        budget.tree(ty, depth)?;
        children.push((index as u32, link(t, resources, budget, depth + 1)?));
    }
    Ok(DropPlan::Product(llvm_type(ty)?, children))
}
fn plan(
    ty: &Type,
    resources: &[ResourceDefinition],
    budget: &mut Budget<'_>,
    depth: usize,
) -> CompileResult<DropPlan> {
    budget.tree(ty, depth)?;
    Ok(match ty {
        t if t.is_copy_value() => DropPlan::Empty,
        Type::Managed(_) => return link(ty, resources, budget, depth),
        Type::Package { fields, .. } => {
            return product_plan(
                ty,
                &fields.iter().map(|(_, t)| t).collect::<Vec<_>>(),
                resources,
                budget,
                depth,
            );
        }
        Type::Tuple(fields) => {
            return product_plan(
                ty,
                &fields.iter().collect::<Vec<_>>(),
                resources,
                budget,
                depth,
            );
        }
        Type::Enum { variants, .. } => {
            let mut children = Vec::new();
            for (_, fields) in variants {
                budget.tree(ty, depth)?;
                if fields.iter().all(Type::is_copy_value) {
                    children.push(DropPlan::Empty);
                    continue;
                }
                let tuple = Type::Tuple(fields.clone());
                children.push(product_plan(
                    &tuple,
                    &fields.iter().collect::<Vec<_>>(),
                    resources,
                    budget,
                    depth + 1,
                )?);
            }
            DropPlan::Enum(llvm_type(ty)?, children)
        }
        Type::ErrorUnion { error, result } => DropPlan::Error(
            llvm_type(ty)?,
            Box::new(link(error, resources, budget, depth + 1)?),
            Box::new(link(result, resources, budget, depth + 1)?),
        ),
        _ => return unsupported("unsupported complete Sum cleanup leaf", None),
    })
}

struct Emitter {
    code: m::Code,
    next: usize,
}
impl Emitter {
    fn fresh(&mut self) -> String {
        let n = self.next;
        self.next += 1;
        format!("drop.{n}")
    }
    fn inst(&mut self, result: Option<String>, op: m::Op) {
        self.code
            .push(m::Event::Instruction(m::Inst::new(result, op)));
    }
    fn gep(&mut self, ty: &m::Ty, ptr: &str, index: u32) -> String {
        let name = self.fresh();
        self.inst(
            Some(name.clone()),
            m::Op::Gep {
                ty: ty.clone(),
                ptr: m::local(ptr.into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), index),
                ],
            },
        );
        name
    }
    fn emit(&mut self, plan: &DropPlan, ptr: &str) {
        match plan {
            DropPlan::Empty => {}
            DropPlan::Leaf(callee) => self.inst(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(callee.clone()),
                    args: vec![m::Arg::new(m::Ty::Ptr, m::local(ptr.into()))],
                    vararg: None,
                },
            ),
            DropPlan::Product(ty, fields) => {
                for (index, p) in fields {
                    if !matches!(p, DropPlan::Empty) {
                        let child = self.gep(ty, ptr, *index);
                        self.emit(p, &child);
                    }
                }
            }
            DropPlan::Error(ty, error, result) => {
                let tagp = self.gep(ty, ptr, 0);
                let tag = self.fresh();
                self.inst(
                    Some(tag.clone()),
                    m::Op::Load {
                        ty: m::Ty::Int(1),
                        ptr: m::local(tagp),
                        align: 1,
                    },
                );
                let yes = self.fresh();
                let no = self.fresh();
                let done = self.fresh();
                self.inst(
                    None,
                    m::Op::CondBranch {
                        condition: m::local(tag),
                        yes: yes.clone(),
                        no: no.clone(),
                    },
                );
                self.code.push(m::Event::Label(yes));
                let child = self.gep(ty, ptr, 1);
                self.emit(error, &child);
                self.inst(None, m::Op::Branch(done.clone()));
                self.code.push(m::Event::Label(no));
                let child = self.gep(ty, ptr, 2);
                self.emit(result, &child);
                self.inst(None, m::Op::Branch(done.clone()));
                self.code.push(m::Event::Label(done));
            }
            DropPlan::Enum(ty, variants) => {
                let tagp = self.gep(ty, ptr, 0);
                let tag = self.fresh();
                self.inst(
                    Some(tag.clone()),
                    m::Op::Load {
                        ty: m::Ty::Int(32),
                        ptr: m::local(tagp),
                        align: 4,
                    },
                );
                let done = self.fresh();
                for (i, p) in variants.iter().enumerate() {
                    let yes = self.fresh();
                    let no = self.fresh();
                    let matched = self.fresh();
                    self.inst(
                        Some(matched.clone()),
                        m::Op::Compare {
                            predicate: "eq".into(),
                            ty: m::Ty::Int(32),
                            left: m::local(tag.clone()),
                            right: m::val(i as u32),
                        },
                    );
                    self.inst(
                        None,
                        m::Op::CondBranch {
                            condition: m::local(matched),
                            yes: yes.clone(),
                            no: no.clone(),
                        },
                    );
                    self.code.push(m::Event::Label(yes));
                    let child = self.gep(ty, ptr, 1);
                    self.emit(p, &child);
                    self.inst(None, m::Op::Branch(done.clone()));
                    self.code.push(m::Event::Label(no));
                }
                self.inst(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("llvm.trap".into()),
                        args: vec![],
                        vararg: None,
                    },
                );
                self.inst(None, m::Op::Unreachable);
                self.code.push(m::Event::Label(done));
            }
        }
    }
}

pub(super) fn functions(
    lir: &LirView<'_>,
    limits: &zil_base::Limits,
    enabled: bool,
    entry: EntryPoint,
) -> CompileResult<m::Code> {
    if !enabled {
        return Ok(m::Code::new());
    }
    let mut budget = Budget::new(limits);
    let mut found = BTreeMap::new();
    let mut seen = BTreeMap::new();
    fn visit<'a>(
        ty: &'a Type,
        found: &mut BTreeMap<zil_base::Fingerprint, &'a Type>,
        seen: &mut BTreeMap<zil_base::Fingerprint, (&'a Type, bool)>,
        budget: &mut Budget<'_>,
        force: bool,
        depth: usize,
    ) -> CompileResult<()> {
        // Hash and equality consume the actual complete type. Hashes only
        // locate memo candidates; distinct types are never equated by digest.
        budget.tree(ty, depth)?;
        budget.pay(depth, usize::BITS as usize)?;
        let hash = key(ty);
        if let Some((previous, expanded)) = seen.get(&hash) {
            budget.tree(ty, depth)?;
            if *previous != ty {
                return invalid(
                    "E-CODEGEN-SUM-HASH",
                    "distinct complete Sum types share a helper key",
                    None,
                );
            }
            if *expanded || !force {
                return Ok(());
            }
        }
        let expand = force || new_shape(ty);
        seen.insert(hash, (ty, expand));
        if !expand {
            return Ok(());
        }
        if !ty.is_copy_value()
            && matches!(
                ty,
                Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
            )
        {
            found.insert(hash, ty);
        }
        match ty {
            Type::Package { fields, .. } => {
                for (_, t) in fields {
                    visit(t, found, seen, budget, true, depth + 1)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        visit(t, found, seen, budget, true, depth + 1)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                visit(error, found, seen, budget, true, depth + 1)?;
                visit(result, found, seen, budget, true, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    for f in lir.functions {
        for ty in std::iter::once(&f.return_type)
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(f.values.iter())
        {
            visit(ty, &mut found, &mut seen, &mut budget, false, 0)?;
        }
    }
    // Callback data ABI creates typed aggregate parameters and results even
    // when a value does not appear in an ordinary function's local table.
    // Prepay those complete trees in this same machine planning budget and
    // provide their active-branch cleanup helpers before emission begins.
    for closure in lir.closures {
        for ty in std::iter::once(&closure.result).chain(closure.parameters.iter()) {
            if matches!(
                ty,
                Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && !ty.is_copy_value()
            {
                visit(ty, &mut found, &mut seen, &mut budget, true, 0)?;
            }
        }
    }
    // Entry error extraction, storage, typed cleanup GEPs and helper names
    // are emitted outside the ordinary instruction loop, once per module.
    if let Some(entry) = lir.functions.iter().find(|f| match entry {
        EntryPoint::None => false,
        EntryPoint::Main => f.name == "main",
        EntryPoint::ReturnValue(id) => f.id == id,
    }) && let Type::ErrorUnion { error, .. } = &entry.return_type
        && !error.is_copy_value()
    {
        for _ in 0..8 {
            budget.tree(&entry.return_type, 0)?;
        }
    }
    // Pay new local operations, complete layout copies, and their emission
    // bound before FunctionEmitter builds any new scratch/target instructions.
    for f in lir.functions {
        for block in &f.blocks {
            for instruction in &block.instructions {
                budget.pay(0, 1)?;
                match &instruction.kind {
                    InstructionKind::VariantTagLocal { local, path }
                    | InstructionKind::ReadVariantLocal { local, path } => {
                        let ty = &f.locals[local.0 as usize].ty;
                        for _ in 0..=path.len() {
                            budget.tree(ty, 0)?;
                        }
                        budget.pay(path.len(), path.len().saturating_mul(64))?;
                    }
                    InstructionKind::UnpackVariant { local, fields, .. } => {
                        for _ in 0..=fields.len() {
                            budget.tree(&f.locals[local.0 as usize].ty, 0)?;
                        }
                        for target in fields {
                            budget.tree(&f.locals[target.0 as usize].ty, 0)?;
                        }
                        budget.pay(0, fields.len().saturating_mul(64))?;
                    }
                    InstructionKind::DropEnum { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropErrorUnion { local } => {
                        // Existing cleanup opcodes also hash Sum helper keys and
                        // construct typed GEPs at every distinct cleanup site.
                        let ty = &f.locals[local.0 as usize].ty;
                        let repeats = match ty {
                            Type::Package { fields, .. } => fields.len().saturating_add(4),
                            _ => 8,
                        };
                        for _ in 0..repeats {
                            budget.tree(ty, 0)?;
                        }
                    }
                    InstructionKind::DropPackageField { local, index } => {
                        let ty = &f.locals[local.0 as usize].ty;
                        budget.tree(ty, 0)?;
                        if let Type::Package { fields, .. } = ty {
                            for _ in 0..8 {
                                budget.tree(&fields[*index as usize].1, 0)?;
                            }
                        }
                    }
                    InstructionKind::MakeVariant { fields, .. }
                        if !f.values
                            [instruction.result.expect("verified variant result").0 as usize]
                            .is_copy_value() =>
                    {
                        for field in fields {
                            let bound = budget.store_bound(&f.values[field.0 as usize], 0)?;
                            budget.pay(0, bound)?;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut code = m::Code::new();
    for ty in found.into_values() {
        let p = plan(ty, lir.resources, &mut budget, 0)?;
        let mut emitter = Emitter {
            code: m::Code::new(),
            next: 0,
        };
        emitter.code.push(m::Event::Begin(m::Function {
            name: symbol(ty),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![m::Param::new(m::Ty::Ptr, "payload")],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
        emitter.code.push(m::Event::Label("entry".into()));
        emitter.emit(&p, "payload");
        emitter.inst(None, m::Op::Return(None));
        emitter.code.push(m::Event::End);
        code.append(emitter.code);
    }
    Ok(code)
}

#[cfg(test)]
mod impl243_plan_tests {
    use super::*;
    fn packet() -> Type {
        let leaf = Type::Package {
            name: "Leaf".into(),
            fields: vec![("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
        };
        Type::Enum {
            name: "Packet".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                ("Pair".into(), vec![leaf.clone(), leaf]),
            ],
        }
    }
    fn resources() -> Vec<ResourceDefinition> {
        vec![ResourceDefinition {
            name: zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }]
    }
    // Private recipe input only. This is deliberately not VerifiedModule or a
    // source/native acceptance fixture, and cannot enter the public backend.
    fn recipe_input() -> LirModule {
        LirModule {
            schema_version: SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            verified_fingerprint: zil_base::Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            resources: resources(),
            closures: vec![],
            layouts: vec![],
            functions: vec![TypedFunction {
                id: FunctionId(0),
                name: "recipe".into(),
                parameters: vec![],
                return_type: Type::Unit,
                locals: vec![],
                values: vec![packet()],
                blocks: vec![],
                entry: zil_ir::BlockId(0),
                span: Span::default(),
                foreign: None,
                environment: None,
                callable_contract: None,
                callable_return_contract: None,
                result_mode: zil_ir::PassingMode::Infer,
            }],
        }
    }
    #[test]
    fn impl243_drop_plan_keeps_variant_choice_above_owned_fields() {
        let limits = zil_base::Limits::default();
        let p = plan(&packet(), &resources(), &mut Budget::new(&limits), 0).unwrap();
        let DropPlan::Enum(_, variants) = p else {
            panic!("expected structural Sum")
        };
        assert!(matches!(variants[0], DropPlan::Empty));
        let DropPlan::Product(_, fields) = &variants[1] else {
            panic!("variant tuple must retain ordered product")
        };
        assert_eq!(
            fields.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1]
        );
        for (_, child) in fields {
            assert!(matches!(child,DropPlan::Leaf(name) if name.starts_with("zil.sum.drop.")));
        }
        let code = functions(&recipe_input().as_view(), &limits, true, EntryPoint::None).unwrap();
        let text = m::Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: String::new(),
            items: code.into_items().unwrap(),
        }
        .to_text();
        assert!(text.contains("icmp eq i32"));
        assert!(text.contains("br i1"));
        assert!(text.contains("call void @llvm.trap()"));
    }
    #[test]
    fn impl243_drop_plan_honors_current_shared_limit_and_fresh_boundary() {
        let lir = recipe_input();
        let mut limits = zil_base::Limits {
            max_analysis_steps: 0,
            ..zil_base::Limits::default()
        };
        assert!(functions(&lir.as_view(), &limits, false, EntryPoint::None).is_ok());
        assert_eq!(
            functions(&lir.as_view(), &limits, true, EntryPoint::None)
                .err()
                .unwrap()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        let (mut low, mut high) = (0, 100_000);
        while low + 1 < high {
            let mid = (low + high) / 2;
            limits.max_analysis_steps = mid;
            if functions(&lir.as_view(), &limits, true, EntryPoint::None).is_ok() {
                high = mid
            } else {
                low = mid
            }
        }
        limits.max_analysis_steps = high;
        assert!(functions(&lir.as_view(), &limits, true, EntryPoint::None).is_ok());
        limits.max_analysis_steps = high - 1;
        assert_eq!(
            functions(&lir.as_view(), &limits, true, EntryPoint::None)
                .err()
                .unwrap()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        limits.max_analysis_steps = high;
        assert!(functions(&lir.as_view(), &limits, true, EntryPoint::None).is_ok());
    }
}
