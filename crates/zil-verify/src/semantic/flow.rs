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
    for instance in &program.instances {
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
        }
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
    fn record(&self,scope:ScopeKey,key:OperandKey,value:&CheckedType,b:&mut Budget)->Check<()> {
        dynamic_metadata::ty(&value.ty,b)?;
        b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
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
        for row in &instance.header_embedding {self.class(module,&row.actual,environment,b)?;}
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
    fn claim(&self,module:&TypedModule,claim:&SemanticTypeValue,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let (ty,life,expressions)=resolved(claim)?;
        let mut classes=Vec::new();for expression in expressions {classes.push(self.class(module,expression,environment,b)?);}
        let actual=self.checked_value(module,ty,life,classes,b)?;
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
