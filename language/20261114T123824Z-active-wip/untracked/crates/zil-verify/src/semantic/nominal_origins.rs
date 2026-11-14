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
        dynamic_metadata::recipe(&source.domain,b)?;
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
        if x!=y||a.declaration!=z.declaration||a.domain!=z.domain||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Err(bad("nominal claim changes its complete original identity/domain/arguments"));}
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
        dynamic_metadata::recipe(domain,b)?;
        let source=Rc::new(NominalOrigin{declaration,actuals:actuals.to_vec(),environment:sealed.to_vec(),domain:domain.clone(),class_sources:sources.to_vec()});
        b.spend(sources.len())?;
        Ok((0..domain.binder_count).map(|class|IntrinsicOrigin::Nominal{source:source.clone(),class}).collect())
    }

    /// Rebuild each root-domain origin from complete typed occurrence maps.
    /// Nominal children are boundaries; their intrinsic indices are never
    /// collapsed into the outer borrow or another child with equal images.
    pub(super) fn typed_origins(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[ClassKey],b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        dynamic_metadata::ty(ty,b)?;dynamic_metadata::recipe(life,b)?;
        if classes.len()!=life.binder_count as usize{return Err(bad("typed intrinsic origin directory loses a class"));}
        b.spend(classes.len())?;
        let mut origins=classes.iter().cloned().map(|source|IntrinsicOrigin::DerivedClass{source}).collect::<Vec<_>>();
        let mut nominal=vec![false;classes.len()];
        let mut pending=vec![(ty,Vec::<LifeTypeProjection>::new())];
        while let Some((current,path))=pending.pop(){
            b.spend(path.len().saturating_add(1))?;
            let storage=match current{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
            if matches!(storage,Type::Record{..}|Type::Package{..}|Type::Enum{..}){
                let child_life=match current{Type::BorrowedStorage{life,..}=>life.clone(),_=>empty_life()};
                if child_life.binder_count==0{continue;}
                let mut map=Vec::new();let mut images=Vec::new();
                for class in 0..child_life.binder_count{
                    let binder=if path.is_empty(){class}else{let mut rows=life.retained.iter().filter(|row|row.path==path&&row.class==class);let row=rows.next().ok_or_else(||bad("nominal child has no complete parent intrinsic map"))?;if rows.next().is_some(){return Err(bad("nominal child intrinsic map is duplicated"));}row.binder};
                    images.push(classes.get(binder as usize).cloned().ok_or_else(||bad("nominal child maps outside its complete parent domain"))?);map.push(binder);
                }
                let child=self.typed_nominal_origins(module,current,&child_life,&images,b)?;
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
    }

    fn typed_nominal_origins(&self,module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[ClassKey],b:&mut Budget)->Check<Vec<IntrinsicOrigin>>{
        b.spend(module.life_instances.len())?;
        let mut rows=module.life_instances.iter().filter(|row|row.ty==*ty);
        let instance=rows.next().ok_or_else(||bad("nominal origin has no complete actual declaration/arguments"))?;
        if rows.next().is_some(){return Err(bad("nominal origin has ambiguous complete actual arguments"));}
        dynamic_metadata::recipe(instance,b)?;
        let mut actuals=Vec::new();let mut sources=vec![None;classes.len()];
        for(ordinal,actual)in instance.arguments.iter().enumerate(){match actual{
            LifeNominalActual::Lifetime(class)=>{let key=classes.get(*class as usize).cloned().ok_or_else(||bad("nominal Life actual escapes its complete intrinsic domain"))?;sources[*class as usize]=Some(IntrinsicOrigin::DerivedClass{source:key.clone()});actuals.push(CheckedActual::Life(key));},
            LifeNominalActual::Const(value)=>actuals.push(CheckedActual::Const{carrier:CheckedType{ty:value.ty(),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]},value:value.clone()}),
            LifeNominalActual::Type(child)|LifeNominalActual::QualifiedType{ty:child,..}=>{
                let child_life=match actual{LifeNominalActual::QualifiedType{life,..}=>life.clone(),LifeNominalActual::Type(Type::BorrowedStorage{life,..})=>life.clone(),_=>empty_life()};
                let mut images=Vec::new();let mut binders=Vec::new();
                for class in 0..child_life.binder_count{let mut rows=life.retained.iter().filter(|row|row.path==[LifeTypeProjection::TypeArgument(ordinal as u32)]&&row.class==class);let row=rows.next().ok_or_else(||bad("nominal Type actual lacks its full intrinsic occurrence map"))?;if rows.next().is_some(){return Err(bad("nominal Type actual intrinsic occurrence repeats"));}images.push(classes.get(row.binder as usize).cloned().ok_or_else(||bad("nominal Type actual maps outside its parent"))?);binders.push(row.binder);}
                let origins=self.typed_origins(module,child,&child_life,&images,b)?;
                for(binder,origin)in binders.into_iter().zip(&origins){if let Some(old)=&sources[binder as usize]{if old!=origin{return Err(bad("nominal actuals coalesce independent intrinsic origins"));}}sources[binder as usize]=Some(origin.clone());}
                actuals.push(CheckedActual::Type(CheckedType{ty:child.clone(),life:child_life,classes:images,origins,statics:vec![]}));
            },
        }}
        // Class sources arising only in physical fields still have their
        // original nominal domain; the complete typed shape was checked by
        // the independent inventory before this constructor is called.
        let sources=sources.into_iter().zip(classes).map(|(origin,key)|origin.unwrap_or_else(||IntrinsicOrigin::DerivedClass{source:key.clone()})).collect::<Vec<_>>();
        self.nominal_origins(instance.declaration,&actuals,&[],life,&sources,b)
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
                    dynamic_metadata::recipe(&source.domain,b)?;
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
                    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::recipe(&value.life,b)?;
                    b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
                    for key in &value.classes{charge_key(key,b)?;}
                    pending.extend(value.origins.iter().map(|origin|Work::Origin(origin,depth+1)));
                    for row in &value.statics{match row{StaticOccurrence::Direct{path,leaf,..}=>{b.spend(path.len())?;dynamic_metadata::ty(leaf,b)?;},StaticOccurrence::Logical{position,leaf,..}=>{dynamic_metadata::recipe(position,b)?;dynamic_metadata::ty(leaf,b)?;}}}
                    if let CheckedActual::Const{value,..}=actual{dynamic_metadata::recipe(value,b)?;}
                },
            }
        },
    }}Ok(())
}
fn charge_key(key:&ClassKey,b:&mut Budget)->Check<()>{b.spend(1)?;match key{ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=>b.spend(path.len()),_=>Ok(())}}
