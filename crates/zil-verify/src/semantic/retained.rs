//! Interpret a retained type directory in one checked lexical domain. This is
//! type metadata only: no port row creates a target, loan, owner or empty heap.
use super::*;
#[derive(Clone)]
struct Term{key:ClassKey,origin:IntrinsicOrigin}
pub(super) struct CheckedRetained{pub(super) shape:RetainedLifeShape,pub(super) classes:Vec<ClassKey>,pub(super) origins:Vec<IntrinsicOrigin>}
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn retained_shape(&self,module:&TypedModule,recipe:&SemanticRetainedShapeRecipe,env:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedRetained>{
        let parts=self.retained_sequence(module,&recipe.domain,env,external,active,b)?;
        let mut domain:Vec<Term>=Vec::new();
        for term in parts{b.spend(domain.len().saturating_add(1))?;let mut duplicate=false;for previous in &domain{if self.same_origin(env,&previous.origin,&term.origin,b)?{if !self.same(env,&previous.key,&term.key,b)?{return Err(bad("retained intrinsic origin has conflicting class images"));}duplicate=true;break;}}if !duplicate{domain.push(term);}}
        let mut ports=Vec::new();let mut retained=Vec::new();let mut nominal_arguments=Vec::new();
        for row in &recipe.ports{b.spend(1)?;match row{
            SemanticPortPart::One{leaf,mode,class}=>{let value=self.evaluate_type(module,leaf,env,external,active,b)?;let term=self.retained_term(module,class,env,b)?;let binder=self.retained_binder(&domain,&term,env,b)?;
                let valid=matches!((&value.ty,mode),(Type::Address{mutable:false,..},StoredBorrowMode::SharedAddress)|(Type::Address{mutable:true,..},StoredBorrowMode::ExclusiveAddress)|(Type::Desc(_),StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription)|(Type::Mutex{..},StoredBorrowMode::SharedAddress));
                if !valid{return Err(bad("retained port has another complete leaf/mode"));}ports.push(RetainedBorrowPort{leaf:value.ty,mode:*mode,binder});},
            SemanticPortPart::FromType{ty,classes}=>{
                let value=self.evaluate_type(module,ty,env,external,active,b)?;let map=self.retained_sequence(module,classes,env,external,active,b)?;
                if map.len()!=value.classes.len(){return Err(bad("retained FromType omits a complete child class"));}
                for (leaf,mode,class) in self.retained_leaves(module,&value,b)?{let term=map.get(class as usize).ok_or_else(||bad("retained child leaf has no original class map"))?;ports.push(RetainedBorrowPort{leaf,mode,binder:self.retained_binder(&domain,term,env,b)?});}
            },
        }}
        for row in &recipe.retained{match row{
            SemanticRetainedPart::One{path,child_class,parent}=>{b.spend(path.len().saturating_add(1))?;let term=self.retained_term(module,parent,env,b)?;retained.push(StorageRetainedBinding{path:path.clone(),class:*child_class,binder:self.retained_binder(&domain,&term,env,b)?});},
            SemanticRetainedPart::All{path,parents}=>{let terms=self.retained_sequence(module,parents,env,external,active,b)?;for(class,term)in terms.iter().enumerate(){b.spend(path.len().saturating_add(1))?;retained.push(StorageRetainedBinding{path:path.clone(),class:class as u32,binder:self.retained_binder(&domain,term,env,b)?});}},
        }}
        for row in &recipe.nominal{match row{
            SemanticNominalPart::One{path,declaration,argument,class}=>{b.spend(path.len().saturating_add(1))?;let term=self.retained_term(module,class,env,b)?;nominal_arguments.push(NominalLifeArgument{path:path.clone(),declaration:*declaration,argument:*argument,binder:self.retained_binder(&domain,&term,env,b)?});},
            SemanticNominalPart::FromType{path,ty,classes}=>{let value=self.evaluate_type(module,ty,env,external,active,b)?;let map=self.retained_sequence(module,classes,env,external,active,b)?;if map.len()!=value.classes.len(){return Err(bad("nominal FromType omits the complete child/phantom domain"));}for row in &value.life.nominal_arguments{b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;let mut prefix=path.clone();prefix.extend_from_slice(&row.path);let term=map.get(row.binder as usize).ok_or_else(||bad("nominal child class mapping is absent"))?;nominal_arguments.push(NominalLifeArgument{path:prefix,declaration:row.declaration,argument:row.argument,binder:self.retained_binder(&domain,term,env,b)?});}},
        }}
        let mut seen=std::collections::BTreeSet::new();for port in &ports{dynamic_metadata::ty(&port.leaf,b)?;if !seen.insert(port){return Err(bad("retained directory repeats a complete public port"));}}
        if retained.windows(2).any(|rows|rows[0]>=rows[1])||nominal_arguments.windows(2).any(|rows|rows[0]>=rows[1]){return Err(bad("retained class/nominal directory is not canonical and unique"));}
        let shape=RetainedLifeShape{binder_count:domain.len() as u32,ports,retained,nominal_arguments};
        Ok(CheckedRetained{shape,classes:domain.iter().map(|term|term.key.clone()).collect(),origins:domain.into_iter().map(|term|term.origin).collect()})
    }
    fn retained_binder(&self,domain:&[Term],term:&Term,env:&EvalEnv,b:&mut Budget)->Check<u32>{for(index,row)in domain.iter().enumerate(){b.spend(1)?;if self.same_origin(env,&row.origin,&term.origin,b)?{if !self.same(env,&row.key,&term.key,b)?{return Err(bad("retained row changes a whole class occurrence"));}return Ok(index as u32);}}Err(bad("retained row selects a class absent from its full domain"))}
    fn retained_term(&self,module:&TypedModule,recipe:&SemanticLifeRecipe,env:&EvalEnv,b:&mut Budget)->Check<Term>{
        let key=self.life(module,recipe,env,b)?;
        // An explicit intrinsic occurrence keeps its pre-image identity. The
        // outside image may coincide with a different existing child class.
        let origin=if let SemanticLifeRecipe::IntrinsicClass{operand,path,class}=recipe{
            let (scope,operand)=self.recipe_operand(env,operand,b)?;
            IntrinsicOrigin::Existing{scope,operand,path:path.clone(),class:*class}
        }else{IntrinsicOrigin::DerivedClass{source:key.clone()}};
        Ok(Term{key,origin})
    }
    fn retained_sequence(&self,module:&TypedModule,recipe:&SemanticClassSequenceRecipe,env:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<Term>>{
        let mut values=Vec::new();for part in &recipe.parts{b.spend(1)?;match part{
            SemanticClassSequencePart::One(life)=>values.push(self.retained_term(module,life,env,b)?),
            SemanticClassSequencePart::AllClasses{ty,path}=>{
                let value=self.evaluate_type(module,ty,env,external,active,b)?;let child=lifetimes::select(&value.ty,path,b)?;let(count,_)=dynamic_witness::intrinsic_domain(module,child,b)?;
                for class in 0..count{b.spend(path.len().saturating_add(1))?;let binder=if path.is_empty(){class}else{let mut rows=value.life.retained.iter().filter(|row|row.path==*path&&row.class==class);let row=rows.next().ok_or_else(||bad("AllClasses child has no exact whole-domain map"))?;if rows.next().is_some(){return Err(bad("AllClasses child has duplicate whole-domain maps"));}row.binder};values.push(Term{key:value.classes.get(binder as usize).cloned().ok_or_else(||bad("AllClasses child class is absent"))?,origin:value.origins.get(binder as usize).cloned().ok_or_else(||bad("AllClasses child intrinsic origin is absent"))?});}
            },
        }}Ok(values)
    }
    fn retained_leaves(&self,module:&TypedModule,value:&CheckedType,b:&mut Budget)->Check<Vec<(Type,StoredBorrowMode,u32)>>{
        if !value.statics.is_empty(){return Err(unsupported("constant-source retained expansion needs its explicit complete leaf obligations",None));}
        let mut leaves=Vec::new();
        let opaque=match &value.ty{Type::Task{retained_life,..}|Type::MutexGuard{retained_life,..}=>Some(retained_life),Type::Dyn{interface}|Type::OwnedDyn{interface}=>{let row=module.dyn_interfaces.iter().find(|row|row.id==*interface).ok_or_else(||bad("retained dyn interface is absent"))?;Some(&row.retained_life)},_=>None};
        if let Some(shape)=opaque{for port in &shape.ports{dynamic_metadata::ty(&port.leaf,b)?;leaves.push((port.leaf.clone(),port.mode,port.binder));}return Ok(leaves);}
        for slot in &value.life.slots{b.spend(slot.path.len().saturating_add(1))?;let path=slot.path.iter().copied().map(LifeTypeProjection::Place).collect::<Vec<_>>();let leaf=lifetimes::select(&value.ty,&path,b)?;dynamic_metadata::ty(leaf,b)?;leaves.push((leaf.clone(),slot.mode,slot.binder));}
        if matches!(value.ty,Type::Closure(_))&&!value.classes.is_empty(){return Err(unsupported("retained Closure expansion needs its complete actual logical capture directory",None));}
        Ok(leaves)
    }
}
