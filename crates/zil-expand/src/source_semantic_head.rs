//! Original finite foreign/resumption declarations. These are caller-visible
//! promises and ABI inputs, never body, provider or transfer proofs.
use super::*;

fn original_foreign(item: &s::Item, budget: &mut Budget<'_>) -> R<Option<ForeignDeclaration>> {
    let s::ItemKind::Extern { abi, function } = &item.kind else {
        return Ok(None);
    };
    budget.metadata_scope(function.span, false, |budget| {
        let size = protocol::read_contract(function, budget)?;
        budget.text(abi, function.span)?;
        budget.text(&function.name, function.span)?;
        // The normalizer compares original names and checks duplicates. Charge
        // the full lexical comparison upper before either temporary index is
        // constructed; a generic actual's physical spelling is never input.
        let lookups = function
            .params
            .len()
            .checked_add(size.nodes)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| {
                size.text
                    .checked_add(size.nodes)
                    .and_then(|m| m.checked_add(1))
                    .and_then(|m| n.checked_mul(m))
            })
            .ok_or_else(|| error(function.span, "foreign original lookup budget overflow"))?;
        budget.identity_work(lookups, function.span)?;
        let (mut live, mut peak) = (0usize, 0usize);
        let declaration = {
            let mut storage = |new: usize, old: usize| {
                let growing = live
                    .checked_add(new)
                    .ok_or_else(|| error(function.span, "foreign original allocation overflow"))?;
                if growing > peak {
                    budget.scratch_storage(growing - peak, function.span)?;
                    peak = growing;
                }
                live = growing.checked_sub(old).ok_or_else(|| {
                    error(function.span, "foreign original buffer retirement differs")
                })?;
                Ok(())
            };
            let (noescape_parameters, static_callbacks, scoped_callback, thread_driver_transfer) =
                crate::foreign::noescape_contract_with_storage(function, &mut storage)?;
            let abi = if abi == "\"C\"" { "C" } else { abi.as_str() };
            storage(abi.len(), 0)?;
            storage(function.name.len(), 0)?;
            ForeignDeclaration {
                abi: abi.to_owned(),
                symbol: function.name.clone(),
                noescape_parameters,
                static_callbacks,
                scoped_callback,
                thread_driver_transfer,
            }
        };
        prepay_recipe(&declaration, budget, function.span, 0)?;
        Ok(Some(declaration.clone()))
    })
}
fn original_resume(function: &s::Function, budget: &mut Budget<'_>) -> R<TaskResumeBound> {
    budget.metadata_scope(function.span, false, |budget| {
        protocol::read_contract(function, budget)?;
        let before = budget.remaining_limits().max_analysis_steps;
        let mut remaining = before;
        let selected = crate::tasks::selection(function, &mut remaining);
        budget.identity_work(before - remaining, function.span)?;
        selected.map(|(_, resume)| resume)
    })
}
impl Builder<'_, '_, '_, '_> {
    pub(super) fn original_foreign(&mut self, item: &s::Item) -> R<Option<ForeignDeclaration>> {
        original_foreign(item, self.budget)
    }
    pub(super) fn original_resume_bound(&mut self, function: &s::Function) -> R<TaskResumeBound> {
        original_resume(function, self.budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    fn item(text: &str) -> s::Item {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "original-head.zil", text);
        zil_syntax::parse(&source)
            .into_result()
            .unwrap()
            .items
            .into_iter()
            .next()
            .unwrap()
    }
    #[test]
    fn original_foreign_head_keeps_all_external_promises_and_errors() {
        let limits = Limits::default();
        let declared = item(
            "extern \"C\" fn observe(first:&i32,second:&i32)->i32 contract{effects(ffi);provides(noescape(second),noescape(first));};",
        );
        let row = original_foreign(&declared, &mut Budget::new(&limits))
            .unwrap()
            .unwrap();
        assert_eq!(row.abi, "C");
        assert_eq!(row.symbol, "observe");
        assert_eq!(row.noescape_parameters, vec![0, 1]);
        assert!(!row.static_callbacks);
        assert!(row.scoped_callback.is_none());
        assert!(!row.thread_driver_transfer);
        let promised = item(
            "extern \"C\" fn register(callback:extern \"C\" fn(c_handle,i32)->i64,context:c_handle)->c_handle contract{effects(ffi);provides(static_callbacks(),thread_async_callback_register(),thread_async_driver_transfer());};",
        );
        let promised = original_foreign(&promised, &mut Budget::new(&limits))
            .unwrap()
            .unwrap();
        assert!(promised.static_callbacks && promised.thread_driver_transfer);
        assert_eq!(
            promised.scoped_callback,
            Some(ScopedCallbackRole::ThreadAsyncRegister)
        );
        assert!(promised.noescape_parameters.is_empty());
        for invalid in [
            "extern \"C\" fn bad(value:&i32) contract{provides(noescape(value));};",
            "extern \"C\" fn bad(value:&i32) contract{effects(ffi);provides(noescape(missing));};",
            "extern \"C\" fn bad() contract{effects(ffi);provides(thread_async_driver_transfer());};",
        ] {
            assert_eq!(
                original_foreign(&item(invalid), &mut Budget::new(&limits))
                    .unwrap_err()
                    .code,
                "E-FFI-DECLARATION"
            );
        }
        assert!(
            original_foreign(&item("fn plain(){}"), &mut Budget::new(&limits))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn original_resume_bound_is_independent_and_strict() {
        let limits = Limits::default();
        for (text, expected) in [
            ("fn plain(){}", TaskResumeBound::Local),
            (
                "fn worker() contract{effects(suspend);resume(transfer);}{}",
                TaskResumeBound::Transfer,
            ),
            (
                "fn worker() contract{resume(local);}{}",
                TaskResumeBound::Local,
            ),
        ] {
            let s::ItemKind::Function(function) = item(text).kind else {
                panic!()
            };
            assert_eq!(
                original_resume(&function, &mut Budget::new(&limits)).unwrap(),
                expected
            );
        }
        for text in [
            "fn worker() contract{resume(transfer);resume(local);}{}",
            "fn worker() contract{effects(io);resume(transfer);}{}",
            "fn worker() contract{resume(unknown);}{}",
        ] {
            let s::ItemKind::Function(function) = item(text).kind else {
                panic!()
            };
            assert_eq!(
                original_resume(&function, &mut Budget::new(&limits))
                    .unwrap_err()
                    .code,
                "E-TASK-ABI"
            );
        }
    }
}
