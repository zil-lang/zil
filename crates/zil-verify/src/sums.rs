//! Independent ordinary Sum admission, tag facts and deep source transport.
//! Nothing in this module reads producer responsibility/tag annotations.
use super::*;
use std::collections::{BTreeMap, VecDeque};
mod partial_places;

pub(super) fn owned_enum(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. }) && !ty.is_copy_value()
}

pub(super) fn owned(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
    ) && !ty.is_copy_value()
}

fn bad(span: Option<Span>, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-SUM", span, message)
}

/// Used only after the paid raw-tree pass has detected a new Sum profile.
pub(super) fn needs_shape(ty: &Type, budget: &mut Budget) -> Check<bool> {
    if !budget.sums {
        return Ok(false);
    }
    resources::charge_payload(ty, budget)?;
    let mut pending = vec![(ty, false, false)];
    while let Some((ty, in_error, in_product)) = pending.pop() {
        match ty {
            Type::Tuple(_) if !ty.is_copy_value() => return Ok(true),
            Type::Enum { .. } if owned_enum(ty) => return Ok(true),
            Type::ErrorUnion { error, result } => {
                if (in_product || in_error) && owned(ty) {
                    return Ok(true);
                }
                if **error != Type::Unit
                    && **result != Type::Unit
                    && (packages::has_children(error) || packages::has_children(result))
                {
                    return Ok(true);
                }
                pending.push((error, true, in_product));
                pending.push((result, true, in_product));
            }
            Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| (ty, in_error, true)));
            }
            Type::Managed(payload)
                if in_error && matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    resources::charge_payload(ty, budget)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        resources::charge_payload(ty, budget)?;
        if ty.is_copy_value() {
            check_type(ty, span)?;
            continue;
        }
        match ty {
            Type::Managed(_) => resources::check_type_for(module, ty, span, budget)?,
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                for field in fields {
                    pending.push(field);
                }
            }
            Type::Package { name, fields } => {
                budget.spend(name.len().saturating_add(fields.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || fields.is_empty() {
                    return Err(bad(
                        span,
                        "ordinary Product requires a nonempty nominal identity and owning fields",
                    ));
                }
                let mut owns = false;
                for (name, field) in fields {
                    budget.spend(name.len().saturating_mul(fields.len().saturating_add(1)))?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Product fields must have unique nonempty names"));
                    }
                    owns |=
                        matches!(field, Type::Managed(_) | Type::Package { .. }) || owned(field);
                    pending.push(field);
                }
                if !owns {
                    return Err(bad(span, "ordinary Product requires a real owning child"));
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || variants.is_empty() {
                    return Err(bad(
                        span,
                        "owned Enum requires a nonempty nominal identity and variants",
                    ));
                }
                for (name, fields) in variants {
                    budget.spend(
                        name.len()
                            .saturating_mul(variants.len().saturating_add(1))
                            .saturating_add(fields.len()),
                    )?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Enum variants must have unique nonempty names"));
                    }
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                // Keep the existing resource-success profile. Ordinary Sum
                // composition does not turn an owning error into a Copy error.
                if matches!(result.as_ref(), Type::Managed(_)) {
                    resources::charge_payload(error, budget)?;
                    if !error.is_copy_value() {
                        return Err(unsupported(
                            "direct Managed error-union success type requires a finite Copy error",
                            span,
                        ));
                    }
                } else {
                    for payload in [error.as_ref(), result.as_ref()] {
                        if !matches!(payload, Type::Package { .. } | Type::Enum { .. }) {
                            resources::charge_payload(payload, budget)?;
                            if !payload.is_copy_value() {
                                return Err(unsupported(
                                    "ordinary error-union payload slot types require finite Copy, Product or Enum values",
                                    span,
                                ));
                            }
                        }
                    }
                }
                pending.push(error);
                pending.push(result);
            }
            _ => {
                return Err(bad(
                    span,
                    "ordinary Sum payload must be Copy, canonical Managed, Product or finite Sum",
                ));
            }
        }
    }
    Ok(())
}

fn projected_type<'a>(
    f: &'a TypedFunction,
    local: LocalId,
    path: &[VariantProjection],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<&'a Type> {
    let mut ty = local_type(f, local, span)?;
    budget.spend(path.len().saturating_add(1))?;
    for projection in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(bad(
                span,
                "VariantProjection may only traverse an actual Enum",
            ));
        };
        ty = variants
            .get(projection.variant as usize)
            .and_then(|(_, fields)| fields.get(projection.index as usize))
            .ok_or_else(|| bad(span, "VariantProjection index is outside its exact Enum"))?;
    }
    Ok(ty)
}

pub(super) fn place_type<'a>(f:&'a TypedFunction,local:LocalId,path:&[PlaceProjection],span:Option<Span>,budget:&mut Budget)->Check<&'a Type> {
    let mut at=local_type(f,local,span)?;
    if path.is_empty() || path.len()>budget.limits.max_call_depth {return Err(bad(span,"selective transfer requires a nonempty bounded actual path"));}
    budget.spend(path.len().saturating_add(1))?;
    for step in path {
        at=match (step,at) {
            (PlaceProjection::Field(index),Type::Tuple(fields))=>fields.get(*index as usize),
            (PlaceProjection::Field(index),Type::Package{fields,..})=>fields.get(*index as usize).map(|(_,t)|t),
            (PlaceProjection::Variant(p),Type::Enum{variants,..})=>variants.get(p.variant as usize).and_then(|(_,fs)|fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e),Type::ErrorUnion{error,result})=>Some(if *e{error.as_ref()}else{result.as_ref()}),
            _=>None,
        }.ok_or_else(||bad(span,"selective path crosses absent, non-inline or differently typed storage"))?;
    }
    Ok(at)
}
pub(super) fn place_instruction(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<Option<Type>> {
    b.mark_sum(Some(f.id.0 as usize))?;
    let span=Some(i.span);
    let (local,path)=match &i.kind {InstructionKind::BorrowPlaceLocal{local,path,..}|InstructionKind::PlaceTagLocal{local,path}=>(*local,path),_=>unreachable!()};
    let root=local_type(f,local,span)?;
    check_type_for(module,root,span,b)?;
    let endpoint=if path.is_empty(){root}else{place_type(f,local,path,span,b)?};
    resources::charge_payload(endpoint,b)?;
    match &i.kind {
        InstructionKind::BorrowPlaceLocal{mutable,..}=>{
            if path.is_empty() || (*mutable && !f.locals[local.0 as usize].mutable) {return Err(bad(span,"precise mixed borrow requires a nonempty path and actual mutable authority"));}
            if *mutable && matches!(endpoint,Type::Managed(_)) {return Err(bad(span,"ordinary resource head address remains shared"));}
            check_type_for(module,endpoint,span,b)?;
            Ok(Some(Type::Address{mutable:*mutable,inner:Box::new(endpoint.clone())}))
        }
        InstructionKind::PlaceTagLocal{..}=>{
            if !matches!(endpoint,Type::Enum{..}|Type::ErrorUnion{..}) {return Err(bad(span,"mixed tag read must end at an actual Enum or ErrorUnion"));}
            Ok(Some(Type::I32))
        }
        _=>unreachable!(),
    }
}
pub(super) fn check_instruction(
    f: &TypedFunction,
    i: &Instruction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    budget.mark_sum(Some(f.id.0 as usize))?;
    match &i.kind {
        InstructionKind::TakeTupleField{local,path}|InstructionKind::TakeVariantField{local,path}=>{
            let root=local_type(f,*local,span)?;
            resources::charge_payload(root,budget)?;
            if root.is_copy_value() || !match i.kind {InstructionKind::TakeTupleField{..}=>matches!(root,Type::Tuple(_)),_=>matches!(root,Type::Enum{..}|Type::ErrorUnion{..})} {
                return Err(bad(span,"selective transfer requires the exact owning Tuple or Enum root"));
            }
            let endpoint=place_type(f,*local,path,span,budget)?;
            resources::charge_payload(endpoint,budget)?;
            Ok(Some(endpoint.clone()))
        }
        InstructionKind::MakeOwnedTuple { fields } => {
            let ty = i
                .result
                .and_then(|v| f.values.get(v.0 as usize))
                .ok_or_else(|| bad(span, "owned Tuple result absent"))?;
            let Type::Tuple(types) = ty else {
                return Err(bad(span, "owned Tuple result requires actual Tuple"));
            };
            resources::charge_payload(ty, budget)?;
            if ty.is_copy_value() || fields.len() != types.len() {
                return Err(bad(span, "owned Tuple fields or ownership mismatch"));
            }
            for (v, t) in fields.iter().zip(types) {
                if value_type(f, *v, span)? != t {
                    return Err(bad(span, "owned Tuple exact field Type mismatch"));
                }
            }
            Ok(Some(ty.clone()))
        }
        InstructionKind::UnpackTuple { local, fields } => {
            let ty = local_type(f, *local, span)?;
            let Type::Tuple(types) = ty else {
                return Err(bad(span, "UnpackTuple requires actual Tuple"));
            };
            resources::charge_payload(ty, budget)?;
            budget.spend(fields.len().saturating_mul(fields.len() + 2))?;
            if ty.is_copy_value() || fields.len() != types.len() {
                return Err(bad(span, "UnpackTuple requires all owning Tuple fields"));
            }
            let mut seen = BTreeSet::new();
            for (l, t) in fields.iter().zip(types) {
                if *l == *local || !seen.insert(*l) || local_type(f, *l, span)? != t {
                    return Err(bad(span, "UnpackTuple exact distinct targets mismatch"));
                }
            }
            Ok(None)
        }
        InstructionKind::DropTuple { local } => {
            let ty = local_type(f, *local, span)?;
            if !matches!(ty, Type::Tuple(_)) || ty.is_copy_value() {
                return Err(bad(span, "DropTuple requires owning Tuple"));
            }
            Ok(None)
        }
        InstructionKind::VariantTagLocal { local, path }
        | InstructionKind::ReadVariantLocal { local, path } => {
            if path.len() > limits.max_call_depth {
                return Err(limit("Sum projection depth exceeds the selected limit"));
            }
            let ty = projected_type(f, *local, path, span, budget)?;
            if matches!(i.kind, InstructionKind::VariantTagLocal { .. }) {
                if !matches!(ty, Type::Enum { .. }) {
                    return Err(bad(span, "TagLocal projection must end at an Enum"));
                }
                Ok(Some(Type::I32))
            } else {
                if path.is_empty() || !ty.is_copy_value() {
                    return Err(bad(
                        span,
                        "ReadVariantLocal requires a nonempty path ending at finite Copy",
                    ));
                }
                resources::charge_payload(ty, budget)?;
                check_type(ty, span)?;
                Ok(Some(ty.clone()))
            }
        }
        InstructionKind::BorrowVariantLocal {
            local,
            path,
            mutable,
        } => {
            if path.is_empty() || path.len() > limits.max_call_depth {
                return Err(bad(
                    span,
                    "payload borrow requires a nonempty bounded Enum path",
                ));
            }
            let ty = projected_type(f, *local, path, span, budget)?;
            resources::charge_payload(ty, budget)?;
            if (!ty.is_copy_value() && !matches!(ty, Type::Package { .. }))
                || (*mutable && !f.locals[local.0 as usize].mutable)
            {
                return Err(bad(
                    span,
                    "payload borrow requires finite Copy and actual mutable authority",
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(ty.clone()),
            }))
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            let ty = local_type(f, *local, span)?;
            let Type::Enum { variants, .. } = ty else {
                return Err(bad(span, "UnpackVariant requires an owned Enum local"));
            };
            if !owned_enum(ty) {
                return Err(bad(span, "UnpackVariant requires a non-Copy Enum"));
            }
            let payload = variants
                .get(*variant as usize)
                .ok_or_else(|| bad(span, "Unpack variant is out of bounds"))?;
            budget.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
            if fields.len() != payload.1.len() {
                return Err(bad(
                    span,
                    "Unpack must initialize every selected payload field",
                ));
            }
            let mut destinations = BTreeSet::new();
            for (target, expected) in fields.iter().zip(&payload.1) {
                resources::charge_payload(expected, budget)?;
                if *target == *local
                    || !destinations.insert(*target)
                    || local_type(f, *target, span)? != expected
                {
                    return Err(bad(
                        span,
                        "Unpack targets must be distinct from the source and each other, with exact field Types",
                    ));
                }
            }
            Ok(None)
        }
        InstructionKind::DropEnum { local } => {
            if !owned_enum(local_type(f, *local, span)?) {
                return Err(bad(span, "DropEnum requires an owned Enum local"));
            }
            Ok(None)
        }
        _ => unreachable!("Sum opcode dispatch"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Fresh,
    Input(u32, Vec<u32>),
    ReturnedFresh,
    ReturnedInput(u32, Vec<u32>),
    Unknown,
}

/// Tags are per carrier; alternatives are not cross-multiplied with other Sums.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Content {
    Missing(Box<Content>),
    Copy,
    Leaf(BTreeSet<Source>),
    Product(Vec<Content>),
    Sum(BTreeMap<u32, Vec<Content>>, Option<(u32, Vec<u32>)>),
}

impl Content {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(1)?;
        match self {
            Self::Missing(child) => child.cost(budget)?,
            Self::Leaf(sources) => {
                budget.spend(sources.len())?;
                for source in sources {
                    if let Source::Input(_, path) | Source::ReturnedInput(_, path) = source {
                        budget.spend(path.len())?;
                    }
                }
            }
            Self::Product(fields) => {
                budget.spend(fields.len())?;
                for field in fields {
                    field.cost(budget)?;
                }
            }
            Self::Sum(variants, route) => {
                if let Some((_, path)) = route {
                    budget.spend(path.len().saturating_add(1))?;
                }
                budget.spend(variants.len())?;
                for fields in variants.values() {
                    budget.spend(fields.len())?;
                    for field in fields {
                        field.cost(budget)?;
                    }
                }
            }
            Self::Copy => {}
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        if matches!(other, Self::Missing(_)) && !matches!(self, Self::Missing(_)) {
            self.cost(budget)?;
            let previous=std::mem::replace(self,Self::Copy);
            *self=Self::Missing(Box::new(previous));
            changed=true;
        }
        match (self, other) {
            (Self::Missing(a),Self::Missing(b)) => { changed |= a.merge(b,budget)?; }
            (Self::Missing(a), b) => { changed |= a.merge(b,budget)?; }
            (Self::Leaf(a), Self::Leaf(b)) => {
                let old = a.len();
                a.extend(b.iter().cloned());
                changed = old != a.len();
            }
            (Self::Product(a), Self::Product(b)) => {
                for (a, b) in a.iter_mut().zip(b) {
                    changed |= a.merge(b, budget)?;
                }
            }
            (Self::Sum(a, route), Self::Sum(b, other_route)) => {
                if a.is_empty() && !b.is_empty() {
                    *route = other_route.clone();
                    changed = true;
                } else if !b.is_empty() && route != other_route && route.take().is_some() {
                    changed = true;
                }
                for (&tag, fields) in b {
                    if let Some(target) = a.get_mut(&tag) {
                        for (a, b) in target.iter_mut().zip(fields) {
                            changed |= a.merge(b, budget)?;
                        }
                    } else {
                        for field in fields {
                            field.cost(budget)?;
                        }
                        budget.spend(fields.len())?;
                        a.insert(tag, fields.clone());
                        changed = true;
                    }
                }
            }
            (Self::Copy, Self::Copy) => {}
            _ => return Err(bad(None, "Sum source shapes differ at transfer")),
        }
        Ok(changed)
    }
    fn take_place(&mut self,path:&[PlaceProjection],validate:bool,b:&mut Budget)->Check<Content> {
        b.spend(1)?;
        if let Self::Missing(child)=self {
            if validate{return Err(bad(None,"selective transfer prefix or subtree is already moved on an incoming path"));}
            return if path.is_empty(){child.duplicate(b)}else{child.take_place(path,false,b)};
        }
        let Some((first,rest))=path.split_first() else {
            if validate && !self.complete(b)?{return Err(bad(None,"selective transfer requires the complete selected initialized subtree"));}
            let output=self.duplicate(b)?;
            b.spend(1)?;
            let previous=std::mem::replace(self,Self::Copy);
            *self=Self::Missing(Box::new(previous));
            return Ok(output);
        };
        match(first,self){
            (PlaceProjection::Field(index),Self::Product(fields))=>fields.get_mut(*index as usize).ok_or_else(||bad(None,"selective initialization field absent"))?.take_place(rest,validate,b),
            (PlaceProjection::Variant(p),Self::Sum(variants,_))=>variants.get_mut(&p.variant).and_then(|fs|fs.get_mut(p.index as usize)).ok_or_else(||bad(None,"selective payload has no current content"))?.take_place(rest,validate,b),
            (PlaceProjection::ErrorArm(e),Self::Sum(variants,_))=>variants.get_mut(&if *e{0}else{1}).and_then(|fs|fs.first_mut()).ok_or_else(||bad(None,"selective error/success content absent"))?.take_place(rest,validate,b),
            _=>Err(bad(None,"selective initialization path differs from actual content")),
        }
    }
    fn complete(&self,budget:&mut Budget)->Check<bool> {
        budget.spend(1)?;
        match self {
            Self::Missing(_)=>Ok(false),
            Self::Product(fs)=>{for f in fs {if !f.complete(budget)?{return Ok(false)}}Ok(true)},
            Self::Sum(vs,_)=>{if vs.is_empty(){return Ok(false)}for fs in vs.values(){for f in fs{if !f.complete(budget)?{return Ok(false)}}}Ok(true)},
            _=>Ok(true),
        }
    }
    fn fresh(&self) -> u8 {
        match self {
            Self::Leaf(sources)
                if !sources.is_empty() && sources.iter().all(|s| *s == Source::Fresh) =>
            {
                1
            }
            _ => 2,
        }
    }
    /// Ordinary Value results have no per-leaf create contract (§8.1.1).
    /// Keep their actual roots/routes, but never retain fresh authority.
    fn returned(&mut self, budget: &mut Budget) -> Check<()> {
        self.cost(budget)?;
        match self {
            Self::Missing(child)=>child.returned(budget)?, 
            Self::Leaf(sources) => {
                *sources = sources
                    .iter()
                    .map(|source| match source {
                        Source::Fresh => Source::ReturnedFresh,
                        Source::Input(p, path) => Source::ReturnedInput(*p, path.clone()),
                        source => source.clone(),
                    })
                    .collect();
            }
            Self::Product(fields) => {
                for field in fields {
                    field.returned(budget)?;
                }
            }
            Self::Sum(variants, _) => {
                for fields in variants.values_mut() {
                    for field in fields {
                        field.returned(budget)?;
                    }
                }
            }
            Self::Copy => {}
        }
        Ok(())
    }
}

fn content(
    ty: &Type,
    parameter: Option<u32>,
    path: &mut Vec<u32>,
    bottom: bool,
    budget: &mut Budget,
) -> Check<Content> {
    budget.spend(path.len().saturating_add(2))?;
    Ok(match ty {
        Type::Managed(_) => Content::Leaf(if bottom {
            BTreeSet::new()
        } else {
            BTreeSet::from([parameter.map_or(Source::Unknown, |p| Source::Input(p, path.clone()))])
        }),
        Type::Tuple(fields) => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, ty) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content(ty, parameter, path, bottom, budget)?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Package { fields, .. } => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content(ty, parameter, path, bottom, budget)?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Enum { variants, .. } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(variants.len())?;
            let mut alternatives = BTreeMap::new();
            for (variant, (_, fields)) in variants.iter().enumerate() {
                budget.spend(fields.len())?;
                let mut children = Vec::with_capacity(fields.len());
                for (index, ty) in fields.iter().enumerate() {
                    path.extend([1, variant as u32, index as u32]);
                    children.push(content(ty, parameter, path, bottom, budget)?);
                    path.truncate(path.len() - 3);
                }
                if !bottom {
                    alternatives.insert(variant as u32, children);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        Type::ErrorUnion { error, result } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(2)?;
            let mut alternatives = BTreeMap::new();
            for (variant, ty) in [(0, error.as_ref()), (1, result.as_ref())] {
                path.extend([1, variant, 0]);
                let child = content(ty, parameter, path, bottom, budget)?;
                path.truncate(path.len() - 3);
                if !bottom {
                    alternatives.insert(variant, vec![child]);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        _ => Content::Copy,
    })
}

type Prefix = Vec<(u32, u32)>;
type Fact = (LocalId, Prefix, u32);
#[derive(Clone, PartialEq, Eq)]
struct State {
    initialized: Vec<u8>, // 1 vacant, 2 initialized; join is may-union.
    locals: Vec<Arc<Content>>,
    values: Vec<Arc<Content>>,
    facts: BTreeSet<Fact>,
    observations: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}

impl State {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(
            self.initialized
                .len()
                .saturating_add(self.locals.len())
                .saturating_add(self.values.len())
                .saturating_add(self.observations.len())
                .saturating_add(self.facts.len()),
        )?;
        if let Some(p)=&self.partial {p.cost(budget)?;}
        for (_, path, _) in &self.facts {
            budget.spend(path.len())?;
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        if let (Some(a),Some(b))=(&mut self.partial,&other.partial){changed|=a.join(b,budget)?;}
        for (a, b) in self.initialized.iter_mut().zip(&other.initialized) {
            let old = *a;
            *a |= *b;
            changed |= old != *a;
        }
        for (a, b) in self
            .locals
            .iter_mut()
            .zip(&other.locals)
            .chain(self.values.iter_mut().zip(&other.values))
        {
            if !Arc::ptr_eq(a, b) {
                if Arc::strong_count(a) > 1 {
                    a.cost(budget)?;
                }
                changed |= Arc::make_mut(a).merge(b, budget)?;
            }
        }
        let old = self.facts.len();
        self.facts.retain(|f| other.facts.contains(f));
        changed |= old != self.facts.len();
        let old = self.observations.len();
        self.observations.retain(|v| other.observations.contains(v));
        changed |= old != self.observations.len();
        Ok(changed)
    }
}

#[derive(Default)]
pub(super) struct Proofs {
    pub(super) rows: Vec<Vec<Vec<Bridge>>>,
}
type FieldFresh = Vec<(usize, u8)>;
type UnpackedFresh = (LocalId, u8, FieldFresh);
#[derive(Clone, Default)]
pub(super) struct Bridge {
    pub(super) result: Option<u8>,
    pub(super) package: FieldFresh,
    pub(super) unpack: Vec<UnpackedFresh>,
}

fn direct_fields(value: &Content, budget: &mut Budget) -> Check<Vec<(usize, u8)>> {
    let mut result = Vec::new();
    if let Content::Product(fields) = value {
        budget.spend(fields.len())?;
        for (index, field) in fields.iter().enumerate() {
            if let Content::Leaf(sources) = field {
                budget.spend(sources.len())?;
                result.push((index, field.fresh()));
            }
        }
    }
    Ok(result)
}

struct Flow<'a> {
    f: &'a TypedFunction,
    module: &'a TypedModule,
    definitions: Vec<Option<(usize, usize)>>,
    observers: Vec<Option<(LocalId, Prefix)>>,
    summaries: &'a [Content],
    partial: bool,
    place_meta: Option<partial_places::Meta>,
}

impl Flow<'_> {
    fn kill(&self, state: &mut State, local: LocalId, budget: &mut Budget) -> Check<()> {
        budget.spend(state.facts.len().saturating_add(state.observations.len()))?;
        state.facts.retain(|(root, _, _)| *root != local);
        state.observations.retain(|v| {
            self.observers[v.0 as usize]
                .as_ref()
                .is_none_or(|(root, _)| *root != local)
        });
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn selected<'a>(
        &self,
        state: &'a State,
        local: LocalId,
        path: &[VariantProjection],
        last: Option<u32>,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<&'a Content> {
        let mut current = state.locals[local.0 as usize].as_ref();
        let mut prefix = Vec::new();
        budget.spend(path.len().saturating_mul(path.len().saturating_add(2)))?;
        if validate && state.initialized[local.0 as usize] != 2 {
            return Err(bad(
                Some(span),
                "Sum observation/consume requires a complete live current local",
            ));
        }
        for step in path {
            self.require_tag(
                state,
                local,
                &prefix,
                current,
                step.variant,
                validate,
                span,
                budget,
            )?;
            let Content::Sum(variants, _) = current else {
                return Err(bad(Some(span), "Sum path is not an Enum"));
            };
            let Some(fields) = variants.get(&step.variant) else {
                return Ok(current);
            };
            current = &fields[step.index as usize];
            prefix.push((step.variant, step.index));
        }
        if let Some(tag) = last {
            self.require_tag(state, local, &prefix, current, tag, validate, span, budget)?;
        }
        Ok(current)
    }
    #[allow(clippy::too_many_arguments)]
    fn require_tag(
        &self,
        state: &State,
        local: LocalId,
        path: &Prefix,
        value: &Content,
        tag: u32,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(
            path.len()
                .saturating_add(state.facts.len())
                .saturating_add(1),
        )?;
        let known = matches!(value,Content::Sum(variants, _) if variants.len()==1 && variants.contains_key(&tag));
        if validate && !known && !state.facts.contains(&(local, path.clone(), tag)) {
            return Err(bad(
                Some(span),
                "payload requires the exact selected tag of this current local and complete prefix on every incoming path",
            ));
        }
        Ok(())
    }
    fn edge(&self, state: &mut State, block: usize, next: usize, budget: &mut Budget) -> Check<()> {
        if let (Some(meta),Some(p))=(&self.place_meta,&mut state.partial){meta.edge(p,&self.f.blocks[block].terminator,next,budget)?;}
        let Terminator::Branch {
            condition,
            then_block,
            else_block,
        } = self.f.blocks[block].terminator
        else {
            return Ok(());
        };
        if then_block == else_block || then_block.0 as usize != next {
            return Ok(());
        }
        let Some((b, i)) = self.definitions[condition.0 as usize] else {
            return Ok(());
        };
        let InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } = self.f.blocks[b].instructions[i].kind
        else {
            return Ok(());
        };
        for (tag, constant) in [(left, right), (right, left)] {
            if !state.observations.contains(&tag) {
                continue;
            }
            let Some((cb, ci)) = self.definitions[constant.0 as usize] else {
                continue;
            };
            let InstructionKind::Constant(Constant::I32(variant)) =
                self.f.blocks[cb].instructions[ci].kind
            else {
                continue;
            };
            if variant < 0 {
                continue;
            }
            if let Some((root, path)) = &self.observers[tag.0 as usize] {
                budget.spend(
                    path.len()
                        .saturating_add(state.facts.len())
                        .saturating_add(1),
                )?;
                state.facts.insert((*root, path.clone(), variant as u32));
            }
        }
        Ok(())
    }
    fn instantiate(
        &self,
        value: &Content,
        args: &[ValueId],
        state: &State,
        budget: &mut Budget,
    ) -> Check<Content> {
        value.cost(budget)?;
        Ok(match value {
            Content::Leaf(sources) => {
                let mut result = BTreeSet::new();
                for source in sources {
                    match source {
                        Source::Input(parameter, path) | Source::ReturnedInput(parameter, path) => {
                            let returned = matches!(source, Source::ReturnedInput(..));
                            budget.spend(path.len().saturating_add(1))?;
                            let mut source =
                                state.values[args[*parameter as usize].0 as usize].as_ref();
                            let mut cursor = 0;
                            while cursor < path.len() {
                                match (path[cursor], source) {
                                    (0, Content::Product(fields)) => {
                                        source = &fields[path[cursor + 1] as usize];
                                        cursor += 2;
                                    }
                                    (1, Content::Sum(variants, _)) => {
                                        let Some(fields) = variants.get(&path[cursor + 1]) else {
                                            break;
                                        };
                                        source = &fields[path[cursor + 2] as usize];
                                        cursor += 3;
                                    }
                                    _ => break,
                                }
                            }
                            if cursor == path.len() {
                                if let Content::Leaf(origins) = source {
                                    budget.spend(origins.len())?;
                                    result.extend(origins.iter().map(|origin| {
                                        if returned {
                                            match origin {
                                                Source::Fresh => Source::ReturnedFresh,
                                                Source::Input(p, path) => {
                                                    Source::ReturnedInput(*p, path.clone())
                                                }
                                                source => source.clone(),
                                            }
                                        } else {
                                            origin.clone()
                                        }
                                    }));
                                } else {
                                    result.insert(Source::Unknown);
                                }
                            } else {
                                result.insert(Source::Unknown);
                            }
                        }
                        source => {
                            result.insert(source.clone());
                        }
                    }
                }
                Content::Leaf(result)
            }
            Content::Product(fields) => Content::Product(
                fields
                    .iter()
                    .map(|c| self.instantiate(c, args, state, budget))
                    .collect::<Check<_>>()?,
            ),
            Content::Sum(_, Some((parameter, path))) => {
                budget.spend(path.len().saturating_add(1))?;
                let mut actual = state.values[args[*parameter as usize].0 as usize].as_ref();
                let mut cursor = 0;
                while cursor < path.len() {
                    match (path[cursor], actual) {
                        (0, Content::Product(fields)) => {
                            actual = &fields[path[cursor + 1] as usize];
                            cursor += 2;
                        }
                        (1, Content::Sum(variants, _)) => {
                            let Some(fields) = variants.get(&path[cursor + 1]) else {
                                break;
                            };
                            actual = &fields[path[cursor + 2] as usize];
                            cursor += 3;
                        }
                        _ => break,
                    }
                }
                if cursor == path.len() {
                    actual.duplicate(budget)?
                } else {
                    // The actual caller cannot take this inactive path. Keep
                    // the summary's conservative shape and leaf sources.
                    let Content::Sum(variants, _) = value else {
                        unreachable!()
                    };
                    let without_route = Content::Sum(variants.clone(), None);
                    self.instantiate(&without_route, args, state, budget)?
                }
            }
            Content::Sum(variants, None) => Content::Sum(
                variants
                    .iter()
                    .map(|(&v, fields)| {
                        Ok((
                            v,
                            fields
                                .iter()
                                .map(|c| self.instantiate(c, args, state, budget))
                                .collect::<Check<_>>()?,
                        ))
                    })
                    .collect::<Check<_>>()?,
                None,
            ),
            Content::Missing(child)=>Content::Missing(Box::new(self.instantiate(child,args,state,budget)?)),
            Content::Copy => Content::Copy,
        })
    }
    fn block(
        &self,
        block: usize,
        state: &mut State,
        validate: bool,
        record: bool,
        budget: &mut Budget,
    ) -> Check<Vec<Bridge>> {
        if record {
            budget.spend(self.f.blocks[block].instructions.len())?;
        }
        let mut bridges = Vec::new();
        for instruction in &self.f.blocks[block].instructions {
            budget.spend(1)?;
            let mut result = None;
            let mut bridge = Bridge::default();
            if let (Some(meta),Some(p))=(&self.place_meta,&mut state.partial){meta.step(p,instruction,budget)?;}
            match &instruction.kind {
                InstructionKind::BorrowLocalMut { local }
                    if matches!(self.f.locals[local.0 as usize].ty, Type::Enum { .. }) =>
                {
                    fn forget(at: &mut Content, t: &Type, b: &mut Budget) -> Check<()> {
                        resources::charge_payload(t, b)?;
                        if matches!(t, Type::Enum { .. }) && t.is_copy_value() {
                            *at = content(t, None, &mut Vec::new(), false, b)?;
                            return Ok(());
                        }
                        match (at, t) {
                            (Content::Sum(vs, _), Type::Enum { variants, .. }) => {
                                for (tag, fields) in vs {
                                    for (child, t) in
                                        fields.iter_mut().zip(&variants[*tag as usize].1)
                                    {
                                        forget(child, t, b)?;
                                    }
                                }
                            }
                            (Content::Product(children), Type::Package { fields, .. }) => {
                                for (child, (_, t)) in children.iter_mut().zip(fields) {
                                    forget(child, t, b)?;
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    forget(
                        Arc::make_mut(&mut state.locals[local.0 as usize]),
                        &self.f.locals[local.0 as usize].ty,
                        budget,
                    )?;
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew
                | InstructionKind::OwnedStoreNew { .. } => {
                    result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])))
                }
                InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedTuple { fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Product(
                        fields
                            .iter()
                            .map(|v| state.values[v.0 as usize].duplicate(budget))
                            .collect::<Check<_>>()?,
                    ));
                }
                InstructionKind::MakeVariant { variant, fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            *variant,
                            fields
                                .iter()
                                .map(|v| state.values[v.0 as usize].duplicate(budget))
                                .collect::<Check<_>>()?,
                        )]),
                        None,
                    ));
                }
                InstructionKind::MakeErrorUnion { is_error, value } => {
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            if *is_error { 0 } else { 1 },
                            vec![state.values[value.0 as usize].duplicate(budget)?],
                        )]),
                        None,
                    ))
                }
                InstructionKind::TakeErrorUnion { value, is_error } => {
                    if let Content::Sum(variants, _) = state.values[value.0 as usize].as_ref()
                        && let Some(fields) = variants.get(&(if *is_error { 0 } else { 1 }))
                    {
                        result = Some(fields[0].duplicate(budget)?);
                    }
                }
                InstructionKind::Store { local, value } => {
                    self.kill(state, *local, budget)?;
                    budget.spend(1)?;
                    state.locals[local.0 as usize] = state.values[value.0 as usize].clone();
                    state.initialized[local.0 as usize] = 2;
                }
                InstructionKind::TakeTupleField{local,path}|InstructionKind::TakeVariantField{local,path}=>{
                    if validate {
                        if state.initialized[local.0 as usize]!=2{return Err(bad(Some(instruction.span),"selective source storage is not initialized"));}
                        state.partial.as_ref().ok_or_else(||bad(None,"selective tag state is absent"))?.require(*local,path,budget)?;
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    result=Some(Arc::make_mut(&mut state.locals[local.0 as usize]).take_place(path,validate,budget)?);
                    self.kill(state,*local,budget)?;
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if validate && self.partial && !state.locals[local.0 as usize].complete(budget)? {
                        return Err(bad(Some(instruction.span),"whole local transfer/read requires every subtree initialized"));
                    }
                    result = Some(state.locals[local.0 as usize].duplicate(budget)?);
                    if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                        self.kill(state, *local, budget)?;
                        state.initialized[local.0 as usize] = 1;
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::ReadPackageField { local, index } => {
                    if let Content::Product(fields) = state.locals[local.0 as usize].as_ref() {
                        result = Some(fields[*index as usize].duplicate(budget)?);
                    }
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } => {
                    self.kill(state, *local, budget)?;
                    if Arc::strong_count(&state.locals[local.0 as usize]) > 1 {
                        state.locals[local.0 as usize].cost(budget)?;
                    }
                    if let Content::Product(fields) =
                        Arc::make_mut(&mut state.locals[local.0 as usize])
                    {
                        fields[*index as usize] =
                            state.values[value.0 as usize].duplicate(budget)?;
                    }
                }
                InstructionKind::VariantTagLocal { local, path }
                | InstructionKind::ReadVariantLocal { local, path }
                | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                    self.selected(
                        state,
                        *local,
                        path,
                        None,
                        validate,
                        instruction.span,
                        budget,
                    )?;
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowVariantLocal { mutable: true, .. }
                    ) {
                        let ty =
                            projected_type(self.f, *local, path, Some(instruction.span), budget)?;
                        if matches!(ty, Type::Enum { .. }) && ty.is_copy_value() {
                            fn forget(
                                at: &mut Content,
                                path: &[VariantProjection],
                                ty: &Type,
                                budget: &mut Budget,
                            ) -> Check<()> {
                                if path.is_empty() {
                                    *at = content(ty, None, &mut Vec::new(), false, budget)?;
                                    return Ok(());
                                }
                                let Content::Sum(vs, _) = at else {
                                    return Err(bad(None, "mutable payload path crosses non-Enum"));
                                };
                                if let Some(fs) = vs.get_mut(&path[0].variant) {
                                    let child = fs
                                        .get_mut(path[0].index as usize)
                                        .ok_or_else(|| bad(None, "mutable payload field absent"))?;
                                    forget(child, &path[1..], ty, budget)?;
                                }
                                Ok(())
                            }
                            state.locals[local.0 as usize].cost(budget)?;
                            forget(
                                Arc::make_mut(&mut state.locals[local.0 as usize]),
                                path,
                                ty,
                                budget,
                            )?;
                            budget.spend(
                                path.len()
                                    .saturating_add(state.facts.len())
                                    .saturating_add(state.observations.len()),
                            )?;
                            let prefix = path
                                .iter()
                                .map(|p| (p.variant, p.index))
                                .collect::<Vec<_>>();
                            state.facts.retain(|(root, observed, _)| {
                                *root != *local || !observed.starts_with(&prefix)
                            });
                            state.observations.retain(|v| {
                                self.observers[v.0 as usize].as_ref().is_none_or(
                                    |(root, observed)| {
                                        *root != *local || !observed.starts_with(&prefix)
                                    },
                                )
                            });
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::VariantTagLocal { .. }) {
                        budget.spend(state.observations.len().saturating_add(1))?;
                        state
                            .observations
                            .insert(instruction.result.expect("typed tag"));
                    }
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant: _,
                    fields,
                }
                | InstructionKind::UnpackTuple { local, fields } => {
                    let variant =
                        if let InstructionKind::UnpackVariant { variant, .. } = instruction.kind {
                            Some(variant)
                        } else {
                            None
                        };
                    if variant.is_some() {
                        self.selected(
                            state,
                            *local,
                            &[],
                            variant,
                            validate,
                            instruction.span,
                            budget,
                        )?;
                    }
                    if validate && self.partial && !state.locals[local.0 as usize].complete(budget)? {
                        return Err(bad(Some(instruction.span),"atomic unpack requires every subtree initialized"));
                    }
                    if validate && state.initialized[local.0 as usize] != 2 {
                        return Err(bad(
                            Some(instruction.span),
                            "Unpack source is not initialized",
                        ));
                    }
                    budget.spend(fields.len())?;
                    if validate && fields.iter().any(|l| state.initialized[l.0 as usize] != 1) {
                        return Err(bad(
                            Some(instruction.span),
                            "every Unpack destination must be vacant on every incoming path before the atomic transfer",
                        ));
                    }
                    let payload = match state.locals[local.0 as usize].as_ref() {
                        Content::Sum(variants, _) if variant.is_some() => {
                            variants.get(&variant.unwrap())
                        }
                        Content::Product(fields) if variant.is_none() => Some(fields),
                        _ => None,
                    };
                    let mut prepared = Vec::with_capacity(fields.len());
                    for (index, target) in fields.iter().enumerate() {
                        prepared.push(if let Some(payload) = payload {
                            payload[index].duplicate(budget)?
                        } else {
                            content(
                                &self.f.locals[target.0 as usize].ty,
                                None,
                                &mut Vec::new(),
                                false,
                                budget,
                            )?
                        });
                    }
                    self.kill(state, *local, budget)?;
                    state.initialized[local.0 as usize] = 1;
                    for (&target, value) in fields.iter().zip(prepared) {
                        self.kill(state, target, budget)?;
                        if record {
                            bridge.unpack.push((
                                target,
                                value.fresh(),
                                if matches!(
                                    self.f.locals[target.0 as usize].ty,
                                    Type::Package { .. }
                                ) {
                                    direct_fields(&value, budget)?
                                } else {
                                    Vec::new()
                                },
                            ));
                        }
                        state.locals[target.0 as usize] = Arc::new(value);
                        state.initialized[target.0 as usize] = 2;
                    }
                }
                InstructionKind::DropTuple { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => {
                    self.kill(state, *local, budget)?;
                    state.initialized[local.0 as usize] = 1;
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let mut returned = self.instantiate(
                        &self.summaries[function.0 as usize],
                        arguments,
                        state,
                        budget,
                    )?;
                    returned.returned(budget)?;
                    if self.module.functions[function.0 as usize].result_mode == PassingMode::Create
                        && errors::managed_success(
                            &self.module.functions[function.0 as usize].return_type,
                        )
                        && let Content::Sum(variants, _) = &mut returned
                        && let Some(fields) = variants.get_mut(&1)
                    {
                        fields[0] = Content::Leaf(BTreeSet::from([Source::Fresh]));
                    }
                    result = Some(returned);
                    if matches!(
                        self.f
                            .values
                            .get(instruction.result.map_or(usize::MAX, |v| v.0 as usize)),
                        Some(Type::Managed(_))
                    ) && self.module.functions[function.0 as usize].result_mode
                        == PassingMode::Create
                    {
                        result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])));
                    }
                }
                _ => {}
            }
            if let Some(output) = instruction.result {
                let value = if let Some(result) = result {
                    result
                } else {
                    content(
                        &self.f.values[output.0 as usize],
                        None,
                        &mut Vec::new(),
                        false,
                        budget,
                    )?
                };
                if record {
                    bridge.result = matches!(value, Content::Leaf(_)).then(|| value.fresh());
                    if matches!(self.f.values[output.0 as usize], Type::Package { .. }) {
                        bridge.package = direct_fields(&value, budget)?;
                    }
                }
                state.values[output.0 as usize] = Arc::new(value);
            }
            if record {
                bridges.push(bridge);
            }
        }
        Ok(bridges)
    }
}

fn run(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
) -> Check<(Content, Vec<Vec<Bridge>>)> {
    let mut locals = Vec::new();
    let mut values = Vec::new();
    budget.spend(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(f.blocks.len().saturating_mul(3)),
    )?;
    for local in &f.locals {
        locals.push(Arc::new(content(
            &local.ty,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?));
    }
    for ty in &f.values {
        values.push(Arc::new(content(ty, None, &mut Vec::new(), true, budget)?));
    }
    let mut state = State {
        initialized: vec![1; f.locals.len()],
        locals,
        values,
        facts: BTreeSet::new(),
        observations: BTreeSet::new(),
        partial: None,
    };
    for (index, local) in f.parameters.iter().enumerate() {
        state.initialized[local.0 as usize] = 2;
        state.locals[local.0 as usize] = Arc::new(content(
            &f.locals[local.0 as usize].ty,
            Some(index as u32),
            &mut Vec::new(),
            false,
            budget,
        )?);
    }
    let mut partial = false;
    let mut definitions = vec![None; f.values.len()];
    let mut observers = vec![None; f.values.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            partial |= matches!(instruction.kind,InstructionKind::BorrowPlaceLocal{..}|InstructionKind::PlaceTagLocal{..}|InstructionKind::TakeTupleField{..}|InstructionKind::TakeVariantField{..});
            if let Some(v) = instruction.result {
                definitions[v.0 as usize] = Some((b, i));
                if let InstructionKind::VariantTagLocal { local, path } = &instruction.kind {
                    budget.spend(path.len())?;
                    observers[v.0 as usize] =
                        Some((*local, path.iter().map(|p| (p.variant, p.index)).collect()));
                }
            }
        }
    }
    if partial {state.partial=Some(partial_places::State::default());}
    let flow = Flow {
        f,
        module,
        definitions,
        observers,
        summaries,
        partial,
        place_meta:if partial {Some(partial_places::Meta::new(f,budget)?)} else {None},
    };
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(state);
    let mut pending = VecDeque::from([f.entry.0 as usize]);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    while let Some(block) = pending.pop_front() {
        queued[block] = false;
        let mut output = inputs[block].as_ref().unwrap().duplicate(budget)?;
        flow.block(block, &mut output, false, false, budget)?;
        for &next in &graph.successors[block] {
            let mut edge = output.duplicate(budget)?;
            flow.edge(&mut edge, block, next, budget)?;
            let changed = if let Some(input) = &mut inputs[next] {
                input.merge(&edge, budget)?
            } else {
                inputs[next] = Some(edge);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push_back(next);
            }
        }
    }
    let mut result = content(&f.return_type, None, &mut Vec::new(), true, budget)?;
    let mut rows = vec![Vec::new(); f.blocks.len()];
    for (block, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            rows[block] = flow.block(block, &mut state, validate, validate, budget)?;
            if let Terminator::Return(Some(value)) = f.blocks[block].terminator {
                result.merge(&state.values[value.0 as usize], budget)?;
            }
        }
    }
    Ok((result, rows))
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Proofs> {
    if !budget.sums {
        return Ok(Proofs::default());
    }
    budget.spend(module.functions.len().saturating_mul(2))?;
    let mut summaries = Vec::new();
    for f in &module.functions {
        summaries.push(content(
            &f.return_type,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?);
    }
    let count = module.functions.len();
    for f in &module.functions {
        resources::charge_payload(&f.return_type, budget)?;
    }
    budget.spend(count.saturating_mul(6))?;
    let needed = module
        .functions
        .iter()
        .map(|f| {
            f.foreign.is_none()
                && (matches!(f.return_type, Type::Managed(_) | Type::Package { .. })
                    || owned(&f.return_type))
        })
        .collect::<Vec<_>>();
    let mut users = vec![BTreeSet::new(); count];
    let mut callees = vec![BTreeSet::new(); count];
    for (index, f) in module.functions.iter().enumerate() {
        if !needed[index] {
            continue;
        }
        for block in &f.blocks {
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call { function, .. } = instruction.kind {
                    let callee = function.0 as usize;
                    if needed[callee] {
                        budget.spend(
                            users[callee]
                                .len()
                                .saturating_add(callees[index].len())
                                .saturating_add(2),
                        )?;
                        users[callee].insert(index);
                        callees[index].insert(callee);
                    }
                }
            }
        }
    }
    let mut visited = vec![0u8; count];
    let mut order = VecDeque::new();
    for index in 0..count {
        if !needed[index] || visited[index] != 0 {
            continue;
        }
        let mut stack = vec![(index, false)];
        while let Some((node, finish)) = stack.pop() {
            budget.spend(1)?;
            if finish {
                visited[node] = 2;
                order.push_back(node);
            } else if visited[node] == 0 {
                visited[node] = 1;
                budget.spend(callees[node].len().saturating_add(1))?;
                stack.push((node, true));
                for &callee in &callees[node] {
                    if visited[callee] == 0 {
                        stack.push((callee, false));
                    }
                }
            }
        }
    }
    let mut queued = needed.clone();
    while let Some(index) = order.pop_front() {
        queued[index] = false;
        let (result, _) = run(
            module,
            &module.functions[index],
            &graphs[index],
            &summaries,
            false,
            budget,
        )?;
        if summaries[index].merge(&result, budget)? {
            budget.spend(users[index].len())?;
            for &user in &users[index] {
                if !queued[user] {
                    queued[user] = true;
                    order.push_back(user);
                }
            }
        }
    }
    let mut rows = Vec::new();
    for (index, f) in module.functions.iter().enumerate() {
        rows.push(if f.foreign.is_some() || !budget.sum_function(f.id) {
            Vec::new()
        } else {
            run(module, f, &graphs[index], &summaries, true, budget)?.1
        });
    }
    Ok(Proofs { rows })
}
