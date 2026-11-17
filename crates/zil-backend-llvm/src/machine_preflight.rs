//! Reuse an actual full preflight only while the owned model is unchanged.
//! Kept within one build: no serialized receipt or source-to-machine authority.
use zil_llvm_api::model::Module;

#[derive(Default)]
pub(crate) struct CurrentPreflight {
    checked: Option<Module>,
    bodies: std::rc::Rc<std::cell::RefCell<zil_llvm_api::PreflightSession>>,
    pub(crate) executions: usize,
    pub(crate) reuses: usize,
    pub(crate) elapsed_ns: u64,
}
impl CurrentPreflight {
    pub(crate) fn check_aliases(
        &mut self,
        current: &crate::machine_output::AliasModule,
    ) -> Result<(), zil_llvm_api::Error> {
        self.executions += 1;
        let start = std::time::Instant::now();
        let result = self
            .bodies
            .borrow_mut()
            .validate_aliases(&current.triple, &current.views());
        self.elapsed_ns += crate::object_cache::elapsed_ns(start);
        result
    }
    pub(crate) fn body_session(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<zil_llvm_api::PreflightSession>> {
        self.bodies.clone()
    }
    pub(crate) fn check(&mut self, current: &Module) -> Result<(), zil_llvm_api::Error> {
        // SharedVec owns immutable items. Keeping this owner alive forces every
        // subsequent mutation through COW, including edits inside a Function.
        // No hash, recursive equality, or caller-provided bit is a receipt.
        if self.checked.as_ref().is_some_and(|old| {
            old.triple == current.triple
                && old.fingerprint == current.fingerprint
                && old.items.ptr_eq(&current.items)
        }) {
            self.reuses += 1;
            return Ok(());
        }
        self.executions += 1;
        let start = std::time::Instant::now();
        let result = self.bodies.borrow_mut().validate(current);
        self.elapsed_ns += crate::object_cache::elapsed_ns(start);
        result?;
        self.checked = Some(current.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::*;
    fn module() -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current".into(),
            items: vec![Item::Function(Function {
                name: "f".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), vec![]),
                attrs: vec![],
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))),
                        )],
                    }]
                    .into(),
                ),
            })]
            .into(),
        }
    }
    #[test]
    fn only_actual_immutable_owner_reuses_and_cow_mutation_revalidates() {
        let original = module();
        let mut checks = CurrentPreflight::default();
        checks.check(&original).unwrap();
        checks.check(&original.clone()).unwrap();
        assert_eq!((checks.executions, checks.reuses), (1, 1));
        let mut changed = original.clone();
        let Item::Function(f) = &mut changed.items[0] else {
            unreachable!()
        };
        f.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(64), Val::Int(7))));
        assert!(checks.check(&changed).is_err());
        assert!(zil_llvm_api::validate_module(&changed).is_err());
        checks.check(&original).unwrap();
        assert_eq!((checks.executions, checks.reuses), (2, 2));
        let fresh_equal = module();
        assert_eq!(fresh_equal, original);
        checks.check(&fresh_equal).unwrap();
        assert_eq!((checks.executions, checks.reuses), (3, 2));
    }
    #[test]
    fn model_headers_and_unrelated_declarations_are_current_inputs() {
        let original = module();
        let mut checks = CurrentPreflight::default();
        checks.check(&original).unwrap();
        let mut changed = original.clone();
        changed.fingerprint = "another".into();
        checks.check(&changed).unwrap();
        assert_eq!((checks.executions, checks.reuses), (2, 0));
        let mut invalid = changed.clone();
        let duplicate = invalid.items[0].clone();
        invalid.items.push(duplicate);
        assert!(checks.check(&invalid).is_err());
        assert!(zil_llvm_api::validate_module(&invalid).is_err());
        checks.check(&changed).unwrap();
        assert_eq!((checks.executions, checks.reuses), (3, 1));
    }
}
