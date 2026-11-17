//! Request-local physical representation. Only exact-bound input constructs the
//! production context. A recipe grants no Copy, borrow, cleanup or ABI capability.
use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Position {
    Value,
    Field,
    Capture,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FieldRole {
    Ordinary,
    Capture(u32),
    CaptureFlag { capture: u32, ordinal: u32 },
    Tag,
    Payload,
}
#[derive(Clone)]
pub(super) struct Field {
    pub index: u32,
    pub offset: u64,
    pub role: FieldRole,
    pub recipe: Arc<Recipe>,
}
#[derive(Clone)]
pub(super) struct Variant {
    pub fields: Vec<Field>,
    pub size: u64,
    pub align: u32,
}
#[derive(Clone)]
pub(super) struct Enum {
    pub words: u64,
    pub word_align: u32,
    pub align: u32,
    pub variants: Vec<Variant>,
}
#[derive(Clone)]
pub(super) struct Recipe {
    pub machine: m::Ty,
    pub size: u64,
    pub align: u32,
    pub fields: Vec<Field>,
    pub enumeration: Option<Enum>,
    pub error_union: bool,
    nodes: usize,
}

pub(super) struct LayoutContext<'a> {
    target: &'a TargetProfile,
    closures: BTreeMap<ClosureId, &'a ClosureDefinition>,
    environments: BTreeMap<ClosureId, Arc<Recipe>>,
    names: BTreeMap<String, ClosureId>,
    limits: zil_base::Limits,
    meter: tasks::Meter,
}
/// Flags are appended after all data fields, grouped by capture order.
/// Package groups are shell followed by the same owned direct fields used by
/// local storage, Take/refill, capture binding and destruction.
pub(super) fn capture_flag_count(ty: &Type) -> usize {
    if matches!(ty,Type::BorrowedStorage{..}) && packages::physical_package(ty).is_none() && !drop_plan::scoped_debt(ty) {return 0;}
    match enums::storage_shape(ty) {
        Type::Managed(_) | Type::Closure(_) | Type::OwnedDyn { .. } => 1,
        ty if drop_plan::owned_sum(ty) => 1,
        Type::Package { fields, .. } => {
            1 + fields
                .iter()
                .filter(|(_, ty)| packages::owned_field(ty))
                .count()
        }
        _ => 0,
    }
}

/// Pay traversal, names, containers and the full copy before allocating a
/// machine type. Arrays clone one element type, not their runtime cardinality.
fn clone_machine_with(ty:&m::Ty,limits:&zil_base::Limits,charge:&mut impl FnMut(usize)->CompileResult<()>)->CompileResult<m::Ty> {
    fn weight(ty:&m::Ty,limits:&zil_base::Limits,charge:&mut impl FnMut(usize)->CompileResult<()>,nodes:&mut usize,depth:usize)->CompileResult<usize> {
        charge(1)?;*nodes=nodes.checked_add(1).ok_or_else(overflow)?;
        if depth>limits.max_call_depth.min(zil_base::Limits::default().max_call_depth)||*nodes>limits.max_nodes{return Err(overflow());}
        Ok(match ty {
            m::Ty::Named(name)=>{charge(name.len())?;name.len().saturating_add(1)},
            m::Ty::Struct(fields)=>{charge(fields.len())?;let mut out=fields.len().saturating_add(1);for field in fields {out=out.checked_add(weight(field,limits,charge,nodes,depth+1)?).ok_or_else(overflow)?;}out},
            m::Ty::Array(_,element)=>weight(element,limits,charge,nodes,depth+1)?.checked_add(1).ok_or_else(overflow)?,
            _=>1
        })
    }
    let work=weight(ty,limits,charge,&mut 0,0)?;charge(work)?;Ok(ty.clone())
}
fn lookup_work(entries:usize,width:usize)->usize {width.saturating_mul(entries.checked_ilog2().map_or(1,|log|log as usize+2))}
trait Environments {
    fn environment(
        &mut self,
        id: ClosureId,
        depth: usize,
        scratch: &mut Scratch,
    ) -> CompileResult<Arc<Recipe>>;
    fn spend(&self, amount: usize) -> CompileResult<()>;
    fn limits(&self) -> &zil_base::Limits;
}
struct Scratch {
    nodes: usize,
}
fn fail<T>(message: &str) -> CompileResult<T> {
    invalid("E-CODEGEN-LAYOUT", message, None)
}
fn overflow() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "physical layout size/depth/work exhausted",
        None,
    )]
}
fn round(size: u64, align: u32) -> CompileResult<u64> {
    size.checked_add(u64::from(align) - 1)
        .map(|n| n / u64::from(align) * u64::from(align))
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(overflow)
}
fn leaf(machine: m::Ty, size: u64, align: u32) -> Arc<Recipe> {
    Arc::new(Recipe {
        machine,
        size,
        align,
        fields: vec![],
        enumeration: None,
        error_union: false,
        nodes: 1,
    })
}
fn node(cx: &impl Environments, depth: usize, scratch: &mut Scratch) -> CompileResult<()> {
    cx.spend(1)?;
    if depth
        > cx.limits()
            .max_call_depth
            .min(zil_base::Limits::default().max_call_depth)
        || scratch.nodes >= cx.limits().max_nodes
    {
        return Err(overflow());
    }
    scratch.nodes += 1;
    Ok(())
}
fn product(
    cx: &impl Environments,
    children: Vec<(FieldRole, Arc<Recipe>)>,
) -> CompileResult<Arc<Recipe>> {
    cx.spend(children.len().saturating_mul(3).saturating_add(1))?;
    let (mut size, mut align, mut nodes) = (0u64, 1u32, 1usize);
    let mut fields = Vec::with_capacity(children.len());
    let mut machine = Vec::with_capacity(children.len());
    for (role, child) in children {
        let offset = round(size, child.align)?;
        size = offset
            .checked_add(child.size)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(overflow)?;
        align = align.max(child.align);
        nodes = nodes.checked_add(child.nodes).ok_or_else(overflow)?;
        let index = u32::try_from(fields.len()).map_err(|_| overflow())?;
        machine.push(clone_machine_with(&child.machine,cx.limits(),&mut |amount|cx.spend(amount))?);
        fields.push(Field {
            index,
            offset,
            role,
            recipe: child,
        });
    }
    Ok(Arc::new(Recipe {
        machine: m::Ty::Struct(machine),
        size: round(size, align)?,
        align,
        fields,
        enumeration: None,
        error_union: false,
        nodes,
    }))
}
fn compute(
    cx: &mut impl Environments,
    ty: &Type,
    position: Position,
    depth: usize,
    scratch: &mut Scratch,
) -> CompileResult<Arc<Recipe>> {
    node(cx, depth, scratch)?;
    Ok(match ty {
        Type::BorrowedStorage { storage, .. } | Type::Managed(storage) => {
            return compute(cx, storage, position, depth + 1, scratch);
        }
        Type::Unit => match position {
            Position::Value => leaf(m::Ty::Void, 0, 1),
            Position::Field => leaf(m::Ty::Struct(vec![]), 0, 1),
            Position::Capture => leaf(m::Ty::Int(8), 1, 1),
        },
        Type::Bool => leaf(m::Ty::Int(1), 1, 1),
        Type::I8 | Type::U8 => leaf(m::Ty::Int(8), 1, 1),
        Type::I16 | Type::U16 => leaf(m::Ty::Int(16), 2, 2),
        Type::I32 | Type::U32 => leaf(m::Ty::Int(32), 4, 4),
        Type::I64 | Type::U64 | Type::Usize | Type::Isize => leaf(m::Ty::Int(64), 8, 8),
        Type::F32 => leaf(m::Ty::F32, 4, 4),
        Type::F64 => leaf(m::Ty::F64, 8, 8),
        Type::ForeignHandle
        | Type::CWideString
        | Type::ForeignCallback { .. }
        | Type::FunctionPointer { .. }
        | Type::BorrowedFunctionPointer { .. }
        | Type::Desc(_)
        | Type::Mutex { .. }
        | Type::MutexGuard { .. }
        | Type::Registration { .. }
        | Type::AsyncRegistration { .. }
        | Type::Task { .. }
        | Type::TaskI32 => leaf(m::Ty::Ptr, 8, 8),
        Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Slice { .. } | Type::Dyn { .. }) =>
        {
            product(
                cx,
                vec![
                    (FieldRole::Ordinary, leaf(m::Ty::Ptr, 8, 8)),
                    (
                        FieldRole::Ordinary,
                        leaf(
                            if matches!(inner.as_ref(), Type::Dyn { .. }) {
                                m::Ty::Ptr
                            } else {
                                m::Ty::Int(64)
                            },
                            8,
                            8,
                        ),
                    ),
                ],
            )?
        }
        // Pointer edges terminate physical recursion. Pointee permission and
        // semantic well-formedness were proved independently before binding.
        Type::Address { .. } => leaf(m::Ty::Ptr, 8, 8),
        Type::OwnedDyn { .. } => product(
            cx,
            vec![
                (FieldRole::Ordinary, leaf(m::Ty::Ptr, 8, 8)),
                (FieldRole::Ordinary, leaf(m::Ty::Ptr, 8, 8)),
            ],
        )?,
        Type::ByteBuffer | Type::OwnedStore { .. } => product(
            cx,
            vec![
                (FieldRole::Ordinary, leaf(m::Ty::Ptr, 8, 8)),
                (FieldRole::Ordinary, leaf(m::Ty::Int(64), 8, 8)),
                (FieldRole::Ordinary, leaf(m::Ty::Int(64), 8, 8)),
            ],
        )?,
        Type::Closure(id) => return cx.environment(*id, depth + 1, scratch),
        Type::Tuple(children) => {
            cx.spend(children.len())?;
            let mut fields = Vec::with_capacity(children.len());
            for child in children {
                fields.push((
                    FieldRole::Ordinary,
                    compute(cx, child, Position::Field, depth + 1, scratch)?,
                ));
            }
            product(cx, fields)?
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            cx.spend(name.len().saturating_add(fields.len()))?;
            let mut children = Vec::with_capacity(fields.len());
            for (name, child) in fields {
                cx.spend(name.len())?;
                children.push((
                    FieldRole::Ordinary,
                    compute(cx, child, Position::Field, depth + 1, scratch)?,
                ));
            }
            product(cx, children)?
        }
        Type::Array { element, length } => {
            let child = compute(cx, element, Position::Field, depth + 1, scratch)?;
            cx.spend(child.nodes)?;
            let size = child
                .size
                .checked_mul(u64::from(*length))
                .filter(|n| *n <= i64::MAX as u64)
                .ok_or_else(overflow)?;
            Arc::new(Recipe {
                machine: m::Ty::array(u64::from(*length), clone_machine_with(&child.machine,cx.limits(),&mut |amount|cx.spend(amount))?),
                size,
                align: child.align,
                fields: vec![Field {
                    index: 0,
                    offset: 0,
                    role: FieldRole::Ordinary,
                    recipe: child.clone(),
                }],
                enumeration: None,
                error_union: false,
                nodes: child.nodes.saturating_add(1),
            })
        }
        Type::ErrorUnion { error, result } => {
            let error = compute(cx, error, Position::Field, depth + 1, scratch)?;
            let result = compute(cx, result, Position::Field, depth + 1, scratch)?;
            let mut recipe = product(
                cx,
                vec![
                    (FieldRole::Tag, leaf(m::Ty::Int(1), 1, 1)),
                    (FieldRole::Ordinary, error),
                    (FieldRole::Ordinary, result),
                ],
            )?;
            Arc::get_mut(&mut recipe)
                .expect("new error product")
                .error_union = true;
            recipe
        }
        Type::Enum { name, variants } => {
            cx.spend(name.len().saturating_add(variants.len()))?;
            let (mut bytes, mut align) = (0u64, 1u32);
            let mut rows = Vec::with_capacity(variants.len());
            for (name, children) in variants {
                cx.spend(name.len().saturating_add(children.len()))?;
                let mut fields = Vec::with_capacity(children.len());
                for child in children {
                    fields.push((
                        FieldRole::Ordinary,
                        compute(cx, child, Position::Field, depth + 1, scratch)?,
                    ));
                }
                let payload = product(cx, fields)?;
                bytes = bytes.max(payload.size);
                align = align.max(payload.align);
                cx.spend(payload.fields.len())?;
                rows.push(Variant {
                    fields: payload.fields.clone(),
                    size: payload.size,
                    align: payload.align,
                });
            }
            let words = round(bytes, align)? / u64::from(align);
            let storage = leaf(
                m::Ty::array(words, m::Ty::Int(align * 8)),
                words.checked_mul(u64::from(align)).ok_or_else(overflow)?,
                align,
            );
            let mut result = product(
                cx,
                vec![
                    (FieldRole::Tag, leaf(m::Ty::Int(32), 4, 4)),
                    (FieldRole::Payload, storage),
                ],
            )?;
            Arc::get_mut(&mut result)
                .expect("new physical product")
                .enumeration = Some(Enum {
                words,
                word_align: align,
                align: align.max(4),
                variants: rows,
            });
            result
        }
        Type::Named(_) | Type::Opaque { .. } | Type::Unsupported(_) => {
            return fail("unresolved language type has no prepared physical layout");
        }
        Type::Slice { .. } | Type::Dyn { .. } | Type::SyncCell(_) => {
            return fail("unsized/native-only type has no standalone physical layout");
        }
    })
}

struct Builder<'a, 'c, 'm> {
    closures: &'c BTreeMap<ClosureId, &'a ClosureDefinition>,
    environments: BTreeMap<ClosureId, Arc<Recipe>>,
    active: BTreeSet<ClosureId>,
    limits: &'m zil_base::Limits,
    meter: &'m tasks::Meter,
}
impl Environments for Builder<'_, '_, '_> {
    fn spend(&self, amount: usize) -> CompileResult<()> {
        self.meter.spend(amount)
    }
    fn limits(&self) -> &zil_base::Limits {
        self.limits
    }
    fn environment(
        &mut self,
        id: ClosureId,
        depth: usize,
        scratch: &mut Scratch,
    ) -> CompileResult<Arc<Recipe>> {
        node(self, depth, scratch)?;
        self.meter.spend(lookup_work(self.environments.len(),1))?;
        if let Some(recipe) = self.environments.get(&id) {
            self.meter.spend(1)?;
            return Ok(recipe.clone());
        }
        self.meter.spend(lookup_work(self.active.len(),1))?;
        if !self.active.insert(id) {
            return fail("closure environment contains an infinite inline layout cycle");
        }
        self.meter.spend(lookup_work(self.closures.len(),1))?;
        let definition = *self.closures.get(&id).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-LAYOUT",
                None,
                "closure physical layout has no exact actual definition",
            )]
        })?;
        self.meter.spend(
            definition
                .captures
                .len()
                .saturating_mul(2)
                .saturating_add(1),
        )?;
        let mut fields = Vec::new();
        for (index, ty) in definition.captures.iter().enumerate() {
            fields.push((
                FieldRole::Capture(index as u32),
                compute(self, ty, Position::Capture, depth + 1, scratch)?,
            ));
        }
        for (index, ty) in definition.captures.iter().enumerate() {
            let count = capture_flag_count(ty);
            self.meter.spend(count)?;
            for ordinal in 0..count {
                fields.push((
                    FieldRole::CaptureFlag {
                        capture: index as u32,
                        ordinal: ordinal as u32,
                    },
                    leaf(m::Ty::Int(1), 1, 1),
                ));
            }
        }
        let mut recipe = product(self, fields)?;
        self.meter.spend(lookup_work(self.active.len(),1).saturating_add(lookup_work(self.environments.len(),1)).saturating_add(32))?;
        Arc::get_mut(&mut recipe).expect("new environment recipe").machine=environment_type(id);
        self.active.remove(&id);
        self.environments.insert(id, recipe.clone());
        Ok(recipe)
    }
}
struct Reader<'a, 'b>(&'a LayoutContext<'b>);
impl Environments for Reader<'_, '_> {
    fn spend(&self, amount: usize) -> CompileResult<()> {
        self.0.meter.spend(amount)
    }
    fn limits(&self) -> &zil_base::Limits {
        &self.0.limits
    }
    fn environment(
        &mut self,
        id: ClosureId,
        depth: usize,
        scratch: &mut Scratch,
    ) -> CompileResult<Arc<Recipe>> {
        node(self, depth, scratch)?;
        self.0.meter.spend(lookup_work(self.0.environments.len(),1))?;
        self.0.environments.get(&id).cloned().ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-LAYOUT",
                None,
                "actual closure is absent from the frozen layout directory",
            )]
        })
    }
}
impl<'a> LayoutContext<'a> {
    pub(super) fn build(
        bound: &super::BoundModule<'a>,
        remaining: usize,
        limits: &zil_base::Limits,
    ) -> CompileResult<Self> {
        Self::from_directory(
            bound.lir.target,
            bound.lir.functions,
            bound.lir.closures,
            remaining,
            limits,
        )
    }
    fn from_directory(
        target: &'a TargetProfile,
        functions: &'a [TypedFunction],
        definitions: &'a [ClosureDefinition],
        remaining: usize,
        limits: &zil_base::Limits,
    ) -> CompileResult<Self> {
        if !target.is_supported_x64_llvm() {
            return unsupported("physical recipes require checked x64 LLVM target", None);
        }
        let meter = tasks::Meter::with_limits(remaining, limits);
        meter.spend(definitions.len().saturating_mul(3))?;
        let mut closures = BTreeMap::new();
        for (ordinal, definition) in definitions.iter().enumerate() {
            meter.spend(lookup_work(closures.len(),1))?;
            if definition.id.0 as usize != ordinal
                || closures.insert(definition.id, definition).is_some()
            {
                return fail(
                    "actual closure physical directory has a missing, duplicate or wrong identity",
                );
            }
            let entry = functions
                .get(definition.function.0 as usize)
                .filter(|entry| entry.id == definition.function)
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-LAYOUT",
                        None,
                        "actual closure entry is absent",
                    )]
                })?;
            if entry.foreign.is_some()
                || entry.environment != Some(definition.id)
                || entry.parameters.len() != definition.parameters.len()
            {
                return fail("closure physical environment is not bound to its exact actual entry");
            }
            // Full Type comparisons are prepaid by the same metadata walker
            // used by machine snapshot preparation; no Named/pointer guess.
            let mut work = meter.remaining();
            let before = work;
            copy_aggregates::reserve_enum_metadata(&entry.return_type, &mut work, 0, limits)?;
            copy_aggregates::reserve_enum_metadata(&definition.result, &mut work, 0, limits)?;
            if entry.return_type != definition.result {
                return fail("closure actual result differs from its environment declaration");
            }
            for (parameter, required) in entry.parameters.iter().zip(&definition.parameters) {
                let actual = &entry
                    .locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Codegen,
                            "E-CODEGEN-LAYOUT",
                            None,
                            "actual closure parameter slot is absent",
                        )]
                    })?
                    .ty;
                copy_aggregates::reserve_enum_metadata(actual, &mut work, 0, limits)?;
                copy_aggregates::reserve_enum_metadata(required, &mut work, 0, limits)?;
                if actual != required {
                    return fail("closure actual input differs from its environment declaration");
                }
            }
            meter.spend(before.checked_sub(work).ok_or_else(overflow)?)?;
        }
        let mut builder = Builder {
            closures: &closures,
            environments: BTreeMap::new(),
            active: BTreeSet::new(),
            limits,
            meter: &meter,
        };
        for definition in definitions {
            builder.environment(definition.id, 0, &mut Scratch { nodes: 0 })?;
        }
        let environments = builder.environments;
        let mut names = BTreeMap::new();
        for (id, recipe) in &environments {
            let m::Ty::Named(name) = &recipe.machine else {
                return fail("environment recipe lost its actual nominal identity");
            };
            meter.spend(lookup_work(names.len(),name.len()).saturating_add(name.len()))?;
            if names.insert(name.clone(), *id).is_some() {
                return fail("environment physical name repeats");
            }
        }
        Ok(Self {
            target,
            closures,
            environments,
            names,
            limits: limits.clone(),
            meter,
        })
    }
    pub(super) fn recipe(&self, ty: &Type, position: Position) -> CompileResult<Arc<Recipe>> {
        compute(
            &mut Reader(self),
            ty,
            position,
            0,
            &mut Scratch { nodes: 0 },
        )
    }
    pub(super) fn value_type(&self, ty: &Type) -> CompileResult<m::Ty> {
        let recipe = self.recipe(ty, Position::Value)?;
        self.clone_machine(&recipe.machine)
    }
    pub(super) fn field_type(&self, ty: &Type) -> CompileResult<m::Ty> {
        let recipe = self.recipe(ty, Position::Field)?;
        self.clone_machine(&recipe.machine)
    }
    pub(super) fn capture_type(&self, ty: &Type) -> CompileResult<m::Ty> {
        let recipe = self.recipe(ty, Position::Capture)?;
        self.clone_machine(&recipe.machine)
    }
    pub(super) fn alignment(&self, ty: &Type) -> CompileResult<u32> {
        Ok(self.recipe(ty, Position::Field)?.align)
    }
    pub(super) fn environment(&self, id: ClosureId) -> CompileResult<&Recipe> {
        self.meter.spend(lookup_work(self.environments.len(),1))?;
        self.environments
            .get(&id)
            .map(AsRef::as_ref)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-LAYOUT",
                    None,
                    "environment recipe has no exact actual definition",
                )]
            })
    }
    pub(super) fn reserve_environment_emission(
        &self,
        id: ClosureId,
        per_node: usize,
    ) -> CompileResult<()> {
        let recipe = self.environment(id)?;
        self.meter
            .spend(recipe.nodes.saturating_mul(per_node).saturating_add(128))
    }
    pub(super) fn clone_machine(&self,ty:&m::Ty)->CompileResult<m::Ty>{clone_machine_with(ty,&self.limits,&mut |amount|self.meter.spend(amount))}
    pub(super) fn environment_fields(&self,id:ClosureId)->CompileResult<Vec<m::Ty>> {
        let recipe=self.environment(id)?;self.meter.spend(recipe.fields.len())?;
        recipe.fields.iter().map(|field|self.clone_machine(&field.recipe.machine)).collect()
    }
    pub(super) fn flag_start(&self, id: ClosureId, capture: u32) -> CompileResult<usize> {
        let recipe = self.environment(id)?;
        self.meter.spend(recipe.fields.len())?;
        recipe
            .fields
            .iter()
            .find(|field| {
                field.role
                    == FieldRole::CaptureFlag {
                        capture,
                        ordinal: 0,
                    }
            })
            .map(|field| field.index as usize)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-LAYOUT",
                    None,
                    "capture has no actual live-flag field",
                )]
            })
    }
    pub(super) fn with_work<R>(
        &self,
        operation: impl FnOnce(&mut usize) -> CompileResult<R>,
    ) -> CompileResult<R> {
        let before = self.meter.remaining();
        let mut work = before;
        let result = operation(&mut work);
        self.meter
            .spend(before.checked_sub(work).ok_or_else(overflow)?)?;
        result
    }
    pub(super) fn meter(&self) -> &tasks::Meter {
        &self.meter
    }
    pub(super) fn target(&self) -> &TargetProfile {
        self.target
    }
    pub(super) fn closures(&self) -> &BTreeMap<ClosureId, &'a ClosureDefinition> {
        &self.closures
    }
    pub(super) fn union(&self, ty: &Type) -> CompileResult<Enum> {
        let recipe = self.recipe(ty, Position::Field)?;
        if let Some(enumeration)=&recipe.enumeration {self.meter.spend(enumeration.variants.len().saturating_add(enumeration.variants.iter().map(|variant|variant.fields.len()).sum::<usize>()))?;}
        recipe.enumeration.clone().ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-LAYOUT",
                None,
                "enum recipe requested for a non-enum value",
            )]
        })
    }
    pub(super) fn machine_layout(&self, ty: &m::Ty) -> CompileResult<(u64, u32)> {
        machine_layout_with(
            ty,
            &mut |name| {
                self.meter.spend(lookup_work(self.names.len(),name.len()))?;
                let id = self.names.get(name).ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-LAYOUT",
                        None,
                        "unbound Named machine type has no actual physical recipe",
                    )]
                })?;
                let recipe = self.environment(*id)?;
                Ok(NamedLayout::Prepared(recipe.size, recipe.align))
            },
            &mut |amount| self.meter.spend(amount),
            self.limits.max_call_depth,
        )
    }
    #[cfg(test)]
    pub(super) fn for_test_view(
        view: &zil_ir::LirView<'a>,
        limits: &zil_base::Limits,
    ) -> CompileResult<Self> {
        Self::from_directory(
            view.target,
            view.functions,
            view.closures,
            limits.max_analysis_steps,
            limits,
        )
    }
    #[cfg(test)]
    pub(super) fn for_test_module(
        module: &'a zil_ir::TypedModule,
        limits: &zil_base::Limits,
    ) -> CompileResult<Self> {
        Self::from_directory(
            &module.target,
            &module.functions,
            &module.closures,
            limits.max_analysis_steps,
            limits,
        )
    }
}

struct Structural<'a, 'c, C> {
    limits: &'a zil_base::Limits,
    charge: std::cell::RefCell<&'c mut C>,
}
impl<C: FnMut(usize) -> CompileResult<()>> Environments for Structural<'_, '_, C> {
    fn spend(&self, amount: usize) -> CompileResult<()> {
        (self.charge.borrow_mut())(amount)
    }
    fn limits(&self) -> &zil_base::Limits {
        self.limits
    }
    fn environment(
        &mut self,
        _: ClosureId,
        _: usize,
        _: &mut Scratch,
    ) -> CompileResult<Arc<Recipe>> {
        fail("checked structural layout has no actual closure directory")
    }
}
pub(super) fn checked_structural(
    ty: &Type,
    target: &TargetProfile,
    limits: &zil_base::Limits,
    charge: &mut impl FnMut(usize) -> CompileResult<()>,
) -> CompileResult<Arc<Recipe>> {
    if !target.is_supported_x64_llvm() {
        return unsupported("checked structural layout requires x64 target", None);
    }
    compute(
        &mut Structural {
            limits,
            charge: std::cell::RefCell::new(charge),
        },
        ty,
        Position::Field,
        0,
        &mut Scratch { nodes: 0 },
    )
}

#[cfg(test)]
pub(super) fn scalar_test_context() -> LayoutContext<'static> {
    // Existing isolated fee-oracle tests measure their explicit planner
    // counter. Their separate physical fixture contains no environments;
    // full-request accounting is tested through PreparedEmission/layout tests.
    static TARGET: std::sync::OnceLock<TargetProfile> = std::sync::OnceLock::new();
    let limits = zil_base::Limits {
        max_analysis_steps: usize::MAX,
        ..zil_base::Limits::default()
    };
    LayoutContext::from_directory(
        TARGET.get_or_init(TargetProfile::host_llvm),
        &[],
        &[],
        usize::MAX,
        &limits,
    )
    .unwrap()
}

/// Generated frame names are resolved from their complete actual Machine IR;
/// language environment names resolve from already-frozen actual recipes.
/// Both callers use this one target interpreter, never Named-as-pointer.
pub(crate) enum NamedLayout<'a> {
    Prepared(u64, u32),
    Fields(&'a [m::Ty]),
}
pub(crate) fn machine_layout_with<'a>(
    ty: &m::Ty,
    resolve: &mut impl FnMut(&str) -> CompileResult<NamedLayout<'a>>,
    charge: &mut impl FnMut(usize) -> CompileResult<()>,
    max_depth: usize,
) -> CompileResult<(u64, u32)> {
    fn product<'a>(
        fields: &[m::Ty],
        resolve: &mut impl FnMut(&str) -> CompileResult<NamedLayout<'a>>,
        charge: &mut impl FnMut(usize) -> CompileResult<()>,
        max: usize,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> CompileResult<(u64, u32)> {
        charge(fields.len())?;
        let (mut size, mut align) = (0u64, 1u32);
        for field in fields {
            let (bytes, a) = walk(field, resolve, charge, max, active, depth + 1)?;
            size = round(size, a)?
                .checked_add(bytes)
                .filter(|n| *n <= i64::MAX as u64)
                .ok_or_else(overflow)?;
            align = align.max(a);
        }
        Ok((round(size, align)?, align))
    }
    fn walk<'a>(
        ty: &m::Ty,
        resolve: &mut impl FnMut(&str) -> CompileResult<NamedLayout<'a>>,
        charge: &mut impl FnMut(usize) -> CompileResult<()>,
        max: usize,
        active: &mut BTreeSet<String>,
        depth: usize,
    ) -> CompileResult<(u64, u32)> {
        charge(16)?;
        if depth > max.min(zil_base::Limits::default().max_call_depth) {
            return Err(overflow());
        }
        Ok(match ty {
            m::Ty::Void => (0, 1),
            m::Ty::Ptr => (8, 8),
            m::Ty::F32 => (4, 4),
            m::Ty::F64 => (8, 8),
            m::Ty::Int(bits) => {
                let size = u64::from(*bits).div_ceil(8);
                if size == 0 {
                    return fail("zero-width machine integer");
                }
                {
                    let align = size.next_power_of_two().min(16) as u32;
                    (round(size, align)?, align)
                }
            }
            m::Ty::Named(name) => {
                charge(lookup_work(active.len(),name.len()).saturating_add(name.len()))?;
                if !active.insert(name.clone()) {
                    return fail("machine named type is recursively inline");
                }
                let result = match resolve(name)? {
                    NamedLayout::Prepared(size, align) => (size, align),
                    NamedLayout::Fields(fields) => {
                        product(fields, resolve, charge, max, active, depth + 1)?
                    }
                };
                active.remove(name);
                result
            }
            m::Ty::Struct(fields) => product(fields, resolve, charge, max, active, depth)?,
            m::Ty::Array(length, element) => {
                let (size, align) = walk(element, resolve, charge, max, active, depth + 1)?;
                (
                    size.checked_mul(*length)
                        .filter(|n| *n <= i64::MAX as u64)
                        .ok_or_else(overflow)?,
                    align,
                )
            }
        })
    }
    walk(ty, resolve, charge, max_depth, &mut BTreeSet::new(), 0)
}

#[cfg(test)]
pub(super) fn test_environments(captures: Vec<Vec<Type>>) -> zil_ir::TypedModule {
    let mut definitions = Vec::new();
    let mut functions = Vec::new();
    for (index, captures) in captures.into_iter().enumerate() {
        let id = ClosureId(index as u32);
        let function = FunctionId(index as u32);
        definitions.push(ClosureDefinition {
            id,
            function,
            captures,
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
            storage_life: None,
        });
        functions.push(TypedFunction {
            id: function,
            name: format!("actual_environment_{index}"),
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: zil_ir::PassingMode::Infer,
            locals: vec![],
            values: vec![],
            blocks: vec![zil_ir::BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span: Span::default(),
            foreign: None,
            environment: Some(id),
            callable_contract: None,
            protocol_contracts: vec![],
            callable_return_contract: None,
        });
    }
    no_dyn_test_module(TargetProfile::host_llvm(), &functions, &definitions, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actual() -> zil_ir::TypedModule {
        let mut captures = vec![vec![]; 8];
        captures[7] = vec![Type::Managed(Box::new(Type::ByteBuffer)), Type::Unit];
        test_environments(captures)
    }
    #[test]
    fn actual_environment_fields_flags_and_enum_payload_share_one_recipe() {
        let module = actual();
        let limits = zil_base::Limits::default();
        let layouts = LayoutContext::for_test_module(&module, &limits).unwrap();
        let closure = layouts
            .recipe(&Type::Closure(ClosureId(7)), Position::Field)
            .unwrap();
        assert_eq!((closure.size, closure.align), (32, 8));
        assert_eq!(
            closure
                .fields
                .iter()
                .map(|field| field.offset)
                .collect::<Vec<_>>(),
            vec![0, 24, 25]
        );
        assert_eq!(layouts.flag_start(ClosureId(7), 0).unwrap(), 2);
        assert_eq!(
            layouts.environment_fields(ClosureId(7)).unwrap(),
            vec![
                m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                m::Ty::Int(8),
                m::Ty::Int(1)
            ]
        );
        assert_eq!(layouts.value_type(&Type::Unit).unwrap(), m::Ty::Void);
        assert_eq!(
            layouts.field_type(&Type::Unit).unwrap(),
            m::Ty::Struct(vec![])
        );
        assert_eq!(layouts.capture_type(&Type::Unit).unwrap(), m::Ty::Int(8));
        let ty = Type::Enum {
            name: "ActualOwners".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                (
                    "Full".into(),
                    vec![
                        Type::Closure(ClosureId(7)),
                        Type::OwnedDyn {
                            interface: zil_ir::DynInterfaceId {
                                module: zil_base::ModuleId(0),
                                index: 1,
                            },
                        },
                    ],
                ),
            ],
        };
        let recipe = layouts.recipe(&ty, Position::Field).unwrap();
        assert_eq!((recipe.size, recipe.align), (56, 8));
        let enumeration = recipe.enumeration.as_ref().unwrap();
        assert_eq!((enumeration.words, enumeration.word_align), (6, 8));
        assert_eq!(enumeration.variants[1].fields[1].offset, 32);
        assert_eq!(layouts.machine_layout(&recipe.machine).unwrap(), (56, 8));
        assert_eq!(
            layouts
                .machine_layout(&environment_type(ClosureId(7)))
                .unwrap(),
            (32, 8)
        );
        assert!(
            layouts
                .machine_layout(&environment_type(ClosureId(8)))
                .is_err()
        );
    }
    #[test]
    fn actual_environment_directory_rejects_missing_wrong_entry_and_inline_cycles() {
        let limits = zil_base::Limits::default();
        let module = actual();
        let context = LayoutContext::for_test_module(&module, &limits).unwrap();
        assert!(
            context
                .recipe(&Type::Closure(ClosureId(8)), Position::Field)
                .is_err()
        );
        let mut wrong = actual();
        wrong.closures[7].id = ClosureId(8);
        assert!(LayoutContext::for_test_module(&wrong, &limits).is_err());
        let mut absent = actual();
        absent.functions.pop();
        assert!(LayoutContext::for_test_module(&absent, &limits).is_err());
        let mut entry = actual();
        entry.functions[7].environment = Some(ClosureId(6));
        assert!(LayoutContext::for_test_module(&entry, &limits).is_err());
        let mut result = actual();
        result.functions[7].return_type = Type::I32;
        assert!(LayoutContext::for_test_module(&result, &limits).is_err());
        let cycle = test_environments(vec![vec![Type::Closure(ClosureId(0))]]);
        assert!(LayoutContext::for_test_module(&cycle, &limits).is_err());
        let pointer = test_environments(vec![vec![Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(0))),
        }]]);
        let pointer_layout = LayoutContext::for_test_module(&pointer, &limits).unwrap();
        assert_eq!(pointer_layout.environment(ClosureId(0)).unwrap().size, 8);
        let mut target = actual();
        target.target.pointer_width = 32;
        assert!(LayoutContext::for_test_module(&target, &limits).is_err());
    }
    #[test]
    fn actual_layout_has_exact_request_budget_and_checked_overflow() {
        let module = actual();
        let ty = Type::Enum {
            name: "Exact".into(),
            variants: vec![("Some".into(), vec![Type::Closure(ClosureId(7))])],
        };
        let run = |steps| -> CompileResult<usize> {
            let limits = zil_base::Limits {
                max_analysis_steps: steps,
                ..zil_base::Limits::default()
            };
            let layouts = LayoutContext::for_test_module(&module, &limits)?;
            let value = layouts.recipe(&ty, Position::Field)?;
            layouts.machine_layout(&value.machine)?;
            Ok(steps - layouts.meter().remaining())
        };
        let exact = run(100_000).unwrap();
        assert_eq!(run(exact).unwrap(), exact);
        assert_eq!(
            run(exact - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(run(exact).unwrap(), exact);
        let context =
            LayoutContext::for_test_module(&module, &zil_base::Limits::default()).unwrap();
        let huge = m::Ty::array(u64::MAX, m::Ty::Int(64));
        assert!(context.machine_layout(&huge).is_err());
        let overflow = Type::Array {
            length: u32::MAX,
            element: Box::new(Type::Array {
                length: u32::MAX,
                element: Box::new(Type::I64),
            }),
        };
        assert!(context.recipe(&overflow, Position::Field).is_err());
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    fn qualified(storage:Type)->Type {
        Type::BorrowedStorage{storage:Box::new(storage),life:zil_ir::StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]}}
    }
    #[test]
    fn qualified_owner_captures_preserve_storage_debt_and_package_partial_flags() {
        // This inspects physical recipes only. No life shape, including N0,
        // supplies a Copy, Closed or SyncDrop proof to the semantic checker.
        let bytes=Type::Managed(Box::new(Type::ByteBuffer));
        let package=Type::Package{name:"Partial".into(),fields:vec![("a".into(),bytes.clone()),("b".into(),bytes.clone())]};
        let owners=vec![qualified(Type::Enum{name:"Choice".into(),variants:vec![("Some".into(),vec![bytes.clone()])]}),qualified(Type::Array{element:Box::new(bytes.clone()),length:2}),qualified(Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(bytes.clone())}),qualified(qualified(Type::Tuple(vec![bytes]))),qualified(qualified(package))];
        for ty in &owners[..4]{assert_eq!(capture_flag_count(ty),1);assert!(drop_plan::scoped_debt(ty));}
        assert_eq!(capture_flag_count(&owners[4]),3,"the Package shell and both child flags remain distinct");
        let mut module=test_environments(vec![owners]);module.resources=vec![ResourceDefinition{name:zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),payload:Type::ByteBuffer,teardown:None}];
        let context=LayoutContext::for_test_module(&module,&zil_base::Limits::default()).unwrap();
        assert_eq!(context.environment(ClosureId(0)).unwrap().fields.len(),12);
        assert_eq!(context.flag_start(ClosureId(0),4).unwrap(),9);
        let code=resource_closures::drop_functions(&context,&module.closures,&module.resources).unwrap();
        for index in 0..5 {assert!(code.0.iter().any(|event|matches!(event,m::Event::Begin(function)if function.name==resource_closures::capture_drop_symbol(ClosureId(0),index))));}
        assert!(code.0.iter().any(|event|matches!(event,m::Event::Instruction(m::Inst{op:m::Op::CondBranch{yes,..},..})if yes=="field.drop.4.1")));
        let borrowed=qualified(Type::Tuple(vec![Type::Address{mutable:true,inner:Box::new(Type::I32)}]));
        assert_eq!(capture_flag_count(&borrowed),0,"a borrowed pointee never becomes a native owner");
    }
    #[test]
    fn repeated_full_machine_clones_are_prepaid_with_exact_fresh_request_boundaries() {
        let module=test_environments(vec![vec![]]);
        let tree=m::Ty::Struct((0..32).map(|_|m::Ty::Struct(vec![environment_type(ClosureId(0)),m::Ty::Int(64)])).collect());
        let run=|steps|->CompileResult<usize>{
            let limits=zil_base::Limits{max_analysis_steps:steps,..zil_base::Limits::default()};
            let context=LayoutContext::for_test_module(&module,&limits)?;
            for _ in 0..32{let copy=context.clone_machine(&tree)?;assert_eq!(copy,tree);}
            Ok(steps-context.meter().remaining())
        };
        let exact=run(1_000_000).unwrap();assert!(exact>32*32*10,"complete repeated names and field trees must be charged");
        assert_eq!(run(exact).unwrap(),exact);
        assert_eq!(run(exact-1).unwrap_err()[0].category,zil_base::ResultCategory::ResourceLimit);
        assert_eq!(run(exact).unwrap(),exact);
    }
}
