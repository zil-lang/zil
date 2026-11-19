//! A pattern binding retains its actual subject and discriminator path. This
//! directory supplies identities only; initialized tags and loans are checked
//! by the actual storage kernels.
use super::*;
use zil_base::Span;

pub(in crate::semantic) struct PatternSubject {
    pub(in crate::semantic) node: NodeId,
    pub(in crate::semantic) success_projections: usize,
    pub(in crate::semantic) anchor: Option<LocalAnchor>,
    pub(in crate::semantic) path: Vec<u32>,
    pub(super) retained: Option<Term>,
}
#[derive(Clone, Copy)]
pub(in crate::semantic) struct PatternBorrow {
    pub(in crate::semantic) mutable: bool,
    pub(in crate::semantic) ordinal: u32,
    pub(in crate::semantic) span: zil_base::Span,
}
pub(in crate::semantic) struct PatternOrigin {
    pub(in crate::semantic) owner: SemanticDefinitionId,
    pub(in crate::semantic) subject: PatternSubject,
    pub(in crate::semantic) path: Vec<PlaceProjection>,
    pub(in crate::semantic) borrow: Option<PatternBorrow>,
    pub(in crate::semantic) binding: Option<(Span, String)>,
    pub(in crate::semantic) record: bool,
}
impl PatternSubject {
    fn value(source: &SemanticOriginalExpr, b: &mut Budget) -> CompileResult<Self> {
        let mut operand = source;
        let mut success_projections = 0usize;
        loop {
            match &operand.kind {
                CoreExprKind::Try(inner) => {
                    b.spend(1, WORK)?;
                    success_projections += 1;
                    operand = inner;
                }
                CoreExprKind::Group(inner) => {
                    b.spend(1, WORK)?;
                    operand = inner;
                }
                _ => break,
            }
        }
        Ok(Self {
            node: if success_projections == 0 { source.id } else { operand.id },
            success_projections,
            anchor: None,
            path: Vec::new(),
            retained: None,
        })
    }
}

impl Solver<'_, '_> {
    pub(super) fn observed_pattern_subject(
        &mut self, frame: &mut Frame, source: &SemanticOriginalExpr, mode: Option<bool>,
        b: &mut Budget, depth: usize,
    ) -> CompileResult<(Term, PatternSubject)> {
        // Resolve the subject in the incoming state. Evaluating it can move
        // an affine field out of this very place. The pattern binds that
        // produced value and must keep its origin across the transfer.
        let place = self.place(frame, source, mode.is_some(), b, depth + 1)?;
        if let Some(mutable) = mode {
            if let Some(place) = place {
                if mutable {
                    let anchor = place.anchor.ok_or_else(|| fail("exclusive reference pattern lacks its actual writable root"))?;
                    b.spend(frame.locals.len(), WORK)?;
                    let local = frame.locals.iter().find(|local| local.anchor == Some(anchor))
                        .ok_or_else(|| fail("exclusive reference pattern lacks its actual writable root"))?;
                    if !local.mutable || local.borrowed {
                        return Err(fail("exclusive reference pattern requires a writable original root"));
                    }
                }
                return Ok((place.selected, PatternSubject {
                    node: source.id, success_projections: 0, anchor: place.anchor, path: place.path, retained: Some(place.selected),
                }));
            }
            let term = self.expression(frame, source, b, depth + 1)?;
            let mut subject = PatternSubject::value(source, b)?;
            subject.retained = Some(term);
            return Ok((term, subject));
        }
        let subject = match place {
            Some(place) => PatternSubject {
                node: source.id, success_projections: 0, anchor: place.anchor,
                path: place.path, retained: None,
            },
            None => PatternSubject::value(source, b)?,
        };
        Ok((self.expression(frame, source, b, depth + 1)?, subject))
    }

    // Original products already flatten declared enum fields in variant
    // order. Keep this lexical vacancy index distinct from the actual
    // Variant projection that still has to pass independent tag checks.
    pub(super) fn pattern_vacancy(
        &self, source: &PatternSubject, physical: &[PlaceProjection], b: &mut Budget,
    ) -> CompileResult<Vec<u32>> {
        let mut subject = source.retained.ok_or_else(|| fail("selective pattern lacks its retained subject"))?;
        b.spend(source.path.len().saturating_add(physical.len()), WORK)?;
        let mut path = source.path.clone();
        for step in physical {
            let (ordinal, child) = match (step, &self.state.arena[self.type_root(subject, b)?]) {
                (PlaceProjection::Field(index), Kind::Tuple(fields) | Kind::Nominal { fields, .. }) => {
                    (*index, fields.get(*index as usize).copied())
                }
                (PlaceProjection::Variant(VariantProjection { variant, index }), Kind::Nominal { definition, fields, .. }) => {
                    let declaration = self.inventory.definition(*definition, b)?;
                    let SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Enum { variants }, .. } = &declaration.body
                    else { return Err(fail("selective original pattern has a different nominal Sum")); };
                    b.spend(variants.len(), WORK)?;
                    let variant = *variant as usize;
                    let selected = variants.get(variant).ok_or_else(|| fail("selective original pattern variant is absent"))?;
                    if *index as usize >= selected.fields.len() { return Err(fail("selective original pattern field is absent")); }
                    let flat = variants[..variant].iter().fold(*index as usize, |n, row| n.saturating_add(row.fields.len()));
                    (u32::try_from(flat).map_err(|_| resource("original pattern vacancy ordinal overflow"))?, fields.get(flat).copied())
                }
                (PlaceProjection::ErrorArm(is_error), Kind::ErrorUnion { error, result }) => {
                    (u32::from(*is_error), Some(if *is_error { *error } else { *result }))
                }
                _ => return Err(fail("selective original pattern changes its retained place")),
            };
            path.push(ordinal);
            subject = child.ok_or_else(|| fail("selective original pattern leaves its retained place"))?;
        }
        Ok(path)
    }

    pub(super) fn pattern_anchor(
        &mut self,
        owner: SemanticDefinitionId,
        source: &PatternSubject,
        path: &[PlaceProjection],
        borrow: Option<PatternBorrow>,
        binding: Option<(Span, &str)>,
        record: bool,
        b: &mut Budget,
    ) -> CompileResult<LocalAnchor> {
        b.spend(source.path.len().saturating_add(path.len() + 1), WORK)?;
        if let Some((_, name)) = binding { b.spend(name.len(), WORK)?; }
        let index = u32::try_from(self.state.pattern_origins.len())
            .map_err(|_| resource("original pattern origin ordinal overflow"))?;
        Self::push(
            &mut self.state.pattern_origins,
            PatternOrigin {
                owner,
                subject: PatternSubject {
                    node: source.node,
                    success_projections: source.success_projections,
                    anchor: source.anchor,
                    path: source.path.clone(),
                    retained: source.retained,
                },
                path: path.to_vec(),
                borrow,
                binding: binding.map(|(span, name)| (span, name.to_owned())),
                record,
            },
            b,
        )?;
        Ok(LocalAnchor::Pattern(index))
    }
}
impl Checked {
    pub(in crate::semantic) fn pattern_origins(&self) -> &[PatternOrigin] { &self.pattern_origins }

    pub(in crate::semantic) fn pattern_origin(
        &self,
        ordinal: u32,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<&PatternOrigin> {
        b.lookup(self.pattern_origins.len(), WORK)?;
        self.pattern_origins
            .get(ordinal as usize)
            .filter(|origin| origin.owner == owner)
            .ok_or_else(|| fail("pattern origin belongs to another original definition"))
    }
}
