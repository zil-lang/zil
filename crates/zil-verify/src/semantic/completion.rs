//! One publication boundary for independently checked original executable
//! worlds. Registration carries no theorem; only the actual body interpreter
//! can produce the private completion token accepted here. Memory permissions,
//! phase/cleanup and deferred Static obligations remain separate kernels.
use super::*;

pub(super) struct Builder {
    module:usize,
    expected: BTreeMap<SemanticInstanceId, (FunctionId,SemanticDefinitionId)>,
    checked: std::collections::BTreeSet<SemanticInstanceId>,
    worlds: Vec<(SemanticInstanceId, EvalEnv)>,
    results: Vec<(SemanticInstanceId, CheckedType)>,
}

/// Immutable original-world facts. There is no constructor taking arbitrary
/// parallel environment/result arrays, nor a conversion from producer flags.
pub(crate) struct Completed {
    worlds: Vec<(SemanticInstanceId, EvalEnv)>,
    results: Vec<(SemanticInstanceId, CheckedType)>,
}
impl Builder {
    pub(super) fn register(module: &TypedModule, b: &mut Budget) -> Check<Self> {
        let program=module.semantic.as_ref().ok_or_else(||bad("world registration lacks its original programme"))?;
        let mut expected=BTreeMap::new();
        for row in &program.instances {
            b.spend(1)?;
            if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=row.target {
                b.spend(expected.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
                let entry=module.functions.get(function.0 as usize).filter(|entry|entry.id==function).ok_or_else(||bad("registered world has no exact actual executable"))?;
                if expected.insert(row.id,(entry.id,row.definition)).is_some(){return Err(bad("world registration repeats an original instance"));}
            }
        }
        b.spend(4)?;
        Ok(Self{module:module as *const TypedModule as usize,expected,checked:Default::default(),worlds:vec![],results:vec![]})
    }
    pub(super) fn record(&mut self,proof:flow::BodyChecked,b:&mut Budget)->Check<()> {
        let(id,definition,function,environment,result)=proof.into_parts();
        b.spend(self.expected.len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(self.checked.len().checked_ilog2().map_or(1,|n|n as usize+2)))?;
        if self.expected.get(&id)!=Some(&(function,definition))||self.checked.contains(&id){return Err(bad("completed body differs from its registered original world"));}
        // Reserve both records before changing either table. A failed budget
        // never publishes an environment without its exact result (or inverse).
        b.spend(environment.lexical.len().saturating_add(3))?;
        self.checked.insert(id);self.worlds.push((id,environment));self.results.push((id,result));
        Ok(())
    }
    pub(super) fn checked(&self)->&std::collections::BTreeSet<SemanticInstanceId>{&self.checked}
    /// Provisional records are visible only to this semantic kernel's pending
    /// child/member finalizers, never to downstream permission readers.
    pub(super) fn worlds(&self)->&[(SemanticInstanceId,EvalEnv)]{&self.worlds}
    pub(super) fn seal(self,module:&TypedModule,b:&mut Budget)->Check<Completed>{
        if self.module!=module as *const TypedModule as usize{return Err(bad("world completion selects another immutable module"));}
        b.spend(self.expected.len().saturating_add(self.checked.len()).saturating_add(self.worlds.len()).saturating_add(self.results.len()))?;
        if self.expected.len()!=self.checked.len()||self.worlds.len()!=self.expected.len()||self.results.len()!=self.expected.len(){return Err(bad("original world directory still has an unresolved executable dependency"));}
        for id in self.expected.keys(){if !self.checked.contains(id){return Err(bad("registered original world was never independently executed"));}}
        // A scope-only instance cannot hide an actual executable body.
        for entry in &module.functions{
            b.spend(self.expected.len().saturating_add(1))?;
            if entry.foreign.is_none()&&!self.expected.values().any(|(id,_)|*id==entry.id){return Err(bad("semantic programme omits an actual function instance"));}
        }
        Ok(Completed{worlds:self.worlds,results:self.results})
    }
}
impl Completed {
    pub(crate) fn worlds(&self)->&[(SemanticInstanceId,EvalEnv)]{&self.worlds}
    pub(crate) fn result(&self,id:SemanticInstanceId,b:&mut Budget)->Check<&CheckedType>{
        b.spend(self.results.len())?;
        self.results.iter().find(|(world,_)|*world==id).map(|(_,result)|result).ok_or_else(||bad("completed world loses its exact original result frame"))
    }
}
