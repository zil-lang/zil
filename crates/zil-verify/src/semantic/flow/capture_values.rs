//! A class-free owning capture still needs its exact original binder and real
//! creator operand. Equal types and capture-array order supply neither.
use super::*;
impl CheckedContext {
    pub(super) fn classfree_capture(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,index:u32,value:ValueId,b:&mut Budget)->Check<()> {
        let definition=self.definition(module,child.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("owning capture lacks its whole original child body"));};
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(entry))=child.target else{return Err(bad("owning capture child is not executable"));};
        let f=module.functions.get(entry.0 as usize).ok_or_else(||bad("owning capture entry is absent"))?;
        let program=module.semantic.as_ref().ok_or_else(||bad("owning capture has no original site ledger"))?;
        let mut source=None;
        for site in program.sites.iter().filter(|site|site.instance==child.id){
            b.spend(1)?;
            let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("owning capture retains a Core operation"));};
            let Some(operation)=f.blocks.get(block.0 as usize).and_then(|block|block.instructions.get(instruction as usize))else{continue;};
            if !matches!(operation.kind,InstructionKind::ReadCapture{index:actual}|InstructionKind::TakeCapture{index:actual}|InstructionKind::DescribeCapture{index:actual}|InstructionKind::BindPackageCapture{index:actual,..} if actual==index){continue;}
            let SemanticSiteAction::TypeUse{subject,..}=&site.action else{continue;};
            let exact=match (&operation.kind,subject){(InstructionKind::BindPackageCapture{local,..},SemanticOperand::Local(actual))=>local==actual,(_,SemanticOperand::Value(actual))=>operation.result==Some(*actual),_=>false};
            if !exact{return Err(bad("capture use selects another actual SSA/local definition"));}
            b.spend(site_nodes.len())?;
            let node=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("capture use has no exact original node"))?;
            let SemanticOriginalNode::Expression(node)=node.node else{return Err(bad("capture use is not an original value expression"));};
            let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("capture use has no independent original binder proof"))?;
            let current=original.capture_read_source(child.definition,node,b)?;
            if source.replace(current).is_some_and(|old|old!=current){return Err(bad("one actual capture slot substitutes several original free binders"));}
        }
        let source=source.ok_or_else(||bad("owning capture slot has no checked original free-value use"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("capture creator is not an actual function"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture creator is absent"))?;
        self.intrinsic_buffer_source(module,creator,f,value,source,b)
    }
}
