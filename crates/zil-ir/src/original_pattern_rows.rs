//! Structural specialization of original pattern rows. The caller derives every
//! constructor and payload column from its independently checked subject type.
use crate::{CorePatternKind, SemanticOriginalPattern};

pub type OriginalPatternRow<'a> = Vec<Option<&'a SemanticOriginalPattern>>;

/// Reserved implicit lifetime paths begin with this marker, followed by the
/// original statement node and the logical pattern binding coordinates.
pub const ORIGINAL_PATTERN_BORROW: u32 = 48;

/// A syntactic reference pattern retains its subject place. Its strongest
/// authored mode is used to check the subject's independently derived access.
pub fn original_pattern_reference_mode<E>(
    pattern: &SemanticOriginalPattern,
    depth: usize,
    pay: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Option<bool>, E> {
    pay(depth)?;
    let mut reference = None;
    let mut visit = |child| -> Result<(), E> {
        if let Some(mutable) = original_pattern_reference_mode(child, depth + 1, pay)? {
            reference = Some(reference.unwrap_or(false) || mutable);
        }
        Ok(())
    };
    match &pattern.kind {
        CorePatternKind::Reference { mutable, .. } => return Ok(Some(*mutable)),
        CorePatternKind::Tuple(fields)
        | CorePatternKind::Variant { fields, .. }
        | CorePatternKind::Constructor { fields, .. } => {
            for field in fields {
                visit(field)?;
            }
        }
        CorePatternKind::Record { fields, .. }
        | CorePatternKind::RecordConstructor { fields, .. } => {
            for (_, field) in fields {
                visit(field)?;
            }
        }
        CorePatternKind::ErrorArm { pattern, .. } => visit(pattern)?,
        _ => {}
    }
    Ok(reference)
}

#[derive(Clone, Copy)]
pub enum OriginalPatternHead<'a> {
    Default,
    Bool(bool),
    Tuple,
    Reference,
    ErrorArm(bool),
    Variant(&'a str),
    Record(&'a [&'a str]),
}

pub fn original_pattern_any(pattern: Option<&SemanticOriginalPattern>) -> bool {
    pattern.is_none_or(|pattern| match &pattern.kind {
        CorePatternKind::Name(_) | CorePatternKind::Wildcard => true,
        CorePatternKind::Reference { pattern, .. } => matches!(
            pattern.kind,
            CorePatternKind::Name(_) | CorePatternKind::Wildcard
        ),
        _ => false,
    })
}

/// Replace the head column by this constructor's selected child columns.
/// A synthesized None is an unrestricted column. The caller prepays the
/// row and constructor arity work, including ordered record label lookup.
pub fn specialize_original_patterns<'a>(
    rows: &[OriginalPatternRow<'a>],
    head: OriginalPatternHead<'_>,
    arity: usize,
) -> Vec<OriginalPatternRow<'a>> {
    rows.iter()
        .filter_map(|row| {
            let (first, tail) = row.split_first()?;
            let first = *first;
            let mut fields = Vec::with_capacity(arity.saturating_add(tail.len()));
            if original_pattern_any(first) {
                fields.resize(arity, None);
            } else {
                match (&head, &first?.kind) {
                    (OriginalPatternHead::Bool(expected), CorePatternKind::Bool(actual))
                        if expected == actual => {}
                    (OriginalPatternHead::Tuple, CorePatternKind::Tuple(children)) => {
                        fields.extend(children.iter().map(Some));
                    }
                    (
                        OriginalPatternHead::Reference,
                        CorePatternKind::Reference { pattern, .. },
                    ) => {
                        fields.push(Some(pattern.as_ref()));
                    }
                    (
                        OriginalPatternHead::ErrorArm(expected),
                        CorePatternKind::ErrorArm { is_error, pattern },
                    ) if expected == is_error => fields.push(Some(pattern.as_ref())),
                    (
                        OriginalPatternHead::Variant(expected),
                        CorePatternKind::Variant {
                            variant,
                            fields: children,
                            ..
                        }
                        | CorePatternKind::Constructor {
                            variant,
                            fields: children,
                            ..
                        },
                    ) if *expected == variant => fields.extend(children.iter().map(Some)),
                    (
                        OriginalPatternHead::Record(names),
                        CorePatternKind::Record {
                            fields: children, ..
                        }
                        | CorePatternKind::RecordConstructor {
                            fields: children, ..
                        },
                    ) => {
                        for label in *names {
                            fields.push(Some(&children.iter().find(|(name, _)| name == *label)?.1));
                        }
                    }
                    _ => return None,
                }
            }
            if fields.len() != arity {
                return None;
            }
            fields.extend_from_slice(tail);
            Some(fields)
        })
        .collect()
}
