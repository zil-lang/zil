//! Original enum constructors and actual finite product observations. Full
//! declared nominal identities and source actuals precede opcode admission.
use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn product_instruction(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,sites:&[&SemanticSite],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        match &instruction.kind{
            InstructionKind::MakeVariant{variant,fields}=>{
                let mut calls=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::BodyCall{..}));
                let site=calls.next().ok_or_else(||bad("actual enum constructor has no original exact Call"))?;if calls.next().is_some(){return Err(bad("actual enum constructor has several original Call claims"));}
                let SemanticSiteAction::BodyCall{actuals}=&site.action else{unreachable!()};
                let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("actual enum constructor lacks universal original proof"))?;
                let (expected_variant,recipes)=original.variant_call(module,instance.definition,site.recipe,environment,b)?;
                if *variant!=expected_variant||actuals.len()!=recipes.len(){return Err(bad("actual enum constructor changes its original variant or mixed arity"));}
                for(actual,recipe)in actuals.iter().zip(&recipes){let expected=self.evaluate_actual(module,recipe,environment,external,active,b)?;self.actual_claim(module,actual,&expected,environment,b)?;}
                let definition=self.definition(module,instance.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("enum constructor caller has no original body"));};b.spend(site_nodes.len())?;
                let source=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("enum constructor has no original node link"))?;let SemanticOriginalNode::Expression(node)=source.node else{return Err(bad("enum constructor source is not a Call expression"));};
                let recipe=original.type_recipe(module,instance.definition,&SemanticOriginalTypeSource::Expression(node),environment,b)?;
                let value=self.evaluate_type(module,&recipe,environment,external,active,b)?;
                let storage=match &value.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};let Type::Enum{variants,..}=storage else{return Err(bad("original enum constructor has another complete nominal result"));};
                let(_,expected)=variants.get(*variant as usize).ok_or_else(||bad("original enum variant is outside its complete declaration"))?;
                if fields.len()!=expected.len(){return Err(bad("actual enum constructor omits original fields"));}
                for(field,expected)in fields.iter().zip(expected){let actual=value_at(values,*field)?;if actual.ty!=*expected{return Err(bad("actual enum field differs from its original full nominal type"));}if !actual.classes.is_empty()||!actual.statics.is_empty(){return Err(unsupported("enum field needs complete actual child-class/source substitution",Some(instruction.span)));}self.closed_value(module,expected,b)?;}
                Ok(Some(value))
            },
            InstructionKind::VariantTag{value}=>{let parent=value_at(values,*value)?;self.closed_value(module,&parent.ty,b)?;if !parent.classes.is_empty()||!parent.statics.is_empty(){return Err(unsupported("enum tag retains an unresolved complete class domain",Some(instruction.span)));}Ok(Some(atomic(&Type::I32,b)?))},
            InstructionKind::ReadVariant{value,variant,index}=>{
                let parent=value_at(values,*value)?;if !parent.classes.is_empty()||!parent.statics.is_empty(){return Err(unsupported("enum observation needs its complete selected child occurrence map",Some(instruction.span)));}
                let storage=match &parent.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};let Type::Enum{variants,..}=storage else{return Err(bad("variant observation has another nominal parent"));};
                let child=variants.get(*variant as usize).and_then(|(_,fields)|fields.get(*index as usize)).ok_or_else(||bad("variant observation selects no declared field"))?;Ok(Some(self.closed_value(module,child,b)?))
            },
            InstructionKind::ExtractIndex{aggregate,index}=>{
                let parent=value_at(values,*aggregate)?;let index=value_at(values,*index)?;if !index.ty.is_integer(){return Err(bad("actual array index lacks its full integer carrier"));}
                if !parent.classes.is_empty()||!parent.statics.is_empty(){return Err(unsupported("array extraction needs its complete element class/source map",Some(instruction.span)));}
                let storage=match &parent.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};let Type::Array{element,..}=storage else{return Err(bad("actual indexed aggregate is not its original full Array"));};Ok(Some(self.closed_value(module,element,b)?))
            },
            InstructionKind::AddressIndex{address,index}=>{
                let parent=value_at(values,*address)?;if !value_at(values,*index)?.ty.is_integer(){return Err(bad("actual address index lacks an integer carrier"));}
                let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual indexed address has no complete parent head"));};let storage=match inner.as_ref(){Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                let child=match storage{Type::Array{element,..}|Type::Slice{element}=>self.closed_value(module,element,b)?,_=>return Err(bad("actual indexed address has another full child type"))};
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};let mut slots=parent.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==mode);let row=slots.next().ok_or_else(||bad("actual index address has no variable parent head occurrence"))?;if slots.next().is_some(){return Err(bad("actual index parent head is ambiguous"));}
                let key=parent.classes.get(row.binder as usize).cloned().ok_or_else(||bad("actual index parent class is absent"))?;
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},&[child],active,b).map(Some)
            },
            InstructionKind::SliceLength{address}=>{value_at(values,*address)?;Ok(Some(atomic(&Type::I64,b)?))},
            _=>Err(bad("another actual operation reached the finite product consumer")),
        }
    }
}
