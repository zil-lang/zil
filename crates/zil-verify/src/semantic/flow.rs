//! Actual ghost SSA/local flow. Type metadata never supplies a data target.
//! Every admitted instruction is replayed against its real operands and CFG.
use super::*;
use std::collections::VecDeque;
#[derive(Clone,PartialEq,Eq)]
struct State {locals:Vec<Option<CheckedType>>}
pub(crate) fn check_program(module:&TypedModule,b:&mut Budget)->Check<()> {
    let context=CheckedContext::catalogue(module,b)?;
    let program=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?;
    let mut active=ActiveQueries::new();
    let mut external=dynamic_witness::semantic_external::Proofs::default();
    let mut checked_instances=std::collections::BTreeSet::new();
    let mut ordered=Vec::new();
    for instance in &program.instances {
        let mut depth=0usize;let mut parent=instance.parent;
        while let Some(id)=parent {b.spend(1)?;depth+=1;parent=context.instance(module,id,b)?.parent;}
        ordered.push((depth,instance));
    }
    b.spend(ordered.len().saturating_mul(ordered.len().checked_ilog2().map_or(1,|n|n as usize+1)))?;
    ordered.sort_by_key(|(depth,instance)|(*depth,instance.id));
    for (_,instance) in ordered {
        if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target {
            let environment=context.environment_for_instance(module,instance.id,&mut active,b)?;
            let definition=context.definition(module,instance.definition,b)?;
            context.evaluate_requirements(module,&definition.requirements,&environment,RequirementMode::DeclarationPremise,&mut external,&mut active,b)?;
            for formal in &definition.parameters {
                if let SemanticFormal::Type{requirements}=formal {context.evaluate_requirements(module,requirements,&environment,RequirementMode::DeclarationPremise,&mut external,&mut active,b)?;}
            }
            let SemanticDefinitionBody::Function{signature,..}=&definition.body else {return Err(bad("actual function has no original signature"));};
            let signature=context.evaluate_signature(module,signature,&environment,&mut external,&mut active,b)?;
            context.function(module,instance,function,&signature,&environment,&mut external,&mut active,b)?;
            checked_instances.insert(instance.id);
        }
    }
    for cursor in context.pending_closures.borrow().iter() {
        b.spend(cursor.arguments.len().saturating_add(cursor.sealed.len()).saturating_add(cursor.environment.lexical.len()).saturating_add(cursor.expected.classes.len()))?;
        let instance=context.instance(module,cursor.instance,b)?;
        let definition=context.definition(module,instance.definition,b)?;
        if !checked_instances.contains(&instance.id)||!context.seals.borrow().contains_key(&instance.id) {return Err(unsupported("pending Closure type cursor has no complete actual creator seal and checked entry",None));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target else {return Err(bad("pending Closure cursor selects a nonexecutable scope"));};
        let entry=module.functions.get(function.0 as usize).ok_or_else(||bad("pending Closure entry is absent"))?;
        let closure=entry.environment.ok_or_else(||bad("pending Closure entry lacks its environment identity"))?;
        let relation=entry.storage_life.as_ref().ok_or_else(||bad("pending Closure entry complete header is Unknown"))?;
        if cursor.expected.ty!=Type::Closure(closure)||cursor.expected.life.binder_count!=relation.fixed_binder_count||cursor.expected.classes.len()!=relation.fixed_binder_count as usize||cursor.arguments.len()!=definition.parameters.len()||cursor.sealed.len()!=definition.environment.len() {return Err(bad("pending Closure cursor contradicts its fully checked actual entry/domain"));}
        // The caller-side cursor may instantiate the generic creator's rigid
        // lifetime into another caller domain. Its original recipe and every
        // actual Call substitution are checked separately; no global union
        // equates those outside images with the rigid creator's identities.
    }
    // External shape requests are obligations, not proof tokens. No Some
    // program is published while its actual entry/static memory is pending.
    if !external.callable_shapes.is_empty()||!external.pending_statics.is_empty() {
        return Err(unsupported("semantic external signature still has undischarged actual entry/static obligations",None));
    }
    // Scope-only tables cannot hide actual class-bearing functions. A complete
    // semantic program accounts for every nonforeign Typed function.
    for function in &module.functions {
        b.spend(program.instances.len())?;
        if function.foreign.is_none()&&!program.instances.iter().any(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==function.id)) {return Err(bad("semantic programme omits an actual function instance"));}
    }
    Ok(())
}
impl CheckedContext {
    pub(super) fn record(&self,scope:ScopeKey,key:OperandKey,value:&CheckedType,b:&mut Budget)->Check<()> {
        dynamic_metadata::ty(&value.ty,b)?;
        b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
        let mut aliases=self.origin_aliases.borrow_mut();
        for (class,origin) in value.origins.iter().enumerate() {
            let occurrence=IntrinsicOrigin::Existing{scope,operand:key.clone(),path:vec![],class:class as u32};
            if &occurrence!=origin {aliases.insert(occurrence,origin.clone());}
        }
        for row in &value.life.retained {
            b.spend(row.path.len().saturating_add(1))?;
            let origin=value.origins.get(row.binder as usize).ok_or_else(||bad("actual child origin is absent"))?;
            let occurrence=IntrinsicOrigin::Existing{scope,operand:key.clone(),path:row.path.clone(),class:row.class};
            if &occurrence!=origin {aliases.insert(occurrence,origin.clone());}
        }
        drop(aliases);
        self.occurrences.borrow_mut().insert((scope,key),value.clone());Ok(())
    }
    fn compatible(&self,environment:&EvalEnv,actual:&CheckedType,expected:&CheckedType,b:&mut Budget)->Check<bool> {
        if self.same_type(environment,actual,expected,b)? {return Ok(true);}
        if actual.ty!=expected.ty||!matches!(actual.ty,Type::Address{..}|Type::Desc(_)) {return Ok(false);}
        b.spend(actual.life.retained.len().saturating_add(expected.life.retained.len()).saturating_add(actual.life.nominal_arguments.len()).saturating_add(expected.life.nominal_arguments.len()))?;
        if actual.life.retained.len()!=expected.life.retained.len()||actual.life.nominal_arguments.len()!=expected.life.nominal_arguments.len() {return Ok(false);}
        for (a,c) in actual.life.retained.iter().zip(&expected.life.retained) {
            if a.path!=c.path||a.class!=c.class {return Ok(false);}
            let a=actual.classes.get(a.binder as usize).ok_or_else(||bad("actual child class is absent"))?;
            let c=expected.classes.get(c.binder as usize).ok_or_else(||bad("expected child class is absent"))?;
            if !self.same(environment,a,c,b)? {return Ok(false);}
        }
        for (a,c) in actual.life.nominal_arguments.iter().zip(&expected.life.nominal_arguments) {
            if a.path!=c.path||a.declaration!=c.declaration||a.argument!=c.argument {return Ok(false);}
            let a=actual.classes.get(a.binder as usize).ok_or_else(||bad("actual nominal class is absent"))?;
            let c=expected.classes.get(c.binder as usize).ok_or_else(||bad("expected nominal class is absent"))?;
            if !self.same(environment,a,c,b)? {return Ok(false);}
        }
        let a=actual.life.slots.iter().find(|row|row.path.is_empty());
        let c=expected.life.slots.iter().find(|row|row.path.is_empty());
        match (a,c) {
            (Some(a),Some(c)) if a.mode==c.mode=>self.outlives(environment,actual.classes.get(a.binder as usize).ok_or_else(||bad("actual outer class is absent"))?,expected.classes.get(c.binder as usize).ok_or_else(||bad("expected outer class is absent"))?,b),
            (None,Some(c))=>Ok(actual.statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode} if path.is_empty()&&leaf==&actual.ty&&*mode==c.mode))),
            _=>Ok(false),
        }
    }
    fn function(&self,module:&TypedModule,instance:&SemanticInstance,function:FunctionId,signature:&CheckedSignature,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("semantic actual function is absent"))?;
        if f.parameters.len()!=signature.parameters.len()||f.return_type!=signature.result.ty||f.result_mode!=signature.result_mode||f.call_kind!=signature.call_kind {return Err(bad("actual function ABI differs from the original complete signature"));}
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        b.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(f.blocks.len()))?;
        let mut initial=State{locals:vec![None;f.locals.len()]};
        for (index,(parameter,(expected,_))) in f.parameters.iter().zip(&signature.parameters).enumerate() {
            let local=f.locals.get(parameter.0 as usize).ok_or_else(||bad("actual parameter local is absent"))?;
            if local.ty!=expected.ty {return Err(bad("actual parameter whole type differs from its original recipe"));}
            initial.locals[parameter.0 as usize]=Some(expected.clone());
            self.record(scope,OperandKey::Parameter(index as u32),expected,b)?;
        }
        self.header(module,instance,f,signature,environment,b)?;
        if f.foreign.is_some() {return Err(unsupported("semantic foreign body requires its complete original external contract",None));}
        let mut states=vec![None;f.blocks.len()];
        let entry=f.entry.0 as usize;
        if entry>=states.len() {return Err(bad("actual CFG entry is absent"));}
        states[entry]=Some(initial);
        let mut queue=VecDeque::from([entry]);
        let mut values=vec![None;f.values.len()];
        let mut constraints=vec![None;f.locals.len()];
        while let Some(block_id)=queue.pop_front() {
            b.spend(1)?;
            let mut state=states[block_id].clone().ok_or_else(||bad("reachable semantic block has no incoming state"))?;
            let block=&f.blocks[block_id];
            for (ordinal,instruction) in block.instructions.iter().enumerate() {
                let mut environment=environment.clone();
                environment.at=Some(SitePoint{function,block:block.id,instruction:ordinal as u32,after:false});
                let sites=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?.sites.iter().filter(|site|site.instance==instance.id&&matches!(site.at,SemanticSiteAddress::Typed{block:id,instruction:index,..} if id==block.id&&index==ordinal as u32)).collect::<Vec<_>>();
                b.spend(module.semantic.as_ref().unwrap().sites.len())?;
                for site in &sites {
                    if let SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(local),expected}=&site.action {
                        let value=self.site_expected(module,instance,site,&environment,external,active,b)?;
                        if f.locals[local.0 as usize].ty!=value.ty {return Err(bad("declared local whole type differs from its original semantic constraint"));}
                        self.claim(module,expected,&value,&environment,b)?;
                        if let Some(old)=&constraints[local.0 as usize] {if !self.same_type(&environment,old,&value,b)? {return Err(bad("local declarations impose different semantic constraints"));}}
                        constraints[local.0 as usize]=Some(value);
                    }
                }
                let produced=match &instruction.kind {
                    InstructionKind::Constant(constant)=>Some(atomic(&constant.ty(),b)?),
                    InstructionKind::Load(local)|InstructionKind::TakeLocal{local}=>{
                        let value=state.locals.get(local.0 as usize).and_then(Option::as_ref).cloned().ok_or_else(||bad("semantic local use is not initialized on every incoming edge"))?;
                        self.record(scope,OperandKey::Local{local:*local,at:environment.at.unwrap()},&value,b)?;
                        if matches!(instruction.kind,InstructionKind::TakeLocal{..}) {state.locals[local.0 as usize]=None;}
                        Some(value)
                    }
                    InstructionKind::Store{local,value}=>{
                        let value=value_at(&values,*value)?;
                        if let Some(expected)=&constraints[local.0 as usize] {if !self.compatible(&environment,value,expected,b)? {return Err(bad("actual Store violates the declared rigid semantic class constraint"));}}
                        if f.locals[local.0 as usize].ty!=value.ty {return Err(bad("actual Store changes its local complete type"));}
                        state.locals[local.0 as usize]=Some(value.clone());None
                    }
                    InstructionKind::StorageLive(local)|InstructionKind::StorageDead(local)=>{state.locals[local.0 as usize]=None;None},
                    InstructionKind::MakeAggregate{fields}|InstructionKind::MakePackage{fields}|InstructionKind::MakeManaged{fields}|InstructionKind::MakeOwnedArray{elements:fields}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Construct{..}));
                        let site=declarations.next().ok_or_else(||bad("actual aggregate construction has no original semantic site"))?;
                        if declarations.next().is_some() {return Err(bad("actual aggregate construction has duplicate semantic claims"));}
                        let value=self.site_expected(module,instance,site,&environment,external,active,b)?;
                        let mut storage=&value.ty;
                        while let Type::BorrowedStorage{storage:inner,..}|Type::Managed(inner)=storage {storage=inner;}
                        let expected_fields=match storage {
                            Type::Tuple(fields)=>fields.iter().collect::<Vec<_>>(),
                            Type::Record{fields,..}|Type::Package{fields,..}=>fields.iter().map(|(_,ty)|ty).collect(),
                            Type::Array{element,length}=>{b.spend(*length as usize)?;vec![element.as_ref();*length as usize]},
                            _=>return Err(bad("actual construction does not match the original complete product storage")),
                        };
                        if fields.len()!=expected_fields.len() {return Err(bad("actual construction omits or duplicates original fields"));}
                        for (field,expected) in fields.iter().zip(expected_fields) {
                            let actual=value_at(&values,*field)?;
                            if actual.ty!=*expected {return Err(bad("actual construction field differs from its original complete type"));}
                            if !expected.is_b0_scalar() {return Err(unsupported("semantic construction requires complete child-class occurrence substitution",Some(instruction.span)));}
                        }
                        if let Some(result)=instruction.result {self.record(scope,OperandKey::Value(result),&value,b)?;}
                        if let SemanticSiteAction::Construct{value:claim,..}=&site.action {self.claim(module,claim,&value,&environment,b)?;}
                        Some(value)
                    }
                    InstructionKind::Call{function:callee,arguments}|InstructionKind::SuspendCall{function:callee,arguments}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Call{..}));
                        let site=declarations.next().ok_or_else(||bad("actual Call has no original semantic specialization site"))?;
                        if declarations.next().is_some() {return Err(bad("actual Call has duplicate semantic specialization claims"));}
                        self.actual_call(module,instance,site,*callee,arguments,&values,&environment,external,active,b)?
                    },
                    InstructionKind::MakeClosure{closure,captures}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Closure{..}));
                        let site=declarations.next().ok_or_else(||bad("actual Closure creation has no original semantic site"))?;
                        if declarations.next().is_some() {return Err(bad("actual Closure creation has duplicate semantic claims"));}
                        Some(self.seal_closure(module,instance,site,*closure,captures,&values,&environment,external,active,b)?)
                    }
                    InstructionKind::ScopeEnter{..}|InstructionKind::ScopeExit{..}=>None,
                    InstructionKind::Unary{operand,..}=>{let value=value_at(&values,*operand)?;if !value.ty.is_b0_scalar() {return Err(bad("unary operation changes a semantic carrier"));}Some(atomic(f.values.get(instruction.result.ok_or_else(||bad("unary result is absent"))?.0 as usize).ok_or_else(||bad("unary result type is absent"))?,b)?)},
                    InstructionKind::Binary{left,right,..}=>{if !value_at(&values,*left)?.ty.is_b0_scalar()||!value_at(&values,*right)?.ty.is_b0_scalar() {return Err(bad("binary operation changes a semantic carrier"));}Some(atomic(f.values.get(instruction.result.ok_or_else(||bad("binary result is absent"))?.0 as usize).ok_or_else(||bad("binary result type is absent"))?,b)?)},
                    _=>return Err(unsupported("actual instruction lacks complete independent semantic transfer",Some(instruction.span))),
                };
                if let Some(id)=instruction.result {
                    let value=produced.ok_or_else(||bad("actual result has no independently reconstructed semantic value"))?;
                    if f.values.get(id.0 as usize)!=Some(&value.ty) {return Err(bad("actual SSA complete type differs from its original semantic value"));}
                    if let Some(old)=values.get(id.0 as usize).and_then(Option::as_ref) {if !self.same_type(&environment,old,&value,b)? {return Err(bad("actual SSA definition changes rigid classes across CFG visits"));}}
                    values[id.0 as usize]=Some(value.clone());self.record(scope,OperandKey::Value(id),&value,b)?;
                }
                for site in &sites {
                    if let SemanticSiteAction::TypeUse{subject,expected}=&site.action {
                        let required=self.site_expected(module,instance,site,&environment,external,active,b)?;
                        self.claim(module,expected,&required,&environment,b)?;
                        let value=match subject {SemanticOperand::Value(value)=>value_at(&values,*value)?,SemanticOperand::Local(local)=>state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual type use reads an uninitialized local"))?,_=>return Err(unsupported("semantic TypeUse requires its exact actual operand ledger",None))};
                        if !self.compatible(&environment,value,&required,b)? {return Err(bad("actual TypeUse violates its original rigid class requirement"));}
                    }
                }
            }
            let successors=match &block.terminator {
                Terminator::Return(Some(value))=>{if !self.compatible(environment,value_at(&values,*value)?,&signature.result,b)? {return Err(bad("actual Return violates the original rigid semantic result"));}vec![]},
                Terminator::Return(None)=>{if signature.result.ty!=Type::Unit {return Err(bad("semantic non-Unit function has a Unit Return"));}vec![]},
                Terminator::Jump(target)=>vec![*target],Terminator::Branch{condition,then_block,else_block}=>{if value_at(&values,*condition)?.ty!=Type::Bool {return Err(bad("actual semantic CFG condition is not Bool"));}vec![*then_block,*else_block]},
                Terminator::Trap=>vec![],Terminator::Unreachable=>return Err(bad("reachable semantic Unreachable terminator")),
            };
            for target in successors {
                let target=target.0 as usize;
                if target>=states.len() {return Err(bad("actual semantic successor is absent"));}
                let changed=if let Some(old)=&mut states[target] {
                    let mut changed=false;
                    for (old,new) in old.locals.iter_mut().zip(&state.locals) {
                        b.spend(1)?;
                        match (&*old,new) {
                            (Some(left),Some(right))=>if !self.same_type(environment,left,right,b)? {return Err(bad("CFG join merges incompatible original rigid classes"));},
                            (Some(_),None)=>{*old=None;changed=true;},_=>{},
                        }
                    }
                    changed
                }else{states[target]=Some(state.clone());true};
                if changed {queue.push_back(target);}
            }
        }
        Ok(())
    }
    fn seal_closure(&self,module:&TypedModule,creator:&SemanticInstance,site:&SemanticSite,closure:ClosureId,captures:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType> {
        let creator_definition=self.definition(module,creator.definition,b)?;
        let SemanticDefinitionBody::Function{sites,..}=&creator_definition.body else{return Err(bad("actual Closure creator has no original body"));};
        let recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("actual Closure has no original creation occurrence"))?;
        let SemanticSiteRecipeKind::Closure{definition,environment:recipes}=&recipe.kind else{return Err(bad("actual Closure has a different original emission role"));};
        let child_definition=self.definition(module,*definition,b)?;
        let SemanticSiteAction::Closure{closure:SemanticClosureTarget::Typed(actual),environment:claims,..}=&site.action else{return Err(bad("actual Closure retains a Core creation claim"));};
        if *actual!=closure||recipes.len()!=child_definition.environment.len()||claims.len()!=recipes.len() {return Err(bad("Closure environment omits an original free variable or chooses another definition"));}
        let mut definitions=module.closures.iter().filter(|row|row.id==closure);
        let actual_definition=definitions.next().ok_or_else(||bad("actual Closure definition is absent"))?;
        if definitions.next().is_some()||actual_definition.captures.len()!=captures.len(){return Err(bad("actual Closure has an ambiguous definition or incomplete physical captures"));}
        for (capture,expected) in captures.iter().zip(&actual_definition.captures) {if value_at(values,*capture)?.ty!=*expected {return Err(bad("actual capture whole type differs from its real Closure definition"));}}
        let program=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?;
        let mut children=program.instances.iter().filter(|row|row.definition==*definition&&row.parent==Some(creator.id)&&matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) if function==actual_definition.function)&&row.environment.as_ref().is_some_and(|env|matches!(env.closure,SemanticClosureTarget::Typed(id) if id==closure)));
        let child=children.next().ok_or_else(||bad("actual Closure has no exact child/creator semantic instance"))?;
        if children.next().is_some(){return Err(bad("actual Closure child instance is ambiguous"));}
        let actual_environment=child.environment.as_ref().ok_or_else(||bad("actual Closure has no sealed environment claim"))?;
        let mut checked=Vec::new();
        for (index,((recipe,formal),claim)) in recipes.iter().zip(&child_definition.environment).zip(claims).enumerate() {
            let expected=self.evaluate_actual(module,&formal.source,environment,external,active,b)?;
            let proposed=self.evaluate_actual(module,recipe,environment,external,active,b)?;
            if !self.actual_same(environment,&expected,&proposed,b)?{return Err(bad("creation environment recipe differs from the original parent-scoped free variable"));}
            self.actual_claim(module,claim,&expected,environment,b)?;
            self.actual_claim(module,actual_environment.actuals.get(index).ok_or_else(||bad("child environment actual is absent"))?,&expected,environment,b)?;
            match (&formal.kind,&expected) {
                (SemanticFormal::Lifetime,CheckedActual::Life(_))=>{},
                (SemanticFormal::Type{requirements},CheckedActual::Type(_))=>self.evaluate_requirements(module,requirements,environment,RequirementMode::ActualObligation,external,active,b)?,
                (SemanticFormal::Const{carrier},CheckedActual::Const{carrier:actual,..})=>{let expected=self.evaluate_type(module,carrier,environment,external,active,b)?;if !self.same_type(environment,actual,&expected,b)?{return Err(bad("sealed Const environment has a different original parent-scoped carrier"));}},
                _=>return Err(bad("sealed environment has a different original formal kind")),
            }
            checked.push(expected);
        }
        if let Some(previous)=self.seals.borrow().get(&child.id) {for (left,right) in previous.iter().zip(&checked){if !self.actual_same(environment,left,right,b)?{return Err(bad("different actual creation sites do not seal the same child environment"));}}}
        b.spend(checked.len())?;
        self.seals.borrow_mut().insert(child.id,checked);
        let child_environment=self.environment_for_instance(module,child.id,active,b)?;
        let child_scope=ScopeKey::Instance(child.id);
        for (index,capture) in captures.iter().enumerate() {self.record(child_scope,OperandKey::Capture(index as u32),value_at(values,*capture)?,b)?;}
        let relation=actual_definition.storage_life.as_ref().ok_or_else(||bad("Closure complete Fixed/Invocation relation is Unknown"))?;
        let entry=module.functions.get(actual_definition.function.0 as usize).ok_or_else(||bad("Closure actual entry is absent"))?;
        if entry.environment!=Some(closure)||entry.storage_life.as_ref()!=Some(relation) {return Err(bad("Closure and actual entry identity/complete header differ"));}
        signature_life::charge(relation,b)?;
        let mut classes=Vec::new();let mut origins=Vec::new();let mut statics=Vec::new();
        for class in 0..relation.fixed_binder_count {
            let mut rows=child.header_embedding.iter().filter(|row|row.header==StorageBinder::Fixed(class));
            let row=rows.next().ok_or_else(||bad("Closure has an unbound actual free Fixed class"))?;
            if rows.next().is_some(){return Err(bad("Closure Fixed occurrence is ambiguously embedded"));}
            let key=self.class(module,&row.actual,&child_environment,b)?;
            let SemanticSiteAction::Closure{result:SemanticOperand::Value(result),..}=&site.action else {return Err(bad("actual Closure result is not its defining SSA value"));};
            let creator_scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
            let origin=IntrinsicOrigin::Existing{scope:creator_scope,operand:OperandKey::Value(*result),path:vec![],class};
            let canonical=IntrinsicOrigin::Existing{scope:ScopeKey::Instance(child.id),operand:OperandKey::Receiver,path:vec![],class};
            b.spend(1)?;self.origin_aliases.borrow_mut().insert(origin.clone(),canonical);
            origins.push(origin);classes.push(key);
        }
        for position in &relation.static_bindings {
            if !position.path.is_empty(){return Err(unsupported("opaque static occurrence needs its complete nested logical leaf inventory",None));}
            let leaf=match position.base {
                StorageBase::Capture(index)=>actual_definition.captures.get(index as usize),
                StorageBase::Parameter(index)=>actual_definition.parameters.get(index as usize),
                StorageBase::Result=>Some(&actual_definition.result),StorageBase::Receiver=>None,
            }.ok_or_else(||bad("opaque Static occurrence selects an absent actual leaf"))?;
            let mode=match leaf {Type::Address{mutable:false,..}=>StoredBorrowMode::SharedAddress,Type::Desc(_)=>StoredBorrowMode::ReadOnlyDescription,_=>return Err(bad("opaque Static occurrence is not a complete shared borrow leaf"))};
            dynamic_metadata::ty(leaf,b)?;b.spend(position.path.len().saturating_add(1))?;
            statics.push(StaticOccurrence::Logical{closure,position:position.clone(),leaf:leaf.clone(),mode});
        }
        let mut life=empty_life();life.binder_count=classes.len() as u32;
        Ok(CheckedType{ty:Type::Closure(closure),life,classes,origins,statics})
    }
    fn actual_same(&self,environment:&EvalEnv,left:&CheckedActual,right:&CheckedActual,b:&mut Budget)->Check<bool> {
        match (left,right) {
            (CheckedActual::Life(left),CheckedActual::Life(right))=>self.same(environment,left,right,b),
            (CheckedActual::Type(left),CheckedActual::Type(right))=>self.same_type(environment,left,right,b),
            (CheckedActual::Const{carrier:left,value:a},CheckedActual::Const{carrier:right,value:c})=>Ok(a==c&&self.same_type(environment,left,right,b)?),_=>Ok(false),
        }
    }
    fn actual_claim(&self,module:&TypedModule,claim:&SemanticActual,expected:&CheckedActual,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let valid=match (claim,expected) {
            (SemanticActual::Life(actual),CheckedActual::Life(expected))=>self.same(environment,&self.class(module,actual,environment,b)?,expected,b)?,
            (SemanticActual::Type(actual),CheckedActual::Type(expected))=>{self.claim(module,actual,expected,environment,b)?;true},
            (SemanticActual::Const{ty,value},CheckedActual::Const{carrier,value:expected})=>{self.claim(module,ty,carrier,environment,b)?;value==expected},_=>false,
        };
        if !valid{return Err(bad("actual sealed environment differs from the independently interpreted creator source"));}Ok(())
    }
    fn header(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,signature:&CheckedSignature,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let Some(header)=f.storage_life.as_ref() else {
            if f.environment.is_none()&&instance.header_embedding.is_empty()&&signature.result.ty.is_b0_scalar()&&signature.result.classes.is_empty()&&signature.result.statics.is_empty()&&signature.parameters.iter().all(|(value,_)|value.ty.is_b0_scalar()&&value.classes.is_empty()&&value.statics.is_empty()) {return Ok(());}
            return Err(bad("actual semantic header is Unknown"));
        };
        signature_life::charge(header,b)?;
        let mut images=Vec::new();
        for row in &instance.header_embedding {images.push((row.header,self.class(module,&row.actual,environment,b)?));}
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let base=|base:StorageBase|->Check<CheckedType>{
            match base {
                StorageBase::Parameter(index)=>signature.parameters.get(index as usize).map(|(value,_)|value.clone()).ok_or_else(||bad("header selects an absent original parameter")),
                StorageBase::Result=>Ok(signature.result.clone()),
                StorageBase::Capture(index)=>self.occurrences.borrow().get(&(scope,OperandKey::Capture(index))).cloned().ok_or_else(||bad("header Capture has no actual sealed creator value")),
                StorageBase::Receiver=>self.occurrences.borrow().get(&(scope,OperandKey::Receiver)).cloned().ok_or_else(||unsupported("actual receiver header requires its complete independent borrowed head scope",None)),
            }
        };
        for row in &header.bindings {
            let value=base(row.position.base)?;
            let path=row.position.path.iter().map(|part|match part {ContractProjection::Place(step)=>Ok(*step),_=>Err(unsupported("header logical borrow occurrence requires its full opaque port source directory",None))}).collect::<Check<Vec<_>>>()?;
            let mut slots=value.life.slots.iter().filter(|slot|slot.path==path);
            let slot=slots.next().ok_or_else(||bad("actual header invents a variable borrow occurrence absent from its original signature"))?;
            if slots.next().is_some(){return Err(bad("original borrow occurrence is ambiguously mapped"));}
            let expected=value.classes.get(slot.binder as usize).ok_or_else(||bad("original borrow occurrence has no semantic class"))?;
            let actual=&images.iter().find(|(binder,_)|*binder==row.binder).ok_or_else(||bad("actual header variable has no complete lexical embedding"))?.1;
            if !self.same(environment,actual,expected,b)? {return Err(unsupported("actual header borrow role requires a proved original per-occurrence lifetime adaptation",None));}
        }
        for row in &header.classes {
            let value=base(row.position.base)?;
            let class=if row.position.path.is_empty(){row.position.class}else{
                let mut rows=value.life.retained.iter().filter(|retained|retained.path==row.position.path&&retained.class==row.position.class);
                let row=rows.next().ok_or_else(||bad("actual header class has no exact original child occurrence"))?;
                if rows.next().is_some(){return Err(bad("original child class has duplicate maps"));}row.binder
            };
            let expected=value.classes.get(class as usize).ok_or_else(||bad("actual header class escapes the complete original domain"))?;
            let actual=&images.iter().find(|(binder,_)|*binder==row.binder).ok_or_else(||bad("header class has no exact lexical embedding"))?.1;
            if !self.same(environment,actual,expected,b)? {return Err(unsupported("actual header class needs its original occurrence-preserving rigid/partition map",None));}
        }
        for row in &header.static_bindings {
            let value=base(row.base)?;
            let path=row.path.iter().map(|part|match part {ContractProjection::Place(step)=>Ok(LifeTypeProjection::Place(*step)),_=>Err(unsupported("header Static logical occurrence needs the complete checked source directory",None))}).collect::<Check<Vec<_>>>()?;
            if !value.statics.iter().any(|occurrence|matches!(occurrence,StaticOccurrence::Direct{path:actual,..} if *actual==path)||matches!(occurrence,StaticOccurrence::Logical{closure:_,position,..} if position==row)) {return Err(bad("actual header erases an original variable class into a Static occurrence"));}
        }
        // The ordinary signature kernel subsequently checks exact full typed
        // coverage; here every original root class must also remain explicit.
        for (base_kind,value) in signature.parameters.iter().enumerate().map(|(i,(value,_))|(StorageBase::Parameter(i as u32),value)).chain(std::iter::once((StorageBase::Result,&signature.result))) {
            if matches!(value.ty,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::Dyn{..}|Type::OwnedDyn{..}) {
                for class in 0..value.classes.len() as u32 {
                    b.spend(header.classes.len())?;
                    if !header.classes.iter().any(|row|row.position.base==base_kind&&row.position.path.is_empty()&&row.position.class==class) {return Err(bad("actual header omits an original nominal/opaque class occurrence"));}
                }
            }
        }
        Ok(())
    }
    fn actual_call(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,callee:FunctionId,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>> {
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::Function{sites,..}=&definition.body else{return Err(bad("semantic caller has no original body"));};
        let site_recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("actual Call has no original recipe"))?;
        let SemanticSiteRecipeKind::Call{target,actuals}=&site_recipe.kind else {return Err(bad("actual Call has another original role"));};
        let target_definition=self.definition(module,*target,b)?;
        let SemanticSiteAction::Call{target:SemanticFunctionTarget::Typed(actual_target),actuals:claims}=&site.action else {return Err(bad("actual Call retains a non-Typed specialization"));};
        if *actual_target!=callee||actuals.len()!=target_definition.parameters.len()||claims.len()!=actuals.len() {return Err(bad("actual Call changes its original specialization target/full actual order"));}
        let mut checked=Vec::new();
        for (recipe,claim) in actuals.iter().zip(claims) {
            let expected=self.evaluate_actual(module,recipe,environment,external,active,b)?;
            let equal=match (&expected,claim) {
                (CheckedActual::Type(expected),SemanticActual::Type(claim))=>{self.claim(module,claim,expected,environment,b)?;true},
                (CheckedActual::Const{carrier,value},SemanticActual::Const{ty,value:actual})=>{self.claim(module,ty,carrier,environment,b)?;value==actual},
                (CheckedActual::Life(expected),SemanticActual::Life(actual))=>self.same(environment,expected,&self.class(module,actual,environment,b)?,b)?,
                _=>false,
            };
            if !equal {return Err(bad("actual Call actual differs from its original caller-scoped recipe"));}
            checked.push(expected);
        }
        let token=active.fresh(b)?;
        let key=ScopeKey::Witness{definition:*target,frame:token};
        let mut lexical=Vec::new();
        if let Some(parent)=target_definition.parent {
            let position=environment.lexical.iter().position(|frame|frame.definition==parent).ok_or_else(||unsupported("Call target lexical parent is not available in the actual caller context",None))?;
            lexical.extend_from_slice(&environment.lexical[position..]);
        }
        lexical.insert(0,LexicalFrame{definition:*target,key,actuals:checked,environment:vec![],self_type:None});
        if !target_definition.environment.is_empty() {return Err(unsupported("ordinary Call cannot manufacture a closure's sealed ghost environment",None));}
        let call_environment=EvalEnv{lexical,assumptions:environment.assumptions.clone(),bounds:environment.bounds.clone(),invocations:vec![],at:environment.at};
        self.evaluate_requirements(module,&target_definition.requirements,&call_environment,RequirementMode::ActualObligation,external,active,b)?;
        for formal in &target_definition.parameters {if let SemanticFormal::Type{requirements}=formal {self.evaluate_requirements(module,requirements,&call_environment,RequirementMode::ActualObligation,external,active,b)?;}}
        let SemanticDefinitionBody::Function{signature,..}=&target_definition.body else {return Err(bad("actual Call target is not an original function"));};
        let mut signature=self.evaluate_signature(module,signature,&call_environment,external,active,b)?;
        if signature.parameters.len()!=arguments.len() {return Err(bad("actual Call operand count differs from its original signature"));}
        let mut invocation=Vec::<(ClassKey,ClassKey)>::new();
        for ((expected,_),argument) in signature.parameters.iter().zip(arguments) {
            let actual=value_at(values,*argument)?;
            for slot in &expected.life.slots {
                if !slot.path.is_empty() {continue;}
                let expected_key=expected.classes.get(slot.binder as usize).ok_or_else(||bad("Call parameter class is absent"))?;
                let fresh=signature.invocation.contains(expected_key)||matches!(expected_key,ClassKey::Implicit{scope,..} if *scope==key);
                if !fresh {continue;}
                let Some(actual_slot)=actual.life.slots.iter().find(|row|row.path==slot.path&&row.mode==slot.mode) else {return Err(unsupported("Call fresh Invocation requires complete actual static/borrow role instantiation",None));};
                let actual_key=actual.classes.get(actual_slot.binder as usize).ok_or_else(||bad("actual Call argument class is absent"))?;
                if let Some((_,previous))=invocation.iter().find(|(key,_)|key==expected_key) {if !self.same(environment,previous,actual_key,b)? {return Err(bad("actual Call violates a repeated original invocation relation"));}}
                else {invocation.push((expected_key.clone(),actual_key.clone()));}
            }
        }
        for (parameter,_) in &mut signature.parameters {substitute(parameter,&invocation,b)?;}
        substitute(&mut signature.result,&invocation,b)?;
        let actual_function=module.functions.get(callee.0 as usize).ok_or_else(||bad("actual Call function is absent"))?;
        for ((expected,_),argument) in signature.parameters.iter().zip(arguments) {
            if !self.compatible(environment,value_at(values,*argument)?,expected,b)? {return Err(bad("actual Call argument violates the original invariant class specialization"));}
        }
        if actual_function.return_type!=signature.result.ty {return Err(bad("actual Call specialization result whole type differs"));}
        Ok(if signature.result.ty==Type::Unit {None}else{Some(signature.result)})
    }
    fn site_expected(&self,module:&TypedModule,instance:&SemanticInstance,site:&SemanticSite,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType> {
        let definition=self.definition(module,instance.definition,b)?;
        let SemanticDefinitionBody::Function{sites,..}=&definition.body else{return Err(bad("semantic site has no original function"));};
        b.spend(sites.len())?;
        let recipe=sites.iter().find(|recipe|recipe.id==site.recipe).ok_or_else(||bad("semantic site has no original recipe"))?;
        let recipe=match &recipe.kind {SemanticSiteRecipeKind::Construct{result}|SemanticSiteRecipeKind::DeclareLocal{expected:result}|SemanticSiteRecipeKind::TypeUse{expected:result}=>result,_=>return Err(bad("site role has no direct expected type recipe"))};
        self.evaluate_type(module,recipe,environment,external,active,b)
    }
    fn expression_origin(&self,expression:&SemanticClassExpr,environment:&EvalEnv,b:&mut Budget)->Check<Option<IntrinsicOrigin>> {
        let SemanticClassExpr::IntrinsicClass{operand,path,class}=expression else {return Ok(None);};
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let (scope,operand)=match operand {
            SemanticOperand::Parameter(index)=>(frame.key,OperandKey::Parameter(*index)),
            SemanticOperand::Capture(index)=>(frame.key,OperandKey::Capture(*index)),
            SemanticOperand::Receiver=>(frame.key,OperandKey::Receiver),
            SemanticOperand::Value(value)=>(frame.key,OperandKey::Value(*value)),
            SemanticOperand::Local(local)=>(frame.key,OperandKey::Local{local:*local,at:environment.at.ok_or_else(||bad("Local origin needs an actual CFG point"))?}),
            SemanticOperand::TypeArgument{scope,ordinal}=>{let frame=self.frame(environment,*scope,b)?;(frame.key,OperandKey::TypeArgument{scope:frame.key,ordinal:*ordinal})},
            SemanticOperand::Environment(ordinal)=>(frame.key,OperandKey::Environment{scope:frame.key,ordinal:*ordinal}),
            SemanticOperand::CoreExpression{..}=>return Err(bad("Typed origin retains a Core expression")),
        };
        b.spend(path.len().saturating_add(1))?;
        Ok(Some(IntrinsicOrigin::Existing{scope,operand,path:path.clone(),class:*class}))
    }
    fn claim(&self,module:&TypedModule,claim:&SemanticTypeValue,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let (ty,life,expressions)=resolved(claim)?;
        let mut classes=Vec::new();for expression in expressions {classes.push(self.class(module,expression,environment,b)?);}
        let mut actual=self.checked_value(module,ty,life,classes,b)?;
        for (index,expression) in expressions.iter().enumerate() {
            if let Some(origin)=self.expression_origin(expression,environment,b)? {actual.origins[index]=origin;}
        }
        if !self.same_type(environment,&actual,expected,b)? {return Err(bad("site complete type/class claim differs from its original scoped recipe"));}
        Ok(())
    }
}
fn value_at(values:&[Option<CheckedType>],id:ValueId)->Check<&CheckedType> {values.get(id.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual SSA use has no independent semantic definition"))}
fn atomic(ty:&Type,b:&mut Budget)->Check<CheckedType> {b.spend(1)?;if !ty.is_b0_scalar() {return Err(bad("actual scalar instruction contains a borrowed/opaque semantic carrier"));}Ok(CheckedType{ty:ty.clone(),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]})}

fn substitute(value:&mut CheckedType,map:&[(ClassKey,ClassKey)],b:&mut Budget)->Check<()> {
    b.spend(value.classes.len().saturating_mul(map.len().saturating_add(1)))?;
    for key in &mut value.classes {if let Some((_,replacement))=map.iter().find(|(source,_)|source==key) {*key=replacement.clone();}}
    // Intrinsic origins remain the exact original occurrences. Only their
    // outside class images are instantiated; no N2 domain is compressed.
    Ok(())
}
