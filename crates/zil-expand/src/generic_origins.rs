//! Private declaration provenance assigned by actual module traversal. Spans
//! and generated spellings are never owner authority.
use super::*;
use zil_base::ModuleId;

#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct DeclId{pub(super) module:ModuleId,pub(super) ordinal:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct MemberId{pub(super) declaration:DeclId,pub(super) ordinal:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Origin{pub(super) declaration:DeclId,pub(super) member:Option<u32>,pub(super) provider:bool,pub(super) compiler:bool}
#[derive(Default)]
pub(super) struct Origins{pub(super) entries:Vec<Origin>}
impl Origin{
    pub(super) fn source(module:ModuleId,ordinal:usize,span:Span)->R<Self>{Ok(Self{declaration:DeclId{module,ordinal:u32::try_from(ordinal).map_err(|_|Diagnostic::limit(Phase::Expand,"declaration identity ordinal exhausted",Some(span)))?},member:None,provider:false,compiler:false})}
    pub(super) fn member(self,ordinal:usize,span:Span)->R<Self>{Ok(Self{member:Some(u32::try_from(ordinal).map_err(|_|Diagnostic::limit(Phase::Expand,"member identity ordinal exhausted",Some(span)))?),provider:false,..self})}
    pub(super) fn id(self,span:Span)->R<MemberId>{Ok(MemberId{declaration:self.declaration,ordinal:self.member.ok_or_else(||error(span,"method source member identity absent"))?})}
}
impl Origins{
    pub(super) fn single(module:&Module,inserted_option:bool,budget:&mut Budget<'_>)->R<Self>{
        let mut entries=Vec::new();
        for (index,item) in module.items.iter().enumerate(){budget.node(item.span,0)?;let mut origin=Origin::source(module.id,index.saturating_sub(usize::from(inserted_option)),item.span)?;if inserted_option&&index==0{origin.compiler=true;}entries.push(origin);}
        Ok(Self{entries})
    }
    pub(super) fn insert_option(&mut self,module:&Module,budget:&mut Budget<'_>)->R<()>{
        if self.entries.len()+1!=module.items.len(){return Err(error(module.span,"Option provenance shape differs"));}
        for _ in 0..=self.entries.len(){budget.node(module.span,0)?;}
        let mut origin=Origin::source(module.id,0,module.span)?;origin.compiler=true;self.entries.insert(0,origin);Ok(())
    }
    pub(super) fn aliases(&mut self,module:&Module,budget:&mut Budget<'_>)->R<()>{
        if self.entries.len()!=module.items.len(){return Err(error(module.span,"alias provenance shape differs"));}
        let mut retained=Vec::new();for (item,origin) in module.items.iter().zip(std::mem::take(&mut self.entries)){budget.node(item.span,0)?;if !matches!(item.kind,ItemKind::TypeAlias(_)){retained.push(origin);}}
        self.entries=retained;Ok(())
    }
    pub(super) fn get(&self,index:usize,span:Span)->R<Origin>{self.entries.get(index).copied().ok_or_else(||error(span,"declaration provenance absent"))}
}
pub(super) struct Pending{pub(super) item:Item,pub(super) origin:Option<Origin>}
impl Engine<'_>{
    pub(super) fn enqueue(&mut self,item:Item,origin:Option<Origin>)->R<()>{
        if self.origins.is_some(){self.budget.node(item.span,0)?;if origin.is_none(){return Err(error(item.span,"generated declaration provenance absent"));}}
        self.pending.push_back(Pending{item,origin});Ok(())
    }
    pub(super) fn template_origin(&mut self,item:&Item)->R<Option<Origin>>{self.traits.origin(item,&mut self.budget)}
}
