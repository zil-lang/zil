//! Only original equality premises enter this graph. Callable/variance results
//! are retained as role requests and never become global lifetime equalities.
use super::*;
impl CheckedContext {
    fn scope_keys(environment: &EvalEnv) -> impl Iterator<Item = ScopeKey> + '_ {
        environment.assumptions.iter().copied()
    }
    pub(crate) fn same(&self, environment: &EvalEnv, left: &ClassKey, right: &ClassKey, b: &mut Budget) -> Check<bool> {
        self.relation(environment, left, right, false, b)
    }
    pub(crate) fn outlives(&self, environment: &EvalEnv, longer: &ClassKey, shorter: &ClassKey, b: &mut Budget) -> Check<bool> {
        self.relation(environment, longer, shorter, true, b)
    }
    fn relation(&self, environment: &EvalEnv, left: &ClassKey, right: &ClassKey, directed: bool, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        if left == right { return Ok(true); }
        let graph = self.premises.borrow();
        let mut pending = vec![left.clone()];
        let mut seen = Vec::new();
        while let Some(current) = pending.pop() {
            b.spend(seen.len().saturating_add(1))?;
            if seen.contains(&current) { continue; }
            for key in Self::scope_keys(environment) {
                if let Some(rows) = graph.get(&key) {
                    b.spend(rows.relations.equalities.len().saturating_add(rows.relations.outlives.len()))?;
                    for (a,c) in &rows.relations.equalities {
                        if a == &current { if c == right { return Ok(true); } pending.push(c.clone()); }
                        if c == &current { if a == right { return Ok(true); } pending.push(a.clone()); }
                    }
                    if directed {
                        for (a,c) in &rows.relations.outlives {
                            if a == &current { if c == right { return Ok(true); } pending.push(c.clone()); }
                        }
                    }
                }
            }
            seen.push(current);
        }
        Ok(false)
    }
    pub(crate) fn same_type(&self, environment: &EvalEnv, left: &CheckedType, right: &CheckedType, b: &mut Budget) -> Check<bool> {
        dynamic_metadata::ty(&left.ty,b)?;
        dynamic_metadata::ty(&right.ty,b)?;
        b.spend(left.classes.len().saturating_add(right.classes.len()).saturating_add(left.statics.len()).saturating_add(right.statics.len()))?;
        // Existing domains keep cardinality and occurrence identity, including
        // repeated outside images. Only a fresh construction may canonicalize.
        if left.ty != right.ty || left.life != right.life || left.classes.len() != right.classes.len() || left.origins.len()!=left.classes.len() || right.origins.len()!=right.classes.len() || left.statics != right.statics { return Ok(false); }
        for (a,c) in left.classes.iter().zip(&right.classes) {
            if !self.same(environment,a,c,b)? { return Ok(false); }
        }
        for (a,c) in left.origins.iter().zip(&right.origins) {
            if !self.same_origin(environment,a,c,b)? {return Ok(false);}
        }
        Ok(true)
    }
    fn canonical_origin(&self,origin:&IntrinsicOrigin,b:&mut Budget)->Check<IntrinsicOrigin> {
        let mut current=match origin {
            IntrinsicOrigin::DerivedClass{source:ClassKey::Intrinsic{scope,operand,path,class}}=>IntrinsicOrigin::Existing{scope:*scope,operand:operand.clone(),path:path.clone(),class:*class},
            origin=>origin.clone(),
        };
        let aliases=self.origin_aliases.borrow();
        let mut seen=Vec::new();
        loop {
            b.spend(seen.len().saturating_add(1))?;
            if seen.contains(&current){return Err(bad("cyclic semantic intrinsic occurrence substitution"));}
            let Some(next)=aliases.get(&current) else {return Ok(current);};
            seen.push(current);current=next.clone();
        }
    }
    pub(super) fn same_origin(&self,environment:&EvalEnv,left:&IntrinsicOrigin,right:&IntrinsicOrigin,b:&mut Budget)->Check<bool> {
        let left=self.canonical_origin(left,b)?;let right=self.canonical_origin(right,b)?;
        if matches!((&left,&right),(IntrinsicOrigin::Existing{..},IntrinsicOrigin::Existing{..})) {return Ok(left==right);}
        let key=|origin:IntrinsicOrigin|match origin {
            IntrinsicOrigin::DerivedClass{source}=>source,
            IntrinsicOrigin::Existing{scope,operand,path,class}=>ClassKey::Intrinsic{scope,operand,path,class},
        };
        // Compare original source identities under original Γ, never the
        // already collapsed outside images held in CheckedType.classes.
        self.same(environment,&key(left),&key(right),b)
    }
}
