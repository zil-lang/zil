//! Current executable tag observations for selective inline moves.
//! Address origins come from actual definitions and unique local stores.
use super::*;
type Route=Vec<(u8,u32,u32)>;
type Origin=(LocalId,Route);
type Proof=(LocalId,Route,u32);
#[derive(Clone,Debug,Default,PartialEq,Eq)]
pub(super) struct State {reads:BTreeSet<ValueId>, tags:BTreeSet<Proof>}
impl State {
    pub(super) fn cost(&self,b:&mut Budget)->CompileResult<()> {
        pay(b,self.reads.len().saturating_add(self.tags.len()))?;
        for(_,p,_)in &self.tags {pay(b,p.len())?;}
        Ok(())
    }
    pub(super) fn join(&mut self,other:&Self,b:&mut Budget)->CompileResult<bool> {
        self.cost(b)?;other.cost(b)?;
        for _ in &self.reads {search_fee(b,0,other.reads.len())?;}
        for (_,route,_) in &self.tags {search_fee(b,route.len(),other.tags.len())?;}
        let before=(self.reads.len(),self.tags.len());
        self.reads.retain(|v|other.reads.contains(v));
        self.tags.retain(|v|other.tags.contains(v));
        Ok(before!=(self.reads.len(),self.tags.len()))
    }
    pub(super) fn require(&self,local:LocalId,path:&[PlaceProjection],b:&mut Budget)->CompileResult<()> {
        pay(b,path.len().saturating_mul(path.len().saturating_add(self.tags.len()).saturating_add(3)))?;
        let mut prefix=Vec::new();
        for step in path {
            match step {
                PlaceProjection::Variant(p)=>{
                    search_fee(b,prefix.len(),self.tags.len())?;
                    if !self.tags.contains(&(local,prefix.clone(),p.variant)) {return Err(invalid("selective move lacks a current actual tag guard for every prefix"));}
                    prefix.push((1,p.variant,p.index));
                }
                PlaceProjection::Field(i)=>prefix.push((0,*i,0)),
                PlaceProjection::ErrorArm(is_error)=>{
                    search_fee(b,prefix.len(),self.tags.len())?;
                    if !self.tags.contains(&(local,prefix.clone(),u32::from(*is_error))) {return Err(invalid("selective move lacks a current actual error/success tag guard"));}
                    prefix.push((2,u32::from(*is_error),0));
                }
            }
        }
        Ok(())
    }
}
pub(super) struct Meta {
    observes:BTreeMap<ValueId,Origin>,
    tests:BTreeMap<ValueId,(ValueId,u32,bool)>,
}
fn route(path:&[VariantProjection],b:&mut Budget)->CompileResult<Route> {
    pay(b,path.len().saturating_add(1))?;
    Ok(path.iter().map(|p|(1,p.variant,p.index)).collect())
}
fn origin(f:&TypedFunction,defs:&[Option<&InstructionKind>],stores:&[Option<ValueId>],v:ValueId,depth:usize,active:&mut BTreeSet<ValueId>,b:&mut Budget)->CompileResult<Option<Origin>> {
    pay(b,active.len().saturating_add(1))?;
    if depth>b.partial_depth {return Err(exhausted());}
    if !active.insert(v){return Err(invalid("cyclic actual address origin"));}
    let out=match defs.get(v.0 as usize).copied().flatten() {
        Some(InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local})=>Some((*local,Vec::new())),
        Some(InstructionKind::BorrowFieldLocal{local,path,..})=>{pay(b,path.len().saturating_add(1))?;Some((*local,path.iter().map(|i|(0,*i,0)).collect()))},
        Some(InstructionKind::BorrowVariantLocal{local,path,..})=>Some((*local,route(path,b)?)),
        Some(InstructionKind::BorrowPlaceLocal{local,path,..})=>{
            pay(b,path.len().saturating_add(1))?;
            Some((*local,path.iter().map(|p|match p{PlaceProjection::Field(i)=>(0,*i,0),PlaceProjection::Variant(p)=>(1,p.variant,p.index),PlaceProjection::ErrorArm(e)=>(2,u32::from(*e),0)}).collect()))
        },
        Some(InstructionKind::Load(l)) if matches!(f.values.get(v.0 as usize),Some(Type::Address{..}))=>{
            match stores.get(l.0 as usize).copied().flatten() {Some(v)=>origin(f,defs,stores,v,depth+1,active,b)?,None=>None}
        }
        Some(InstructionKind::ReborrowAddress{address,..})=>origin(f,defs,stores,*address,depth+1,active,b)?,
        Some(InstructionKind::AddressField{address,index})=>{
            if let Some((l,mut p))=origin(f,defs,stores,*address,depth+1,active,b)? {if p.len()>=b.partial_depth{return Err(exhausted());}pay(b,1)?;p.push((0,*index,0));Some((l,p))}else{None}
        }
        Some(InstructionKind::BorrowVariantAddress{address,path,..})=>{
            if let Some((l,mut p))=origin(f,defs,stores,*address,depth+1,active,b)? {if p.len().checked_add(path.len()).is_none_or(|n|n>b.partial_depth){return Err(exhausted());}pay(b,path.len())?;p.extend(path.iter().map(|p|(1,p.variant,p.index)));Some((l,p))}else{None}
        }
        _=>None,
    };
    active.remove(&v);
    if out.as_ref().is_some_and(|(_,p)|p.len()>b.partial_depth){return Err(exhausted());}
    Ok(out)
}
impl Meta {
    pub(super) fn new(f:&TypedFunction,b:&mut Budget)->CompileResult<Self> {
        pay(b,f.values.len().saturating_add(f.locals.len().saturating_mul(2)))?;
        let mut defs=vec![None;f.values.len()];let mut stores=vec![None;f.locals.len()];let mut seen=vec![false;f.locals.len()];
        for i in f.blocks.iter().flat_map(|b|&b.instructions) {
            pay(b,1)?;
            if let Some(v)=i.result {defs[v.0 as usize]=Some(&i.kind);}
            if let InstructionKind::Store{local,value}=i.kind {
                if seen[local.0 as usize] {stores[local.0 as usize]=None;}else{seen[local.0 as usize]=true;stores[local.0 as usize]=Some(value);}
            }
        }
        let mut observes=BTreeMap::new();
        for i in f.blocks.iter().flat_map(|b|&b.instructions) {
            pay(b,1)?;
            let Some(v)=i.result else{continue};
            let at=match &i.kind {
                InstructionKind::VariantTagLocal{local,path}=>Some((*local,route(path,b)?)),
                InstructionKind::PlaceTagLocal{local,path}=>{
                    pay(b,path.len().saturating_add(1))?;
                    Some((*local,path.iter().map(|p|match p{PlaceProjection::Field(i)=>(0,*i,0),PlaceProjection::Variant(p)=>(1,p.variant,p.index),PlaceProjection::ErrorArm(e)=>(2,u32::from(*e),0)}).collect()))
                },
                InstructionKind::VariantTagAddress{address,path}=>{
                    if let Some((l,mut p))=origin(f,&defs,&stores,*address,0,&mut BTreeSet::new(),b)? {if p.len().checked_add(path.len()).is_none_or(|n|n>b.partial_depth){return Err(exhausted());}pay(b,path.len())?;p.extend(path.iter().map(|p|(1,p.variant,p.index)));Some((l,p))}else{None}
                }
                _=>None,
            };
            if let Some(at)=at {pay(b,observes.len().saturating_add(1))?;observes.insert(v,at);}
        }
        let mut tests=BTreeMap::new();
        for i in f.blocks.iter().flat_map(|b|&b.instructions) {
            pay(b,1)?;
            if let (Some(out),InstructionKind::Binary{op,left,right})=(i.result,&i.kind)
                && matches!(op,BinaryOp::Equal|BinaryOp::NotEqual)
                && f.values[out.0 as usize]==Type::Bool {
                for(tag,c)in[(*left,*right),(*right,*left)] {
                    pay(b,observes.len().saturating_add(1))?;
                    if observes.contains_key(&tag) && f.values[tag.0 as usize]==Type::I32 && f.values[c.0 as usize]==Type::I32
                        && let Some(InstructionKind::Constant(Constant::I32(n)))=defs[c.0 as usize] && *n>=0 {
                        pay(b,tests.len().saturating_add(1))?;tests.insert(out,(tag,*n as u32,*op==BinaryOp::Equal));
                    }
                }
            }
        }
        Ok(Self{observes,tests})
    }
    pub(super) fn step(&self,state:&mut State,i:&Instruction,b:&mut Budget)->CompileResult<()> {
        pay(b,1)?;
        let pure=!invalidates(&i.kind);
        if !pure {state.cost(b)?;state.reads.clear();state.tags.clear();}
        if let Some(v)=i.result && self.observes.contains_key(&v) {pay(b,state.reads.len().saturating_add(1))?;state.reads.insert(v);}
        Ok(())
    }
    pub(super) fn edge(&self,state:&mut State,term:&Terminator,next:usize,b:&mut Budget)->CompileResult<Option<(LocalId,Vec<PlaceProjection>,u32,bool)>> {
        let Terminator::Branch{condition,then_block,else_block}=term else{return Ok(None)};
        if then_block==else_block{return Ok(None)};
        let Some((tag,n,equal))=self.tests.get(condition)else{return Ok(None)};
        if !state.reads.contains(tag){return Ok(None)};
        let Some((local,path))=self.observes.get(tag)else{return Ok(None)};
        let want=(then_block.0 as usize==next)==*equal;
        pay(b,path.len().saturating_mul(2).saturating_add(state.tags.len()).saturating_add(1))?;
        if want {search_fee(b,path.len(),state.tags.len())?;state.tags.insert((*local,path.clone(),*n));}
        let path=path.iter().map(|(k,a,c)|match k {0=>PlaceProjection::Field(*a),1=>PlaceProjection::Variant(VariantProjection{variant:*a,index:*c}),_=>PlaceProjection::ErrorArm(*a!=0)}).collect();
        Ok(Some((*local,path,*n,want)))
    }
}

fn pay(b:&mut Budget,n:usize)->CompileResult<()> {b.spend(n,"partial place tag/origin proof")}
fn invalid(m:&str)->Vec<Diagnostic>{error(m)}
fn exhausted()->Vec<Diagnostic>{vec![Diagnostic::limit(Phase::Analyze,"partial origin depth exceeds selected limit",None)]}

fn search_fee(b:&mut Budget,path:usize,count:usize)->CompileResult<()> {
    let factor=(count.checked_ilog2().unwrap_or(0) as usize+2)*8;
    let fee=path.checked_add(2).and_then(|n|n.checked_mul(factor)).ok_or_else(exhausted)?;
    pay(b,fee)
}

pub(super) fn invalidates(kind:&InstructionKind)->bool {
    !matches!(kind,
            InstructionKind::Constant(_)|InstructionKind::Binary{..}|InstructionKind::Unary{..}|InstructionKind::ConvertInteger{..}|
            InstructionKind::Load(_)|InstructionKind::ReadAddress{..}|InstructionKind::BorrowLocal{..}|InstructionKind::BorrowLocalMut{..}|
            InstructionKind::BorrowPlaceLocal{..}|InstructionKind::PlaceTagLocal{..}|InstructionKind::BorrowFieldLocal{..}|InstructionKind::VariantTagLocal{..}|InstructionKind::ReadVariantLocal{..}|InstructionKind::BorrowVariantLocal{..}|
            InstructionKind::VariantTagAddress{..}|InstructionKind::BorrowVariantAddress{..}|InstructionKind::ReborrowAddress{..}|InstructionKind::AddressField{..}|
            InstructionKind::MakeAggregate{..}|InstructionKind::MakeManaged{..}|InstructionKind::MakePackage{..}|InstructionKind::MakeVariant{..}|InstructionKind::MakeOwnedTuple{..}|
            InstructionKind::TakeTupleField{..}|InstructionKind::TakeVariantField{..}|InstructionKind::ScopeEnter{..}|InstructionKind::ScopeExit{..})
}
