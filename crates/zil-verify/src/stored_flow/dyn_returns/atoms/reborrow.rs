//! A child of a real held leaf keeps its data class. The head is a separate
//! parent obligation; it is never substituted as a pointee data target.
use super::*;

pub(super) fn resolve(
    module: &TypedModule,
    f: &TypedFunction,
    source: &StoragePosition,
    head: &StoragePosition,
    output: &Type,
    b: &mut Budget,
) -> Check<(Type, Option<StorageBinder>, Selection)> {
    dynamic_metadata::ty(output, b)?;
    let StorageBase::Parameter(parameter) = head.base else {
        return Err(bad("reborrow head is not an actual input permission"));
    };
    if source.base != head.base || source.path.is_empty() || !head.path.is_empty() {
        return Err(bad(
            "reborrow source is not a leaf held by that exact input head",
        ));
    }
    let Type::Address {
        mutable: head_mutable,
        inner,
    } = parameter_type(f, parameter)?
    else {
        return Err(bad("reborrow has no actual address head"));
    };
    match inner.as_ref() {
        Type::BorrowedStorage { .. } => {}
        Type::OwnedDyn { interface } | Type::Dyn { interface }
            if crate::dynamic_ports::interface(module, *interface, b)?
                .context
                .is_some() => {}
        _ => {
            return Err(unsupported(
                "reborrow head lacks a complete actual held-leaf directory",
                None,
            ));
        }
    }
    let (input_leaf, data) = input(module, f, source, b)?;
    let Type::Address {
        mutable: input_mutable,
        inner: pointee,
    } = &input_leaf
    else {
        return Err(unsupported(
            "reborrowed descriptor requires independent epoch and retirement proof",
            None,
        ));
    };
    let Type::Address {
        mutable,
        inner: output_pointee,
    } = output
    else {
        return Err(bad("reborrow result is not a complete address leaf"));
    };
    if (*mutable && (!input_mutable || !head_mutable)) || pointee != output_pointee {
        return Err(bad(
            "reborrow changes its full data Type or strengthens actual access",
        ));
    }
    // A slice carries its actual bounds with the borrowed value. Its element
    // still needs a complete class-free Type; this does not turn an unsized
    // slice into an inline OwnedView directory or cross an element borrow.
    let data_plain = match pointee.as_ref() {
        Type::Slice(element) => owned::plain(element, 0, b)?,
        pointee => owned::plain(pointee, 0, b)?,
    };
    if !data_plain {
        return Err(unsupported(
            "reborrow pointee requires a complete qualified occurrence map",
            None,
        ));
    }
    let data_class = binding(module, f, &data, b)?;
    // Validate the real head's independent scope. It remains distinct from the
    // original data class and is instantiated by the actual caller head loan.
    let _head_scope = relation(f, StorageBase::Parameter(parameter), &[], b)?;
    let (_, head) = input(module, f, head, b)?;
    dynamic_metadata::ty(output, b)?;
    Ok((
        output.clone(),
        Some(data_class),
        Selection::Reborrow { data, head },
    ))
}
