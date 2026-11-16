//! Original dynamic types retain the full trait application and erased Context.
//! No runtime table or current specialization determines this symbolic term.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynType {pub(super) owned:bool,pub(super) application:DynApplication,pub(super) context:DynContext,pub(super) associated:Vec<DynAssociation>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynApplication {pub(super) declaration:LifeDeclarationId,pub(super) arguments:Vec<Arg>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum DynContext {Closed,Type(Box<Ty>),Shape(Box<Retained>)}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct DynAssociation {pub(super) application:DynApplication,pub(super) member:u32,pub(super) value:Arg}
#[derive(Default)]
struct Dependencies {classes:Vec<Life>,ports:Vec<Ty>,whole:Vec<(Ty,bool)>}
impl DynType {
    pub(super) fn explicit_life(&self,b:&mut Budget)->Check<bool>{
        let mut arguments=self.application.arguments.iter().collect::<Vec<_>>();for row in &self.associated{arguments.extend(&row.application.arguments);arguments.push(&row.value);}b.spend(arguments.len())?;if arguments.iter().any(|arg|matches!(arg,Arg::Life(_))){return Ok(true);}
        if let DynContext::Shape(shape)=&self.context{b.spend(shape.domain.len().saturating_add(shape.ports.len()).saturating_add(shape.retained.len()).saturating_add(shape.nominal.len()))?;if shape.domain.iter().any(|row|matches!(row,ClassPart::One(_)))||shape.ports.iter().any(|row|matches!(row,PortPart::One{..}))||shape.retained.iter().any(|row|matches!(row,RetainedPart::One{..})||matches!(row,RetainedPart::All{parents,..}if parents.iter().any(|row|matches!(row,ClassPart::One(_)))))||shape.nominal.iter().any(|row|matches!(row,NominalPart::One{..})){return Ok(true);}}
        Ok(false)
    }
    pub(super) fn map(&self,mut f:impl FnMut(&Arg,&mut Budget)->Check<Arg>,b:&mut Budget)->Check<Self>{
        fn app(value:&DynApplication,f:&mut impl FnMut(&Arg,&mut Budget)->Check<Arg>,b:&mut Budget)->Check<DynApplication>{let mut arguments=Vec::new();for arg in &value.arguments{b.spend(1)?;arguments.push(f(arg,b)?);}Ok(DynApplication{declaration:value.declaration,arguments})}
        let application=app(&self.application,&mut f,b)?;
        let context=match &self.context{
            DynContext::Closed=>DynContext::Closed,
            DynContext::Type(ty)=>match f(&Arg::Type((**ty).clone()),b)?{Arg::Type(ty)=>DynContext::Type(Box::new(ty)),_=>return Err(bad("dynamic Context substitution changes Type kind"))},
            DynContext::Shape(shape)=>{
                let mut shape=shape.map(|ty,b|match f(&Arg::Type(ty.clone()),b)?{Arg::Type(ty)=>Ok(ty),_=>Err(bad("dynamic Context child changes Type kind"))},|life,b|{b.spend(1)?;Ok(life.clone())},b)?;
                shape=shape.map(|ty,b|{b.spend(1)?;Ok(ty.clone())},|life,b|match f(&Arg::Life(life.clone()),b)?{Arg::Life(life)=>Ok(life),_=>Err(bad("dynamic Context class changes Lifetime kind"))},b)?;
                DynContext::Shape(Box::new(shape))
            },
        };
        let mut associated=Vec::new();for row in &self.associated{b.spend(1)?;associated.push(DynAssociation{application:app(&row.application,&mut f,b)?,member:row.member,value:f(&row.value,b)?});}
        Ok(Self{owned:self.owned,application,context,associated})
    }
    pub(super) fn types<'a>(&'a self,output:&mut Vec<&'a Ty>,b:&mut Budget)->Check<()>{
        fn arg<'a>(value:&'a Arg,output:&mut Vec<&'a Ty>){match value{Arg::Type(ty)=>output.push(ty),Arg::Const(value)=>output.push(&value.carrier),Arg::Life(_)=>{}}}
        b.spend(self.application.arguments.len().saturating_add(self.associated.len()))?;
        for value in &self.application.arguments{arg(value,output);}
        match &self.context{DynContext::Closed=>{},DynContext::Type(ty)=>output.push(ty),DynContext::Shape(shape)=>shape.types(output,b)?,}
        for row in &self.associated{b.spend(row.application.arguments.len())?;for value in &row.application.arguments{arg(value,output);}arg(&row.value,output);}
        Ok(())
    }
}
impl CheckedOriginal {
    pub(super) fn dyn_type(&mut self,module:&TypedModule,context:&CheckedContext,owned:bool,recipe:&SemanticDynInterfaceRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<DynType>{
        let application=self.dyn_application(module,context,&recipe.application,env,active,b)?;
        let retained=match &recipe.context{SemanticDynContextRecipe::Closed=>DynContext::Closed,SemanticDynContextRecipe::Type(ty)=>DynContext::Type(Box::new(self.ty(module,context,ty,env,active,b)?)),SemanticDynContextRecipe::Shape(shape)=>DynContext::Shape(Box::new(self.retained(module,context,shape,env,active,b)?))};
        let mut associated=Vec::<DynAssociation>::new();for row in &recipe.associated{
            b.spend(associated.len().saturating_add(1))?;
            let application=self.dyn_application(module,context,&row.application,env,active,b)?;
            if associated.iter().any(|old|old.application==application&&old.member==row.member){return Err(bad("original dynamic interface repeats an associated binding"));}
            let value=match &row.value{SemanticAssociatedRecipe::Type(ty)=>Arg::Type(self.ty(module,context,ty,env,active,b)?),SemanticAssociatedRecipe::Const{carrier,value}=>{let carrier=self.ty(module,context,carrier,env,active,b)?;Arg::Const(self.constant(module,context,value,&carrier,env,active,b)?)} };
            associated.push(DynAssociation{application,member:row.member,value});
        }
        associated.sort_by_key(|row|(row.application.declaration,row.member));
        Ok(DynType{owned,application,context:retained,associated})
    }
    fn dyn_application(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticTraitRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<DynApplication>{
        let scope=dynamic_witness::semantic_external::original_scope(module,recipe.declaration,b)?;
        if !matches!(scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})if declaration==recipe.declaration)||scope.parameters.len()!=recipe.arguments.len(){return Err(bad("original dynamic trait application has another identity or mixed arity"));}
        let mut arguments=Vec::new();for(formal,recipe)in scope.parameters.iter().zip(&recipe.arguments){let value=self.actual(module,context,recipe,env,active,b)?;if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original dynamic trait application changes a full-ordinal kind"));}arguments.push(value);}
        Ok(DynApplication{declaration:recipe.declaration,arguments})
    }
    pub(super) fn reify_dyn(&self,module:&TypedModule,value:&DynType,env:&EvalEnv,b:&mut Budget)->Check<SemanticDynInterfaceRecipe>{
        let application=self.reify_dyn_application(module,&value.application,env,b)?;
        let context=match &value.context{DynContext::Closed=>SemanticDynContextRecipe::Closed,DynContext::Type(ty)=>SemanticDynContextRecipe::Type(Box::new(self.reify_type(module,ty,env,b)?)),DynContext::Shape(shape)=>SemanticDynContextRecipe::Shape(Box::new(self.reify_retained(module,shape,env,b)?))};
        let mut associated=Vec::new();for row in &value.associated{let application=self.reify_dyn_application(module,&row.application,env,b)?;let value=match self.reify_argument(module,&row.value,env,b)?{SemanticActualRecipe::Type(ty)=>SemanticAssociatedRecipe::Type(ty),SemanticActualRecipe::Const{carrier,value}=>SemanticAssociatedRecipe::Const{carrier,value},_=>return Err(bad("original dynamic associated value changes member kind"))};associated.push(SemanticAssociatedBindingRecipe{application,member:row.member,value});}
        Ok(SemanticDynInterfaceRecipe{application,context,associated})
    }
    fn reify_dyn_application(&self,module:&TypedModule,value:&DynApplication,env:&EvalEnv,b:&mut Budget)->Check<SemanticTraitRecipe>{let mut arguments=Vec::new();for value in &value.arguments{arguments.push(self.reify_argument(module,value,env,b)?);}Ok(SemanticTraitRecipe{declaration:value.declaration,arguments})}
    pub(super) fn dyn_scheme(&mut self,module:&TypedModule,context:&CheckedContext,value:&DynType,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let applications=self.dyn_applications(module,context,value,caller,active,b)?;
        let mut types=Vec::new();value.types(&mut types,b)?;for ty in types{self.scheme_term(module,context,ty,caller,active,b)?;}
        for binding in &value.associated{
            let mut valid=false;for application in &applications{if application.declaration==binding.application.declaration&&self.same_arguments_readonly(&application.arguments,&binding.application.arguments,b)?{valid=true;}}
            if !valid{return Err(bad("dynamic associated binding selects no original interface trait"));}
            b.spend(module.dyn_declarations.len())?;
            let declaration=module.dyn_declarations.iter().find(|row|row.id==binding.application.declaration).ok_or_else(||bad("dynamic associated declaration is absent"))?;
            if !matches!((declaration.members.get(binding.member as usize),&binding.value),(Some(DynMemberDeclaration{kind:DynMemberKind::Type,..}),Arg::Type(_))|(Some(DynMemberDeclaration{kind:DynMemberKind::Constant,..}),Arg::Const(_))){return Err(bad("dynamic associated binding changes exact member kind"));}
            let scope=dynamic_witness::semantic_external::original_scope(module,binding.application.declaration,b)?;
            let mut selected=self.environment(module,context,scope.id,active,b)?;selected.lexical[0].actuals=binding.application.arguments.clone();let mut erased=value.clone();erased.owned=false;selected.self_type=Some((scope.id,Ty::Dyn(Box::new(erased))));
            let rows=declaration.associated_definitions.as_ref().ok_or_else(||bad("dynamic associated original bounds directory is Unknown"))?;b.spend(rows.len())?;let mut rows=rows.iter().filter(|row|row.member==binding.member);let row=rows.next().ok_or_else(||bad("dynamic associated binding omits its original bounds row"))?;if rows.next().is_some(){return Err(bad("dynamic associated original bounds row is duplicated"));}
            if let Arg::Const(value)=&binding.value{let recipe=dynamic_witness::semantic_external::original_type_recipe(module,binding.application.declaration,row.carrier.as_ref().ok_or_else(||bad("dynamic Const member omits its original carrier"))?,b)?;let carrier=self.ty(module,context,&recipe,&selected,active,b)?;self.scheme_carrier(&carrier,b)?;if !self.same(&carrier,&value.carrier,b)?{return Err(bad("dynamic associated Const binding changes declared carrier"));}}
            for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module,binding.application.declaration,&row.requirements,b)?{self.obligation(module,context,&requirement,&selected,caller,active,b)?;}

        }
        Ok(())
    }
    pub(super) fn dyn_applications(&mut self,module:&TypedModule,context:&CheckedContext,value:&DynType,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<DynApplication>>{
        let mut pending=vec![value.application.clone()];let mut checked=Vec::<DynApplication>::new();
        while let Some(application)=pending.pop(){
            b.spend(checked.len().saturating_add(1))?;let mut duplicate=false;for old in &checked{if old.declaration==application.declaration&&self.same_arguments_readonly(&old.arguments,&application.arguments,b)?{duplicate=true;break;}}if duplicate{continue;}
            if checked.len()>b.limits.max_nodes{return Err(limit("original dynamic supertrait closure exceeded"));}
            let scope=dynamic_witness::semantic_external::original_scope(module,application.declaration,b)?;
            if scope.parameters.len()!=application.arguments.len(){return Err(bad("dynamic supertrait application omits ordered mixed arguments"));}
            let mut selected=self.environment(module,context,scope.id,active,b)?;selected.lexical[0].actuals=application.arguments.clone();let mut erased=value.clone();erased.owned=false;selected.self_type=Some((scope.id,Ty::Dyn(Box::new(erased))));
            let mut requirements=scope.requirements.iter().collect::<Vec<_>>();
            while let Some(requirement)=requirements.pop(){b.spend(1)?;match requirement{
                SemanticRequirementRecipe::Trait{ty,application}=>{let ty=self.ty(module,context,ty,&selected,active,b)?;if !self.same(&ty,&selected.self_type.as_ref().unwrap().1,b)?{let target=self.dyn_application(module,context,application,&selected,active,b)?;self.original_trait_obligation(module,context,&ty,target.declaration,&target.arguments,caller,active,b)?;}else{pending.push(self.dyn_application(module,context,application,&selected,active,b)?);}},
                SemanticRequirementRecipe::ForAll{lifetimes:0,requirements:rows}=>requirements.extend(rows),
                _=>self.obligation(module,context,requirement,&selected,caller,active,b)?,
            }}
            for(formal,actual)in scope.parameters.iter().zip(&application.arguments){match(formal,actual){(SemanticFormal::Type{requirements},Arg::Type(ty))=>{self.scheme_term(module,context,ty,caller,active,b)?;for requirement in requirements{self.obligation(module,context,requirement,&selected,caller,active,b)?;}},(SemanticFormal::Lifetime,Arg::Life(_))=>{},(SemanticFormal::Const{carrier},Arg::Const(value))=>{let carrier=self.ty(module,context,carrier,&selected,active,b)?;if !self.same(&carrier,&value.carrier,b)?{return Err(bad("dynamic trait Const actual changes declared carrier"));}},_=>return Err(bad("dynamic trait actual changes original mixed kind"))}}
            checked.push(application);
        }
        Ok(checked)
    }
}
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn owned_dyn(&mut self,module:&TypedModule,context:&CheckedContext,site:SemanticSiteId,written:&[Arg],arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let (target,envelope)=match written{
            [Arg::Type(Ty::Dyn(interface))]=>(interface,None),
            [Arg::Type(Ty::Dyn(interface)),Arg::Type(envelope)]=>(interface,Some(envelope)),
            _=>return Err(bad("owned_dyn requires its original interface Type and optional whole Context Type")),
        };
        if !matches!(target.context,DynContext::Closed){return Err(bad("owned_dyn writes competing original Context operands"));}
        let mut interface=(**target).clone();
        if let Some(envelope)=envelope{interface.context=DynContext::Type(Box::new(self.resolved(envelope,b)?));}
        if interface.owned||arguments.len()!=1{return Err(bad("owned_dyn interface/value scheme arity differs"));}
        self.execute_effect(env,CallKind::Direct,b)?;
        self.dyn_scheme(module,context,&interface,env,active,b)?;
        let source=self.expression(module,context,&arguments[0],env,bindings,None,active,b)?;
        if !self.value_type(module,context,&source,env,active,b)?||!self.sync_drop(module,context,&source,env,active,b)?{return Err(bad("original owned erasure source lacks complete Value and SyncDrop proofs"));}
        let applications=self.dyn_applications(module,context,&interface,env,active,b)?;
        self.dyn_context_covers(module,context,&interface,&applications,&source,env,active,b)?;
        for application in &applications{self.original_trait_obligation(module,context,&source,application.declaration,&application.arguments,env,active,b)?;}
        for binding in &interface.associated{match &binding.value{
            Arg::Type(expected)=>{let actual=self.associated_type(module,context,&source,binding.application.declaration,&binding.application.arguments,binding.member,env,active,b)?;if !self.same(&actual,expected,b)?{return Err(bad("owned erasure changes its original associated Type equality"));}},
            Arg::Const(expected)=>{let actual=self.dyn_associated_constant(module,context,&source,&binding.application,binding.member,env,active,b)?;if !self.same(&actual.carrier,&expected.carrier,b)?||actual.value!=expected.value{return Err(bad("owned erasure changes its original associated Const equality"));}},
            Arg::Life(_)=>return Err(bad("associated binding has a Lifetime value")),
        }}
        self.calls.insert((env.current()?,site),vec![Arg::Type(Ty::Dyn(Box::new(interface.clone()))),Arg::Type(source)]);
        self.intrinsic_type_sources.insert((env.current()?,site,1),arguments[0].id);
        let mut operand=&arguments[0];while let CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=&operand.kind{b.spend(1)?;operand=inner;}
        let buffer=if self.value_places.contains_key(&(env.current()?,operand.id)){Some(self.operation_source(module,operand,env,bindings,b)?)}else{None};
        self.intrinsic_calls.insert((env.current()?,site),intrinsics::Use{kind:intrinsics::Intrinsic::OwnedDyn,buffer,worker:None,providers:None});
        let mut owner=interface;owner.owned=true;
        Ok(Ty::ErrorUnion(Box::new(Ty::SealedError(SealedError::DynCreate)),Box::new(Ty::Dyn(Box::new(owner)))))
    }

    /// Universal envelope inclusion, before any concrete vtable exists. Full
    /// phantom class dependencies and physical typed ports are different sets;
    /// a phantom Type actual alone cannot cover that Type's runtime ports.
    fn dyn_context_covers(&mut self,module:&TypedModule,context:&CheckedContext,interface:&DynType,applications:&[DynApplication],source:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let mut required=Vec::new();for application in applications{b.spend(application.arguments.len())?;required.extend(&application.arguments);}for row in &interface.associated{b.spend(row.application.arguments.len().saturating_add(1))?;required.extend(&row.application.arguments);required.push(&row.value);}
        let DynContext::Type(envelope)=&interface.context else{
            if !matches!(interface.context,DynContext::Closed)||!self.closed_value(module,context,source,env,active,b)?{return Err(bad("closed owned erasure lacks its original ClosedEnvironment proof"));}
            for argument in required{match argument{Arg::Life(_)=>return Err(bad("closed erased interface omits a persistent trait/associated lifetime actual")),Arg::Type(ty)=>if !self.closed_value(module,context,ty,env,active,b)?{return Err(bad("closed erased interface omits a complete phantom Type domain"));},Arg::Const(_)=>{}}}
            return Ok(());
        };
        let mut upper=Dependencies::default();let mut actual=Dependencies::default();
        self.dyn_dependencies(module,context,envelope,true,&mut upper,&mut Vec::new(),env,active,b,0)?;
        self.dyn_dependencies(module,context,source,true,&mut actual,&mut Vec::new(),env,active,b,0)?;
        for argument in required{match argument{Arg::Life(life)=>actual.classes.push(self.resolved_life(life,b)?),Arg::Type(ty)=>self.dyn_dependencies(module,context,ty,false,&mut actual,&mut Vec::new(),env,active,b,0)?,Arg::Const(value)=>self.dyn_dependencies(module,context,&value.carrier,false,&mut actual,&mut Vec::new(),env,active,b,0)?,}}
        for life in &actual.classes{let mut covered=false;for candidate in &upper.classes{if self.same_life(life,candidate,b)?{covered=true;break;}}if !covered{return Err(bad("original erased Context omits a persistent source lifetime or phantom occurrence"));}}
        for(ty,ports)in &actual.whole{let mut covered=false;for(candidate,physical)in &upper.whole{if (!*ports||*physical)&&self.same(ty,candidate,b)?{covered=true;break;}}if !covered{return Err(bad("original erased Context omits a complete symbolic Type domain or its physical ports"));}}
        for leaf in &actual.ports{
            let mut covered=false;
            for candidate in &upper.ports{
                let same=match(leaf,candidate){
                    (Ty::Address{mutable:a,life:x,inner:t},Ty::Address{mutable:z,life:y,inner:u})=>a==z&&self.same_life(x,y,b)?&&self.same(t,u,b)?,
                    (Ty::Desc{life:x,inner:t},Ty::Desc{life:y,inner:u})=>self.same_life(x,y,b)?&&self.same(t,u,b)?,
                    _=>false,
                };
                if same{covered=true;break;}
            }
            if !covered{return Err(bad("original erased Context has no complete compatible external source port"));}
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn dyn_dependencies(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,physical:bool,out:&mut Dependencies,stack:&mut Vec<SemanticDefinitionId>,env:&Env,active:&mut ActiveQueries,b:&mut Budget,depth:usize)->Check<()>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original dynamic Context dependency depth exceeded"));}
        let ty=self.resolved(ty,b)?;
        match &ty{
            Ty::Address{life,inner,..}|Ty::Desc{life,inner}=>{
                out.classes.push(self.resolved_life(life,b)?);
                if physical{out.ports.push(self.resolved(&ty,b)?);}
                self.dyn_dependencies(module,context,inner,false,out,stack,env,active,b,depth+1)?;
            },
            Ty::Nominal(id,arguments)=>{
                for argument in arguments{match argument{
                    Arg::Life(life)=>out.classes.push(self.resolved_life(life,b)?),
                    Arg::Type(ty)=>self.dyn_dependencies(module,context,ty,false,out,stack,env,active,b,depth+1)?,
                    Arg::Const(value)=>self.dyn_dependencies(module,context,&value.carrier,false,out,stack,env,active,b,depth+1)?,
                }}
                b.spend(stack.len())?;if stack.contains(id){return Ok(());}
                let original=context.definition(module,*id,b)?;
                let SemanticDefinitionBody::Nominal{body,..}=&original.body else{return Err(bad("original Context dependency has no complete nominal declaration"));};
                let mut selected=self.environment(module,context,*id,active,b)?;selected.lexical[0].actuals=self.resolve_arguments(arguments,0,b)?;
                stack.push(*id);
                let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|field|&field.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>variants.iter().flat_map(|variant|&variant.fields).collect::<Vec<_>>()};
                b.spend(fields.len())?;
                for field in fields{let field=self.ty(module,context,field,&selected,active,b)?;self.dyn_dependencies(module,context,&field,physical,out,stack,env,active,b,depth+1)?;}
                stack.pop();
            },
            Ty::Tuple(fields)=>for field in fields{self.dyn_dependencies(module,context,field,physical,out,stack,env,active,b,depth+1)?;},
            Ty::Array(inner,_)|Ty::Slice(inner)|Ty::Managed(inner)|Ty::Store(inner)=>self.dyn_dependencies(module,context,inner,physical,out,stack,env,active,b,depth+1)?,
            Ty::ErrorUnion(error,result)=>{self.dyn_dependencies(module,context,error,physical,out,stack,env,active,b,depth+1)?;self.dyn_dependencies(module,context,result,physical,out,stack,env,active,b,depth+1)?;},
            Ty::Atom(_)|Ty::SealedError(_)|Ty::Function(_)=>{},
            Ty::Code{kind:SemanticCodeKind::FunctionPointer|SemanticCodeKind::ForeignCallback,..}=>{},
            _=>{
                if !self.closed_value(module,context,&ty,env,active,b)?{out.whole.push((ty,physical));}
            },
        }
        Ok(())
    }
}
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dyn_associated_constant(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,application:&DynApplication,member:u32,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Const>{
        let evidence=self.proof_environment(caller,b)?;let caller=&evidence;
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Const,token},b,|active,b|{
            let receiver=self.resolved(receiver,b)?;
            if let Ty::Dyn(value)=&receiver{for row in &value.associated{if row.member==member&&row.application.declaration==application.declaration&&self.same_arguments_readonly(&row.application.arguments,&application.arguments,b)?{return match &row.value{Arg::Const(value)=>Ok(value.clone()),_=>Err(bad("erased associated Const selects a Type binding"))};}}return Err(bad("erased associated Const omits its complete original binding"));}
            let scope=dynamic_witness::semantic_external::original_scope(module,application.declaration,b)?;
            b.spend(module.dyn_declarations.len())?;let mut declarations=module.dyn_declarations.iter().filter(|row|row.id==application.declaration);let declaration=declarations.next().ok_or_else(||bad("associated Const original trait is absent"))?;if declarations.next().is_some()||declaration.kind!=DynDeclarationKind::Trait||!matches!(declaration.members.get(member as usize),Some(DynMemberDeclaration{kind:DynMemberKind::Constant,..})){return Err(bad("associated Const changes complete trait/member identity"));}
            let rows=declaration.associated_definitions.as_ref().ok_or_else(||bad("associated Const original member directory is Unknown"))?;b.spend(rows.len())?;let mut rows=rows.iter().filter(|row|row.member==member);let row=rows.next().ok_or_else(||bad("associated Const original definition is absent"))?;if rows.next().is_some(){return Err(bad("associated Const original definition is ambiguous"));}
            let mut trait_env=self.environment(module,context,scope.id,active,b)?;trait_env.lexical[0].actuals=self.resolve_arguments(&application.arguments,0,b)?;trait_env.self_type=Some((scope.id,receiver.clone()));
            let declared_carrier=dynamic_witness::semantic_external::original_type_recipe(module,application.declaration,row.carrier.as_ref().ok_or_else(||bad("associated Const omits its original declared carrier"))?,b)?;
            let expected=self.ty(module,context,&declared_carrier,&trait_env,active,b)?;self.scheme_carrier(&expected,b)?;
            let provider=self.original_provider(module,context,&receiver,application.declaration,&application.arguments,caller,active,b)?.ok_or_else(||bad("associated Const receiver has no exact original implementation"))?;
            let implementation=module.dyn_declarations.iter().find(|row|row.id==provider.declaration).ok_or_else(||bad("associated Const implementation is absent"))?;
            let name=&declaration.members[member as usize].name;b.spend(implementation.members.len())?;let mut overrides=implementation.members.iter().enumerate().filter(|(_,row)|row.name==*name&&row.kind==DynMemberKind::Constant);let ordinal=overrides.next().map(|(ordinal,_)|ordinal as u32);if overrides.next().is_some(){return Err(bad("associated Const implementation duplicates its member"));}
            let override_row=if let Some(ordinal)=ordinal{let rows=implementation.associated_definitions.as_ref().ok_or_else(||bad("associated Const implementation definitions are Unknown"))?;b.spend(rows.len())?;let mut rows=rows.iter().filter(|row|row.member==ordinal);let row=rows.next().ok_or_else(||bad("associated Const override RHS is absent"))?;if rows.next().is_some(){return Err(bad("associated Const override RHS is ambiguous"));}Some(row)}else{None};
            let (rhs,owner,selected)=if let Some(row)=override_row{(row,provider.declaration,&provider.environment)}else{(row,application.declaration,&trait_env)};
            let (ty,value):(&DynTypePattern,&DynConstPattern)=match rhs.value.as_ref(){Some(DynAssociatedValueRecipe::Constant{ty,value})=>(ty,value),_=>return Err(bad("associated Const has neither exact original override nor trait default RHS"))};
            let carrier=dynamic_witness::semantic_external::original_type_recipe(module,owner,ty,b)?;let carrier=self.ty(module,context,&carrier,selected,active,b)?;if !self.same(&carrier,&expected,b)?{return Err(bad("associated Const RHS changes declared carrier"));}
            let value=dynamic_witness::semantic_external::original_constant_recipe(module,owner,value,b)?;let result=self.constant(module,context,&value,&carrier,selected,active,b)?;
            for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module,owner,&rhs.requirements,b)?{self.obligation(module,context,&requirement,selected,caller,active,b)?;}
            for requirement in dynamic_witness::semantic_external::original_requirement_recipes(module,application.declaration,&row.requirements,b)?{self.obligation(module,context,&requirement,&trait_env,caller,active,b)?;}
            Ok(result)
        })
    }
}
impl CheckedOriginal {
    pub(super) fn unify_dyn(&mut self,a:&DynType,z:&DynType,b:&mut Budget)->Check<()>{
        fn args(this:&mut CheckedOriginal,a:&[Arg],z:&[Arg],b:&mut Budget)->Check<()>{if a.len()!=z.len(){return Err(bad("dynamic application changes full mixed arity"));}for(a,z)in a.iter().zip(z){b.spend(1)?;match(a,z){(Arg::Type(a),Arg::Type(z))=>this.unify(a,z,b)?,(Arg::Life(a),Arg::Life(z))=>this.unify_life(a,z,b)?,(Arg::Const(a),Arg::Const(z))if a.value==z.value=>this.unify(&a.carrier,&z.carrier,b)?,_=>return Err(bad("dynamic type equality changes original mixed actual kind/value"))}}Ok(())}
        if a.owned!=z.owned||a.application.declaration!=z.application.declaration||a.associated.len()!=z.associated.len(){return Err(bad("dynamic type equality changes owning form or full original interface identity"));}
        args(self,&a.application.arguments,&z.application.arguments,b)?;
        match(&a.context,&z.context){(DynContext::Closed,DynContext::Closed)=>{},(DynContext::Type(a),DynContext::Type(z))=>self.unify(a,z,b)?,(DynContext::Shape(a),DynContext::Shape(z))if a==z=>{},_=>return Err(bad("dynamic type equality changes its full original Context"))}
        for(a,z)in a.associated.iter().zip(&z.associated){if a.application.declaration!=z.application.declaration||a.member!=z.member{return Err(bad("dynamic equality changes an original associated member"));}args(self,&a.application.arguments,&z.application.arguments,b)?;args(self,std::slice::from_ref(&a.value),std::slice::from_ref(&z.value),b)?;}Ok(())
    }
}
