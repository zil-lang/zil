//! Scope interpretation never reads a same-numbered operand in another frame.
use super::*;
impl CheckedContext {
    pub(super) fn frame<'a>(&self, environment: &'a EvalEnv, scope: SemanticScopeRef, b: &mut Budget) -> Check<&'a LexicalFrame> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        b.spend(depth as usize + 1)?;
        environment.lexical.get(depth as usize).ok_or_else(|| bad("semantic scope escapes its actual lexical parent chain"))
    }
    pub(super) fn formal<'a>(&self, environment: &'a EvalEnv, reference: &SemanticFormalRef, b: &mut Budget) -> Check<&'a CheckedActual> {
        self.frame(environment,reference.scope,b)?.actuals.get(reference.ordinal as usize).ok_or_else(|| bad("semantic formal full ordinal is absent"))
    }
    pub(super) fn environment_actual<'a>(&self, environment: &'a EvalEnv, scope: SemanticScopeRef, ordinal: u32, b: &mut Budget) -> Check<&'a CheckedActual> {
        self.frame(environment,scope,b)?.environment.get(ordinal as usize).ok_or_else(|| bad("semantic environment slot is absent or not sealed"))
    }
    pub(super) fn life(&self, module: &TypedModule, recipe: &SemanticLifeRecipe, environment: &EvalEnv, b: &mut Budget) -> Check<ClassKey> {
        b.spend(1)?;
        let key=match recipe {
            SemanticLifeRecipe::Formal(reference) => match self.formal(environment,reference,b)? {
                CheckedActual::Life(key) => Ok(key.clone()), _ => Err(bad("lifetime recipe selects a non-Life formal")),
            },
            SemanticLifeRecipe::Environment{scope,ordinal} => match self.environment_actual(environment,*scope,*ordinal,b)? {
                CheckedActual::Life(key) => Ok(key.clone()), _ => Err(bad("lifetime recipe selects a non-Life sealed slot")),
            },
            SemanticLifeRecipe::Implicit{scope,ordinal} => {
                let frame = self.frame(environment,*scope,b)?;
                let definition = self.definition(module,frame.definition,b)?;
                b.spend(definition.implicit_lifetimes.len())?;
                if !definition.implicit_lifetimes.iter().any(|row| row.ordinal == *ordinal) { return Err(bad("implicit lifetime has no original occurrence")); }
                Ok(ClassKey::Implicit{scope:frame.key,ordinal:*ordinal})
            }
            SemanticLifeRecipe::Bound{depth,index} => environment.bounds.get(*depth as usize).and_then(|frame|frame.classes.get(*index as usize)).cloned().ok_or_else(||bad("Bound lifetime escapes the real nonzero quantifier frames")),
            SemanticLifeRecipe::Invocation(index) => environment.invocations.get(*index as usize).cloned().ok_or_else(||bad("Invocation lifetime has no current code scope")),
            SemanticLifeRecipe::OuterOccurrence{operand,path,mode} => {
                let (scope,operand) = self.recipe_operand(environment,operand,b)?;
                self.occurrence(module,environment,scope,operand,path,Some(*mode),None,b)
            }
            SemanticLifeRecipe::IntrinsicClass{operand,path,class} => {
                let (scope,operand) = self.recipe_operand(environment,operand,b)?;
                self.occurrence(module,environment,scope,operand,path,None,Some(*class),b)
            }
        }?;
        b.spend(environment.images.len())?;
        Ok(environment.images.iter().find(|(source,_)|source==&key).map_or(key.clone(),|(_,image)|image.clone()))
    }
    pub(super) fn recipe_operand(&self, environment:&EvalEnv, operand:&SemanticRecipeOperand,b:&mut Budget)->Check<(ScopeKey,OperandKey)> {
        let current = self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        Ok(match operand {
            SemanticRecipeOperand::TypeFormal(reference) => {
                let frame=self.frame(environment,reference.scope,b)?;
                match frame.actuals.get(reference.ordinal as usize) { Some(CheckedActual::Type(_))=>{},_=>return Err(bad("class operand selects a non-Type formal")) }
                (frame.key,OperandKey::TypeArgument{scope:frame.key,ordinal:reference.ordinal})
            }
            SemanticRecipeOperand::Environment{scope,ordinal} => {
                let frame=self.frame(environment,*scope,b)?;
                match frame.environment.get(*ordinal as usize) { Some(CheckedActual::Type(_))=>{},_=>return Err(bad("class operand selects a non-Type sealed slot")) }
                (frame.key,OperandKey::Environment{scope:frame.key,ordinal:*ordinal})
            }
            SemanticRecipeOperand::Parameter(i)=>(current.key,OperandKey::Parameter(*i)),
            SemanticRecipeOperand::Receiver=>(current.key,OperandKey::Receiver),
        })
    }
    pub(super) fn class(&self,module:&TypedModule,expression:&SemanticClassExpr,environment:&EvalEnv,b:&mut Budget)->Check<ClassKey> {
        match expression {
            SemanticClassExpr::Formal{scope,ordinal}=>self.life(module,&SemanticLifeRecipe::Formal(SemanticFormalRef{scope:*scope,ordinal:*ordinal}),environment,b),
            SemanticClassExpr::Environment{scope,ordinal}=>self.life(module,&SemanticLifeRecipe::Environment{scope:*scope,ordinal:*ordinal},environment,b),
            SemanticClassExpr::Implicit{scope,ordinal}=>self.life(module,&SemanticLifeRecipe::Implicit{scope:*scope,ordinal:*ordinal},environment,b),
            SemanticClassExpr::Bound{depth,index}=>self.life(module,&SemanticLifeRecipe::Bound{depth:*depth,index:*index},environment,b),
            SemanticClassExpr::OuterOccurrence{operand,path,..}|SemanticClassExpr::IntrinsicClass{operand,path,..}=> {
                let current=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
                let (scope,key)=match operand {
                    SemanticOperand::TypeArgument{scope,ordinal}=>{
                        let frame=self.frame(environment,*scope,b)?;
                        (frame.key,OperandKey::TypeArgument{scope:frame.key,ordinal:*ordinal})
                    }
                    SemanticOperand::Environment(ordinal)=>(current.key,OperandKey::Environment{scope:current.key,ordinal:*ordinal}),
                    SemanticOperand::Parameter(i)=>(current.key,OperandKey::Parameter(*i)),
                    SemanticOperand::Capture(i)=>(current.key,OperandKey::Capture(*i)),
                    SemanticOperand::Receiver=>(current.key,OperandKey::Receiver),
                    SemanticOperand::Value(v)=>(current.key,OperandKey::Value(*v)),
                    SemanticOperand::Local(local)=>(current.key,OperandKey::Local{local:*local,at:environment.at.ok_or_else(||bad("Local class requires an actual CFG point"))?}),
                    SemanticOperand::CoreExpression{..}=>return Err(bad("Typed class expression retains a Core operand")),
                };
                let (outer,class)=match expression {
                    SemanticClassExpr::OuterOccurrence{mode,..}=>(Some(*mode),None),
                    SemanticClassExpr::IntrinsicClass{class,..}=>(None,Some(*class)),_=>unreachable!(),
                };
                self.occurrence(module,environment,scope,key,path,outer,class,b)
            }
        }
    }
    fn occurrence(&self,module:&TypedModule,environment:&EvalEnv,scope:ScopeKey,operand:OperandKey,path:&[LifeTypeProjection],outer:Option<StoredBorrowMode>,class:Option<u32>,b:&mut Budget)->Check<ClassKey> {
        b.spend(path.len().saturating_add(1))?;
        let value=match &operand {
            OperandKey::TypeArgument{scope:key,ordinal}=>match environment.lexical.iter().find(|row|row.key==*key).and_then(|row|row.actuals.get(*ordinal as usize)) {
                Some(CheckedActual::Type(value))=>value.clone(),_=>return Err(bad("intrinsic class source is not a complete Type formal")),
            },
            OperandKey::Environment{scope:key,ordinal}=>match environment.lexical.iter().find(|row|row.key==*key).and_then(|row|row.environment.get(*ordinal as usize)) {
                Some(CheckedActual::Type(value))=>value.clone(),_=>return Err(bad("intrinsic class source is not a sealed Type slot")),
            },
            _=>self.occurrences.borrow().get(&(scope,operand.clone())).cloned().ok_or_else(||unsupported("semantic runtime occurrence has no actual SSA/Local ledger fact",None))?,
        };
        dynamic_metadata::ty(&value.ty,b)?;
        b.spend(value.classes.len())?;
        let selected=select(&value.ty,path,b)?;
        if let Some(mode)=outer {
            let valid=matches!((mode,selected),(StoredBorrowMode::SharedAddress,Type::Address{mutable:false,..})|(StoredBorrowMode::ExclusiveAddress,Type::Address{mutable:true,..})|(StoredBorrowMode::ReadOnlyDescription,Type::Desc(_))|(StoredBorrowMode::WritableDescription,Type::Desc(_)));
            if !valid { return Err(bad("outer lifetime occurrence has a different full borrow mode")); }
            let physical=path.iter().map(|step|match step {LifeTypeProjection::Place(projection)=>Ok(*projection),_=>Err(unsupported("nested outer lifetime requires its complete retained occurrence map",None))}).collect::<Check<Vec<_>>>()?;
            let mut rows=value.life.slots.iter().filter(|slot|slot.path==physical&&slot.mode==mode);
            let row=rows.next().ok_or_else(||bad("outer lifetime occurrence is static or has no actual variable slot"))?;
            if rows.next().is_some(){return Err(bad("outer occurrence has duplicate actual slot maps"));}
            value.classes.get(row.binder as usize).cloned().ok_or_else(||bad("outer slot selects an absent intrinsic class"))
        } else {
            let count=intrinsic_count(module,selected,b)?;
            let class=class.ok_or_else(||bad("intrinsic occurrence lacks its class"))?;
            if class>=count {return Err(bad("intrinsic occurrence escapes the complete actual child domain"));}
            let binder=if path.is_empty() {class}else{
                let mut rows=value.life.retained.iter().filter(|row|row.path==path&&row.class==class);
                let row=rows.next().ok_or_else(||bad("intrinsic child occurrence has no exact outer map"))?;
                if rows.next().is_some(){return Err(bad("intrinsic child occurrence has duplicate maps"));}
                row.binder
            };
            value.classes.get(binder as usize).cloned().ok_or_else(||bad("intrinsic occurrence maps outside its actual domain"))
        }
    }
}
pub(super) fn select<'a>(ty:&'a Type,path:&[LifeTypeProjection],b:&mut Budget)->Check<&'a Type> {
    b.spend(path.len().saturating_add(1))?;
    let mut current=ty;
    for step in path {
        while let Type::BorrowedStorage{storage,..}=current { current=storage; }
        current=match (step,current) {
            (LifeTypeProjection::Pointee,Type::Address{inner,..}|Type::Managed(inner)|Type::Desc(inner))=>inner,
            (LifeTypeProjection::Element,Type::Array{element,..}|Type::Slice{element}|Type::OwnedStore{element})=>element,
            (LifeTypeProjection::Place(PlaceProjection::Field(i)),Type::Tuple(fields))=>fields.get(*i as usize).ok_or_else(||bad("class field is absent"))?,
            (LifeTypeProjection::Place(PlaceProjection::Field(i)),Type::Record{fields,..}|Type::Package{fields,..})=>&fields.get(*i as usize).ok_or_else(||bad("class nominal field is absent"))?.1,
            (LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)),Type::ErrorUnion{error,result})=>if *arm {error} else {result},
            _=>return Err(unsupported("semantic type occurrence selector requires its exact child inventory",None)),
        };
    }
    Ok(current)
}
