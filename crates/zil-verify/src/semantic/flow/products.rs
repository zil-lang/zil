//! Original enum constructors and actual finite product observations. Full
//! declared nominal identities and source actuals precede opcode admission.
use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn product_instruction(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,sites:&[&SemanticSite],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        match &instruction.kind{
            InstructionKind::MakeVariant{variant,fields}=>{
                let calls=sites.iter().copied().filter(|site|matches!(site.action,SemanticSiteAction::BodyCall{..})).collect::<Vec<_>>();
                let sites=if calls.is_empty(){sites.iter().copied().filter(|site|matches!(site.action,SemanticSiteAction::Construct{..})).collect::<Vec<_>>()}else{calls};
                if sites.len()!=1{return Err(bad("actual enum constructor lacks one exact Call or formation claim"));}let site=sites[0];
                let actuals=match &site.action{SemanticSiteAction::BodyCall{actuals}=>Some(actuals.as_slice()),SemanticSiteAction::Construct{..}=>None,_=>unreachable!()};
                let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("actual enum constructor lacks universal original proof"))?;
                let (expected_variant,recipes)=original.variant_call(module,instance.definition,site.recipe,environment,b)?;
                if *variant!=expected_variant||actuals.is_some_and(|actuals|actuals.len()!=recipes.len()){return Err(bad("actual enum constructor changes its original variant or mixed arity"));}
                for(actual,recipe)in actuals.into_iter().flatten().zip(&recipes){let expected=self.evaluate_actual(module,recipe,environment,external,active,b)?;self.actual_claim(module,actual,&expected,environment,b)?;}
                let definition=self.definition(module,instance.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("enum constructor caller has no original body"));};b.spend(site_nodes.len())?;
                let source=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("enum constructor has no original node link"))?;let SemanticOriginalNode::Expression(node)=source.node else{return Err(bad("enum constructor source is not a Call expression"));};
                let recipe=original.type_recipe(module,instance.definition,&SemanticOriginalTypeSource::Expression(node),environment,b)?;
                let value=self.evaluate_type(module,&recipe,environment,external,active,b)?;
                let storage=match &value.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};let Type::Enum{variants,..}=storage else{return Err(bad("original enum constructor has another complete nominal result"));};
                let(_,expected)=variants.get(*variant as usize).ok_or_else(||bad("original enum variant is outside its complete declaration"))?;
                if fields.len()!=expected.len(){return Err(bad("actual enum constructor omits original fields"));}
                for(index,(field,expected))in fields.iter().zip(expected).enumerate(){
                    let actual=value_at(values,*field)?;if actual.ty!=*expected{return Err(bad("actual enum field differs from its original full nominal type"));}
                    let child=self.child_value(module,&value,&[LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:*variant,index:index as u32}))],b)?;
                    if !self.compatible(environment,actual,&child,b)?{return Err(bad("actual enum field changes its complete original class/source occurrence"));}
                }
                if let SemanticSiteAction::Construct{value:claim,..}=&site.action{self.claim(module,claim,&value,environment,b)?;}
                Ok(Some(value))
            },
            InstructionKind::VariantTag{value}=>{value_at(values,*value)?;Ok(Some(atomic(&Type::I32,b)?))},
            InstructionKind::ReadVariant{value,variant,index}=>{
                let parent=value_at(values,*value)?;
                self.child_value(module,parent,&[LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:*variant,index:*index}))],b).map(Some)
            },
            InstructionKind::ExtractIndex{aggregate,index}=>{
                let parent=value_at(values,*aggregate)?;if !value_at(values,*index)?.ty.is_integer(){return Err(bad("actual array index lacks its full integer carrier"));}
                self.child_value(module,parent,&[LifeTypeProjection::Element],b).map(Some)
            },
            InstructionKind::AddressIndex{address,index}=>{
                let parent=value_at(values,*address)?;if !value_at(values,*index)?.ty.is_integer(){return Err(bad("actual address index lacks an integer carrier"));}
                let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual indexed address has no complete parent head"));};let storage=match inner.as_ref(){Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                if !matches!(storage,Type::Array{..}|Type::Slice{..}){return Err(bad("actual indexed address has another full child type"));}
                let child=self.child_value(module,parent,&[LifeTypeProjection::Pointee,LifeTypeProjection::Element],b)?;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};let mut slots=parent.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==mode);let row=slots.next().ok_or_else(||bad("actual index address has no variable parent head occurrence"))?;if slots.next().is_some(){return Err(bad("actual index parent head is ambiguous"));}
                let key=parent.classes.get(row.binder as usize).cloned().ok_or_else(||bad("actual index parent class is absent"))?;
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},&[child],active,b).map(Some)
            },
            InstructionKind::SliceLength{address}=>{value_at(values,*address)?;Ok(Some(atomic(&Type::I64,b)?))},
            _=>Err(bad("another actual operation reached the finite product consumer")),
        }
    }
}
