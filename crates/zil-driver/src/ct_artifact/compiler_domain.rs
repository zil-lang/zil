use super::*;
struct Domain<'a> {
    records: &'a [CtRecordSchema],
    enums: &'a [CtEnumSchema],
    budget: &'a mut CtDeclaredBudget,
}
impl Domain<'_> {
    fn spend(&mut self, n: usize) -> R<()> {
        self.budget.max_analysis_work = self
            .budget
            .max_analysis_work
            .checked_sub(n as u64)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT-only domain classification work exhausted",
                    None,
                )]
            })?;
        Ok(())
    }
    fn nodes(&mut self, n: usize) -> R<()> {
        self.budget.max_generated_nodes = self
            .budget
            .max_generated_nodes
            .checked_sub(n as u64)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT-only domain classification nodes exhausted",
                    None,
                )]
            })?;
        Ok(())
    }
    fn special(&mut self, ty: &CtTy, depth: u32, seen: &mut Vec<(bool, u32)>) -> R<bool> {
        self.spend(1)?;
        if depth > self.budget.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT-only domain type nesting exhausted",
                None,
            )]);
        }
        Ok(match ty {
            CtTy::Scalar(_) => false,
            CtTy::Array { element, .. } => self.special(element, depth + 1, seen)?,
            CtTy::Tuple(fields) => {
                let mut found = false;
                for ty in fields {
                    if self.special(ty, depth + 1, seen)? {
                        found = true;
                        break;
                    }
                }
                found
            }
            CtTy::Record(id) => {
                self.spend(seen.len())?;
                if seen.contains(&(false, id.0)) {
                    return Ok(false);
                }
                self.nodes(1)?;
                seen.push((false, id.0));
                let records = self.records;
                let fields = &records
                    .get(id.0 as usize)
                    .ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Ct,
                            "E-CT-SOURCE",
                            None,
                            "CT record schema absent during domain classification",
                        )]
                    })?
                    .fields;
                let mut found = false;
                for ty in fields {
                    if self.special(ty, depth + 1, seen)? {
                        found = true;
                        break;
                    }
                }
                seen.pop();
                found
            }
            CtTy::Enum(id) => {
                self.spend(seen.len())?;
                if seen.contains(&(true, id.0)) {
                    return Ok(false);
                }
                self.nodes(1)?;
                seen.push((true, id.0));
                let enums = self.enums;
                let variants = &enums
                    .get(id.0 as usize)
                    .ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Ct,
                            "E-CT-SOURCE",
                            None,
                            "CT enum schema absent during domain classification",
                        )]
                    })?
                    .variants;
                let mut found = false;
                for fields in variants {
                    for ty in fields {
                        if self.special(ty, depth + 1, seen)? {
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                seen.pop();
                found
            }
            // List and the remaining data kinds have no runtime value ABI.
            _ => true,
        })
    }
}
/// Derive CT-only definitions from actual attributed types and their callers.
/// Pure scalar helpers remain available to the ordinary runtime pipeline.
pub(super) fn definitions(raw: &mut CtArtifactProgram) -> R<Vec<CtSymbolId>> {
    let mut d = Domain {
        records: &raw.record_schemas,
        enums: &raw.enum_schemas,
        budget: &mut raw.budget,
    };
    d.spend(raw.functions.len())?;
    d.nodes(raw.functions.len())?;
    let mut flags = vec![false; raw.functions.len()];
    for (index, function) in raw.functions.iter().enumerate() {
        for ty in function
            .parameter_types
            .iter()
            .chain(std::iter::once(&function.result_type))
            .chain(&function.locals)
            .chain(&function.values)
        {
            if d.special(ty, 0, &mut Vec::new())? {
                flags[index] = true;
                break;
            }
        }
    }
    loop {
        let mut changed = false;
        for (index, function) in raw.functions.iter().enumerate() {
            d.spend(1)?;
            if flags[index] {
                continue;
            }
            for block in &function.blocks {
                d.spend(1)?;
                for instruction in &block.instructions {
                    d.spend(1)?;
                    if let CtInstructionKind::Call { function, .. } = instruction.kind {
                        let callee = *flags.get(function.0 as usize).ok_or_else(|| {
                            vec![Diagnostic::error(
                                Phase::Ct,
                                "E-CT-SOURCE",
                                None,
                                "CT call target absent during domain classification",
                            )]
                        })?;
                        if callee {
                            flags[index] = true;
                            changed = true;
                            break;
                        }
                    }
                }
                if flags[index] {
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut result = Vec::new();
    for (index, flag) in flags.into_iter().enumerate() {
        d.spend(1)?;
        if flag {
            d.nodes(1)?;
            result.push(raw.functions[index].definition.clone());
        }
    }
    Ok(result)
}
