//! Charge exact original lexical use queries to the Types request.
use crate::*;

pub(super) fn find(
    definition: &SemanticDefinition,
    name: &str,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<zil_base::NodeId>> {
    zil_ir::original_capture_use(
        definition,
        name,
        zil_ir::CaptureUse::First,
        |work, depth, span| {
            if depth > limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "capture first-use depth exceeded",
                    Some(span),
                ));
            }
            *left = left.checked_sub(work).ok_or_else(|| {
                let message = if work == 1 {
                    "capture first-use work exceeded"
                } else {
                    "capture lexical binding work exceeded"
                };
                Diagnostic::limit(Phase::Types, message, Some(span))
            })?;
            Ok(())
        },
        |message, span| Diagnostic::unsupported(Phase::Types, message, Some(span)),
    )
}
