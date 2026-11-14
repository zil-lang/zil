//! Finite protocol declarations from the unchanged original source function.
//! These rows are untrusted contracts, never callee body/typestate proofs.
use super::*;

struct Reader<'a, 'l> {
    budget: &'a mut Budget<'l>,
    nodes: usize,
    text: usize,
}
impl crate::ct_selection::Visitor for Reader<'_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.nodes = self.nodes.saturating_add(1);
        self.budget.node(span, depth).map_err(|error| vec![error])
    }
    fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
        self.text = self.text.saturating_add(value.len());
        self.budget.text(value, span).map_err(|error| vec![error])
    }
}
struct Rows<'a>(&'a [FunctionProtocolContract]);
impl DynRecipeMetadata for Rows<'_> {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.rows(self.0.len(), depth)?;
        visitor.allocation(std::mem::size_of_val(self.0), depth)?;
        for row in self.0 {
            row.visit_dyn_metadata(visitor, depth + 1)?;
        }
        Ok(())
    }
}
/// Full original contract input size, useful for bounding a normalizer's
/// repeated lexical lookups without reading any transformed header.
pub(super) struct ContractSize {
    pub nodes: usize,
    pub text: usize,
}
pub(super) fn read_contract(function: &s::Function, budget: &mut Budget<'_>) -> R<ContractSize> {
    use crate::ct_selection::Visitor;
    let mut reader = Reader {
        budget,
        nodes: 0,
        text: 0,
    };
    reader
        .node(function.span, 0)
        .map_err(|mut errors| errors.remove(0))?;
    for parameter in &function.params {
        reader
            .node(parameter.span, 1)
            .map_err(|mut errors| errors.remove(0))?;
        reader
            .text(&parameter.name, parameter.span)
            .map_err(|mut errors| errors.remove(0))?;
    }
    for clause in &function.contract {
        reader
            .node(clause.span, 1)
            .map_err(|mut errors| errors.remove(0))?;
        reader
            .text(&clause.name, clause.span)
            .map_err(|mut errors| errors.remove(0))?;
        for term in &clause.terms {
            crate::ct_selection::walk_contract_term(&mut reader, term, 2)
                .map_err(|mut errors| errors.remove(0))?;
        }
    }
    Ok(ContractSize {
        nodes: reader.nodes,
        text: reader.text,
    })
}
fn original_rows(
    function: &s::Function,
    budget: &mut Budget<'_>,
) -> R<Vec<FunctionProtocolContract>> {
    budget.metadata_scope(function.span, false, |budget| {
        // Type/constant arguments and indices are read even when rejected by
        // the finite normalizer. Rejection never publishes an empty contract.
        read_contract(function, budget)?;
        let available = budget.remaining_limits().max_analysis_steps;
        let mut work = available;
        let (mut live, mut peak) = (0usize, 0usize);
        let parsed =
            crate::resources::protocol_rows_with_storage(function, &mut work, &mut |new, old| {
                let growing = live
                    .checked_add(new)
                    .ok_or_else(|| error(function.span, "original protocol allocation overflow"))?;
                if growing > peak {
                    budget.scratch_storage(growing - peak, function.span)?;
                    peak = growing;
                }
                live = growing.checked_sub(old).ok_or_else(|| {
                    error(function.span, "original protocol buffer retirement differs")
                })?;
                Ok(())
            });
        budget.identity_work(available - work, function.span)?;
        let parsed = parsed?;
        // The normalizer owns a temporary table. Prepay the complete clone
        // before the original definition retains its independent table; parser
        // buffers and walker stacks disappear on leaving this scope.
        prepay_recipe(&Rows(&parsed), budget, function.span, 0)?;
        Ok(parsed.clone())
    })
}
impl Builder<'_, '_, '_, '_> {
    pub(super) fn original_protocol_contracts(
        &mut self,
        function: &s::Function,
    ) -> R<Vec<FunctionProtocolContract>> {
        original_rows(function, self.budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    fn function(contract: &str) -> s::Function {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "original-protocol.zil",
            format!(
                "fn close<F:FnMut(i32)->i32 + ClosedEnvironment>(self:i32,give owner:AsyncRegistration<F>)->i32 contract{{{contract}}}{{return self}}"
            ),
        );
        let module = zil_syntax::parse(&source).into_result().unwrap();
        let s::ItemKind::Function(function) = module.items.into_iter().next().unwrap().kind else {
            panic!()
        };
        function
    }
    fn actual_heap(rows: &[FunctionProtocolContract]) -> usize {
        std::mem::size_of_val(rows)
            + rows
                .iter()
                .map(|row| {
                    std::mem::size_of_val(row.entry.as_slice())
                        + std::mem::size_of_val(row.exits.as_slice())
                        + row
                            .exits
                            .iter()
                            .map(|edge| std::mem::size_of_val(edge.output.as_slice()))
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
    #[test]
    fn original_protocol_keeps_value_ordinals_and_rejects_incomplete_relations() {
        let input = function("requires(state(owner,Closed,Open,Open));provides(retired(owner));");
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let before = budget.remaining_limits().max_source_bytes;
        let rows = original_rows(&input, &mut budget).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].parameter, 1); // Actual source self is ordinal 0.
        assert_eq!(rows[0].protocol, ProtocolKind::AsyncRegistrationV1);
        assert_eq!(
            rows[0].entry,
            vec![ProtocolState::Open, ProtocolState::Closed]
        );
        assert_eq!(rows[0].exits.len(), 2);
        assert!(
            rows[0]
                .exits
                .iter()
                .all(|edge| edge.exit == ProtocolExit::Return && edge.output.is_empty())
        );
        assert_eq!(
            before - budget.remaining_limits().max_source_bytes,
            actual_heap(&rows)
        );
        let mut work = usize::MAX;
        assert_eq!(
            rows,
            crate::resources::protocol_rows(&input, &mut work).unwrap()
        );
        for invalid in [
            "requires(state(owner,Open));",
            "provides(retired(owner));",
            "requires(state(owner,Open),state(owner,Closed));provides(retired(owner));",
            "requires(state(owner,Open));provides(on_ready(state(missing,Closed)));",
        ] {
            let bad = function(invalid);
            assert_eq!(
                original_rows(&bad, &mut Budget::new(&limits))
                    .unwrap_err()
                    .code,
                "E-PROTOCOL-CONTRACT"
            );
        }
    }
    #[test]
    fn original_protocol_heap_limit_is_exact_and_fresh() {
        let input = function(
            "requires(state(owner,Open,Closing,Closed));provides(on_pending(state(owner,Closing)),on_ready(state(owner,Closed)));",
        );
        let run = |bytes| {
            let limits = Limits {
                max_source_bytes: bytes,
                ..Limits::default()
            };
            original_rows(&input, &mut Budget::new(&limits))
        };
        let mut upper = 4096usize;
        while run(upper).is_err() {
            upper = upper.checked_mul(2).unwrap();
            assert!(upper < 1024 * 1024);
        }
        let mut lower = 0;
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            if run(middle).is_ok() {
                upper = middle;
            } else {
                lower = middle + 1;
            }
        }
        assert!(lower > 0);
        assert!(run(lower).is_ok());
        assert!(run(lower - 1).is_err());
        assert!(run(lower).is_ok());
        assert!(run(lower - 1).is_err());
    }
}
