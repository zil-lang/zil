//! A parameter used as a thin call depends on a real original caller. The
//! speculative shape allows checking the child body once while its operands
//! are still unresolved. The source proof commits only after all calls check.
use super::*;

#[derive(Default)]
pub(super) struct CodeInputs {
    demands: BTreeMap<(SemanticDefinitionId, usize), (Term, Vec<Term>)>,
    authored: BTreeSet<Term>,
    opened: BTreeMap<Term, Term>,
}

impl Solver<'_, '_> {
    pub(super) fn demand_code_input(
        &mut self,
        frame: &Frame,
        head: &SemanticOriginalExpr,
        term: Term,
        arity: usize,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
        if !matches!(self.state.arena[root], Kind::Infer(None)) {
            return Ok(());
        }
        let CoreExprKind::Name(name) = &head.kind else { return Ok(()); };
        let mut anchor = None;
        for local in frame.locals.iter().rev() {
            b.spend(local.name.len().saturating_add(name.len()).saturating_add(1), WORK)?;
            if local.name == *name {
                anchor = local.anchor;
                break;
            }
        }
        let Some(LocalAnchor::Parameter(index)) = anchor else { return Ok(()); };
        let declaration = self.inventory.definition(frame.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { parameters, signature, .. } =
            &declaration.body
        else { return Ok(()); };
        let index = index as usize;
        if declaration.kind != SemanticDefinitionKind::Closure
            || frame.parameters.get(index).copied() != Some(term)
            || !parameters.get(index).is_some_and(|p| p.ty.is_none())
            || !signature.parameters.get(index).is_some_and(|p| matches!(&p.ty,
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(ordinal),
                } if *ordinal as usize == index))
        {
            return Ok(());
        }

        let mut inputs = Vec::new();
        for _ in 0..arity {
            let input = self.var(b)?;
            Self::push(&mut inputs, input, b)?;
        }
        let result = self.var(b)?;
        b.spend(arity, WORK)?;
        let code = self.term(Kind::Code {
            kind: SemanticCodeKind::FunctionPointer,
            parameters: inputs,
            result,
            modes: vec![PassingMode::Infer; arity],
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
        }, b)?;
        self.unify(term, code, depth + 1, b)?;
        b.lookup(self.code_inputs.demands.len(), WORK)?;
        if self.code_inputs.demands.insert((frame.definition, index), (code, Vec::new())).is_some() {
            return Err(fail("original callable parameter has repeated inference demand"));
        }
        Ok(())
    }

    pub(super) fn authored_code(&mut self, code: Term, b: &mut Budget) -> CompileResult<Term> {
        b.lookup(self.code_inputs.authored.len(), WORK)?;
        self.code_inputs.authored.insert(code);
        Ok(code)
    }

    pub(super) fn opened_code(&mut self, code: Term, original: Term, b: &mut Budget) -> CompileResult<Term> {
        b.lookup(self.code_inputs.opened.len(), WORK)?;
        self.code_inputs.opened.insert(code, original);
        Ok(code)
    }

    pub(super) fn observe_code_argument(
        &mut self,
        caller: SemanticDefinitionId,
        node: NodeId,
        ordinal: usize,
        actual: Term,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.code_inputs.demands.is_empty() {
            return Ok(());
        }
        b.lookup(self.state.calls.len(), WORK)?;
        let Some(call) = self.state.calls.get(&(caller, node)).filter(|call| call.receiver.is_some())
        else { return Ok(()); };
        b.lookup(self.code_inputs.demands.len(), WORK)?;
        if let Some((_, witnesses)) = self.code_inputs.demands.get_mut(&(call.target, ordinal)) {
            Self::push(witnesses, actual, b)?;
        }
        Ok(())
    }

    pub(super) fn complete_code_inputs(&self, b: &mut Budget) -> CompileResult<()> {
        let mut proved = BTreeSet::new();
        loop {
            let before = proved.len();
            for (code, witnesses) in self.code_inputs.demands.values() {
                b.lookup(proved.len(), WORK)?;
                if proved.contains(code) {
                    continue;
                }
                for witness in witnesses {
                    let mut subject = self.type_root(*witness, b)?;
                    while let Some(original) = {
                        b.lookup(self.code_inputs.opened.len(), WORK)?;
                        self.code_inputs.opened.get(&subject)
                    } {
                        if *original >= subject {
                            return Err(fail("original code provenance does not name an earlier source"));
                        }
                        subject = *original;
                    }
                    b.lookup(self.code_inputs.authored.len(), WORK)?;
                    b.lookup(proved.len(), WORK)?;
                    if self.code_inputs.authored.contains(&subject) || proved.contains(&subject) {
                        proved.insert(*code);
                        break;
                    }
                }
            }
            if proved.len() == self.code_inputs.demands.len() {
                return Ok(());
            }
            if proved.len() == before {
                return Err(unknown("original inferred code input needs an independently grounded source caller"));
            }
        }
    }
}
