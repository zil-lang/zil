//! An implicit invocation borrow observes the exact original callable Place.
//! Its temporary outer head is not part of the captured callable's intrinsic
//! domain; the actual call/loan kernel still consumes that whole address.
use super::*;
impl CheckedContext{
    pub(super) fn original_callable_source(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,b:&mut Budget)->Check<zil_base::NodeId>{
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("actual borrowed callable has no complete original caller"));};
        b.spend(site_nodes.len())?;
        let mut nodes=site_nodes.iter().filter(|row|row.site==site.recipe);
        let row=nodes.next().ok_or_else(||bad("actual borrowed callable has no exact original call occurrence"))?;
        if nodes.next().is_some(){return Err(bad("actual borrowed callable has ambiguous original call occurrences"));}
        let SemanticOriginalNode::Expression(node)=row.node else{return Err(bad("actual borrowed callable source is not an original expression"));};
        self.original.borrow().as_ref().ok_or_else(||bad("actual borrowed callable lacks its independently checked source proof"))?.callable_source(module,caller.definition,node,b)
    }
}
