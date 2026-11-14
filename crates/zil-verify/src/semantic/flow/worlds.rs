//! A Typed entry may implement several original instantiation worlds. The
//! calling occurrence selects a complete source environment, never the first
//! entry with a matching machine ABI. Each retained world is replayed by the
//! independent function pass as well.
use super::*;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_worlds(&self,module:&TypedModule,function:FunctionId,definition:SemanticDefinitionId,expected:&EvalEnv,caller:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let program=module.semantic.as_ref().ok_or_else(||bad("actual Call lacks its original instance directory"))?;
        b.spend(program.instances.len())?;
        let mut matched=false;
        for instance in program.instances.iter().filter(|row|row.definition==definition&&matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function)) {
            // An actual lexical ancestor is an identity, not a same-typed
            // substitute. Witness frames name independently selected impl
            // scopes and are instead compared by their full actuals below.
            let mut parent=instance.parent;
            let mut compatible=true;
            for frame in expected.lexical.iter().skip(1) {
                b.spend(1)?;
                let Some(id)=parent else{compatible=false;break;};
                let row=self.instance(module,id,b)?;
                if row.definition!=frame.definition||matches!(frame.key,ScopeKey::Instance(expected)if expected!=id){compatible=false;break;}
                parent=row.parent;
            }
            if !compatible||parent.is_some(){continue;}
            let actual=self.environment_for_instance(module,instance.id,active,b)?;
            if actual.lexical.len()!=expected.lexical.len(){return Err(bad("actual Call world changes its full lexical ancestry"));}
            // Different concrete specializations can share one emitted body.
            // Filter by complete substituted shapes and constant values; class
            // identities are checked by a single joint boundary, not erased.
            let mut shape=true;
            for(a,z)in actual.lexical.iter().zip(&expected.lexical){
                b.spend(a.actuals.len().saturating_add(a.environment.len()).saturating_add(1))?;
                if a.definition!=z.definition||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){shape=false;break;}
                if !a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)).all(|(a,z)|same_actual_shape(a,z)){shape=false;break;}
                if !match(&a.self_type,&z.self_type){(Some(a),Some(z))=>same_value_shape(a,z),(None,None)=>true,_=>false}{shape=false;break;}
            }
            if !shape{continue;}
            let mut boundary=trait_members::Boundary::default();
            for(a,z)in actual.lexical.iter().zip(&expected.lexical){
                for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){boundary.actual(self,caller,a,z,b)?;}
                if let(Some(a),Some(z))=(&a.self_type,&z.self_type){boundary.value(self,caller,a,z,b)?;}
            }
            matched=true;
        }
        if !matched{return Err(bad("actual Call has no complete source instance in its exact lexical world"));}
        Ok(())
    }
}
fn same_value_shape(a:&CheckedType,z:&CheckedType)->bool {
    a.ty==z.ty&&a.life==z.life&&a.classes.len()==z.classes.len()&&a.origins.len()==z.origins.len()&&a.statics==z.statics
}
fn same_actual_shape(a:&CheckedActual,z:&CheckedActual)->bool {
    match(a,z){(CheckedActual::Type(a),CheckedActual::Type(z))=>same_value_shape(a,z),(CheckedActual::Life(_),CheckedActual::Life(_))=>true,(CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})=>av==zv&&same_value_shape(a,z),_=>false}
}
