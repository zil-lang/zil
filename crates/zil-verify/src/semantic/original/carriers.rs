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
