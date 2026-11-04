//! Reference bindings are addresses of real storage, never copies of a payload.
use super::*;
pub(super) fn has_reference(p: &CorePattern) -> bool {
    match &p.kind {
        CorePatternKind::Reference { .. } => true,
        CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => fields.iter().any(has_reference),
        _ => false,
    }
}
impl Builder<'_, '_> {
    pub(super) fn reference_binding(&mut self, p: &CorePattern, mutable: bool, expected: Option<&Type>, e: &CoreExpr) -> R<()> {
        let place = if matches!(e.kind, CoreExprKind::Name(_) | CoreExprKind::Member { .. } | CoreExprKind::Index { .. } | CoreExprKind::Unary { op: UnaryOp::Dereference, .. }) {
            e.clone()
        } else {
            let (v,t)=self.transfer_expr(e,expected)?;
            let name=format!("$pattern340${}",self.function.locals.len());
            let local=self.local(&name,t,true,e.span)?;
            self.store(local,v,e.span)?;
            CoreExpr { kind: CoreExprKind::Name(name), ..e.clone() }
        };
        let mut bindings=Vec::new();
        self.binding_place(p,&place,expected,&mut bindings)?;
        for (name,value,ty,span) in bindings {
            let local=self.local(&name,ty,mutable,span)?;self.store(local,value,span)?;
        }
        Ok(())
    }
    fn binding_place(&mut self,p:&CorePattern,e:&CoreExpr,expected:Option<&Type>,bindings:&mut Vec<(String,ValueId,Type,Span)>)->R<()> {
        self.closures.snapshot(1,p.span)?;
        match &p.kind {
            CorePatternKind::Reference { mutable, pattern } => {
                let (value,ty)=self.borrow_place_mode(e,*mutable)?;
                let Type::Address { inner,.. }=&ty else {unreachable!()};
                if let Some(wanted)=expected {self.same(inner,wanted,e.span)?;}
                let fail=self.current.unwrap();self.pattern_test(pattern,value,&ty,fail,bindings)
            }
            CorePatternKind::Tuple(fields) => {
                let ty=self.known_type(e).ok_or_else(||error("E-PATTERN-TYPE",e.span,"tuple reference pattern needs a known tuple place"))?;
                let Type::Tuple(types)=&ty else {return Err(error("E-MATCH-TYPE",p.span,"tuple pattern requires a tuple"))};
                if fields.len()!=types.len(){return Err(error("E-MATCH-ARITY",p.span,"tuple pattern arity mismatch"));}
                for (index,(p,ty)) in fields.iter().zip(types).enumerate(){
                    let child=CoreExpr{kind:CoreExprKind::Member{object:Box::new(e.clone()),member:index.to_string(),owner:None},..e.clone()};
                    self.binding_place(p,&child,Some(ty),bindings)?;
                }
                Ok(())
            }
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => {
                let (v,t)=self.expr(e,expected)?;
                let fail=self.current.unwrap();self.pattern_test(p,v,&t,fail,bindings)
            }
            _=>Err(error("E-PATTERN-REFUTABLE",p.span,"ordinary binding requires an irrefutable pattern")),
        }
    }
    pub(super) fn reference_match(&mut self,e:&CoreExpr,arms:&[CoreMatchArm],span:Span)->R<()> {
        let (root,ty)=if let CoreExprKind::Name(name)=&e.kind {
            let local=self.lookup(name).ok_or_else(||error("E-UNKNOWN-NAME",e.span,"unknown match place"))?;
            (local,self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (v,t)=self.transfer_expr(e,None)?;
            let local=self.temp(t.clone(),e.span)?;self.store(local,v,e.span)?;(local,t)
        };
        self.check_match_coverage(&ty,arms,span)?;
        let mut ends=Vec::new();
        for arm in arms {
            let fail=self.block_id()?;let mut bindings=Vec::new();
            self.reference_local_test(&arm.pattern,root,&mut Vec::new(),&ty,fail,&mut bindings)?;
            self.push_scope(arm.span)?;
            for(name,v,t,span)in bindings{let local=self.local(&name,t,false,span)?;self.store(local,v,span)?;}
            self.block(&arm.body)?;
            if self.current.is_some(){self.end_scopes(self.scopes.len()-1,arm.span)?;}
            if let Some(end)=self.current.take(){ends.push(end)}
            self.pop_scope();self.current=Some(fail);
        }
        self.terminate(Terminator::Trap);
        if !ends.is_empty(){let join=self.block_id()?;for end in ends{self.function.blocks[end.0 as usize].terminator=Terminator::Jump(join)}self.current=Some(join)}
        Ok(())
    }
    fn reference_local_test(&mut self,p:&CorePattern,root:LocalId,path:&mut Vec<VariantProjection>,ty:&Type,fail:BlockId,bindings:&mut Vec<(String,ValueId,Type,Span)>)->R<()> {
        self.closures.snapshot(path.len().saturating_add(1),p.span)?;
        match &p.kind {
            CorePatternKind::Reference {mutable,pattern} => {
                if !matches!(pattern.kind,CorePatternKind::Name(_) | CorePatternKind::Wildcard){return Err(error("E-PATTERN-REFERENCE",p.span,"reference binding requires a name or discard"));}
                if *mutable && !self.function.locals[root.0 as usize].mutable{return Err(error("E-IMMUTABLE-ASSIGNMENT",p.span,"exclusive pattern requires a mutable place"));}
                let t=Type::Address{mutable:*mutable,inner:Box::new(ty.clone())};
                let kind=if path.is_empty(){if *mutable {InstructionKind::BorrowLocalMut{local:root}}else{InstructionKind::BorrowLocal{local:root}}}else{InstructionKind::BorrowVariantLocal{local:root,path:path.clone(),mutable:*mutable}};
                let v=self.value(kind,t.clone(),p.span)?;self.pattern_test(pattern,v,&t,fail,bindings)
            }
            CorePatternKind::Variant{variant,fields,..}=>{
                let Type::Enum{variants,..}=ty else{unreachable!("coverage checked")};
                let(index,(_,types))=variants.iter().enumerate().find(|(_, (n,_))|n==variant).unwrap();
                let tag=self.value(InstructionKind::VariantTagLocal{local:root,path:path.clone()},Type::I32,p.span)?;
                self.pattern_branch(tag,Constant::I32(index as i32),fail,p.span)?;
                for(field,(p,ty))in fields.iter().zip(types).enumerate(){path.push(VariantProjection{variant:index as u32,index:field as u32});self.reference_local_test(p,root,path,ty,fail,bindings)?;path.pop();}Ok(())
            }
            CorePatternKind::Wildcard=>Ok(()),
            _ if ty.is_copy_value()=>{
                let kind=if path.is_empty(){InstructionKind::Load(root)}else{InstructionKind::ReadVariantLocal{local:root,path:path.clone()}};
                let v=self.value(kind,ty.clone(),p.span)?;self.pattern_test(p,v,ty,fail,bindings)
            }
            _=>Err(error("E-PATTERN-MOVE-BORROW",p.span,"a borrowed match payload requires an explicit reference or discard")),
        }
    }
}
