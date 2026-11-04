//! Declaration-local method binders and ordinary generic provider recipes.
use super::*;

pub(super) struct Binding {
    pub(super) original: String,
    pub(super) name: String,
    pub(super) kind: GenericParamKind,
}
pub(super) struct Prepared {
    pub(super) module: Module,
    pub(super) bindings: BTreeMap<(usize,usize),Vec<Binding>>,
}
pub(super) fn prepare(module:&Module,enabled:bool,budget:&mut Budget<'_>)->R<Option<Prepared>>{
    if !enabled{return Ok(None)}
    for item in &module.items{budget.item(item)?;}
    let mut module=module.clone();
    let mut bindings=BTreeMap::new();
    for (owner,item) in module.items.iter_mut().enumerate(){
        let members=match &mut item.kind{ItemKind::Trait(t)=>&mut t.members,ItemKind::Impl(i)=>&mut i.members,_=>continue};
        for (ordinal,member) in members.iter_mut().enumerate(){
            budget.node(member.span,0)?;
            let ItemKind::Function(method)=&member.kind else{continue};
            if method.generics.is_empty(){continue}
            let mut scope=Vec::new();
            let mut substitutions=BTreeMap::new();
            let mut names=BTreeSet::new();
            for (index,parameter) in method.generics.iter().enumerate(){
                budget.node(parameter.span,0)?;
                budget.text(&parameter.name,parameter.span)?;
                if !names.insert(parameter.name.as_str()){return Err(error(parameter.span,"duplicate method binder"))}
                let prefix=match parameter.kind{GenericParamKind::Type(_)=>"type",GenericParamKind::Const(_)=>"const",GenericParamKind::Lifetime=>"life"};
                budget.text("$method_type_00000000000000000000_00000000000000000000_00000000000000000000",parameter.span)?;
                let name=if matches!(parameter.kind,GenericParamKind::Lifetime){parameter.name.clone()}else{format!("$method_{prefix}_{owner}_{ordinal}_{index}")};
                budget.text(&name,parameter.span)?;
                match &parameter.kind{
                    GenericParamKind::Type(bounds)=>{
                        for bound in bounds{budget.ty(bound)?;}
                        substitutions.insert(parameter.name.clone(),named(&name,parameter.span));
                    }
                    GenericParamKind::Const(ty)=>{
                        budget.ty(ty)?;
                        let value=consts::carrier(&Expr{span:parameter.span,kind:ExprKind::Name(name.clone())},budget)?;
                        substitutions.insert(parameter.name.clone(),value);
                    }
                    GenericParamKind::Lifetime=>{},
                }
                scope.push(Binding{original:parameter.name.clone(),name,kind:parameter.kind.clone()});
            }
            budget.substituted_item(member,&substitutions)?;
            consts::prepay_scopes(member,&substitutions,budget)?;
            substitute_item(member,&substitutions)?;
            let ItemKind::Function(method)=&mut member.kind else{unreachable!()};
            for (parameter,binding) in method.generics.iter_mut().zip(&scope){parameter.name=binding.name.clone();}
            bindings.insert((owner,ordinal),scope);
        }
    }
    Ok(Some(Prepared{module,bindings}))
}
