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
    pub(crate) fn canonical_origin(&self,origin:&IntrinsicOrigin,b:&mut Budget)->Check<IntrinsicOrigin> {
        nominal_origins::charge_origin(origin,b)?;
        // A Life image selected through an intrinsic occurrence is still an
        // outer Life source, not the intrinsic κ itself.
        let mut current=origin.clone();
        let aliases=self.origin_aliases.borrow();
        let mut seen=Vec::new();
        loop {
            b.spend(seen.len().saturating_add(1))?;
            if seen.contains(&current){return Err(bad("cyclic semantic intrinsic occurrence substitution"));}
            let Some(next)=aliases.get(&current) else {return Ok(current);};
            seen.push(current);current=next.clone();
        }
    }
    pub(crate) fn same_origin(&self,environment:&EvalEnv,left:&IntrinsicOrigin,right:&IntrinsicOrigin,b:&mut Budget)->Check<bool> {
        let left=self.canonical_origin(left,b)?;let right=self.canonical_origin(right,b)?;
        match (&left,&right){
            (IntrinsicOrigin::Nominal{source:a,class:x},IntrinsicOrigin::Nominal{source:z,class:y})=>{
                nominal_origins::charge_origin(&left,b)?;nominal_origins::charge_origin(&right,b)?;
                if x!=y||a.declaration!=z.declaration||a.domain!=z.domain||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Ok(false);}
                for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){
                    let equal=match(a,z){(CheckedActual::Life(a),CheckedActual::Life(z))=>self.same(environment,a,z,b)?,(CheckedActual::Type(a),CheckedActual::Type(z))=>self.same_type(environment,a,z,b)?,(CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})=>av==zv&&self.same_type(environment,a,z,b)?,_=>false};if !equal{return Ok(false);}
                }
                Ok(true)
            },
            (IntrinsicOrigin::Existing{..},IntrinsicOrigin::Existing{..})=>Ok(left==right),
            (IntrinsicOrigin::DerivedClass{source:a},IntrinsicOrigin::DerivedClass{source:z})=>self.same(environment,a,z,b),
            // A type-domain κ is not an outer Life identity. Only an exact
            // checked occurrence alias above can connect their representations.
            _=>Ok(false),
        }
    }
}
