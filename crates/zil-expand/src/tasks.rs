//! Source effect selection is raw interface input, never a no-external proof.
use super::*;

pub(super) fn selection(
    function: &s::Function,
    remaining: &mut usize,
) -> Result<Option<CallKind>, Diagnostic> {
    let fail = |span, message| Diagnostic::error(Phase::Expand, "E-TASK-ABI", Some(span), message);
    let mut selected = None;
    let mut resume = false;
    for clause in &function.contract {
        *remaining = remaining
            .checked_sub(clause.terms.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "task effect selection budget exceeded",
                    Some(clause.span),
                )
            })?;
        if !matches!(clause.name.as_str(), "effects" | "resume") {
            continue;
        }
        for term in &clause.terms {
            let bytes = term
                .path
                .iter()
                .fold(0usize, |n, part| n.saturating_add(part.len()));
            *remaining = remaining
                .checked_sub(bytes.saturating_add(term.arguments.len()).saturating_add(1))
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Expand,
                        "task effect term budget exceeded",
                        Some(term.span),
                    )
                })?;
        }
        if clause.name == "resume" {
            if resume || clause.terms.len() != 1 || !simple(&clause.terms[0]) {
                return Err(fail(
                    clause.span,
                    "resume requires one unique local restoration bound",
                ));
            }
            match clause.terms[0].path.as_slice() {
                [name] if name == "local" => resume = true,
                [name] if name == "transfer" => {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "task resume(transfer) requires the general Transfer frame contract",
                        Some(clause.span),
                    ));
                }
                _ => return Err(fail(clause.span, "unknown task restoration bound")),
            }
            continue;
        }
        if selected.is_some() {
            return Err(fail(
                clause.span,
                "effects upper bound must be declared once",
            ));
        }
        let mut suspend = false;
        for term in &clause.terms {
            let [name] = term.path.as_slice() else {
                return Err(fail(
                    term.span,
                    "effect marker must be a registered contract term",
                ));
            };
            match name.as_str() {
                "suspend" if simple(term) => suspend = true,
                "alloc" | "io" | "block" | "spawn" | "ffi" | "reenter" | "panic"
                    if simple(term) => {}
                "call" | "cleanup"
                    if term.type_args.is_empty()
                        && term.index.is_none()
                        && term.arguments.len() == 1 => {}
                _ => return Err(fail(term.span, "unknown or malformed effect marker")),
            }
        }
        selected = Some(if suspend {
            CallKind::Suspend
        } else {
            CallKind::Direct
        });
    }
    if resume && selected == Some(CallKind::Direct) {
        return Err(fail(
            function.span,
            "resume(local) conflicts with an explicit effects upper bound excluding suspend",
        ));
    }
    Ok(if resume {
        Some(CallKind::Suspend)
    } else {
        selected
    })
}
fn simple(term: &s::ContractTerm) -> bool {
    term.type_args.is_empty() && term.arguments.is_empty() && term.index.is_none()
}
