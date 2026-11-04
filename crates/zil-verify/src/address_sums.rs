//! Independent addressed-Sum tag checker. Facts refer to the actual address SSA
//! identity and are must facts of the current CFG epoch, never copied values.
use super::*;
use std::collections::{BTreeMap,BTreeSet,VecDeque};
type Prefix=Vec<(u32,u32)>;
type Selection=(ValueId,Prefix,u32);
#[derive(Clone,Default,PartialEq,Eq)]
struct Epoch{selected:BTreeSet<Selection>,reads:BTreeSet<ValueId>}
fn invalid(span:Option<Span>)->Diagnostic{reject("E-VERIFY-SUM-ADDRESS",span,"addressed payload requires a current real-address tag proof")}
fn endpoint<'a>(f:&'a TypedFunction,address:ValueId,path:&[VariantProjection],budget:&mut Budget)->Check<(bool,&'a Type)>{
 let Type::Address{mutable,inner}=value_type(f,address,None)?else{return Err(invalid(None))};
 let mut cursor=inner.as_ref();budget.spend(path.len().saturating_add(1))?;
 for projection in path{let Type::Enum{variants,..}=cursor else{return Err(invalid(None))};cursor=variants.get(projection.variant as usize).and_then(|(_,fields)|fields.get(projection.index as usize)).ok_or_else(||invalid(None))?;}
 Ok((*mutable,cursor))
}
pub(super) fn instruction(module:&TypedModule,f:&TypedFunction,i:&Instruction,budget:&mut Budget)->Check<Option<Type>>{
 let span=Some(i.span);
 if let InstructionKind::BorrowFieldLocal{local,path,mutable}=&i.kind{
  let declaration=f.locals.get(local.0 as usize).ok_or_else(||invalid(span))?;let mut ty=&declaration.ty;
  resources::charge_payload(ty,budget)?;if path.is_empty()||!ty.is_copy_value()||(*mutable&&!declaration.mutable){return Err(invalid(span))}
  for field in path{budget.spend(1)?;ty=match ty{Type::Tuple(fields)=>fields.get(*field as usize),Type::Record{fields,..}=>fields.get(*field as usize).map(|(_,ty)|ty),_=>None}.ok_or_else(||invalid(span))?;}
  resources::charge_payload(ty,budget)?;check_type(ty,span)?;return Ok(Some(Type::Address{mutable:*mutable,inner:Box::new(ty.clone())}))
 }
 let(address,path)=match &i.kind{InstructionKind::VariantTagAddress{address,path}|InstructionKind::BorrowVariantAddress{address,path,..}=>(*address,path),_=>unreachable!()};
 let(available,ty)=endpoint(f,address,path,budget)?;resources::charge_payload(ty,budget)?;
 if let InstructionKind::BorrowVariantAddress{mutable,..}=&i.kind{
  if path.is_empty()||(*mutable&&!available)||(!ty.is_copy_value()&&!matches!(ty,Type::Package{..}|Type::Enum{..})){return Err(invalid(span))}
  match ty{Type::Package{..}=>packages::check_ordinary_type(module,ty,span,budget)?,Type::Enum{..}=>sums::check_type_for(module,ty,span,budget)?,_=>check_type(ty,span)?}
  Ok(Some(Type::Address{mutable:*mutable,inner:Box::new(ty.clone())}))
 }else{
  if !matches!(ty,Type::Enum{..}){return Err(invalid(span))}
  sums::check_type_for(module,ty,span,budget)?;Ok(Some(Type::I32))
 }
}
fn cost(e:&Epoch,b:&mut Budget)->Check<()>{b.spend(e.selected.iter().map(|(_,p,_)|p.len()+1).sum::<usize>().saturating_add(e.reads.len()).saturating_add(1))}
fn copy(e:&Epoch,b:&mut Budget)->Check<Epoch>{cost(e,b)?;Ok(e.clone())}
fn validate_path(e:&Epoch,address:ValueId,path:&[VariantProjection],span:Span,b:&mut Budget)->Check<()>{
 let mut route=Vec::new();for step in path{b.spend(e.selected.len().saturating_add(route.len()).saturating_add(1))?;if !e.selected.contains(&(address,route.clone(),step.variant)){return Err(invalid(Some(span)))}route.push((step.variant,step.index));}Ok(())
}
fn mutates(i:&InstructionKind)->bool{
 !matches!(i,InstructionKind::Constant(_)|InstructionKind::Unary{..}|InstructionKind::Binary{..}|InstructionKind::ConvertInteger{..}|InstructionKind::Load(_)|InstructionKind::ReadCapture{..}|InstructionKind::ReadAddress{..}|InstructionKind::AddressField{..}|InstructionKind::AddressIndex{..}|InstructionKind::AddressSlice{..}|InstructionKind::ReborrowAddress{..}|InstructionKind::BorrowLocal{..}|InstructionKind::BorrowVariantLocal{mutable:false,..}|InstructionKind::BorrowFieldLocal{mutable:false,..}|InstructionKind::VariantTagLocal{..}|InstructionKind::ReadVariantLocal{..})
}

fn transfer(f:&TypedFunction,block:usize,epoch:&mut Epoch,validate:bool,b:&mut Budget)->Check<()>{
 for i in &f.blocks[block].instructions{b.spend(1)?;match &i.kind{
  InstructionKind::VariantTagAddress{address,path}=>{if validate{validate_path(epoch,*address,path,i.span,b)?}b.spend(1)?;epoch.reads.insert(i.result.unwrap());},
  InstructionKind::BorrowVariantAddress{address,path,mutable}=>{if validate{validate_path(epoch,*address,path,i.span,b)?}let(_,ty)=endpoint(f,*address,path,b)?;if *mutable&&!ty.is_b0_scalar(){epoch.selected.clear();epoch.reads.clear();}},
  other if mutates(other)=>{epoch.selected.clear();epoch.reads.clear();},_=>{},
 }}Ok(())
}
pub(super) fn check(f:&TypedFunction,graph:&FunctionGraph,b:&mut Budget)->Check<()>{
 let mut observations=BTreeMap::new();let mut definitions=BTreeMap::new();
 // Work is already preflight-selected for functions with an addressed Sum op.
 b.spend(f.values.len().saturating_add(f.blocks.len().saturating_mul(2)))?;
 for block in &f.blocks{for i in &block.instructions{b.spend(1)?;if let Some(v)=i.result{definitions.insert(v,&i.kind);if let InstructionKind::VariantTagAddress{address,path}=&i.kind{b.spend(path.len())?;observations.insert(v,(*address,path));}}}}
 let mut incoming=vec![None;f.blocks.len()];incoming[f.entry.0 as usize]=Some(Epoch::default());
 let mut todo=VecDeque::from([f.entry.0 as usize]);
 while let Some(block)=todo.pop_front(){let mut output=copy(incoming[block].as_ref().unwrap(),b)?;transfer(f,block,&mut output,false,b)?;
  let edges=match f.blocks[block].terminator{Terminator::Branch{then_block,else_block,..}=>vec![(then_block.0 as usize,Some(true)),(else_block.0 as usize,Some(false))],_=>graph.successors[block].iter().map(|next|(*next,None)).collect()};
  for (successor,truth) in edges{let mut edge=copy(&output,b)?;
   if let Terminator::Branch{condition,..}=f.blocks[block].terminator{
    if let Some(InstructionKind::Binary{op,left,right})=definitions.get(&condition).copied(){
     if matches!(op,BinaryOp::Equal|BinaryOp::NotEqual){for(tag,literal)in[(*left,*right),(*right,*left)]{
      if let(Some((address,path)),Some(InstructionKind::Constant(Constant::I32(variant))))=(observations.get(&tag),definitions.get(&literal).copied()){
       if *variant>=0 && output.reads.contains(&tag) && truth==Some(*op==BinaryOp::Equal){let route=path.iter().map(|p|(p.variant,p.index)).collect::<Vec<_>>();b.spend(route.len().saturating_add(1))?;edge.selected.insert((*address,route,*variant as u32));}
      }
     }}
    }
   }
   let changed=if let Some(prior)=&mut incoming[successor]{cost(prior,b)?;cost(&edge,b)?;let lengths=(prior.selected.len(),prior.reads.len());prior.selected.retain(|f|edge.selected.contains(f));prior.reads.retain(|r|edge.reads.contains(r));lengths!=(prior.selected.len(),prior.reads.len())}else{incoming[successor]=Some(edge);true};
   if changed{todo.push_back(successor)}
  }
 }
 for(index,live)in graph.reachable.iter().enumerate(){if *live{let mut epoch=copy(incoming[index].as_ref().ok_or_else(||invalid(None))?,b)?;transfer(f,index,&mut epoch,true,b)?}}
 Ok(())
}
