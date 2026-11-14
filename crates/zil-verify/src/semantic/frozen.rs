//! A private read-only S/E view. S is universally checked source data, never a
//! collection of VM targets. Only E's actual Typed functions are replayed.
use super::*;
use zil_base::{ModuleId,NodeId};
mod core_nodes;
mod wrapper;

#[derive(Clone)]
pub(super) struct FrozenFunction {
    pub(super) module:ModuleId,
    pub(super) id:NodeId,
    pub(super) parameters:Vec<(PassingMode,Type)>,
    pub(super) result:Type,
    pub(super) result_mode:PassingMode,
    pub(super) foreign:Option<ForeignDeclaration>,
    pub(super) resume_bound:TaskResumeBound,
    pub(super) protocol_contracts:Vec<FunctionProtocolContract>,
}
pub(crate) struct FrozenContext<'s> {
    source:&'s CoreProgram,
    expected:&'s CtFrozenContext,
    definitions:BTreeMap<SemanticDefinitionId,&'s SemanticDefinition>,
    instances:BTreeMap<SemanticInstanceId,&'s SemanticInstance>,
    core:BTreeMap<(ModuleId,NodeId),&'s CoreFunction>,
    dyn_declarations:BTreeMap<LifeDeclarationId,&'s DynDeclaration>,
    dyn_scopes:Vec<&'s SemanticDynScopeBinding>,
    query:(&'s SemanticDefinition,&'s SemanticInstance,&'s CoreExpr),
}
impl<'s> FrozenContext<'s> {
    pub(crate) fn new(source:&'s CoreProgram,expected:&'s CtFrozenContext,b:&mut Budget)->Check<Self> {
        if source.source_fingerprint!=expected.stage.source_fingerprint {return Err(bad("frozen CT stage source identity differs from the actual supplied view"));}
        b.spend(source.modules.len())?;
        if source.modules.len()>b.limits.max_nodes{return Err(limit("frozen CT module inventory exceeded"));}
        let mut definitions=BTreeMap::new();let mut instances=BTreeMap::new();let mut core=BTreeMap::new();let mut modules=BTreeSet::new();
        let mut dyn_declarations=BTreeMap::<LifeDeclarationId,&DynDeclaration>::new();let mut dyn_scopes=Vec::<&SemanticDynScopeBinding>::new();
        for module in &source.modules {
            if module.core.schema_version!=SCHEMA_VERSION||!modules.insert(module.core.module_id){return Err(bad("frozen source module identity/schema is invalid or duplicated"));}
            let semantic=module.core.semantic.as_ref().ok_or_else(||unsupported("frozen source has no complete original semantic programme",None))?;
            dynamic_metadata::recipe(semantic,b)?;
            b.spend(module.core.dyn_declarations.len().saturating_add(semantic.dyn_scopes.len()))?;
            for declaration in &module.core.dyn_declarations {
                dynamic_metadata::declaration(declaration,b)?;
                if let Some(previous)=dyn_declarations.insert(declaration.id,declaration){if previous!=declaration{return Err(bad("frozen source changes a complete original trait/impl declaration identity"));}}
            }
            for scope in &semantic.dyn_scopes {
                b.spend(dyn_scopes.len().saturating_add(1))?;
                if let Some(previous)=dyn_scopes.iter().find(|row|row.owner==scope.owner){if **previous!=*scope{return Err(bad("frozen source changes an original dynamic owner's exact lexical scope"));}}
                else{dyn_scopes.push(scope);}
            }
            for definition in &semantic.definitions {if definition.id.module!=module.core.module_id||definitions.insert(definition.id,definition).is_some(){return Err(bad("frozen original definition has another module or duplicate identity"));}}
            for instance in &semantic.instances {if instance.id.module!=module.core.module_id||instances.insert(instance.id,instance).is_some(){return Err(bad("frozen original instance has another module or duplicate identity"));}}
            for function in &module.core.functions {
                b.spend(function.name.len().saturating_add(function.parameters.len()))?;
                if core.insert((module.core.module_id,function.id),function).is_some(){return Err(bad("frozen Core function identity is duplicated"));}
                dynamic_metadata::ty(&function.return_type,b)?;
                for parameter in &function.parameters{dynamic_metadata::ty(&parameter.ty,b)?;b.spend(parameter.name.len())?;}
                if let Some(header)=&function.storage_life{signature_life::charge(header,b)?;}
                if let Some(foreign)=&function.foreign{b.spend(foreign.abi.len().saturating_add(foreign.symbol.len()).saturating_add(foreign.noescape_parameters.len()))?;}
                let mut nodes=core_nodes::CoreNodes::default();nodes.walk(&function.body,b)?;
            }
        }
        if !modules.contains(&source.root){return Err(bad("frozen source root module is absent"));}
        for module in &source.modules {
            b.spend(module.imports.len().saturating_add(module.exports.len()))?;
            let mut aliases=BTreeSet::new();for import in &module.imports {b.spend(import.alias.len())?;if !modules.contains(&import.module)||!aliases.insert(&import.alias){return Err(bad("frozen source import has no exact unique module/alias"));}}
            for export in &module.exports {if !core.contains_key(&(module.core.module_id,*export)){return Err(bad("frozen source export names no actual Core function"));}}
        }
        let CtQueryOwner::Instance(owner)=expected.query.owner else{return Err(unsupported("frozen CT query needs its independently selected exact caller instance occurrence",None));};
        let instance=*instances.get(&owner).ok_or_else(||bad("frozen CT query owner instance is absent"))?;
        let definition=*definitions.get(&instance.definition).ok_or_else(||bad("frozen CT query owner definition is absent"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module,function})=instance.target else{return Err(unsupported("frozen CT query owner needs its complete actual Core closure/instance scope",None));};
        let function=*core.get(&(module,function)).ok_or_else(||bad("frozen CT query owner has no exact actual Core function"))?;
        let mut nodes=core_nodes::CoreNodes::default();nodes.walk(&function.body,b)?;
        let outer=*nodes.expressions.get(&expected.query.expression).ok_or_else(||bad("frozen CT query expression is absent from its actual owner"))?;
        let CoreExprKind::CompileTime(payload)=&outer.kind else{return Err(bad("frozen CT query expression is not its actual CompileTime shell"));};
        let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&definition.body else{return Err(bad("frozen CT owner omits its complete original body"));};
        let semantic=source.modules.iter().find(|row|row.core.module_id==module).and_then(|row|row.core.semantic.as_ref()).ok_or_else(||bad("frozen CT owner has no module programme"))?;
        let mut count=0usize;
        for site in &semantic.sites {
            b.spend(1)?;
            if site.instance!=owner||!matches!(site.at,SemanticSiteAddress::Core{expression,occurrence,role:SemanticEmissionRole::Ascription}if expression==expected.query.expression&&occurrence==expected.query.occurrence){continue;}
            let SemanticSiteAction::TypeUse{subject:SemanticOperand::CoreExpression{expression,occurrence},..}=&site.action else{return Err(bad("frozen CT query source site has another operation role"));};
            if *expression!=expected.query.expression||*occurrence!=expected.query.occurrence{return Err(bad("frozen CT query source site selects another expression occurrence"));}
            let link=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("frozen CT query lacks its original operation link"))?;
            if link.node!=expected.query.original{return Err(bad("frozen CT query original occurrence differs from its actual source site"));}
            let recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("frozen CT query original recipe is absent"))?;
            if !matches!(&recipe.kind,SemanticSiteRecipeKind::TypeUse{expected:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(node)}} if expected.query.original==SemanticOriginalNode::Expression(*node)){return Err(bad("frozen CT query has another original scoped type recipe"));}
            count+=1;
        }
        if count!=1{return Err(bad("frozen CT query occurrence is absent or ambiguous"));}
        Ok(Self{source,expected,definitions,instances,core,dyn_declarations,dyn_scopes,query:(definition,instance,payload)})
    }
    pub(super) fn background(&self,instance:&SemanticInstance)->bool {
        !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_)))&&self.instances.contains_key(&instance.id)
    }
    pub(super) fn source_instance(&self,instance:&SemanticInstance,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        let source=self.instances.get(&instance.id).ok_or_else(||bad("Core lookup does not belong to the actual frozen source"))?;
        if *source!=instance{return Err(bad("private frozen source instance was replaced by an executable claim"));}
        if !self.definitions.contains_key(&instance.definition){return Err(bad("frozen source instance has no original declaration"));}
        if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module,function})=instance.target {if !self.core.contains_key(&(module,function)){return Err(bad("frozen source instance has no actual Core function"));}}
        Ok(())
    }
    pub(super) fn functions(&self,module:&TypedModule,b:&mut Budget)->Check<BTreeMap<SemanticInstanceId,FrozenFunction>> {
        let mut functions=BTreeMap::new();
        for instance in &module.semantic.as_ref().ok_or_else(||bad("contextual E programme is absent"))?.instances {
            if !self.background(instance){continue;}
            if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module,function})=instance.target {
                let source=*self.core.get(&(module,function)).ok_or_else(||bad("frozen function lookup is absent"))?;
                b.spend(source.parameters.len())?;
                for contract in &source.protocol_contracts{b.spend(contract.entry.len().saturating_add(contract.exits.len()).saturating_add(1))?;for row in &contract.exits{b.spend(row.output.len())?;}}
                functions.insert(instance.id,FrozenFunction{module,id:function,parameters:source.parameters.iter().map(|row|(row.mode,row.ty.clone())).collect(),result:source.return_type.clone(),result_mode:source.result_mode,foreign:source.foreign.clone(),resume_bound:source.resume_bound,protocol_contracts:source.protocol_contracts.clone()});
            }
        }Ok(functions)
    }
    pub(crate) fn check_executable(&self,execution:&TypedModule,b:&mut Budget)->Check<()> {
        self.charge_execution(execution,b)?;
        self.check_foreign_declarations(execution,b)?;
        let Some(raw)=&execution.semantic else{return Err(bad("contextual CT execution cannot erase its semantic programme"));};
        let mut view=execution.clone();
        let mut semantic=SemanticModule{definitions:self.definitions.values().map(|row|(*row).clone()).collect(),instances:self.instances.values().map(|row|(*row).clone()).collect(),sites:raw.sites.clone(),dyn_scopes:self.dyn_scopes.iter().map(|row|(**row).clone()).collect()};
        // These are original read-only definitions, not execution targets.
        // E may carry equal publication copies but cannot replace S's owner,
        // full signature, associated RHS, premises or lexical scope.
        let mut seen=BTreeSet::new();
        for declaration in &execution.dyn_declarations {
            dynamic_metadata::declaration(declaration,b)?;
            if !seen.insert(declaration.id)||!self.dyn_declarations.get(&declaration.id).is_some_and(|source|**source==*declaration){return Err(bad("CT execution replaces or invents a frozen original trait/impl declaration"));}
        }
        let mut scopes=Vec::new();
        for scope in &raw.dyn_scopes {
            b.spend(scopes.len().saturating_add(self.dyn_scopes.len()).saturating_add(1))?;
            if scopes.contains(&scope.owner)||!self.dyn_scopes.iter().any(|source|**source==*scope){return Err(bad("CT execution replaces or invents an original dynamic lexical owner"));}
            scopes.push(scope.owner.clone());
        }
        view.dyn_declarations=self.dyn_declarations.values().map(|row|(*row).clone()).collect();
        let mut originals=BTreeSet::new();
        for definition in &raw.definitions {
            b.spend(1)?;
            if !originals.insert(definition.id){return Err(bad("CT execution repeats an original definition identity"));}
            if let Some(source)=self.definitions.get(&definition.id){
                // Both complete trees were prepaid above. An equal copy is
                // redundant lookup data: keep the supplied S object, including
                // its real scope, original head/body/names and all obligations.
                // It creates neither an instance nor an executable VM target.
                if *source!=definition{return Err(bad(&format!("CT execution package attempts to replace a frozen original definition: {:?}",definition.id)));}
                continue;
            }
            semantic.definitions.push(definition.clone());
        }
        let mut executable_instances=BTreeSet::new();
        for instance in &raw.instances {
            b.spend(1)?;
            if !executable_instances.insert(instance.id){return Err(bad("CT execution repeats an instance identity"));}
            if !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))){return Err(bad("CT execution instance retains a Core or nonexecutable target"));}
            if let Some(index)=semantic.instances.iter().position(|row|row.id==instance.id){semantic.instances[index]=instance.clone();}else{semantic.instances.push(instance.clone());}
        }
        view.semantic=Some(semantic);
        self.wrapper_identity(&view,execution,b)?;
        flow::check_program_mode(&view,Some(self),b)
    }
    fn charge_execution(&self,module:&TypedModule,b:&mut Budget)->Check<()> {
        b.spend(module.functions.len())?;
        if !module.closures.is_empty()||!module.resources.is_empty()||!module.dyn_interfaces.is_empty()||!module.dyn_vtables.is_empty(){return Err(bad("CT execution package carries a runtime environment/provider inventory"));}
        if let Some(semantic)=&module.semantic{dynamic_metadata::recipe(semantic,b)?;}
        for declaration in &module.dyn_declarations{dynamic_metadata::declaration(declaration,b)?;}
        if !module.unsupported.is_empty(){return Err(bad("CT executable contains unsupported capability metadata"));}
        b.spend(module.life_declarations.len().saturating_add(module.life_instances.len()))?;
        for declaration in &module.life_declarations {
            b.spend(declaration.identity.package.len().saturating_add(declaration.identity.module_path.iter().map(String::len).sum::<usize>()).saturating_add(declaration.identity.declaration_path.iter().map(String::len).sum::<usize>()).saturating_add(declaration.identity.generation_path.len()).saturating_add(declaration.parameters.len()))?;
            for formal in &declaration.parameters{if let LifeParameterKind::Const(ty)=formal{dynamic_metadata::ty(ty,b)?;}}
        }
        for instance in &module.life_instances {
            dynamic_metadata::ty(&instance.ty,b)?;b.spend(instance.arguments.len())?;
            for actual in &instance.arguments {match actual {
                LifeNominalActual::Type(ty)=>dynamic_metadata::ty(ty,b)?,
                LifeNominalActual::QualifiedType{ty,life}=>{dynamic_metadata::ty(ty,b)?;charge_life(life,b)?;},
                LifeNominalActual::Const(Constant::WideString(units))=>b.spend(units.len())?,
                _=>{},
            }}
        }
        for function in &module.functions {
            b.spend(function.name.len().saturating_add(function.locals.len()).saturating_add(function.values.len()).saturating_add(function.blocks.len()).saturating_add(function.parameters.len()))?;
            dynamic_metadata::ty(&function.return_type,b)?;
            for local in &function.locals{b.spend(local.name.len())?;dynamic_metadata::ty(&local.ty,b)?;}
            for ty in &function.values{dynamic_metadata::ty(ty,b)?;}
            if function.foreign.is_some()||function.environment.is_some()||function.dyn_instance.is_some()||function.retained_return_sources.is_some()||function.callable_return_contract.is_some()||function.callable_contract.is_some()||!function.protocol_contracts.is_empty(){return Err(bad("CT execution carries foreign/runtime retained-call authority"));}
            if let Some(header)=&function.storage_life{signature_life::charge(header,b)?;}
            if let Some(rows)=&function.reference_return_bound{b.spend(rows.len())?;}
            for block in &function.blocks {b.spend(block.instructions.len())?;for instruction in &block.instructions {
                match &instruction.kind {
                    InstructionKind::Call{function:target,arguments}=>{b.spend(arguments.len())?;if !module.functions.get(target.0 as usize).is_some_and(|row|row.id==*target&&row.foreign.is_none()&&row.environment.is_none()){return Err(bad("CT Call selects background/unknown execution authority"));}},
                    InstructionKind::MakeAggregate{fields}|InstructionKind::MakeVariant{fields,..}=>b.spend(fields.len())?,
                    InstructionKind::Constant(Constant::WideString(units))=>b.spend(units.len())?,
                    _=>{},
                }
            }}
        }
        // This pure capability scan precedes cloning the private lookup view;
        // ordinary CFG/type/loan verification follows on the same Budget.
        ct_program::pure(module).map_err(|mut errors|errors.remove(0))
    }
}
impl FrozenContext<'_> {
    pub(super) fn declaration<'a>(&'a self,definition:&SemanticDefinition,id:LifeDeclarationId,b:&mut Budget)->Check<&'a LifeNominalDeclaration> {
        let module=self.source.modules.iter().find(|row|row.core.module_id==definition.id.module).ok_or_else(||bad("frozen nominal original module is absent"))?;
        b.spend(module.core.life_declarations.len())?;
        let mut rows=module.core.life_declarations.iter().filter(|row|row.id==id);
        let row=rows.next().ok_or_else(||bad("frozen nominal has no complete actual declaration inventory"))?;
        if rows.next().is_some(){return Err(bad("frozen nominal has duplicate actual declaration identities"));}Ok(row)
    }
    pub(super) fn specializations(&self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let program=module.semantic.as_ref().ok_or_else(||bad("contextual executable semantic programme is absent"))?;
        let mut external=dynamic_witness::semantic_external::Proofs::default();
        for instance in &program.instances {
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))=instance.target else{continue;};
            let Some(source)=self.instances.get(&instance.id) else{continue;};
            if source.definition!=instance.definition||source.parent!=instance.parent||source.lifetime_instantiation!=instance.lifetime_instantiation||source.actuals.len()!=instance.actuals.len(){return Err(bad("selected CT instance changes its frozen original definition/parent/full actual identity"));}
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module:owner,function})=source.target else{return Err(unsupported("selected CT instance requires its exact frozen Core function specialization",None));};
            let function=self.core.get(&(owner,function)).ok_or_else(||bad("selected CT instance has no frozen Core function"))?;
            if function.foreign.is_some(){return Err(bad("a frozen external declaration cannot become a CT execution target"));}
            for (ordinal,(expected,actual)) in source.actuals.iter().zip(&instance.actuals).enumerate() {
                b.spend(1)?;
                match expected {
                    SemanticActual::CoreInferred{scope,call,formal}=>{
                        if *formal!=ordinal as u32{return Err(bad("frozen inference marker changes the full formal ordinal"));}
                        let caller=context.instance(module,*scope,b)?;
                        let caller_environment=context.environment_for_instance(module,*scope,active,b)?;
                        let original=context.original.borrow();let original=original.as_ref().ok_or_else(||bad("frozen specialization lacks original universal body proof"))?;
                        let (target,recipes)=original.call_recipes(module,caller.definition,*call,&caller_environment,b)?;
                        if target!=instance.definition{return Err(bad("frozen inferred actual resolves another original call target"));}
                        let recipe=recipes.get(*formal as usize).ok_or_else(||bad("frozen inferred actual is outside the original full mixed call solution"))?;
                        let expected=context.evaluate_actual(module,recipe,&caller_environment,&mut external,active,b)?;
                        let (SemanticActual::Type(actual),CheckedActual::Type(expected))=(actual,expected) else{return Err(unsupported("frozen mixed Const/Life specialization requires its complete original caller-domain map",None));};
                        if !expected.classes.is_empty(){return Err(unsupported("frozen inferred existing Type domain needs its complete original ordered intrinsic occurrence map",None));}
                        let (ty,life,classes)=resolved(actual)?;
                        if *ty!=expected.ty||*life!=expected.life||classes.len()!=expected.classes.len(){return Err(bad("selected CT actual differs from the independently solved frozen caller argument"));}
                    }
                    SemanticActual::Type(expected)=>{
                        let (ty,life,classes)=resolved(expected)?;
                        let SemanticActual::Type(actual)=actual else{return Err(bad("selected CT actual changes its frozen formal kind"));};
                        let (actual_ty,actual_life,actual_classes)=resolved(actual)?;
                        if actual_ty!=ty||actual_life!=life||actual_classes.len()!=classes.len(){return Err(bad("selected CT Type actual differs from its complete frozen source argument"));}
                        if !classes.is_empty(){return Err(unsupported("frozen existing Type domain requires its complete original occurrence image correspondence",None));}
                    }
                    SemanticActual::Const{ty,value}=>if actual!=&(SemanticActual::Const{ty:ty.clone(),value:value.clone()}){return Err(bad("selected CT Const actual changes its complete frozen carrier/value"));},
                    SemanticActual::Life(_)|SemanticActual::CoreRecipe{..}=>return Err(unsupported("frozen Life/recipe actual requires its complete original caller-domain substitution",None)),
                }
            }
        }
        let wrapper=program.instances.iter().find(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if self.expected.query.wrapper==SemanticFunctionTarget::Typed(id))).ok_or_else(||bad("contextual CT wrapper instance is absent"))?;
        let wrapper_environment=context.environment_for_instance(module,wrapper.id,active,b)?;
        let caller_environment=context.environment_for_instance(module,self.query.1.id,active,b)?;
        let annotations=self.wrapper_annotations(module,wrapper.definition,b)?;
        for (core,recipe) in annotations {
            let actual=context.evaluate_type(module,recipe,&wrapper_environment,&mut external,active,b)?;
            if actual.ty!=*core{return Err(bad("CT wrapper annotation differs from the actual frozen query tree"));}
        }
        let recipe={
            let mut storage=context.original.borrow_mut();let original=storage.as_mut().ok_or_else(||bad("CT wrapper lacks checked original names/body"))?;
            original.frozen_names(module,context,self.query.0.id,wrapper.definition,&self.aliases(b)?,active,b)?;
            // This is the original query's own lexical node, not wrapper SSA.
            let SemanticOriginalNode::Expression(node)=self.expected.query.original else{return Err(bad("frozen CT original query is not an expression"));};
            original.type_recipe(module,self.query.0.id,&SemanticOriginalTypeSource::Expression(node),&caller_environment,b)?
        };
        let expected=context.evaluate_type(module,&recipe,&caller_environment,&mut external,active,b)?;
        let definition=context.definition(module,wrapper.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,..}=&definition.body else{return Err(bad("CT wrapper has no complete original signature"));};
        let signature=context.evaluate_signature(module,signature,&wrapper_environment,&mut external,active,b)?;
        if !context.same_type(&caller_environment,&signature.result,&expected,b)?{return Err(bad("CT wrapper result differs from its original frozen caller/query type"));}
        context.finish_external_methods(module,&mut external,active,b)?;
        if !external.callable_shapes.is_empty()||!external.pending_statics.is_empty(){return Err(unsupported("CT frozen-context substitution leaves actual external obligations",None));}
        Ok(())
    }
    fn aliases(&self,b:&mut Budget)->Check<BTreeMap<(ModuleId,String),SemanticDefinitionId>> {
        let mut aliases=BTreeMap::new();
        for instance in self.instances.values() {
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module,function})=instance.target else{continue;};
            let function=self.core.get(&(module,function)).ok_or_else(||bad("frozen generated name has no real Core owner"))?;
            b.spend(function.name.len().saturating_add(1))?;
            if aliases.insert((module,function.name.clone()),instance.definition).is_some_and(|old|old!=instance.definition){return Err(bad("one frozen Core value name selects conflicting original declarations"));}
        }
        b.spend(aliases.keys().map(|(_,name)|name.len().saturating_add(1)).sum())?;
        let direct=aliases.clone();
        for module in &self.source.modules {for import in &module.imports {
            b.spend(direct.len())?;
            for ((owner,name),definition) in direct.iter().filter(|((owner,_),_)|*owner==import.module) {
                let _=owner;b.spend(import.alias.len().saturating_add(name.len()).saturating_add(1))?;
                let qualified=format!("{}::{}",import.alias,name);
                if aliases.insert((module.core.module_id,qualified),*definition).is_some_and(|old|old!=*definition){return Err(bad("frozen import alias selects conflicting original declarations"));}
            }
        }}
        Ok(aliases)
    }
}
fn charge_life(life:&StorageLifeShape,b:&mut Budget)->Check<()> {
    b.spend(life.binder_count as usize+life.slots.len()+life.retained.len()+life.nominal_arguments.len())?;
    for row in &life.slots{b.spend(row.path.len())?;}
    for row in &life.retained{b.spend(row.path.len())?;}
    for row in &life.nominal_arguments{b.spend(row.path.len())?;}
    Ok(())
}
impl FrozenContext<'_> {
    fn check_foreign_declarations(&self,execution:&TypedModule,b:&mut Budget)->Check<()> {
        // A private ABI data record reuses the independent structural C checker.
        // It is never appended to E, returned as VerifiedModule, or dispatched.
        for source in &self.source.modules {
            let mut abi=TypedModule{semantic:None,dyn_declarations:vec![],dyn_interfaces:vec![],dyn_vtables:vec![],life_declarations:vec![],life_instances:vec![],schema_version:SCHEMA_VERSION,module_id:source.core.module_id,source_fingerprint:source.core.source_fingerprint,target:execution.target.clone(),functions:vec![],closures:vec![],resources:vec![],unsupported:vec![]};
            for function in &source.core.functions {
                let Some(declaration)=&function.foreign else{continue;};
                if !function.body.statements.is_empty(){return Err(bad("frozen foreign declaration hides a Core implementation"));}
                b.spend(function.parameters.len())?;
                let parameters=(0..function.parameters.len()).map(|index|LocalId(index as u32)).collect::<Vec<_>>();
                let locals=function.parameters.iter().enumerate().map(|(index,row)|LocalDecl{id:LocalId(index as u32),name:row.name.clone(),ty:row.ty.clone(),mutable:false,span:row.span}).collect();
                abi.functions.push(TypedFunction{dyn_instance:None,retained_return_sources:None,storage_life:None,call_kind:CallKind::Direct,resume_bound:TaskResumeBound::Local,reference_return_bound:None,callable_return_contract:None,callable_contract:None,protocol_contracts:vec![],id:FunctionId(abi.functions.len() as u32),name:function.name.clone(),foreign:Some(declaration.clone()),environment:None,parameters,return_type:function.return_type.clone(),result_mode:function.result_mode,locals,values:vec![],blocks:vec![],entry:BlockId(0),span:function.span});
            }
            foreign::check_declarations(&abi,b)?;
        }
        Ok(())
    }
}
