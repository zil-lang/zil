//! Only fresh call inference variables are solved. Declaration lifetimes keep
//! their rigid identity; source equality premises are queried, never invented.
use super::*;
impl CheckedOriginal {
    pub(super) fn fresh_life(&mut self,b:&mut Budget)->Check<Life>{b.spend(1)?;let id=self.next_inference;self.next_inference=id.checked_add(1).ok_or_else(||limit("original life inference identity exhausted"))?;Ok(Life::Infer(id))}
    pub(super) fn resolved_life(&self,life:&Life,b:&mut Budget)->Check<Life>{
        let mut current=life;let mut seen=BTreeSet::new();
        while let Life::Infer(id)=current{b.spend(1)?;if !seen.insert(*id){return Err(bad("original lifetime inference cycle"));}match self.life_substitutions.get(id){Some(value)=>current=value,None=>break}}
        b.spend(1)?;Ok(current.clone())
    }
    pub(super) fn same_life(&self,left:&Life,right:&Life,b:&mut Budget)->Check<bool>{
        let left=self.resolved_life(left,b)?;let right=self.resolved_life(right,b)?;if left==right{return Ok(true);}
        let equations=self.current.and_then(|id|self.life_equalities.get(&id)).map(Vec::as_slice).unwrap_or(&[]);
        let mut pending=vec![left];let mut visited=BTreeSet::new();
        while let Some(value)=pending.pop(){b.spend(equations.len().saturating_add(1))?;if !visited.insert(value.clone()){continue;}for(a,c)in equations{let a=self.resolved_life(a,b)?;let c=self.resolved_life(c,b)?;if a==value{if c==right{return Ok(true);}pending.push(c);}else if c==value{if a==right{return Ok(true);}pending.push(a);}}}Ok(false)
    }
    pub(super) fn unify_life(&mut self,left:&Life,right:&Life,b:&mut Budget)->Check<()> {
        let left=self.resolved_life(left,b)?;let right=self.resolved_life(right,b)?;if self.same_life(&left,&right,b)?{return Ok(());}
        match(&left,&right){(Life::Infer(id),value)|(value,Life::Infer(id))=>{b.spend(1)?;self.life_substitutions.insert(*id,value.clone());Ok(())},_=>Err(bad("original call relates independent rigid lifetimes without an original equality"))}
    }
}
