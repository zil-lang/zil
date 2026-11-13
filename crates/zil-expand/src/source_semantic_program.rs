//! One request's original templates, actual Engine owners and Core emissions.
//! All rows remain untrusted program data for Types and the safety kernels.
use super::*;
use std::collections::BTreeMap;
use zil_base::NodeId;
const TEMPLATE:u32=1<<31;
struct FunctionTemplate { definition:SemanticDefinitionId, parameters:Vec<s::GenericParam> }
struct Instance { id:SemanticInstanceId, template:usize, actuals:Vec<SemanticActualRecipe>, core:Option<(ModuleId,NodeId)> }
#[derive(Default)]
pub(crate) struct Program {
    pub(crate) modules:Vec<(ModuleId,SemanticModule)>,
    templates:Vec<FunctionTemplate>,
    instances:Vec<Instance>,
    pub(crate) ledger:emission::Ledger,
}
impl Program {
    pub(crate) fn valid_function(&self,marker:s::SemanticContextMarker)->bool { marker.raw_index().is_some_and(|raw| if raw&TEMPLATE!=0 {self.templates.get((raw&!TEMPLATE)as usize).is_some()}else{self.instances.get(raw as usize).is_some()}) }
    pub(crate) fn valid_node(&self,marker:s::SemanticContextMarker)->bool {self.ledger.is_template(marker)}
    pub(crate) fn ensure_function(&mut self,function:&mut s::Function,budget:&mut Budget<'_>)->R<()> {
        let Some(raw)=function.semantic.raw_index() else{return Ok(())};
        if raw&TEMPLATE==0 {return Ok(())}
        let index=(raw&!TEMPLATE)as usize;
        let template=self.templates.get(index).ok_or_else(||error(function.span,"function Source owner is absent"))?;
        let id=SemanticInstanceId{module:template.definition.module,index:u32::try_from(self.instances.len()).map_err(|_|error(function.span,"semantic instance namespace exhausted"))?};
        let mut actuals=Vec::new();
        for (ordinal,formal) in template.parameters.iter().enumerate() {
            let reference=SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:ordinal as u32};
            let value=match formal.kind {
                s::GenericParamKind::Lifetime=>SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference)),
                s::GenericParamKind::Type(_)=>SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference)),
                s::GenericParamKind::Const(_)=>return Err(unsupported(formal.span,"remaining Const owner requires actual specialization operands")),
            };
            budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(),formal.span)?;actuals.push(value);
        }
        self.ledger.original_owner(id,template.definition,budget,function.span)?;
        budget.lineage_bytes(std::mem::size_of::<Instance>(),function.span)?;
        let marker=u32::try_from(self.instances.len()).map_err(|_|error(function.span,"semantic function marker exhausted"))?;
        if marker>=TEMPLATE {return Err(error(function.span,"semantic function marker overlaps templates"))}
        self.instances.push(Instance{id,template:index,actuals,core:None});
        function.semantic=s::SemanticContextMarker::untrusted(marker);
        self.realize_body(function,budget)
    }
    pub(crate) fn specialize(&mut self,function:&mut s::Function,arguments:&BTreeMap<String,s::TypeRef>,budget:&mut Budget<'_>)->R<()> {
        let Some(raw)=function.semantic.raw_index() else{return Ok(())};
        if raw&TEMPLATE==0 {return Err(error(function.span,"actual function specialization reuses a completed owner"))}
        let index=(raw&!TEMPLATE)as usize;
        let template=self.templates.get(index).ok_or_else(||error(function.span,"specialized Source template is absent"))?;
        let id=SemanticInstanceId{module:template.definition.module,index:self.instances.len()as u32};
        let mut actuals=Vec::new();
        for (ordinal,formal) in template.parameters.iter().enumerate() {
            let value=match &formal.kind {
                s::GenericParamKind::Lifetime=>SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:ordinal as u32})),
                s::GenericParamKind::Type(_)=> {
                    let actual=arguments.get(&formal.name).ok_or_else(||unsupported(formal.span,"Source specialization lacks its original whole Type actual"))?;
                    budget.ty(actual)?;
                    let ty=crate::type_ref(actual);
                    if !ty.is_b0_scalar() && ty!=Type::Unit {return Err(unsupported(actual.span,"whole specialization Type requires its original qualified argument recipe"))}
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(ty))
                },
                s::GenericParamKind::Const(_)=>return Err(unsupported(formal.span,"Source Const specialization requires its complete carrier recipe")),
            };
            budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(),formal.span)?;actuals.push(value);
        }
        self.ledger.original_owner(id,template.definition,budget,function.span)?;
        budget.lineage_bytes(std::mem::size_of::<Instance>(),function.span)?;
        let marker=self.instances.len()as u32;
        self.instances.push(Instance{id,template:index,actuals,core:None});
        function.semantic=s::SemanticContextMarker::untrusted(marker);
        self.realize_body(function,budget)
    }
    fn realize_body(&mut self,function:&mut s::Function,budget:&mut Budget<'_>)->R<()> {
        use crate::generated_spans::Visitor;
        let raw=function.semantic.raw_index().ok_or_else(||error(function.span,"actual function owner is absent"))?;
        let owner=self.instances.get(raw as usize).ok_or_else(||error(function.span,"actual function owner is out of range"))?.id;
        struct Realize<'a,'b,'l>{ledger:&'a mut emission::Ledger,owner:SemanticInstanceId,budget:&'b mut Budget<'l>}
        impl Visitor for Realize<'_, '_, '_>{
            fn node(&mut self,span:&mut Span,depth:usize)->CompileResult<()>{self.budget.node(*span,depth).map_err(|e|vec![e])}
            fn text(&mut self,text:&str,span:Span)->CompileResult<()>{self.budget.text(text,span).map_err(|e|vec![e])}
            fn expression(&mut self,e:&mut s::Expr,depth:usize)->CompileResult<()>{
                if self.ledger.is_template(e.semantic){e.semantic=self.ledger.instantiate_template(e.semantic,self.owner,self.budget,e.span).map_err(|e|vec![e])?;}
                crate::generated_spans::walk_expression(self,e,depth)
            }
            fn statement(&mut self,s:&mut s::Stmt,depth:usize)->CompileResult<()>{
                if self.ledger.is_template(s.semantic){s.semantic=self.ledger.instantiate_template(s.semantic,self.owner,self.budget,s.span).map_err(|e|vec![e])?;}
                crate::generated_spans::walk_statement(self,s,depth)
            }
        }
        let mut pass=Realize{ledger:&mut self.ledger,owner,budget};
        match &mut function.body {s::FunctionBody::Block(body)=>pass.block(body,0),s::FunctionBody::Expression(e)=>pass.expression(e,0),s::FunctionBody::Declaration=>Ok(())}.map_err(|mut e|e.remove(0))
    }
    pub(crate) fn bind_function(&mut self,marker:u32,module:ModuleId,node:NodeId,budget:&mut Budget<'_>,span:Span)->R<()> {
        let instance=self.instances.get_mut(marker as usize).ok_or_else(||error(span,"actual emitted function owner is absent"))?;
        if instance.core.is_some(){return Err(error(span,"Source function owner was emitted twice"))}
        let definition=self.templates[instance.template].definition;
        self.ledger.bind_creator(instance.id,definition,node,budget,span)?;
        instance.core=Some((module,node));Ok(())
    }
    pub(crate) fn publish(&mut self,cores:&mut[(&mut CoreModule,&mut zil_base::SourceMap)],budget:&mut Budget<'_>)->R<()> {
        self.ledger.finish(budget,Span::default())?;
        for instance in &self.instances {
            let (module,function)=instance.core.ok_or_else(||error(Span::default(),"Source instance has no actual Core function target"))?;
            let definition=self.templates[instance.template].definition;
            let data=self.modules.iter_mut().find(|(id,_)|*id==instance.id.module).ok_or_else(||error(Span::default(),"semantic owner module is absent"))?;
            let original=data.1.definitions.iter().find(|row|row.id==definition).ok_or_else(||error(Span::default(),"semantic original definition is absent"))?;
            if original.parent.is_some(){return Err(unsupported(Span::default(),"method owner needs its complete actual lexical scope instance"))}
            let mut actuals=Vec::new();
            for actual in &instance.actuals {
                prepay_recipe(actual,budget,Span::default(),0)?;
                budget.lineage_bytes(std::mem::size_of::<SemanticActual>()+std::mem::size_of::<SemanticActualRecipe>(),Span::default())?;
                actuals.push(SemanticActual::CoreRecipe{scope:instance.id,recipe:Box::new(actual.clone())});
            }
            budget.lineage_bytes(std::mem::size_of::<SemanticInstance>(),Span::default())?;
            data.1.instances.push(SemanticInstance{id:instance.id,definition,target:SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{module,function}),lifetime_instantiation:SemanticLifetimeInstantiation::Rigid,parent:None,actuals,header_embedding:Vec::new(),environment:None,self_type:None});
        }
        for (core,_) in cores.iter_mut(){
            let index=self.modules.iter().position(|(module,_)|*module==core.module_id).ok_or_else(||error(Span::default(),"Core output has no complete original semantic module"))?;
            let data=&mut self.modules[index].1;
            core.semantic=Some(SemanticModule{definitions:std::mem::take(&mut data.definitions),instances:std::mem::take(&mut data.instances),sites:std::mem::take(&mut data.sites),dyn_scopes:std::mem::take(&mut data.dyn_scopes)});
        }
        Ok(())
    }
}

fn item_mut<'a>(module:&'a mut s::Module,path:&[u32])->Option<&'a mut s::Item>{
    let (root,tail)=path.split_first()?;let mut item=module.items.get_mut(*root as usize)?;
    for index in tail {let rows=match &mut item.kind{s::ItemKind::Aggregate(a)=>&mut a.methods,s::ItemKind::Trait(t)=>&mut t.members,s::ItemKind::Impl(i)=>&mut i.members,_=>return None};item=rows.get_mut(*index as usize)?;}
    Some(item)
}
pub(crate) fn prepare(
    syntax:&[&s::Module],origins:&[ModuleDeclarationOrigins],imports:&[Vec<CoreImport>],remaining:&mut Limits,
)->CompileResult<(Vec<s::Module>,Program)>{
    let inventory=collect_original_semantics(syntax,origins,imports,remaining)?;
    let limits=remaining.clone();let mut budget=Budget::new(&limits);budget.limit_temporary_bytes(limits.max_source_bytes);
    let result=(||->R<_>{
        for module in syntax {for item in &module.items {budget.item(item)?;}}
        let mut modules=syntax.iter().map(|module|(*module).clone()).collect::<Vec<_>>();
        let mut program=Program{modules:inventory.modules,..Default::default()};
        for row in &inventory.originals.declarations {
            let Some(original)=row.function() else{continue};
            let target=modules.iter_mut().find(|m|m.id==row.module).ok_or_else(||error(row.item.span,"original function module is absent"))?;
            let item=item_mut(target,&row.path).ok_or_else(||error(row.item.span,"actual cloned original function is absent"))?;
            let function=match &mut item.kind{s::ItemKind::Function(f)|s::ItemKind::Extern{function:f,..}=>f,_=>return Err(error(row.item.span,"original function clone changed declaration kind"))};
            let marker=u32::try_from(program.templates.len()).map_err(|_|error(function.span,"original function namespace exhausted"))?;
            if marker>=TEMPLATE-1{return Err(error(function.span,"original function namespace overlaps absent marker"))}
            for p in &original.generics {budget.text(&p.name,p.span)?;}
            budget.function_snapshot(original)?;
            budget.lineage_bytes(std::mem::size_of::<FunctionTemplate>(),function.span)?;
            program.templates.push(FunctionTemplate{definition:SemanticDefinitionId{module:row.module,index:row.declaration.0},parameters:original.generics.clone()});
            function.semantic=s::SemanticContextMarker::untrusted(marker|TEMPLATE);
        }
        Ok((modules,program))
    })();*remaining=budget.remaining_limits();result.map_err(|e|vec![e])
}
