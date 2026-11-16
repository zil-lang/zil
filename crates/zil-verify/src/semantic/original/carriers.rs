//! Retained recipes remain symbolic in the declaration world. AllClasses(T)
//! is a whole original T domain, never an empty domain inferred from a sample.
use super::*;
impl CheckedOriginal {
    pub(super) fn retained(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticRetainedShapeRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Retained>{
        let domain=self.class_sequence(module,context,&recipe.domain,env,active,b)?;
        let mut ports=Vec::new();let mut retained=Vec::new();let mut nominal=Vec::new();
        for row in &recipe.ports{b.spend(1)?;ports.push(match row{
            SemanticPortPart::One{leaf,mode,class}=>{let leaf=self.ty(module,context,leaf,env,active,b)?;let class=self.life(module,context,class,env,active,b)?;let outer=match(&leaf,mode){(Ty::Address{mutable:false,life,..},StoredBorrowMode::SharedAddress)|(Ty::Address{mutable:true,life,..},StoredBorrowMode::ExclusiveAddress)|(Ty::Desc{life,..},StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription)=>Some(life),(Ty::Carrier{kind:CarrierKind::Mutex,..},StoredBorrowMode::SharedAddress)=>None,_=>return Err(bad("original retained port has another full leaf/mode"))};if let Some(outer)=outer{if !self.same_life(outer,&class,b)?{return Err(bad("original retained port class differs from its complete leaf"));}}PortPart::One{leaf,mode:*mode,class}},
            SemanticPortPart::FromType{ty,classes}=>PortPart::FromType{ty:self.ty(module,context,ty,env,active,b)?,classes:self.class_sequence(module,context,classes,env,active,b)?},
        });}
        for row in &recipe.retained{retained.push(match row{
            SemanticRetainedPart::One{path,child_class,parent}=>{b.spend(path.len().saturating_add(1))?;RetainedPart::One{path:path.clone(),child_class:*child_class,parent:self.life(module,context,parent,env,active,b)?}},
            SemanticRetainedPart::All{path,parents}=>{b.spend(path.len().saturating_add(1))?;RetainedPart::All{path:path.clone(),parents:self.class_sequence(module,context,parents,env,active,b)?}},
        });}
        for row in &recipe.nominal{nominal.push(match row{
            SemanticNominalPart::One{path,declaration,argument,class}=>{b.spend(path.len().saturating_add(1))?;NominalPart::One{path:path.clone(),declaration:*declaration,argument:*argument,class:self.life(module,context,class,env,active,b)?}},
            SemanticNominalPart::FromType{path,ty,classes}=>{b.spend(path.len().saturating_add(1))?;NominalPart::FromType{path:path.clone(),ty:self.ty(module,context,ty,env,active,b)?,classes:self.class_sequence(module,context,classes,env,active,b)?}},
        });}
        Ok(Retained{domain,ports,retained,nominal})
    }
    fn class_sequence(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticClassSequenceRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<ClassPart>>{
        let mut values=Vec::new();for part in &recipe.parts{b.spend(1)?;values.push(match part{
            SemanticClassSequencePart::One(life)=>ClassPart::One(self.life(module,context,life,env,active,b)?),
            SemanticClassSequencePart::AllClasses{ty,path}=>{b.spend(path.len())?;ClassPart::All{ty:self.ty(module,context,ty,env,active,b)?,path:path.clone()}},
        });}Ok(values)
    }
    pub(super) fn reify_retained(&self,module:&TypedModule,shape:&Retained,env:&EvalEnv,b:&mut Budget)->Check<SemanticRetainedShapeRecipe>{
        let domain=self.reify_classes(module,&shape.domain,env,b)?;let mut ports=Vec::new();let mut retained=Vec::new();let mut nominal=Vec::new();
        for row in &shape.ports{b.spend(1)?;ports.push(match row{PortPart::One{leaf,mode,class}=>SemanticPortPart::One{leaf:Box::new(self.reify_type(module,leaf,env,b)?),mode:*mode,class:self.reify_life(module,class,env,b)?},PortPart::FromType{ty,classes}=>SemanticPortPart::FromType{ty:Box::new(self.reify_type(module,ty,env,b)?),classes:self.reify_classes(module,classes,env,b)?}});}
        for row in &shape.retained{retained.push(match row{RetainedPart::One{path,child_class,parent}=>{b.spend(path.len().saturating_add(1))?;SemanticRetainedPart::One{path:path.clone(),child_class:*child_class,parent:self.reify_life(module,parent,env,b)?}},RetainedPart::All{path,parents}=>{b.spend(path.len().saturating_add(1))?;SemanticRetainedPart::All{path:path.clone(),parents:self.reify_classes(module,parents,env,b)?}}});}
        for row in &shape.nominal{nominal.push(match row{NominalPart::One{path,declaration,argument,class}=>{b.spend(path.len().saturating_add(1))?;SemanticNominalPart::One{path:path.clone(),declaration:*declaration,argument:*argument,class:self.reify_life(module,class,env,b)?}},NominalPart::FromType{path,ty,classes}=>{b.spend(path.len().saturating_add(1))?;SemanticNominalPart::FromType{path:path.clone(),ty:Box::new(self.reify_type(module,ty,env,b)?),classes:self.reify_classes(module,classes,env,b)?}}});}
        Ok(SemanticRetainedShapeRecipe{domain,ports,retained,nominal})
    }
    fn reify_classes(&self,module:&TypedModule,parts:&[ClassPart],env:&EvalEnv,b:&mut Budget)->Check<SemanticClassSequenceRecipe>{let mut values=Vec::new();for part in parts{b.spend(1)?;values.push(match part{ClassPart::One(life)=>SemanticClassSequencePart::One(self.reify_life(module,life,env,b)?),ClassPart::All{ty,path}=>{b.spend(path.len())?;SemanticClassSequencePart::AllClasses{ty:Box::new(self.reify_type(module,ty,env,b)?),path:path.clone()}}});}Ok(SemanticClassSequenceRecipe{parts:values})}
}
impl Retained {
    pub(super) fn map(&self,mut ty:impl FnMut(&Ty,&mut Budget)->Check<Ty>,mut life:impl FnMut(&Life,&mut Budget)->Check<Life>,b:&mut Budget)->Check<Self>{
        fn classes(parts:&[ClassPart],ty:&mut impl FnMut(&Ty,&mut Budget)->Check<Ty>,life:&mut impl FnMut(&Life,&mut Budget)->Check<Life>,b:&mut Budget)->Check<Vec<ClassPart>>{let mut result=Vec::new();for row in parts{b.spend(1)?;result.push(match row{ClassPart::One(value)=>ClassPart::One(life(value,b)?),ClassPart::All{ty:value,path}=>{b.spend(path.len())?;ClassPart::All{ty:ty(value,b)?,path:path.clone()}}});}Ok(result)}
        let domain=classes(&self.domain,&mut ty,&mut life,b)?;let mut ports=Vec::new();let mut retained=Vec::new();let mut nominal=Vec::new();
        for row in &self.ports{b.spend(1)?;ports.push(match row{PortPart::One{leaf,mode,class}=>PortPart::One{leaf:ty(leaf,b)?,mode:*mode,class:life(class,b)?},PortPart::FromType{ty:value,classes:rows}=>PortPart::FromType{ty:ty(value,b)?,classes:classes(rows,&mut ty,&mut life,b)?}});}
        for row in &self.retained{retained.push(match row{RetainedPart::One{path,child_class,parent}=>{b.spend(path.len().saturating_add(1))?;RetainedPart::One{path:path.clone(),child_class:*child_class,parent:life(parent,b)?}},RetainedPart::All{path,parents}=>{b.spend(path.len().saturating_add(1))?;RetainedPart::All{path:path.clone(),parents:classes(parents,&mut ty,&mut life,b)?}}});}
        for row in &self.nominal{nominal.push(match row{NominalPart::One{path,declaration,argument,class}=>{b.spend(path.len().saturating_add(1))?;NominalPart::One{path:path.clone(),declaration:*declaration,argument:*argument,class:life(class,b)?}},NominalPart::FromType{path,ty:value,classes:rows}=>{b.spend(path.len().saturating_add(1))?;NominalPart::FromType{path:path.clone(),ty:ty(value,b)?,classes:classes(rows,&mut ty,&mut life,b)?}}});}
        Ok(Self{domain,ports,retained,nominal})
    }
    pub(super) fn types<'a>(&'a self,out:&mut Vec<&'a Ty>,b:&mut Budget)->Check<()>{
        fn classes<'a>(parts:&'a [ClassPart],out:&mut Vec<&'a Ty>,b:&mut Budget)->Check<()>{for row in parts{b.spend(1)?;if let ClassPart::All{ty,..}=row{out.push(ty);}}Ok(())}
        classes(&self.domain,out,b)?;for row in &self.ports{b.spend(1)?;match row{PortPart::One{leaf,..}=>out.push(leaf),PortPart::FromType{ty,classes:rows}=>{out.push(ty);classes(rows,out,b)?;}}}for row in &self.retained{b.spend(1)?;if let RetainedPart::All{parents,..}=row{classes(parents,out,b)?;}}for row in &self.nominal{b.spend(1)?;if let NominalPart::FromType{ty,classes:rows,..}=row{out.push(ty);classes(rows,out,b)?;}}Ok(())
    }
}

impl CheckedOriginal {
    /// Jointly solve only the fresh inference leaves of a complete original
    /// carrier recipe. The four directories and all occurrence selectors are
    /// invariant; All(T) remains a whole symbolic domain.
    pub(super) fn unify_retained(&mut self,left:&Retained,right:&Retained,b:&mut Budget)->Check<()> {
        if left.domain.len()!=right.domain.len()||left.ports.len()!=right.ports.len()||left.retained.len()!=right.retained.len()||left.nominal.len()!=right.nominal.len(){return Err(bad("original carrier changes its complete retained directory cardinalities"));}
        self.unify_class_sequence(&left.domain,&right.domain,b)?;
        for(a,z)in left.ports.iter().zip(&right.ports){b.spend(1)?;match(a,z){
            (PortPart::One{leaf:a,mode:am,class:ac},PortPart::One{leaf:z,mode:zm,class:zc})if am==zm=>{self.unify(a,z,b)?;self.unify_life(ac,zc,b)?;},
            (PortPart::FromType{ty:a,classes:ac},PortPart::FromType{ty:z,classes:zc})=>{self.unify(a,z,b)?;self.unify_class_sequence(ac,zc,b)?;},
            _=>return Err(bad("original carrier changes a retained port kind or mode")),
        }}
        for(a,z)in left.retained.iter().zip(&right.retained){b.spend(1)?;match(a,z){
            (RetainedPart::One{path:a,child_class:ac,parent:ap},RetainedPart::One{path:z,child_class:zc,parent:zp})=>{b.spend(a.len().saturating_add(z.len()))?;if a!=z||ac!=zc{return Err(bad("original carrier changes a retained child occurrence"));}self.unify_life(ap,zp,b)?;},
            (RetainedPart::All{path:a,parents:ap},RetainedPart::All{path:z,parents:zp})=>{b.spend(a.len().saturating_add(z.len()))?;if a!=z{return Err(bad("original carrier changes a retained whole child path"));}self.unify_class_sequence(ap,zp,b)?;},
            _=>return Err(bad("original carrier changes a retained child directory kind")),
        }}
        for(a,z)in left.nominal.iter().zip(&right.nominal){b.spend(1)?;match(a,z){
            (NominalPart::One{path:a,declaration:ad,argument:ai,class:ac},NominalPart::One{path:z,declaration:zd,argument:zi,class:zc})=>{b.spend(a.len().saturating_add(z.len()))?;if a!=z||ad!=zd||ai!=zi{return Err(bad("original carrier changes a complete nominal lifetime occurrence"));}self.unify_life(ac,zc,b)?;},
            (NominalPart::FromType{path:a,ty:at,classes:ac},NominalPart::FromType{path:z,ty:zt,classes:zc})=>{b.spend(a.len().saturating_add(z.len()))?;if a!=z{return Err(bad("original carrier changes a whole nominal Type occurrence"));}self.unify(at,zt,b)?;self.unify_class_sequence(ac,zc,b)?;},
            _=>return Err(bad("original carrier changes its nominal source directory kind")),
        }}Ok(())
    }
    fn unify_class_sequence(&mut self,left:&[ClassPart],right:&[ClassPart],b:&mut Budget)->Check<()> {
        b.spend(left.len().saturating_add(right.len()))?;
        if left.len()!=right.len(){return Err(bad("original carrier changes its complete class sequence cardinality"));}
        for(a,z)in left.iter().zip(right){match(a,z){
            (ClassPart::One(a),ClassPart::One(z))=>self.unify_life(a,z,b)?,
            (ClassPart::All{ty:a,path:ap},ClassPart::All{ty:z,path:zp})=>{b.spend(ap.len().saturating_add(zp.len()))?;if ap!=zp{return Err(bad("original carrier changes its whole Type class selector"));}self.unify(a,z,b)?;},
            _=>return Err(bad("original carrier replaces a whole Type class domain with another sequence kind")),
        }}Ok(())
    }
}

#[cfg(test)]
mod retained_call_inference_tests {
    use super::*;
    fn task(life:Life)->Ty{
        let ty=Ty::Address{mutable:true,life,inner:Box::new(Ty::Atom(Type::I32))};
        let classes=vec![ClassPart::All{ty:ty.clone(),path:vec![]}];
        Ty::Carrier{kind:CarrierKind::Task,child:Box::new(Ty::Atom(Type::I32)),retained:Box::new(Retained{
            domain:classes.clone(),ports:vec![PortPart::FromType{ty:ty.clone(),classes:classes.clone()}],
            retained:vec![RetainedPart::All{path:vec![LifeTypeProjection::Parameter(0)],parents:classes.clone()}],
            nominal:vec![NominalPart::FromType{path:vec![LifeTypeProjection::Parameter(0)],ty,classes}],
        })}
    }
    #[test]
    fn a_fresh_call_actual_is_solved_through_the_complete_captured_task_context(){
        let mut original=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:5};let captured=Life::Formal(owner,0);
        let fresh=original.fresh_life(&mut b).unwrap();
        original.unify(&task(captured.clone()),&task(fresh.clone()),&mut b).unwrap();
        assert_eq!(original.resolved_life(&fresh,&mut b).unwrap(),captured);
    }
    #[test]
    fn an_existing_task_context_cannot_change_its_rigid_lifetime_result_or_retained_selector(){
        let mut original=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:5};let value=task(Life::Formal(owner,0));
        assert!(original.unify(&value,&task(Life::Formal(owner,1)),&mut b).is_err());
        let mut wrong_result=value.clone();let Ty::Carrier{child,..}=&mut wrong_result else{unreachable!()};*child=Box::new(Ty::Atom(Type::I64));
        assert!(original.unify(&value,&wrong_result,&mut b).is_err());
        let mut wrong_path=value.clone();let Ty::Carrier{retained,..}=&mut wrong_path else{unreachable!()};let RetainedPart::All{path,..}=&mut retained.retained[0]else{unreachable!()};*path=vec![LifeTypeProjection::Result];
        assert!(original.unify(&value,&wrong_path,&mut b).is_err());
        let mut erased=value.clone();let Ty::Carrier{retained,..}=&mut erased else{unreachable!()};retained.domain=vec![ClassPart::One(Life::Formal(owner,0))];
        assert!(original.unify(&value,&erased,&mut b).is_err());
    }
}
