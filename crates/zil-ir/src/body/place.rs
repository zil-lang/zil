//! Canonical place identity: a root plus a projection path.
//!
//! A field is identified by its declaration ([`FieldRef`]), never by name.
//! Names are only rendered in diagnostics. The relations defined here depend
//! only on structure, so they belong to the shared schema rather than to any
//! solver or checker.

use super::ids::{AdtId, ArgsId, FieldIdx, Local, TyId, VariantIdx};
use super::ty::{AdtKind, TyKind, TyTable};

/// One declared field of one nominal instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldRef {
    pub adt: AdtId,
    pub variant: VariantIdx,
    pub field: FieldIdx,
    /// Must equal the arguments of the base place's type.
    pub args: ArgsId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaceRoot {
    Local(Local),
    /// The n-th field of the running closure's environment.
    Capture(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Projection {
    /// A declared field; for an Enum it also selects the variant payload.
    Field(FieldRef),
    /// A structural tuple slot.
    TupleField(u32),
    /// The success (`true`) or error (`false`) slot of an error union.
    ErrorArm(bool),
    /// A dynamically indexed element of an array or slice. The index is a
    /// `usize` local read at the time of the access; it never proves apart.
    Index(Local),
    /// A statically known element.
    ConstIndex(u32),
    /// Through an address, a carrier or a descriptor to its target.
    Deref,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    pub root: PlaceRoot,
    pub projections: Vec<Projection>,
}

/// Structural relation between two places evaluated at the same point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceRelation {
    Equal,
    /// The first place is a strict prefix of the second.
    Contains,
    /// The second place is a strict prefix of the first.
    ContainedBy,
    /// Proven apart by distinct fields, slots or constant indices.
    Disjoint,
    /// No structural proof either way (dynamic indices, sibling variants).
    MayOverlap,
}

/// What a step reaches through. Determines whether the subplace is inline
/// storage of the root or memory reached through an indirection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerefBase {
    SharedRef,
    ExclusiveRef,
    Managed,
    Desc,
    OwnedDyn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceTyError {
    UnknownLocal(Local),
    UnknownCapture(u32),
    UnknownType(TyId),
    /// The step does not apply to the base type.
    Mismatch {
        step: usize,
        base: TyId,
    },
    /// The field reference names a different declaration or instance.
    FieldInstance {
        step: usize,
        field: FieldRef,
    },
    NoSuchField {
        step: usize,
        field: FieldRef,
    },
    /// An index local is not `usize`.
    IndexType {
        step: usize,
        index: Local,
    },
    ConstIndexRange {
        step: usize,
        index: u32,
        length: u32,
    },
    /// The substituted field type was not interned by the producer.
    MissingInstance {
        step: usize,
    },
}

impl Place {
    pub fn local(local: Local) -> Self {
        Self {
            root: PlaceRoot::Local(local),
            projections: Vec::new(),
        }
    }
    pub fn capture(index: u32) -> Self {
        Self {
            root: PlaceRoot::Capture(index),
            projections: Vec::new(),
        }
    }
    pub fn project(mut self, step: Projection) -> Self {
        self.projections.push(step);
        self
    }
    pub fn field(self, field: FieldRef) -> Self {
        self.project(Projection::Field(field))
    }
    pub fn deref(self) -> Self {
        self.project(Projection::Deref)
    }
    pub fn as_local(&self) -> Option<Local> {
        match (self.root, self.projections.is_empty()) {
            (PlaceRoot::Local(l), true) => Some(l),
            (PlaceRoot::Local(_) | PlaceRoot::Capture(_), _) => None,
        }
    }
    /// The place with its last `n` steps removed.
    pub fn prefix(&self, len: usize) -> Place {
        Place {
            root: self.root,
            projections: self.projections[..len].to_vec(),
        }
    }
    pub fn parent(&self) -> Option<Place> {
        (!self.projections.is_empty()).then(|| self.prefix(self.projections.len() - 1))
    }
    pub fn is_prefix_of(&self, other: &Place) -> bool {
        self.root == other.root
            && self.projections.len() <= other.projections.len()
            && self.projections == other.projections[..self.projections.len()]
    }
    /// Index of the first `Deref` step, if any. Steps before it are inline
    /// storage of the root.
    pub fn first_deref(&self) -> Option<usize> {
        self.projections
            .iter()
            .position(|p| matches!(p, Projection::Deref))
    }
    /// Locals read to evaluate the place (dynamic indices).
    pub fn index_locals(&self) -> impl Iterator<Item = Local> + '_ {
        self.projections.iter().filter_map(|p| match p {
            Projection::Index(l) => Some(*l),
            Projection::Field(_)
            | Projection::TupleField(_)
            | Projection::ErrorArm(_)
            | Projection::ConstIndex(_)
            | Projection::Deref => None,
        })
    }

    pub fn relation(&self, other: &Place) -> PlaceRelation {
        if self.root != other.root {
            // Distinct locals are distinct storage; distinct captures are
            // distinct environment fields. A path through a Deref may reach
            // any storage its address was borrowed from; loans relate those.
            return if self.first_deref().is_none() && other.first_deref().is_none() {
                PlaceRelation::Disjoint
            } else {
                PlaceRelation::MayOverlap
            };
        }
        for (a, b) in self.projections.iter().zip(&other.projections) {
            match step_relation(a, b) {
                StepRelation::Same => {}
                StepRelation::Apart => return PlaceRelation::Disjoint,
                StepRelation::Unknown => return PlaceRelation::MayOverlap,
            }
        }
        match self.projections.len().cmp(&other.projections.len()) {
            std::cmp::Ordering::Equal => {
                if self.index_locals().next().is_some() {
                    PlaceRelation::MayOverlap
                } else {
                    PlaceRelation::Equal
                }
            }
            std::cmp::Ordering::Less => PlaceRelation::Contains,
            std::cmp::Ordering::Greater => PlaceRelation::ContainedBy,
        }
    }
}

enum StepRelation {
    Same,
    Apart,
    Unknown,
}

fn step_relation(a: &Projection, b: &Projection) -> StepRelation {
    match (a, b) {
        (Projection::Field(x), Projection::Field(y)) => {
            if x == y {
                StepRelation::Same
            } else if x.adt == y.adt && x.args == y.args && x.variant == y.variant {
                StepRelation::Apart
            } else {
                // Sibling variants share payload storage.
                StepRelation::Unknown
            }
        }
        (Projection::TupleField(x), Projection::TupleField(y)) => {
            if x == y {
                StepRelation::Same
            } else {
                StepRelation::Apart
            }
        }
        (Projection::ErrorArm(x), Projection::ErrorArm(y)) => {
            if x == y {
                StepRelation::Same
            } else {
                StepRelation::Unknown
            }
        }
        (Projection::ConstIndex(x), Projection::ConstIndex(y)) => {
            if x == y {
                StepRelation::Same
            } else {
                StepRelation::Apart
            }
        }
        (Projection::Deref, Projection::Deref) => StepRelation::Same,
        // The same index local still names one element per evaluation, but
        // the relation is used across program points; keep it unknown.
        (Projection::Index(_), Projection::Index(_))
        | (Projection::Index(_), Projection::ConstIndex(_))
        | (Projection::ConstIndex(_), Projection::Index(_)) => StepRelation::Unknown,
        (
            Projection::Field(_)
            | Projection::TupleField(_)
            | Projection::ErrorArm(_)
            | Projection::Index(_)
            | Projection::ConstIndex(_)
            | Projection::Deref,
            _,
        ) => StepRelation::Unknown,
    }
}

/// Typing context of a place: the body's local and capture types.
pub trait PlaceTypes {
    fn local_ty(&self, local: Local) -> Option<TyId>;
    fn capture_ty(&self, index: u32) -> Option<TyId>;
    fn tys(&self) -> &TyTable;
}

/// Type reached after each step, plus what each `Deref` went through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceTyping {
    pub root: TyId,
    /// `steps[i]` is the type after projection `i`.
    pub steps: Vec<TyId>,
    /// One entry per `Deref` step, in order.
    pub derefs: Vec<DerefBase>,
}

impl PlaceTyping {
    pub fn ty(&self) -> TyId {
        self.steps.last().copied().unwrap_or(self.root)
    }
    /// Type of the place truncated to `len` steps.
    pub fn ty_at(&self, len: usize) -> TyId {
        if len == 0 {
            self.root
        } else {
            self.steps[len - 1]
        }
    }
}

/// Structural typing of `place`. This is the only definition of what each
/// projection means; every consumer calls it.
pub fn type_place(place: &Place, cx: &dyn PlaceTypes) -> Result<PlaceTyping, PlaceTyError> {
    let tys = cx.tys();
    let root = match place.root {
        PlaceRoot::Local(l) => cx.local_ty(l).ok_or(PlaceTyError::UnknownLocal(l))?,
        PlaceRoot::Capture(i) => cx.capture_ty(i).ok_or(PlaceTyError::UnknownCapture(i))?,
    };
    let mut current = root;
    let mut steps = Vec::with_capacity(place.projections.len());
    let mut derefs = Vec::new();
    for (step, projection) in place.projections.iter().enumerate() {
        let kind = tys.get(current).ok_or(PlaceTyError::UnknownType(current))?;
        let mismatch = PlaceTyError::Mismatch {
            step,
            base: current,
        };
        current = match (projection, kind) {
            (Projection::Field(f), TyKind::Adt { adt, args }) => {
                if f.adt != *adt || f.args != *args {
                    return Err(PlaceTyError::FieldInstance { step, field: *f });
                }
                let def = tys
                    .get_adt(*adt)
                    .ok_or(PlaceTyError::NoSuchField { step, field: *f })?;
                if def.kind != AdtKind::Enum && f.variant.0 != 0 {
                    return Err(PlaceTyError::NoSuchField { step, field: *f });
                }
                if def.field(f.variant, f.field).is_none() {
                    return Err(PlaceTyError::NoSuchField { step, field: *f });
                }
                tys.field_ty(f.adt, f.variant, f.field, f.args)
                    .ok_or(PlaceTyError::MissingInstance { step })?
            }
            (Projection::TupleField(i), TyKind::Tuple(items)) => {
                items.get(*i as usize).copied().ok_or(mismatch)?
            }
            (Projection::ErrorArm(ok), TyKind::ErrorUnion { ok: o, err }) => {
                if *ok {
                    *o
                } else {
                    *err
                }
            }
            (Projection::Index(index), TyKind::Array { element, .. } | TyKind::Slice(element)) => {
                let index_ty = cx
                    .local_ty(*index)
                    .ok_or(PlaceTyError::UnknownLocal(*index))?;
                if tys.get(index_ty) != Some(&TyKind::Int(super::ty::IntTy::Usize)) {
                    return Err(PlaceTyError::IndexType {
                        step,
                        index: *index,
                    });
                }
                *element
            }
            (Projection::ConstIndex(i), TyKind::Array { element, length }) => {
                if i >= length {
                    return Err(PlaceTyError::ConstIndexRange {
                        step,
                        index: *i,
                        length: *length,
                    });
                }
                *element
            }
            (
                Projection::Deref,
                TyKind::Ref {
                    mutability,
                    pointee,
                    ..
                },
            ) => {
                derefs.push(match mutability {
                    super::ty::Mutability::Shared => DerefBase::SharedRef,
                    super::ty::Mutability::Exclusive => DerefBase::ExclusiveRef,
                });
                *pointee
            }
            (Projection::Deref, TyKind::Managed(payload)) => {
                derefs.push(DerefBase::Managed);
                *payload
            }
            (Projection::Deref, TyKind::Desc(payload)) => {
                derefs.push(DerefBase::Desc);
                *payload
            }
            (
                Projection::Deref,
                TyKind::OwnedDyn {
                    def: d, regions, ..
                },
            ) => {
                derefs.push(DerefBase::OwnedDyn);
                tys.lookup(&TyKind::Dyn {
                    def: *d,
                    regions: regions.clone(),
                })
                .ok_or(PlaceTyError::MissingInstance { step })?
            }
            (
                Projection::Field(_)
                | Projection::TupleField(_)
                | Projection::ErrorArm(_)
                | Projection::Index(_)
                | Projection::ConstIndex(_)
                | Projection::Deref,
                _,
            ) => return Err(mismatch),
        };
        steps.push(current);
    }
    Ok(PlaceTyping {
        root,
        steps,
        derefs,
    })
}
