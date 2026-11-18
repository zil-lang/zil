//! A nominal's complete intrinsic domain is distinct from each outside Life
//! image, including the outer lifetime of an address to that nominal value.
use super::*;
use std::rc::Rc;

impl CheckedContext {
    pub(crate) fn normalized_origin(&self,env:&EvalEnv,origin:&IntrinsicOrigin,b:&mut Budget)->Check<IntrinsicOrigin>{
        let origin=self.canonical_origin(origin,b)?;
        let IntrinsicOrigin::Nominal{source,class}=origin else{return Ok(origin);};
        let mut actuals=Vec::new();let mut environment=Vec::new();let mut sources=Vec::new();
        for actual in &source.actuals{actuals.push(self.normalized_origin_actual(env,actual,b)?);}
        for actual in &source.environment{environment.push(self.normalized_origin_actual(env,actual,b)?);}
        for origin in &source.class_sources{sources.push(match origin{IntrinsicOrigin::DerivedClass{source}=>IntrinsicOrigin::DerivedClass{source:self.exact_class_representative(env,source,b)?},other=>self.normalized_origin(env,other,b)?});}
        dynamic_metadata::life_shape(&source.domain,b)?;
        Ok(IntrinsicOrigin::Nominal{source:Rc::new(NominalOrigin{declaration:source.declaration,actuals,environment,domain:source.domain.clone(),class_sources:sources}),class})
    }
    fn normalized_origin_actual(&self,env:&EvalEnv,actual:&CheckedActual,b:&mut Budget)->Check<CheckedActual>{
        charge_actual(actual,b)?;
        let mut result=actual.clone();
        match &mut result{
            CheckedActual::Life(key)=>*key=self.exact_class_representative(env,key,b)?,
            CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>{
                // Existing domains retain every index and κ even if their
                // outside images become equal under an original premise.
                for key in &mut value.classes{*key=self.exact_class_representative(env,key,b)?;}
                for origin in &mut value.origins{*origin=self.normalized_origin(env,origin,b)?;}
            },
        }Ok(result)
    }
    fn exact_class_representative(&self,env:&EvalEnv,key:&ClassKey,b:&mut Budget)->Check<ClassKey>{
        charge_key(key,b)?;let graph=self.premises.borrow();
        let mut least=key.clone();let mut pending=vec![key.clone()];let mut seen=std::collections::BTreeSet::new();
        while let Some(key)=pending.pop(){b.spend(seen.len().saturating_add(1))?;if !seen.insert(key.clone()){continue;}if key<least{least=key.clone();}
            for scope in &env.assumptions{if let Some(rows)=graph.get(scope){b.spend(rows.relations.equalities.len())?;for(a,z)in &rows.relations.equalities{if a==&key{charge_key(z,b)?;pending.push(z.clone());}if z==&key{charge_key(a,b)?;pending.push(a.clone());}}}}
        }Ok(least)
    }
    pub(super) fn nominal_claim_origin(&self,raw:&IntrinsicOrigin,expected:&IntrinsicOrigin,expression:Option<&IntrinsicOrigin>,env:&EvalEnv,b:&mut Budget)->Check<Option<IntrinsicOrigin>>{
        let(IntrinsicOrigin::Nominal{source:a,class:x},IntrinsicOrigin::Nominal{source:z,class:y})=(raw,expected)else{return Ok(None);};
        charge_origin(raw,b)?;charge_origin(expected,b)?;
        if x!=y||a.declaration!=z.declaration||a.domain!=z.domain||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Err(bad("nominal claim changes its complete original identity/domain/arguments").with_note(format!("nominal claim at {:?}; actual class {x}, declaration {:?}, domain {:?}, actuals {:?}, sealed {:?}; original class {y}, declaration {:?}, domain {:?}, actuals {:?}, sealed {:?}",env.at,a.declaration,a.domain,a.actuals,a.environment,z.declaration,z.domain,z.actuals,z.environment)));}
        for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){
            let equal=match(a,z){
                (CheckedActual::Life(a),CheckedActual::Life(z))=>self.same(env,a,z,b)?,
                (CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})=>av==zv&&self.same_type(env,a,z,b)?,
                (CheckedActual::Type(a),CheckedActual::Type(z))=>{
                    if a.ty!=z.ty||a.life!=z.life||a.classes.len()!=z.classes.len()||a.statics!=z.statics{false}else{let mut same=true;for(a,z)in a.classes.iter().zip(&z.classes){if !self.same(env,a,z,b)?{same=false;break;}}same}
                },
                _=>false,
            };if !equal{return Err(bad("nominal claim changes a full mixed actual or its complete child domain"));}
        }
        if let Some(origin)=expression{
            let source=z.class_sources.get(*y as usize).ok_or_else(||bad("nominal original class source is absent"))?;
            if !self.same_origin(env,origin,expected,b)?&&!self.same_origin(env,origin,source,b)?{return Err(bad("nominal claim permutes an independently scoped intrinsic source"));}
        }
        // The original recipe supplied the Type actual's local κ identities;
        // the raw metadata has independently matched the whole actual shape
        // and each retained index above, not replaced those κ by its images.
        Ok(Some(expected.clone()))
    }

    pub(super) fn nominal_origins(&self,declaration:LifeDeclarationId,actuals:&[CheckedActual],sealed:&[CheckedActual],domain:&StorageLifeShape,sources:&[IntrinsicOrigin],b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        if sources.len()!=domain.binder_count as usize{return Err(bad("nominal intrinsic source directory is incomplete"));}
        for actual in actuals.iter().chain(sealed){charge_actual(actual,b)?;}
        for source in sources{charge_origin(source,b)?;}
        dynamic_metadata::life_shape(domain,b)?;
        let source=Rc::new(NominalOrigin{declaration,actuals:actuals.to_vec(),environment:sealed.to_vec(),domain:domain.clone(),class_sources:sources.to_vec()});
        b.spend(sources.len())?;
        Ok((0..domain.binder_count).map(|class|IntrinsicOrigin::Nominal{source:source.clone(),class}).collect())
    }

    /// Rebuild each root-domain origin from complete typed occurrence maps.
    /// Nominal children are boundaries; their intrinsic indices are never
    /// collapsed into the outer borrow or another child with equal images.
    pub(super) fn typed_origins(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[ClassKey],b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        self.typed_origins_at(module,ty,life,classes,&mut Vec::new(),0,b)
    }
    fn typed_origins_at(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[ClassKey],active:&mut Vec<Type>,depth:usize,b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        dynamic_metadata::ty(ty,b)?;dynamic_metadata::life_shape(life,b)?;
        b.spend(active.len().saturating_add(1))?;
        if active.contains(ty){return Err(bad("typed nominal argument origin graph has a cycle"));}
        if depth>b.limits.max_call_depth.min(128){return Err(limit("typed nominal argument origin depth exceeded"));}
        active.push(ty.clone());
        let result=(||{

        if classes.len()!=life.binder_count as usize{return Err(bad("typed intrinsic origin directory loses a class"));}
        b.spend(classes.len())?;
        let mut origins=classes.iter().cloned().map(|source|IntrinsicOrigin::DerivedClass{source}).collect::<Vec<_>>();
        let mut nominal=vec![false;classes.len()];
        let mut pending=vec![(ty,Vec::<LifeTypeProjection>::new())];
        while let Some((current,path))=pending.pop(){
            b.spend(path.len().saturating_add(1))?;
            let storage=match current{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
            if let Type::Closure(id)=storage{
                let Some(child)=self.typed_closure_origins(module,*id,b)?else{continue;};
                let mut injective=std::collections::BTreeSet::new();
                for(class,origin)in child.into_iter().enumerate(){
                    let binder=if path.is_empty(){class as u32}else{
                        b.spend(life.retained.len().saturating_mul(path.len().saturating_add(1)))?;
                        let mut rows=life.retained.iter().filter(|row|row.path==path&&row.class==class as u32);
                        let row=rows.next().ok_or_else(||bad("Closure child loses its complete retained class occurrence"))?;
                        if rows.next().is_some(){return Err(bad("Closure child repeats one retained class occurrence"));}row.binder
                    };
                    b.spend(injective.len().saturating_add(life.slots.len()).saturating_add(life.nominal_arguments.len()))?;
                    if !injective.insert(binder){return Err(bad("Closure child coalesces independent intrinsic indices"));}
                    let index=binder as usize;if index>=origins.len(){return Err(bad("Closure child maps outside the complete parent domain"));}
                    if nominal[index]&&origins[index]!=origin{return Err(bad("Closure child changes another independently scoped intrinsic origin"));}
                    let physical=path.iter().map(|step|if let LifeTypeProjection::Place(field)=step{Some(*field)}else{None}).collect::<Option<Vec<_>>>();
                    if life.nominal_arguments.iter().any(|row|row.binder==binder)||!path.is_empty()&&life.slots.iter().any(|row|row.binder==binder&&physical.as_ref().is_none_or(|prefix|!row.path.starts_with(prefix))){return Err(bad("outer or nominal role coalesces a Closure child intrinsic class"));}
                    nominal[index]=true;origins[index]=origin;
                }
                continue;
            }
            if let Type::Dyn{interface}|Type::OwnedDyn{interface}=storage{
                b.spend(module.dyn_interfaces.len())?;
                let mut interfaces=module.dyn_interfaces.iter().filter(|row|row.id==*interface);
                let interface=interfaces.next().ok_or_else(||bad("erased origin has no actual interface identity"))?;
                if interfaces.next().is_some(){return Err(bad("erased origin has ambiguous complete Context identities"));}
                let Some(context)=&interface.context else{
                    if interface.retained_life.binder_count!=0{return Err(bad("erased origin has an unknown nonempty original Context"));}
                    continue;
                };
                let context_life=crate::tasks::frame_shape::declared_life(module,context,b)?;
                if context_life.binder_count!=interface.retained_life.binder_count||context_life.retained!=interface.retained_life.retained||context_life.nominal_arguments!=interface.retained_life.nominal_arguments{return Err(bad("erased origin Context differs from its complete retained/nominal directory"));}
                let mut map=Vec::new();let mut images=Vec::new();
                for class in 0..context_life.binder_count{
                    let binder=if path.is_empty(){class}else{
                        b.spend(life.retained.len().saturating_mul(path.len().saturating_add(1)))?;
                        let mut rows=life.retained.iter().filter(|row|row.path==path&&row.class==class);
                        let row=rows.next().ok_or_else(||bad("erased child has no complete enclosing intrinsic map"))?;
                        if rows.next().is_some(){return Err(bad("erased child repeats an enclosing intrinsic occurrence"));}row.binder
                    };
                    if map.contains(&binder){return Err(bad("erased child coalesces independent Context indices"));}
                    images.push(classes.get(binder as usize).cloned().ok_or_else(||bad("erased Context class escapes its complete enclosing domain"))?);map.push(binder);
                }
                let child=self.typed_origins_at(module,context,&context_life,&images,active,depth+1,b)?;
                for(binder,origin)in map.into_iter().zip(child){
                    let binder=binder as usize;
                    if nominal[binder]&&origins[binder]!=origin{return Err(bad("erased child changes an independently scoped nominal Context origin"));}
                    let physical=path.iter().map(|step|if let LifeTypeProjection::Place(field)=step{Some(*field)}else{None}).collect::<Option<Vec<_>>>();
                    if !path.is_empty()&&life.slots.iter().any(|row|row.binder as usize==binder&&physical.as_ref().is_none_or(|prefix|!row.path.starts_with(prefix))){return Err(bad("outer borrow coalesces an erased Context intrinsic class"));}
                    nominal[binder]=true;origins[binder]=origin;
                }
                continue;
            }
            if matches!(storage,Type::Record{..}|Type::Package{..}|Type::Enum{..}){
                let child_life=match current{Type::BorrowedStorage{life,..}=>life.clone(),_=>empty_life()};
                if child_life.binder_count==0{continue;}
                let mut map=Vec::new();let mut images=Vec::new();
                for class in 0..child_life.binder_count{
                    let binder=if path.is_empty(){class}else{let mut rows=life.retained.iter().filter(|row|row.path==path&&row.class==class);let row=rows.next().ok_or_else(||bad("nominal child has no complete parent intrinsic map"))?;if rows.next().is_some(){return Err(bad("nominal child intrinsic map is duplicated"));}row.binder};
                    images.push(classes.get(binder as usize).cloned().ok_or_else(||bad("nominal child maps outside its complete parent domain"))?);map.push(binder);
                }
                let child=self.typed_nominal_origins(module,current,&child_life,&images,active,depth+1,b)?;
                for(binder,origin)in map.into_iter().zip(child){
                    let binder=binder as usize;
                    if nominal[binder]&&origins[binder]!=origin{return Err(bad("typed parent coalesces independent nominal intrinsic domains"));}
                    let physical=path.iter().map(|step|if let LifeTypeProjection::Place(field)=step{Some(*field)}else{None}).collect::<Option<Vec<_>>>();
                    if !path.is_empty()&&life.slots.iter().any(|row|row.binder as usize==binder&&physical.as_ref().is_none_or(|prefix|!row.path.starts_with(prefix))){return Err(bad("outer borrow coalesces a separate nominal intrinsic class"));}
                    nominal[binder]=true;origins[binder]=origin;
                }
                continue;
            }
            match storage{
                Type::Address{inner,..}|Type::Desc(inner)|Type::Managed(inner)=>{let mut p=path;p.push(LifeTypeProjection::Pointee);pending.push((inner.as_ref(),p));},
                Type::Tuple(fields)=>for(index,field)in fields.iter().enumerate(){let mut p=path.clone();p.push(LifeTypeProjection::Place(PlaceProjection::Field(index as u32)));pending.push((field,p));},
                Type::Array{element,..}|Type::OwnedStore{element}=>{let mut p=path;p.push(LifeTypeProjection::Element);pending.push((element.as_ref(),p));},
                Type::ErrorUnion{error,result}=>{let mut p=path.clone();p.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(true)));pending.push((error.as_ref(),p));let mut p=path;p.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));pending.push((result.as_ref(),p));},
                _=>{},
            }
        }
        Ok(origins)
        })();active.pop();result
    }

    /// Raw class images never choose a closure's κ or source frame. Only the
    /// complete independently sealed constructor domain may supply these
    /// origins; conflicting actual frames remain an error in closure_domain.
    fn typed_closure_origins(&self,module:&TypedModule,id:ClosureId,b:&mut Budget)->Check<Option<Vec<IntrinsicOrigin>>>{
        let Some(domain)=self.closure_domain(id,b)?else{return Ok(None);};
        domain.charge_clone(b)?;
        b.spend(module.closures.len())?;
        let closure=crate::closures::definition(module,id,None)?;
        let header=closure.storage_life.as_ref().ok_or_else(||bad("Closure origin has no complete actual header"))?;
        if domain.ty!=Type::Closure(id)||domain.life.binder_count!=header.fixed_binder_count||!domain.life.slots.is_empty()||!domain.life.retained.is_empty()||!domain.life.nominal_arguments.is_empty()||domain.classes.len()!=header.fixed_binder_count as usize||domain.origins.len()!=domain.classes.len(){return Err(bad("Closure origin loses its independently sealed whole domain"));}
        b.spend(domain.origins.len())?;let mut frame=None;
        for(class,origin)in domain.origins.iter().enumerate(){
            let IntrinsicOrigin::Existing{scope:ScopeKey::Instance(instance),operand:OperandKey::Receiver,path,class:ordinal}=origin else{return Err(bad("Closure origin does not select its exact sealed Receiver frame"));};
            if !path.is_empty()||*ordinal as usize!=class||frame.is_some_and(|known|known!=*instance){return Err(bad("Closure origin permutes classes or combines independent source frames"));}frame=Some(*instance);
        }
        if let Some(frame)=frame{
            let instance=self.instance(module,frame,b)?;
            if !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==closure.function)||instance.environment.as_ref().is_none_or(|environment|environment.closure!=SemanticClosureTarget::Typed(id))||self.definition(module,instance.definition,b)?.kind!=SemanticDefinitionKind::Closure{return Err(bad("Closure origin substitutes another actual source entry or environment"));}
            if super::closures::intrinsic_scope(module,closure,instance.definition,b)?!=ScopeKey::Instance(frame){return Err(bad("Closure origin chooses another intrinsic directory for the same actual entry"));}
        }
        Ok(Some(domain.origins))
    }

    fn typed_nominal_origins(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[ClassKey],active:&mut Vec<Type>,depth:usize,b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        b.spend(module.life_instances.len())?;
        let mut rows=module.life_instances.iter().filter(|row|row.ty==*ty);
        let instance=rows.next().ok_or_else(||bad("nominal origin has no complete actual declaration/arguments"))?;
        if rows.next().is_some(){return Err(bad("nominal origin has ambiguous complete actual arguments"));}
        dynamic_metadata::ty(&instance.ty,b)?;b.spend(instance.arguments.len().saturating_add(1))?;
        for actual in &instance.arguments{match actual{LifeNominalActual::Type(ty)=>dynamic_metadata::ty(ty,b)?,LifeNominalActual::QualifiedType{ty,life}=>{dynamic_metadata::ty(ty,b)?;dynamic_metadata::life_shape(life,b)?;},LifeNominalActual::Const(value)=>dynamic_metadata::constant(value,b)?,LifeNominalActual::Lifetime(_)=>{}}}
        let mut actuals=Vec::new();let mut sources=vec![None;classes.len()];
        for(ordinal,actual)in instance.arguments.iter().enumerate(){match actual{
            LifeNominalActual::Lifetime(class)=>{let key=classes.get(*class as usize).cloned().ok_or_else(||bad("nominal Life actual escapes its complete intrinsic domain"))?;sources[*class as usize]=Some(IntrinsicOrigin::DerivedClass{source:key.clone()});actuals.push(CheckedActual::Life(key));},
            LifeNominalActual::Const(value)=>actuals.push(CheckedActual::Const{carrier:CheckedType{type_cell: crate::semantic::CheckedTypeData::new(value.ty()),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]},value:value.clone()}),
            LifeNominalActual::Type(child)|LifeNominalActual::QualifiedType{ty:child,..}=>{
                let child_life=match actual{LifeNominalActual::QualifiedType{life,..}=>life.clone(),LifeNominalActual::Type(Type::BorrowedStorage{life,..})=>life.clone(),_=>empty_life()};
                let mut images=Vec::new();let mut binders=Vec::new();
                for class in 0..child_life.binder_count{let mut rows=life.retained.iter().filter(|row|row.path==[LifeTypeProjection::TypeArgument(ordinal as u32)]&&row.class==class);let row=rows.next().ok_or_else(||bad("nominal Type actual lacks its full intrinsic occurrence map"))?;if rows.next().is_some(){return Err(bad("nominal Type actual intrinsic occurrence repeats"));}images.push(classes.get(row.binder as usize).cloned().ok_or_else(||bad("nominal Type actual maps outside its parent"))?);binders.push(row.binder);}
                let origins=self.typed_origins_at(module,child,&child_life,&images,active,depth+1,b)?;
                for(binder,origin)in binders.into_iter().zip(&origins){if let Some(old)=&sources[binder as usize]{if old!=origin{return Err(bad("nominal actuals coalesce independent intrinsic origins"));}}sources[binder as usize]=Some(origin.clone());}
                actuals.push(CheckedActual::Type(CheckedType{type_cell: crate::semantic::CheckedTypeData::new(child.clone()),life:child_life,classes:images,origins,statics:vec![]}));
            },
        }}
        // Class sources arising only in physical fields still have their
        // original nominal domain; the complete typed shape was checked by
        // the independent inventory before this constructor is called.
        let sources=sources.into_iter().zip(classes).map(|(origin,key)|origin.unwrap_or_else(||IntrinsicOrigin::DerivedClass{source:key.clone()})).collect::<Vec<_>>();
        self.nominal_origins(instance.declaration,&actuals,&[],life,&sources,b)
    }
}

#[cfg(test)]
mod closure_child_tests{
    use super::*;
    #[test]
    fn nested_closure_origins_use_the_sealed_frame_even_when_outer_images_agree(){
        let limits=Limits::default();let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/local-closure-complete-head.wire"),&limits).unwrap();let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let programme=module.semantic.as_ref().unwrap();let child=programme.instances.iter().find(|row|row.environment.is_some()).unwrap();let parent=programme.instances.iter().find(|row|Some(row.id)==child.parent).unwrap();
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=parent.target else{panic!()};let id=match child.environment.as_ref().unwrap().closure{SemanticClosureTarget::Typed(id)=>id,_=>panic!()};let count=module.closures[id.0 as usize].storage_life.as_ref().unwrap().fixed_binder_count;assert!(count>0);
        let scope=ScopeKey::Instance(parent.id);let image=ClassKey::Implicit{scope,ordinal:0};let origins=(0..count).map(|class|IntrinsicOrigin::Existing{scope:ScopeKey::Instance(child.id),operand:OperandKey::Receiver,path:vec![],class}).collect::<Vec<_>>();
        // A scoped type/origin kernel control. The private constructor receipt
        // is supplied here; this does not admit an SSA creator or actual body.
        let value=CheckedType{type_cell:CheckedTypeData::new(Type::Closure(id)),life:StorageLifeShape{binder_count:count,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![image.clone();count as usize],origins:origins.clone(),statics:vec![]};
        context.closure_boundaries.borrow_mut().push(ClosureBoundary{creator:scope,node:zil_base::NodeId(0),at:SitePoint{function,block:BlockId(0),instruction:0,after:true},child:child.id,value});
        let ty=Type::Address{mutable:true,inner:Box::new(Type::Closure(id))};let life=StorageLifeShape{binder_count:count+1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::ExclusiveAddress}],retained:(0..count).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class,binder:class+1}).collect(),nominal_arguments:vec![]};let images=vec![image.clone();(count+1)as usize];
        let checked=context.checked_value(&module,&ty,&life,images,&mut b).unwrap();assert_eq!(&checked.origins[1..],origins.as_slice());assert_ne!(checked.origins[0],checked.origins[1]);assert!(checked.classes.iter().all(|class|class==&image));
        let mut collapsed=life.clone();collapsed.retained[0].binder=0;assert!(context.typed_origins(&module,&ty,&collapsed,&checked.classes,&mut b).is_err(),"same outer image cannot merge the retained Closure κ");
        let mut repeated=life.clone();repeated.retained.push(repeated.retained[0].clone());assert!(context.typed_origins(&module,&ty,&repeated,&checked.classes,&mut b).is_err());
        context.closure_boundaries.borrow_mut()[0].value.origins[0]=IntrinsicOrigin::Existing{scope,operand:OperandKey::Receiver,path:vec![],class:0};assert!(context.typed_origins(&module,&ty,&life,&checked.classes,&mut b).is_err(),"the same ClosureId does not authorize another source frame");
    }
}

pub(crate) fn charge_origin(origin:&IntrinsicOrigin,b:&mut Budget)->Check<()> {charge_graph(Work::Origin(origin,0),b)}
fn charge_actual(actual:&CheckedActual,b:&mut Budget)->Check<()> {charge_graph(Work::Actual(actual,0),b)}
enum Work<'a>{Origin(&'a IntrinsicOrigin,usize),Actual(&'a CheckedActual,usize)}
fn charge_graph(root:Work<'_>,b:&mut Budget)->Check<()>{
    let mut pending=vec![root];
    while let Some(work)=pending.pop(){b.spend(1)?;match work{
        Work::Origin(origin,depth)=>{
            if depth>b.limits.max_call_depth.min(128){return Err(limit("complete nominal origin nesting exceeded"));}
            match origin{
                IntrinsicOrigin::Existing{path,..}=>b.spend(path.len())?,
                IntrinsicOrigin::DerivedClass{source}=>charge_key(source,b)?,
                IntrinsicOrigin::Nominal{source,..}=>{
                    dynamic_metadata::life_shape(&source.domain,b)?;
                    b.spend(source.actuals.len().saturating_add(source.environment.len()).saturating_add(source.class_sources.len()))?;
                    pending.extend(source.actuals.iter().chain(&source.environment).map(|actual|Work::Actual(actual,depth+1)));
                    pending.extend(source.class_sources.iter().map(|origin|Work::Origin(origin,depth+1)));
                },
            }
        },
        Work::Actual(actual,depth)=>{
            if depth>b.limits.max_call_depth.min(128){return Err(limit("nominal full actual origin nesting exceeded"));}
            match actual{
                CheckedActual::Life(key)=>charge_key(key,b)?,
                CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>{
                    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
                    b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
                    for key in &value.classes{charge_key(key,b)?;}
                    pending.extend(value.origins.iter().map(|origin|Work::Origin(origin,depth+1)));
                    for row in &value.statics{match row{StaticOccurrence::Direct{path,leaf,..}=>{b.spend(path.len())?;dynamic_metadata::ty(leaf,b)?;},StaticOccurrence::Logical{position,leaf,..}=>{dynamic_metadata::position(position,b)?;dynamic_metadata::ty(leaf,b)?;}}}
                    if let CheckedActual::Const{value,..}=actual{dynamic_metadata::constant(value,b)?;}
                },
            }
        },
    }}Ok(())
}
fn charge_key(key:&ClassKey,b:&mut Budget)->Check<()>{b.spend(1)?;match key{ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=>b.spend(path.len()),_=>Ok(())}}
