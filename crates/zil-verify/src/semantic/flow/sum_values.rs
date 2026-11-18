//! Actual discriminators and selected whole child domains. Runtime active-tag,
//! initialization, moves and conditional cleanup remain independent checks.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sum_instruction(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,sites:&[&SemanticSite],state:&mut State,values:&[Option<CheckedType>],signature:&CheckedSignature,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>> {
        // Semantic sites are checked before the module's general shape pass.
        // Derive the owning Sum profile from the actual full union before
        // classifying a Take or Drop of a nested affine arm.
        if !b.sums {
            let union = match instruction.kind {
                InstructionKind::TakeErrorUnion { value, .. } => Some(value_type(f, value, Some(instruction.span))?),
                InstructionKind::DropErrorUnion { local } => Some(local_type(f, local, Some(instruction.span))?),
                _ => None,
            };
            if let Some(ty) = union && !crate::errors::owned(ty, false) {
                let limits = b.limits.clone();
                crate::check_type_shape(ty, &module.target, &limits, b)?;
            }
        }
        crate::check_instruction(module,f,instruction,b)?;
        let project=|parent:&CheckedType,path:&[PlaceProjection],b:&mut Budget|{
            b.spend(path.len())?;
            let path=path.iter().copied().map(LifeTypeProjection::Place).collect::<Vec<_>>();
            self.child_value(module,parent,&path,b)
        };
        let local=|id:LocalId|state.locals.get(id.0 as usize).and_then(Option::as_ref)
            .ok_or_else(||bad("actual sum/product operation has no initialized local"));
        match &instruction.kind {
            InstructionKind::ErrorTag{value}=>{value_at(values,*value)?;Ok(Some(atomic(&Type::Bool,b)?))},
            InstructionKind::VariantTagLocal{local:source,..}|InstructionKind::PlaceTagLocal{local:source,..}=>{local(*source)?;Ok(Some(atomic(&Type::I32,b)?))},
            InstructionKind::ReadErrorUnion{value,is_error}|InstructionKind::TakeErrorUnion{value,is_error}=>project(value_at(values,*value)?,&[PlaceProjection::ErrorArm(*is_error)],b).map(Some),
            InstructionKind::TakeVariantField{local:source,path}|InstructionKind::TakeTupleField{local:source,path}=>project(local(*source)?,path,b).map(Some),
            InstructionKind::BorrowPlaceLocal{local:source,path,mutable}=>{
                let child=project(local(*source)?,path,b)?;
                let result=instruction.result.ok_or_else(||bad("actual selected-place borrow has no SSA result"))?;
                let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                let outer=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
                // The typed place kernel independently proves actual active
                // tags, initialization and parent loans along every path step.
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(outer)},&[child],active,b).map(Some)
            },
            InstructionKind::ReadVariantLocal{local:source,path}=>{
                b.spend(path.len())?;
                let path=path.iter().copied().map(PlaceProjection::Variant).collect::<Vec<_>>();
                project(local(*source)?,&path,b).map(Some)
            }
            InstructionKind::MakeErrorUnion{value,is_error}=>{
                let actual=value_at(values,*value)?;
                let output=instruction.result.and_then(|id|f.values.get(id.0 as usize)).ok_or_else(||bad("actual sum constructor result is absent"))?;
                let mut expected=None;
                for site in sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::TypeUse{..}|SemanticSiteAction::Construct{..})) {
                    let candidate=self.site_expected(module,instance,site,environment,external,active,b)?;
                    if &candidate.ty!=output{continue;}
                    if let Some(old)=&expected {if !self.same_type(environment,old,&candidate,b)?{return Err(bad("sum constructor has incompatible original whole-result roles"));}}
                    else{expected=Some(candidate);}
                }
                let expected=match expected {
                    Some(value)=>value,
                    None if output==&signature.result.ty=>signature.result.clone(),
                    None=>return Err(bad("sum constructor has no independently checked original result context")),
                };
                let child=project(&expected,&[PlaceProjection::ErrorArm(*is_error)],b)?;
                if !self.compatible(environment,actual,&child,b)?{return Err(bad("sum constructor payload differs from its selected complete original arm"));}
                Ok(Some(expected))
            }
            InstructionKind::UnpackTuple{local:source,fields}|InstructionKind::UnpackVariant{local:source,fields,..}=>{
                let parent=local(*source)?;
                let mut children=Vec::new();
                b.spend(fields.len())?;
                for(index,target)in fields.iter().enumerate(){
                    let step=match instruction.kind{InstructionKind::UnpackVariant{variant,..}=>PlaceProjection::Variant(VariantProjection{variant,index:index as u32}),_=>PlaceProjection::Field(index as u32)};
                    let child=project(parent,&[step],b)?;
                    if f.locals.get(target.0 as usize).is_none_or(|local|local.ty!=child.ty){return Err(bad("actual unpack destination changes a complete child type"));}
                    children.push((*target,child));
                }
                state.locals[source.0 as usize]=None;
                for(target,child)in children{state.locals[target.0 as usize]=Some(child);}
                Ok(None)
            }
            InstructionKind::DropErrorUnion{local:source}|InstructionKind::DropEnum{local:source}|InstructionKind::DropTuple{local:source}=>{
                state.locals[source.0 as usize]=None;Ok(None)
            }
            _=>Err(bad("another operation reached the whole sum/product consumer")),
        }
    }
}
