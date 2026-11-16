//! Resolve an original erased type from its full independently evaluated key.
//! Matching metadata does not authorize any provider, runtime call or cleanup.
use super::*;
struct Context {ty:Option<Type>,shape:RetainedLifeShape,classes:Vec<ClassKey>,origins:Vec<IntrinsicOrigin>}
impl CheckedContext {
    pub(super) fn evaluate_dyn(&self,module:&TypedModule,owned:bool,recipe:&SemanticDynInterfaceRecipe,env:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let context=match &recipe.context{
            SemanticDynContextRecipe::Closed=>Context{ty:None,shape:RetainedLifeShape{binder_count:0,ports:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![],origins:vec![]},
            SemanticDynContextRecipe::Shape(shape)=>{let value=self.retained_shape(module,shape,env,external,active,b)?;Context{ty:None,shape:value.shape,classes:value.classes,origins:value.origins}},
            SemanticDynContextRecipe::Type(ty)=>{let value=self.evaluate_type(module,ty,env,external,active,b)?;let mut ports=Vec::new();for(leaf,mode,binder)in self.retained_leaves(module,&value,b)?{ports.push(RetainedBorrowPort{leaf,mode,binder});}dynamic_metadata::ty(&value.ty,b)?;Context{ty:Some(value.ty.clone()),shape:RetainedLifeShape{binder_count:value.life.binder_count,ports,retained:value.life.retained,nominal_arguments:value.life.nominal_arguments},classes:value.classes,origins:value.origins}},
        };
        let application=self.application(module,&recipe.application,env,external,active,b)?;
        let mut associated=Vec::new();for row in &recipe.associated{let application=self.application(module,&row.application,env,external,active,b)?;let value=match &row.value{SemanticAssociatedRecipe::Type(ty)=>CheckedActual::Type(self.evaluate_type(module,ty,env,external,active,b)?),SemanticAssociatedRecipe::Const{carrier,value}=>{let carrier=self.evaluate_type(module,carrier,env,external,active,b)?;let value=self.evaluate_constant(module,value,&carrier,env,external,active,b)?;CheckedActual::Const{carrier,value}}};associated.push((application,row.member,value));}
        let mut selected=None;
        for interface in &module.dyn_interfaces{
            b.spend(1)?;
            if interface.retained_life!=context.shape||context.ty.as_ref().is_some_and(|ty|interface.context.as_ref()!=Some(ty))||matches!(recipe.context,SemanticDynContextRecipe::Closed)&&interface.context.is_some(){continue;}
            if !self.dyn_application_matches(module,&application,&interface.application,&context.classes,&context.origins,env,b)?{continue;}
            if associated.len()!=interface.associated_types.len().saturating_add(interface.associated_constants.len()){continue;}
            let mut seen_types=std::collections::BTreeSet::new();let mut seen_consts=std::collections::BTreeSet::new();let mut complete=true;
            for(application,member,value)in &associated{
                let mut matches=Vec::new();match value{
                    CheckedActual::Type(expected)=>for(index,row)in interface.associated_types.iter().enumerate(){if row.member.ordinal==*member&&self.dyn_application_matches(module,application,&row.member.application,&context.classes,&context.origins,env,b)?{let actual=self.dyn_value(module,&row.ty,&row.life,&row.classes,&context.classes,&context.origins,b)?;if self.compatible(env,&actual,expected,b)?{matches.push(index);}}},
                    CheckedActual::Const{carrier,value}=>for(index,row)in interface.associated_constants.iter().enumerate(){if row.member.ordinal==*member&&row.ty==carrier.ty&&row.value==*value&&self.dyn_application_matches(module,application,&row.member.application,&context.classes,&context.origins,env,b)?{if !carrier.classes.is_empty(){return Err(bad("dynamic associated Const carrier retains a borrow domain"));}matches.push(index);}},
                    _=>return Err(bad("dynamic associated binding has another actual kind")),
                }
                if matches.len()!=1{complete=false;break;}let index=matches[0];let unique=if matches!(value,CheckedActual::Type(_)){seen_types.insert(index)}else{seen_consts.insert(index)};if !unique{complete=false;break;}
            }
            if !complete{continue;}
            if selected.replace(interface).is_some(){return Err(bad("original dynamic type selects multiple complete actual interfaces"));}
        }
        let interface=selected.ok_or_else(||bad("original dynamic type has no exact complete actual interface"))?;
        let ty=if owned{Type::OwnedDyn{interface:interface.id}}else{Type::Dyn{interface:interface.id}};
        let life=StorageLifeShape{binder_count:context.shape.binder_count,slots:vec![],retained:context.shape.retained,nominal_arguments:context.shape.nominal_arguments};
        let mut result=self.construct_value(module,&ty,&life,context.classes,b)?;result.origins=context.origins;Ok(result)
    }
    pub(super) fn dyn_value(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,actual:&[StorageBinder],domain:&[ClassKey],domain_origins:&[IntrinsicOrigin],b:&mut Budget)->Check<CheckedType>{
        let mut classes=Vec::new();for binder in actual{b.spend(1)?;let StorageBinder::Fixed(index)=binder else{return Err(bad("interface Type actual escapes its complete Fixed Context"));};classes.push(domain.get(*index as usize).ok_or_else(||bad("interface Type actual selects no complete Context class"))?.clone());}let mut value=self.construct_value(module,ty,life,classes,b)?;let mut origins=Vec::new();let mut seen=std::collections::BTreeSet::new();for binder in actual{let StorageBinder::Fixed(index)=binder else{return Err(bad("interface intrinsic source escapes Fixed Context"));};let origin=domain_origins.get(*index as usize).ok_or_else(||bad("interface intrinsic source has no complete original Context occurrence"))?;if !seen.insert(*index){return Err(bad("interface Type actual coalesces independent full intrinsic classes"));}origins.push(origin.clone());}value.origins=origins;Ok(value)
    }
    pub(super) fn dyn_application_matches(&self,module:&TypedModule,expected:&CheckedApplication,actual:&DynTraitApplication,domain:&[ClassKey],origins:&[IntrinsicOrigin],env:&EvalEnv,b:&mut Budget)->Check<bool>{
        if expected.declaration!=actual.declaration||expected.arguments.len()!=actual.actuals.len(){return Ok(false);}
        for(expected,actual)in expected.arguments.iter().zip(&actual.actuals){b.spend(1)?;let matches=match(expected,actual){
            (CheckedActual::Life(expected),DynGenericActual::Life(StorageBinder::Fixed(index)))=>self.same(env,expected,domain.get(*index as usize).ok_or_else(||bad("dynamic application Life escapes its complete Context"))?,b)?,
            (CheckedActual::Type(expected),DynGenericActual::Type(ty))=>{let(count,_)=dynamic_witness::intrinsic_domain(module,ty,b)?;count==0&&expected.ty==*ty&&expected.life==empty_life()&&expected.classes.is_empty()},
            (CheckedActual::Type(expected),DynGenericActual::QualifiedType(value))=>{let actual=self.dyn_value(module,&value.ty,&value.life,&value.classes,domain,origins,b)?;self.compatible(env,&actual,expected,b)?},
            (CheckedActual::Const{carrier,value},DynGenericActual::Const(actual))=>carrier.ty==actual.ty()&&carrier.classes.is_empty()&&value==actual,
            _=>false,
        };if !matches{return Ok(false);}}
        Ok(true)
    }
}
