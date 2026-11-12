//! Compare the actual raw CFGs in their relative source-position domain.
//! This is exactly the old clone/rebase/equality operation, without making
//! mutable copies of either function's shared locals, blocks or instructions.
use zil_base::Span;
use zil_ir::{BasicBlock, Instruction, LocalDecl, TypedFunction};

pub(super) fn relative_equal(left: &TypedFunction, right: &TypedFunction) -> Option<bool> {
    // Do not let an early unequal header/body bypass malformed later spans.
    // The old operation rebased both complete inputs before testing equality;
    // None must still force the original current checker, rather than publish
    // a changed-result classification under an invalid position binding.
    validate_spans(left)?;
    validate_spans(right)?;
    // Exhaustive destructuring makes every added raw field require review.
    // Only the three span locations changed by incremental::rebase are relative;
    // nested contracts, instruction kinds and terminators remain exact values.
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span,
    } = right;
    Some(
        left.storage_life == *storage_life
            && left.call_kind == *call_kind
            && left.reference_return_bound == *reference_return_bound
            && left.callable_return_contract == *callable_return_contract
            && left.callable_contract == *callable_contract
            && left.protocol_contract == *protocol_contract
            && left.id == *id
            && left.name == *name
            && left.foreign == *foreign
            && left.environment == *environment
            && left.parameters == *parameters
            && left.return_type == *return_type
            && left.result_mode == *result_mode
            && left.values == *values
            && left.entry == *entry
            && offsets(left.span, left.span) == offsets(*span, *span)
            && left.locals.len() == locals.len()
            && left
                .locals
                .iter()
                .zip(locals)
                .all(|(left_local, right_local)| {
                    let LocalDecl {
                        id,
                        name,
                        ty,
                        mutable,
                        span,
                    } = right_local;
                    left_local.id == *id
                        && left_local.name == *name
                        && left_local.ty == *ty
                        && left_local.mutable == *mutable
                        && offsets(left_local.span, left.span) == offsets(*span, right.span)
                })
            && left.blocks.len() == blocks.len()
            && left
                .blocks
                .iter()
                .zip(blocks)
                .all(|(left_block, right_block)| {
                    let BasicBlock {
                        id,
                        instructions,
                        terminator,
                    } = right_block;
                    left_block.id == *id
                        && left_block.terminator == *terminator
                        && left_block.instructions.len() == instructions.len()
                        && left_block.instructions.iter().zip(instructions).all(
                            |(left_instruction, right_instruction)| {
                                let Instruction { result, kind, span } = right_instruction;
                                left_instruction.result == *result
                                    && left_instruction.kind == *kind
                                    && offsets(left_instruction.span, left.span)
                                        == offsets(*span, right.span)
                            },
                        )
                }),
    )
}

fn validate_spans(function: &TypedFunction) -> Option<()> {
    function.span.end.checked_sub(function.span.start)?;
    for span in std::iter::once(function.span)
        .chain(function.locals.iter().map(|local| local.span))
        .chain(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .map(|instruction| instruction.span),
        )
    {
        if span.file != function.span.file
            || span.start < function.span.start
            || span.end > function.span.end
            || span.start > span.end
        {
            return None;
        }
    }
    Some(())
}

// Called only after complete containment validation; subtraction cannot wrap.
fn offsets(span: Span, origin: Span) -> (usize, usize) {
    (span.start - origin.start, span.end - origin.start)
}

#[cfg(test)]
#[path = "relative_tests.rs"]
mod tests;
