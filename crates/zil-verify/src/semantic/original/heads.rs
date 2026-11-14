//! Complete original callable headers are checked before any actual proposal
//! supplies an ABI, provider promise, resume capability or state assumption.
use super::*;
impl CheckedOriginal {
    pub(super) fn function_head(&self,definition:&SemanticDefinition,b:&mut Budget)->Check<()> {
        let SemanticDefinitionBody::FunctionWithBody{signature,foreign,resume_bound,body,..}=&definition.body else{return Err(bad("callable header omits its whole original body"));};
        b.spend(signature.parameters.len().saturating_add(1))?;
        if *resume_bound!=TaskResumeBound::Local{return Err(unsupported("original nonlocal resume bound requires its complete universal worker/frame proof",None));}
        match (body,foreign){
            (SemanticOriginalFunctionBody::External,Some(foreign))=>{
                b.spend(foreign.abi.len().saturating_add(foreign.symbol.len()).saturating_add(foreign.noescape_parameters.len()))?;
                if definition.kind==SemanticDefinitionKind::Closure||signature.call_kind!=CallKind::Direct||foreign.abi!="C"||foreign.symbol=="main"||!foreign.symbol.as_bytes().first().is_some_and(|byte|byte.is_ascii_alphabetic()||*byte==b'_')||!foreign.symbol.bytes().all(|byte|byte.is_ascii_alphanumeric()||byte==b'_'){return Err(bad("original foreign ABI/symbol/effect is not its canonical external declaration"));}
                if foreign.noescape_parameters.windows(2).any(|rows|rows[0]>=rows[1])||foreign.noescape_parameters.iter().any(|ordinal|*ordinal as usize>=signature.parameters.len()){return Err(bad("original foreign noescape promises select duplicate or absent parameters"));}
                if foreign.scoped_callback.is_some()&&!foreign.static_callbacks{return Err(bad("original callback provider lacks its explicit static callback promise"));}
                if foreign.thread_driver_transfer&&!matches!(foreign.scoped_callback,Some(ScopedCallbackRole::ThreadAsyncRegister|ScopedCallbackRole::ThreadAsyncRequestClose|ScopedCallbackRole::ThreadAsyncPollDrain)){return Err(bad("original driver migration promise has another provider role"));}
            },
            (SemanticOriginalFunctionBody::External,None)=>return Err(bad("original external body omits its complete ABI/provider header")),
            (_,Some(_))=>return Err(bad("original user body advertises an unrelated external ABI")),
            _=>{},
        }Ok(())
    }
}
