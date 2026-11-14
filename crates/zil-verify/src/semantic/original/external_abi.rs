//! Complete nominal C ABI correspondence. A same-layout record/name is never
//! a declaration identity, and actual ABI data never solves original formals.
use super::*;
impl CheckedOriginal {
    pub(super) fn external_nominal(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,arguments:&[Arg],actual:&Type,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let definition=context.definition(module,id,b)?;
        let SemanticDefinitionBody::Nominal{declaration,kind,body}=&definition.body else{return Err(bad("external nominal selects another original definition kind"));};
        if arguments.len()!=definition.parameters.len(){return Err(bad("external nominal loses full mixed actual order"));}
        self.scheme_term(module,context,&Ty::Nominal(id,arguments.to_vec()),caller,active,b)?;
        b.spend(module.life_declarations.len())?;
        let mut declarations=module.life_declarations.iter().filter(|row|row.id==*declaration);
        let source=declarations.next().ok_or_else(||unsupported("external nominal has no complete original canonical declaration",None))?;
        if declarations.next().is_some()||source.identity!=definition.identity||source.kind!=*kind||source.parameters.len()!=definition.parameters.len(){return Err(bad("external nominal canonical declaration contradicts its original identity"));}
        for((formal,recorded),argument)in definition.parameters.iter().zip(&source.parameters).zip(arguments){
            match(formal,recorded,argument){
                (SemanticFormal::Type{..},LifeParameterKind::Type,Arg::Type(_))=>{},
                (SemanticFormal::Lifetime,LifeParameterKind::Lifetime,Arg::Life(_))=>{},
                (SemanticFormal::Const{..},LifeParameterKind::Const(carrier),Arg::Const(value))=>{if !self.core_abi_type(module,context,&value.carrier,carrier,caller,active,b)?{return Err(bad("external nominal Const declaration changes its original carrier"));}},
                _=>return Err(bad("external nominal changes a complete original mixed formal kind")),
            }
        }
        // These C record lanes are completely class-free. Nonzero/unknown
        // nominal domains keep their qualification and need a separate full
        // occurrence relation; no empty-slot or physical-layout inference.
        let count=crate::dynamic_witness::intrinsic_domain(module,actual,b)?.0;
        if count!=0{return Err(unsupported("external nominal nonzero domain requires its full original ABI occurrence relation",None));}
        let storage=match actual{Type::BorrowedStorage{storage,life}=>{if *life!=empty_life(){return Err(bad("external nominal Known0 has nonempty qualified metadata"));}storage.as_ref()},other=>other};
        b.spend(module.life_instances.len())?;
        let mut matching=false;
        for instance in &module.life_instances {
            dynamic_metadata::ty(&instance.ty,b)?;
            if instance.ty!=*actual{continue;}
            if instance.declaration!=*declaration||instance.arguments.len()!=arguments.len(){return Err(bad("external nominal actual type has another original declaration or full actual list"));}
            let mut same=true;
            for(original,recorded)in arguments.iter().zip(&instance.arguments){
                same&=match(original,recorded){
                    (Arg::Type(ty),LifeNominalActual::Type(actual))=>self.core_abi_type(module,context,ty,actual,caller,active,b)?,
                    (Arg::Type(ty),LifeNominalActual::QualifiedType{ty:actual,life})=>*life==empty_life()&&self.core_abi_type(module,context,ty,actual,caller,active,b)?,
                    (Arg::Const(value),LifeNominalActual::Const(actual))=>self.external_constant(module,value,b)?==*actual,
                    // A lifetime actual cannot disappear from a Known0 domain.
                    (Arg::Life(_),LifeNominalActual::Lifetime(_))=>return Err(bad("external nominal erases an original persistent lifetime actual")),
                    _=>false,
                };
            }
            if !same{return Err(bad("external nominal actual specialization differs from its original symbolic application"));}
            if matching{return Err(bad("external nominal has duplicate actual canonical identity rows"));}matching=true;
        }
        if !matching{return Err(unsupported("external nominal actual type has no exact original canonical specialization",None));}
        let mut environment=self.environment(module,context,id,active,b)?;
        b.spend(arguments.len())?;environment.lexical[0].actuals=arguments.to_vec();
        Ok(match(body,storage,kind){
            (SemanticNominalBody::Product{fields},Type::Record{fields:actual,..},LifeNominalKind::Record)
            |(SemanticNominalBody::Product{fields},Type::Package{fields:actual,..},LifeNominalKind::Package)=>{
                if fields.len()!=actual.len(){return Ok(false);}
                for(field,(name,actual))in fields.iter().zip(actual){b.spend(field.name.len().saturating_add(name.len()))?;if field.name!=*name{return Ok(false);}let expected=self.ty(module,context,&field.ty,&environment,active,b)?;if !self.core_abi_type(module,context,&expected,actual,&environment,active,b)?{return Ok(false);}}
                true
            },
            (SemanticNominalBody::Enum{variants},Type::Enum{variants:actual,..},LifeNominalKind::Enum)=>{
                if variants.len()!=actual.len(){return Ok(false);}
                for(variant,(name,fields))in variants.iter().zip(actual){b.spend(variant.name.len().saturating_add(name.len()))?;if variant.name!=*name||variant.fields.len()!=fields.len(){return Ok(false);}for(field,actual)in variant.fields.iter().zip(fields){let expected=self.ty(module,context,field,&environment,active,b)?;if !self.core_abi_type(module,context,&expected,actual,&environment,active,b)?{return Ok(false);}}}true
            },
            _=>false,
        })
    }
    pub(super) fn external_array_length(&self,module:&TypedModule,value:&Const,b:&mut Budget)->Check<u64>{
        match self.external_constant(module,value,b)?{Constant::Usize(value)=>Ok(value),_=>Err(bad("external array original length has another complete carrier"))}
    }
    fn external_constant(&self,module:&TypedModule,value:&Const,b:&mut Budget)->Check<Constant>{
        fn pattern(checker:&CheckedOriginal,value:&Const,b:&mut Budget,depth:usize)->Check<DynConstPattern>{
            b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("external original constant depth exceeded"));}
            let Ty::Atom(carrier)=checker.resolved(&value.carrier,b)?else{return Err(unsupported("external original Const carrier remains abstract",None));};
            dynamic_metadata::ty(&carrier,b)?;
            Ok(match &value.value{
                ConstValue::Value(value)=>{dynamic_metadata::constant(value,b)?;DynConstPattern::Value(value.clone())},
                ConstValue::Variable(..)=>return Err(unsupported("external original Const value remains universally abstract",None)),
                ConstValue::Unary(op,child)=>DynConstPattern::Unary{op:*op,ty:Box::new(DynTypePattern::Concrete(carrier)),value:Box::new(pattern(checker,child,b,depth+1)?)},
                ConstValue::Binary(op,left,right)=>DynConstPattern::Binary{op:*op,ty:Box::new(DynTypePattern::Concrete(carrier)),left:Box::new(pattern(checker,left,b,depth+1)?),right:Box::new(pattern(checker,right,b,depth+1)?)},
                ConstValue::Convert(child)=>DynConstPattern::Convert{ty:Box::new(DynTypePattern::Concrete(carrier)),value:Box::new(pattern(checker,child,b,depth+1)?)},
            })
        }
        let Ty::Atom(carrier)=self.resolved(&value.carrier,b)?else{return Err(unsupported("external original Const has no concrete full carrier",None));};
        scalar::closed(&pattern(self,value,b,0)?,&carrier,module.target.pointer_width,b)?.ok_or_else(||bad("external original Const value is not independently closed"))
    }
}
