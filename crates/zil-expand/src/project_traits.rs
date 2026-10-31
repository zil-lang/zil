//! Borrow only resolved public project declarations. A declaration supplies a
//! signature, never conformance or permission to implement a foreign Self.
use super::*;
use ct_selection::Visitor;

fn spend(work: &mut usize, amount: usize, span: Span) -> Result<(), Diagnostic> {
    *work = work.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "project trait declaration budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

struct Prepay<'a> {
    work: &'a mut usize,
    nodes: &'a mut usize,
    depth: usize,
}

impl Visitor for Prepay<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.depth || *self.nodes == 0 {
            return Err(vec![Diagnostic::limit(
                Phase::Expand,
                "project trait syntax budget exceeded",
                Some(span),
            )]);
        }
        spend(self.work, 1, span).map_err(|e| vec![e])?;
        *self.nodes -= 1;
        Ok(())
    }

    fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
        spend(self.work, value.len(), span).map_err(|e| vec![e])
    }
}

pub(super) struct Lookup<'a> {
    pub current: zil_base::ModuleId,
    pub name: &'a str,
    pub span: Span,
}

pub(super) fn declaration<'a>(
    lookup: Lookup<'_>,
    declarations: &[&'a s::Module],
    imports: &[CoreImport],
    work: &mut usize,
    nodes: &mut usize,
    limits: &Limits,
) -> Result<Option<&'a s::Item>, Diagnostic> {
    let Lookup {
        current,
        name,
        span,
    } = lookup;
    let mut found = None;
    for module in declarations {
        spend(work, 1, span)?;
        if module.id == current {
            continue;
        }
        for item in &module.items {
            spend(work, 1, item.span)?;
            let s::ItemKind::Trait(trait_declaration) = &item.kind else {
                continue;
            };
            spend(
                work,
                name.len().saturating_add(trait_declaration.name.len()),
                span,
            )?;
            if trait_declaration.name != name {
                continue;
            }
            if found.is_some() {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-TRAIT-DUPLICATE",
                    Some(span),
                    "duplicate resolved project trait identity",
                ));
            }
            if !item.public {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MODULE-PRIVATE",
                    Some(span),
                    "trait implementation requires a public imported declaration",
                ));
            }
            spend(work, imports.len(), span)?;
            if !imports.iter().any(|edge| edge.module == module.id) {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-MODULE-MEMBER",
                    Some(span),
                    "trait implementation declaration must belong to an imported module",
                ));
            }
            // Charge the full borrowed signature before any receiver, type,
            // name or index entry is copied by the existing conformance path.
            Prepay {
                work,
                nodes,
                depth: limits.max_call_depth,
            }
            .item(item, 0)
            .map_err(|mut e| e.remove(0))?;
            spend(work, name.len().saturating_mul(3), span)?;
            found = Some(item);
        }
    }
    Ok(found)
}
