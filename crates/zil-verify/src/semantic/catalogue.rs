//! Raw complete table indexing and executable-boundary checks. No catalogue
//! entry supplies a borrow root, class equation, builtin or sealed environment.
use super::*;
use std::collections::BTreeSet;
impl CheckedContext {
    pub(crate) fn catalogue(module:&TypedModule,b:&mut Budget)->Check<Self> {
        let program=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?;
        dynamic_metadata::recipe(program,b)?;
        let mut context=Self{definitions:BTreeMap::new(),instances:BTreeMap::new(),premises:RefCell::new(BTreeMap::new()),occurrences:RefCell::new(BTreeMap::new()),seals:RefCell::new(BTreeMap::new())};
        let mut identities=BTreeSet::new();
        for (index,definition) in program.definitions.iter().enumerate() {
            b.spend(definition.parameters.len().saturating_add(definition.implicit_lifetimes.len()).saturating_add(2))?;
            if context.definitions.insert(definition.id,index).is_some() || !identities.insert(definition.identity.clone()) {return Err(bad("duplicate original semantic definition identity"));}
            let mut ordinals=BTreeSet::new();let mut occurrences=BTreeSet::new();
            for row in &definition.implicit_lifetimes {
                b.spend(row.occurrence_path.len().saturating_add(1))?;
                if !ordinals.insert(row.ordinal)||!occurrences.insert(row.occurrence_path.clone()) {return Err(bad("duplicate implicit lifetime occurrence"));}
            }
            match (&definition.kind,&definition.body) {
                (SemanticDefinitionKind::Function|SemanticDefinitionKind::Method|SemanticDefinitionKind::Closure|SemanticDefinitionKind::TaskEntry,SemanticDefinitionBody::Function{signature,sites})=>{
                    raw_type(&signature.result,b)?;
                    for parameter in &signature.parameters {raw_type(&parameter.ty,b)?;}
                    let mut ids=BTreeSet::new();let mut paths=BTreeSet::new();
                    for site in sites {
                        if !ids.insert(site.id)||!paths.insert(site.occurrence_path.clone()) {return Err(bad("duplicate original semantic emission occurrence"));}
                        raw_site_recipe(&site.kind,b)?;
                    }
                }
                (SemanticDefinitionKind::Nominal,SemanticDefinitionBody::Nominal{declaration,kind,body})=>{
                    b.spend(module.life_declarations.len())?;
                    let mut actual=module.life_declarations.iter().filter(|row|row.id==*declaration);
                    let actual=actual.next().ok_or_else(||bad("original nominal has no actual declaration inventory"))?;
                    if module.life_declarations.iter().filter(|row|row.id==*declaration).count()!=1 || actual.identity!=definition.identity || actual.kind!=*kind || actual.parameters.len()!=definition.parameters.len() {return Err(bad("nominal original identity/kind/full formal order differs"));}
                    for (formal,actual) in definition.parameters.iter().zip(&actual.parameters) {
                        if !matches!((formal,actual),(SemanticFormal::Type{..},LifeParameterKind::Type)|(SemanticFormal::Lifetime,LifeParameterKind::Lifetime)|(SemanticFormal::Const{..},LifeParameterKind::Const(_))) {return Err(bad("nominal original mixed formal kind differs"));}
                    }
                    match body {
                        SemanticNominalBody::Product{fields}=>{let mut names=BTreeSet::new();for field in fields {if !names.insert(&field.name) {return Err(bad("duplicate original nominal field"));}raw_type(&field.ty,b)?;}},
                        SemanticNominalBody::Enum{variants}=>{let mut names=BTreeSet::new();for variant in variants {if !names.insert(&variant.name) {return Err(bad("duplicate original variant"));}for field in &variant.fields {raw_type(field,b)?;}}},
                    }
                }
                (SemanticDefinitionKind::Scope,SemanticDefinitionBody::Scope(recipe))=>match recipe {
                    SemanticScopeRecipe::Trait{..}=>{},
                    SemanticScopeRecipe::InherentImpl{self_type,..}|SemanticScopeRecipe::TraitImpl{self_type,..}=>raw_type(self_type,b)?,
                },
                _=>return Err(bad("original definition body disagrees with its declared kind")),
            }
            for formal in &definition.parameters {raw_formal(formal,b)?;}
            for row in &definition.environment {raw_formal(&row.kind,b)?;raw_actual_recipe(&row.source,b)?;}
            for requirement in &definition.requirements {raw_requirement(requirement,b)?;}
        }
        for definition in &program.definitions {
            let mut seen=BTreeSet::new();let mut current=Some(definition.id);
            while let Some(id)=current {
                b.spend(1)?;
                if !seen.insert(id) {return Err(bad("original lexical parent cycle"));}
                if seen.len()>b.limits.max_call_depth.min(128) {return Err(limit("original lexical parent depth exceeded"));}
                current=context.definition(module,id,b)?.parent;
            }
        }
        let mut targets=BTreeSet::new();
        for (index,instance) in program.instances.iter().enumerate() {
            b.spend(instance.actuals.len().saturating_add(instance.header_embedding.len()).saturating_add(1))?;
            if context.instances.insert(instance.id,index).is_some() {return Err(bad("duplicate semantic instance identity"));}
            let definition=context.definition(module,instance.definition,b)?;
            if instance.actuals.len()!=definition.parameters.len() {return Err(bad("instance actuals omit an original formal"));}
            for (formal,actual) in definition.parameters.iter().zip(&instance.actuals) {
                raw_actual(actual,b)?;
                if !matches!((formal,actual),(SemanticFormal::Type{..},SemanticActual::Type(_))|(SemanticFormal::Const{..},SemanticActual::Const{..})|(SemanticFormal::Lifetime,SemanticActual::Life(_))) {return Err(bad("instance actual has a different original mixed kind"));}
            }
            if let Some(value)=&instance.self_type {resolved(value)?;}
            match &instance.target {
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=>{
                    let actual=module.functions.get(function.0 as usize).ok_or_else(||bad("semantic actual function is absent"))?;
                    if actual.id!=*function||!targets.insert(*function) {return Err(bad("semantic instance actual function identity is ambiguous"));}
                    if !matches!(definition.body,SemanticDefinitionBody::Function{..}) {return Err(bad("scope instance claims an executable function"));}
                    let header=actual.storage_life.as_ref().ok_or_else(||bad("semantic function complete header is Unknown"))?;
                    signature_life::charge(header,b)?;
                    let mut binders=BTreeSet::new();
                    for row in &instance.header_embedding {
                        if !binders.insert(row.header) {return Err(bad("duplicate semantic header binder mapping"));}
                        if !match row.header {StorageBinder::Fixed(i)=>i<header.fixed_binder_count,StorageBinder::Invocation(i)=>i<header.invocation_binder_count} {return Err(bad("semantic header embedding selects an absent binder"));}
                        raw_class(&row.actual)?;
                    }
                    if binders.len()!=header.fixed_binder_count as usize+header.invocation_binder_count as usize {return Err(bad("semantic header embedding is incomplete"));}
                }
                SemanticInstanceTarget::Function(_)=>return Err(bad("Typed semantic instance retains a Core function target")),
                SemanticInstanceTarget::Scope=>{if !instance.header_embedding.is_empty() {return Err(bad("nonexecutable scope has a function header embedding"));}},
            }
            if let Some(environment)=&instance.environment {
                let SemanticClosureTarget::Typed(closure)=environment.closure else {return Err(bad("Typed semantic environment retains a Core closure"));};
                let mut rows=module.closures.iter().filter(|row|row.id==closure);
                let actual=rows.next().ok_or_else(||bad("semantic closure environment has no actual definition"))?;
                if rows.next().is_some()||!matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) if function==actual.function)||environment.actuals.len()!=definition.environment.len() {return Err(bad("semantic closure environment/entry/full actual order differs"));}
                for actual in &environment.actuals {raw_actual(actual,b)?;}
            }else if !definition.environment.is_empty() {return Err(bad("semantic free environment is absent"));}
        }
        for instance in &program.instances {
            let definition=context.definition(module,instance.definition,b)?;
            match (definition.parent,instance.parent) {
                (None,None)=>{},
                (Some(parent),Some(actual)) if context.instance(module,actual,b)?.definition==parent=>{},
                _=>return Err(bad("instance parent is not its real original lexical parent")),
            }
            if let SemanticLifetimeInstantiation::Partition{rigid}=instance.lifetime_instantiation {
                let rigid=context.instance(module,rigid,b)?;
                if rigid.definition!=instance.definition||!matches!(rigid.lifetime_instantiation,SemanticLifetimeInstantiation::Rigid)||rigid.actuals.len()!=instance.actuals.len() {return Err(bad("partition lacks its exact original rigid representative"));}
            }
            let mut seen=BTreeSet::new();let mut current=Some(instance.id);
            while let Some(id)=current {b.spend(1)?;if !seen.insert(id) {return Err(bad("instance lexical parent cycle"));}if seen.len()>b.limits.max_call_depth.min(128) {return Err(limit("semantic instance parent depth exceeded"));}current=context.instance(module,id,b)?.parent;}
        }
        for site in &program.sites {context.site_structure(module,site,b)?;}
        Ok(context)
    }
    fn site_structure(&self,module:&TypedModule,site:&SemanticSite,b:&mut Budget)->Check<()> {
        let instance=self.instance(module,site.instance,b)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target else {return Err(bad("semantic emission belongs to a nonexecutable instance"));};
        let definition=self.definition(module,instance.definition,b)?;
        let SemanticDefinitionBody::Function{sites,..}=&definition.body else {return Err(bad("semantic emission has no original function body"));};
        b.spend(sites.len())?;
        let recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("actual semantic site has no original recipe"))?;
        let SemanticSiteAddress::Typed{block,instruction,edge}=site.at else {return Err(bad("Typed semantic site retains a Core address"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("semantic site actual function is absent"))?;
        let block=f.blocks.get(block.0 as usize).ok_or_else(||bad("semantic site actual block is absent"))?;
        if instruction as usize>block.instructions.len() || instruction as usize==block.instructions.len()&&edge!=SemanticSiteEdge::Before {return Err(bad("semantic site is outside its actual instruction/before terminator boundary"));}
        let actual=block.instructions.get(instruction as usize);
        match (&recipe.kind,&site.action) {
            (SemanticSiteRecipeKind::Construct{..},SemanticSiteAction::Construct{result,value})=>{
                resolved(value)?;
                let SemanticOperand::Value(value)=result else {return Err(bad("actual constructor result is not its SSA definition"));};
                let actual=actual.ok_or_else(||bad("constructor ghost claim selects a terminator"))?;
                if actual.result!=Some(*value)||edge!=SemanticSiteEdge::After||!matches!(actual.kind,InstructionKind::MakeAggregate{..}|InstructionKind::MakePackage{..}|InstructionKind::MakeManaged{..}|InstructionKind::MakeOwnedArray{..}|InstructionKind::MakeErrorUnion{..}|InstructionKind::MakeDynOwned{..}|InstructionKind::OwnedStoreNew{..}) {return Err(bad("constructor ghost site does not name the actual defining opcode/result"));}
            }
            (SemanticSiteRecipeKind::Call{target,..},SemanticSiteAction::Call{target:actual_target,actuals})=>{
                let SemanticFunctionTarget::Typed(callee)=actual_target else {return Err(bad("semantic Call retains a Core target"));};
                let row=actual.ok_or_else(||bad("semantic Call selects a terminator"))?;
                if !matches!(row.kind,InstructionKind::Call{function,..}|InstructionKind::SuspendCall{function,..} if function==*callee) {return Err(bad("semantic Call target differs from the actual opcode"));}
                let mut targets=module.semantic.as_ref().ok_or_else(||bad("semantic program is absent"))?.instances.iter().filter(|instance|matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) if function==*callee));
                let target_instance=targets.next().ok_or_else(||bad("semantic Call has no actual target instance"))?;
                if targets.next().is_some()||target_instance.definition!=*target {return Err(bad("semantic Call selects another original definition"));}
                for actual in actuals {raw_actual(actual,b)?;}
            }
            (SemanticSiteRecipeKind::CallableCall{..},SemanticSiteAction::CallableCall{callee})=>{
                raw_operand(callee)?;
                let actual=actual.ok_or_else(||bad("callable ghost site selects a terminator"))?;
                let valid=match (&actual.kind,callee) {
                    (InstructionKind::CallClosure{callee,..}|InstructionKind::CallClosureBorrowed{callee,..}|InstructionKind::CallClosureOnce{callee,..},SemanticOperand::Value(value))=>callee==value,
                    (InstructionKind::CallClosureShared{callee,..}|InstructionKind::CallClosureMut{callee,..},SemanticOperand::Local(local))=>callee==local,
                    _=>false,
                };
                if !valid {return Err(bad("callable ghost site selects another actual operand/role"));}
            }
            (SemanticSiteRecipeKind::TypeUse{..},SemanticSiteAction::TypeUse{subject,expected})=>{raw_operand(subject)?;resolved(expected)?;}
            (SemanticSiteRecipeKind::DeclareLocal{..},SemanticSiteAction::DeclareLocal{local,expected})=>{
                let SemanticLocalTarget::Typed(local)=local else {return Err(bad("Typed declaration retains a Core local"));};
                if f.locals.get(local.0 as usize).is_none() {return Err(bad("declared semantic local is absent"));}resolved(expected)?;
            }
            (SemanticSiteRecipeKind::Closure{definition,..},SemanticSiteAction::Closure{result,closure,environment})=>{
                let (SemanticOperand::Value(value),SemanticClosureTarget::Typed(closure))=(result,closure) else {return Err(bad("semantic Closure retains a Core target or non-SSA result"));};
                let actual=actual.ok_or_else(||bad("Closure ghost site selects a terminator"))?;
                if actual.result!=Some(*value)||!matches!(actual.kind,InstructionKind::MakeClosure{closure:id,..} if id==*closure) {return Err(bad("Closure ghost claim differs from the actual creation instruction"));}
                self.definition(module,*definition,b)?;
                for actual in environment {raw_actual(actual,b)?;}
            }
            _=>return Err(bad("actual semantic action differs from its original emission role")),
        }
        Ok(())
    }
}
fn raw_class(class:&SemanticClassExpr)->Check<()> {
    match class {SemanticClassExpr::OuterOccurrence{operand,..}|SemanticClassExpr::IntrinsicClass{operand,..}=>raw_operand(operand),_=>Ok(())}
}
fn raw_operand(operand:&SemanticOperand)->Check<()> {if matches!(operand,SemanticOperand::CoreExpression{..}) {Err(bad("Typed semantic program retains a Core expression operand"))}else{Ok(())}}
fn raw_actual(actual:&SemanticActual,b:&mut Budget)->Check<()> {
    b.spend(1)?;
    match actual {
        SemanticActual::Type(value)|SemanticActual::Const{ty:value,..}=>{let(_,_,classes)=resolved(value)?;for class in classes {raw_class(class)?;}Ok(())},
        SemanticActual::Life(class)=>raw_class(class),SemanticActual::CoreRecipe{..}=>Err(bad("Typed semantic program retains a Core actual recipe")),
    }
}
fn raw_formal(formal:&SemanticFormal,b:&mut Budget)->Check<()> {match formal {SemanticFormal::Type{requirements}=>{for row in requirements {raw_requirement(row,b)?;}Ok(())},SemanticFormal::Const{carrier}=>raw_type(carrier,b),SemanticFormal::Lifetime=>Ok(())}}
fn raw_actual_recipe(recipe:&SemanticActualRecipe,b:&mut Budget)->Check<()> {match recipe {SemanticActualRecipe::Type(ty)=>raw_type(ty,b),SemanticActualRecipe::Const{carrier,..}=>raw_type(carrier,b),SemanticActualRecipe::Life(_)=>Ok(())}}
fn raw_site_recipe(recipe:&SemanticSiteRecipeKind,b:&mut Budget)->Check<()> {match recipe {
    SemanticSiteRecipeKind::Construct{result}|SemanticSiteRecipeKind::TypeUse{expected:result}|SemanticSiteRecipeKind::DeclareLocal{expected:result}|SemanticSiteRecipeKind::CallableCall{callee:result}=>raw_type(result,b),
    SemanticSiteRecipeKind::Call{actuals,..}|SemanticSiteRecipeKind::Closure{environment:actuals,..}=>{for actual in actuals {raw_actual_recipe(actual,b)?;}Ok(())},
}}
fn raw_requirement(recipe:&SemanticRequirementRecipe,b:&mut Budget)->Check<()> {b.spend(1)?;match recipe {
    SemanticRequirementRecipe::Structural{ty,..}=>raw_type(ty,b),
    SemanticRequirementRecipe::Trait{ty,application}=>{raw_type(ty,b)?;for actual in &application.arguments {raw_actual_recipe(actual,b)?;}Ok(())},
    SemanticRequirementRecipe::Callable{ty,signature,..}=>{raw_type(ty,b)?;raw_type(&signature.result,b)?;for parameter in &signature.parameters {raw_type(&parameter.ty,b)?;}Ok(())},
    SemanticRequirementRecipe::TypeEqual{left,right}=>{raw_type(left,b)?;raw_type(right,b)},
    SemanticRequirementRecipe::ConstEqual{carrier,..}=>raw_type(carrier,b),
    SemanticRequirementRecipe::ForAll{requirements,..}=>{for requirement in requirements {raw_requirement(requirement,b)?;}Ok(())},_=>Ok(()),
}}
fn raw_type(recipe:&SemanticTypeRecipe,b:&mut Budget)->Check<()> {
    b.spend(1)?;
    match recipe {
        SemanticTypeRecipe::UnresolvedNamed(_)|SemanticTypeRecipe::Unsupported(_)=>Err(bad("executable original recipe retains unresolved syntax")),
        SemanticTypeRecipe::Atom(_)|SemanticTypeRecipe::Exact(_)=>{
            // Exact is checked in its environment; the complete structural
            // visitor already bounds all Type nodes and shape rows.
            match recipe {SemanticTypeRecipe::Atom(ty) if !ty.is_b0_scalar()&&!matches!(ty,Type::ForeignHandle|Type::CWideString|Type::ByteBuffer|Type::SyncCell(_))=>Err(bad("Atom contains a hidden generic/borrowed subtree")),_=>Ok(())}
        }
        SemanticTypeRecipe::Tuple(fields)|SemanticTypeRecipe::AnonymousSum(fields)=>{for field in fields {raw_type(field,b)?;}Ok(())},
        SemanticTypeRecipe::Array{element,..}|SemanticTypeRecipe::Slice(element)|SemanticTypeRecipe::Address{inner:element,..}|SemanticTypeRecipe::Desc{inner:element,..}|SemanticTypeRecipe::Managed(element)|SemanticTypeRecipe::OwnedStore(element)|SemanticTypeRecipe::Mutex(element)|SemanticTypeRecipe::Registration(element)|SemanticTypeRecipe::AsyncRegistration(element)=>raw_type(element,b),
        SemanticTypeRecipe::ErrorUnion{error,result}=>{raw_type(error,b)?;raw_type(result,b)},
        SemanticTypeRecipe::Nominal{arguments,..}|SemanticTypeRecipe::Closure{arguments,..}|SemanticTypeRecipe::Opaque{arguments,..}=>{for actual in arguments {raw_actual_recipe(actual,b)?;}match recipe {SemanticTypeRecipe::Opaque{hidden,..}=>raw_type(hidden,b),SemanticTypeRecipe::Closure{environment,..}=>{for actual in environment {raw_actual_recipe(actual,b)?;}Ok(())},_=>Ok(())}},
        SemanticTypeRecipe::Qualified{storage,..}=>raw_type(storage,b),
        SemanticTypeRecipe::AssociatedType{receiver,application,..}=>{raw_type(receiver,b)?;for actual in &application.arguments {raw_actual_recipe(actual,b)?;}Ok(())},
        SemanticTypeRecipe::Code{signature,..}=>{for parameter in &signature.parameters {raw_type(&parameter.ty,b)?;}raw_type(&signature.result,b)},
        SemanticTypeRecipe::Task{result,..}|SemanticTypeRecipe::MutexGuard{payload:result,..}=>raw_type(result,b),
        _=>Ok(()),
    }
}
