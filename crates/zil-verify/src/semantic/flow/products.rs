//! Original enum constructors and actual finite product observations. Full
//! declared nominal identities and source actuals precede opcode admission.
use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn product_instruction(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,sites:&[&SemanticSite],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        match &instruction.kind{
            InstructionKind::LoadStoredBorrow{storage,path,mode}=>{
                let parent=value_at(values,*storage)?;
                let selected=path.iter().copied().map(LifeTypeProjection::Place).collect::<Vec<_>>();
                let holder=self.child_value(module,parent,&[LifeTypeProjection::Pointee],b)?;
                let outer=holder.life.slots.iter().find(|row|row.path==*path).map(|row|row.mode);
                let mut child=self.child_value_mode(module,&holder,&selected,outer,b)?;
                // Loading a saved pointer selects its Life image. The outer
                // head is not the holder nominal's κ. Every inner intrinsic
                // occurrence keeps the exact child projection above.
                for slot in child.life.slots.iter().filter(|slot|slot.path.is_empty()){
                    let key=child.classes.get(slot.binder as usize).ok_or_else(||bad("stored child outer class is absent"))?.clone();
                    *child.origins.get_mut(slot.binder as usize).ok_or_else(||bad("stored child outer origin is absent"))?=IntrinsicOrigin::DerivedClass{source:key};
                }
                match mode{
                    StoredBorrowLoad::CopyShared|StoredBorrowLoad::CopyDescription|StoredBorrowLoad::Take=>Ok(Some(child)),
                    StoredBorrowLoad::Reborrow{mutable}=>{
                        let inner=self.child_value(module,&child,&[LifeTypeProjection::Pointee],b)?;
                        let Type::Address{mutable:parent_mutable,..}=&parent.ty else{return Err(bad("stored reborrow has no actual parent Address"));};
                        let parent_mode=if *parent_mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                        let mut heads=parent.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==parent_mode);
                        let outer=if let Some(head)=heads.next(){if heads.next().is_some(){return Err(bad("stored reborrow has ambiguous actual carrier heads"));}Some(parent.classes.get(head.binder as usize).cloned().ok_or_else(||bad("stored reborrow parent head class is absent"))?)}else{
                            let mut statics=parent.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&parent.ty));
                            if *mutable||statics.next().is_none()||statics.next().is_some(){return Err(bad("stored reborrow lacks one bounded or shared Static actual carrier head"));}None
                        };
                        let result = instruction.result.ok_or_else(|| bad("stored reborrow lacks its actual result"))?;
                        let occurrence = self.stored_reborrow_occurrence(module, instance, f, result, *storage, path, b)?;
                        let outer = occurrence.or(outer);
                        let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                        dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer},&[inner],active,b).map(Some)
                    },
                }
            },
            InstructionKind::StoreStoredBorrow{storage,path,value}=>{
                let parent=value_at(values,*storage)?;let holder=self.child_value(module,parent,&[LifeTypeProjection::Pointee],b)?;
                let outer=holder.life.slots.iter().find(|row|row.path==*path).map(|row|row.mode);
                let selected=path.iter().copied().map(LifeTypeProjection::Place).collect::<Vec<_>>();
                let mut child=self.child_value_mode(module,&holder,&selected,outer,b)?;
                for slot in child.life.slots.iter().filter(|slot|slot.path.is_empty()){
                    let key=child.classes.get(slot.binder as usize).ok_or_else(||bad("stored replacement outer class is absent"))?.clone();
                    *child.origins.get_mut(slot.binder as usize).ok_or_else(||bad("stored replacement outer origin is absent"))?=IntrinsicOrigin::DerivedClass{source:key};
                }
                if !self.compatible(environment,value_at(values,*value)?,&child,b)?{return Err(bad("stored replacement changes its complete declared child domain or head bound"));}
                // The storage's declared type stays fixed. Target sets, slot
                // vacancy/epochs and parent loans update in the actual ledger.
                Ok(None)
            },
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
                let recipe=original.construction_recipe(module,instance.definition,&SemanticOriginalTypeSource::Expression(node),environment,b)?;
                let value=self.evaluate_type(module,&recipe,environment,external,active,b)?;
                let storage=match &value.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};let Type::Enum{variants,..}=storage else{return Err(bad("original enum constructor has another complete nominal result"));};
                let(_,expected)=variants.get(*variant as usize).ok_or_else(||bad("original enum variant is outside its complete declaration"))?;
                if fields.len()!=expected.len(){return Err(bad("actual enum constructor omits original fields"));}
                if !fields.is_empty() && crate::stored_flow::inline_frame_cleanup(module, f, storage, b)? {
                    let sources = original.variant_operand_nodes(module, instance.definition, site.recipe, b)?;
                    if sources.len() != fields.len() { return Err(bad("resumable enum changes its complete original payload occurrences")); }
                    for (&field, node) in fields.iter().zip(sources) {
                        let mut operand = field;
                        let mut seen = BTreeSet::new();
                        while let Some(source) = self.prepared_managed_source(module, instance, f, operand, b)? {
                            b.spend(seen.len().checked_ilog2().map_or(1, |depth| depth as usize + 2))?;
                            if !seen.insert(operand) { return Err(bad("resumable enum operand has cyclic owning preparation")); }
                            operand = source;
                        }
                        self.assignment_value_source(module, instance, f, operand, node, b)
                            .map_err(|error| error.with_note("resumable enum field selects another original operand"))?;
                    }
                }
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
            InstructionKind::UpdateField{aggregate,index,value}=>{
                let parent=value_at(values,*aggregate)?;
                let child=self.child_value(module,parent,&[LifeTypeProjection::Place(PlaceProjection::Field(*index))],b)?;
                if !child.ty.is_copy_value()||!self.same_type(environment,value_at(values,*value)?,&child,b)?{
                    return Err(bad("ordinary field update requires the exact Copy child domain"));
                }
                self.copy_field_update(module,instance,f,instruction,b)?;
                Ok(Some(parent.clone()))
            },
            InstructionKind::UpdateIndex{aggregate,index,value}=>{
                let parent=value_at(values,*aggregate)?;if !value_at(values,*index)?.ty.is_integer(){return Err(bad("actual array update index lacks its complete integer carrier"));}
                let child=self.child_value(module,parent,&[LifeTypeProjection::Element],b)?;
                if !self.compatible(environment,value_at(values,*value)?,&child,b)?{return Err(bad("actual array update changes an independent element type/class/source occurrence"));}
                // Updating a runtime element preserves the whole array's
                // intrinsic partition and all zero-length/phantom metadata.
                Ok(Some(parent.clone()))
            },
            InstructionKind::AddressIndex{address,index}=>{
                let parent=value_at(values,*address)?;if !value_at(values,*index)?.ty.is_integer(){return Err(bad("actual address index lacks an integer carrier"));}
                let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual indexed address has no complete parent head"));};let storage=match inner.as_ref(){Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                if !matches!(storage,Type::Array{..}|Type::Slice{..}){return Err(bad("actual indexed address has another full child type"));}
                let child=self.child_value(module,parent,&[LifeTypeProjection::Pointee,LifeTypeProjection::Element],b)?;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};let mut slots=parent.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==mode);
                let outer=if let Some(row)=slots.next(){if slots.next().is_some(){return Err(bad("actual index parent head is ambiguous"));}Some(parent.classes.get(row.binder as usize).cloned().ok_or_else(||bad("actual index parent class is absent"))?)}else{
                    let mut statics=parent.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&parent.ty));
                    if *mutable||statics.next().is_none()||statics.next().is_some(){return Err(bad("actual index has neither a bounded head nor one exact shared static source"));}None
                };
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer},&[child],active,b).map(Some)
            },
            InstructionKind::AddressSlice{address,start,end}=>{
                let parent=value_at(values,*address)?;
                for bound in start.iter().chain(end.iter()){if !value_at(values,*bound)?.ty.is_integer(){return Err(bad("actual slice bound lacks its independently checked integer carrier"));}}
                let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual slice lacks its complete parent address"));};
                let storage=match inner.as_ref(){Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                if !matches!(storage,Type::Array{..}|Type::Slice{..}){return Err(bad("actual slice changes its original array/slice carrier"));}
                let child=self.child_value(module,parent,&[LifeTypeProjection::Pointee,LifeTypeProjection::Element],b)?;
                let mut slice=CheckedType{type_cell: crate::semantic::CheckedTypeData::new(Type::Slice{element:Box::new(child.raw_type_copy(b)?)}),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
                self.attach_child(environment,&mut slice,&child,vec![LifeTypeProjection::Element],None,b)?;slice.life.binder_count=slice.classes.len() as u32;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                let mut heads=parent.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==mode);
                let outer=if let Some(row)=heads.next(){if heads.next().is_some(){return Err(bad("actual slice has duplicate parent head occurrences"));}Some(parent.classes.get(row.binder as usize).cloned().ok_or_else(||bad("actual slice parent class is absent"))?)}else{
                    let mut statics=parent.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&parent.ty));
                    if *mutable||statics.next().is_none()||statics.next().is_some(){return Err(bad("actual slice has neither one real parent head nor an exact shared static source"));}None
                };
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer},&[slice],active,b).map(Some)
            },
            InstructionKind::SliceLength{address}=>{value_at(values,*address)?;Ok(Some(atomic(&Type::I64,b)?))},
            _=>Err(bad("another actual operation reached the finite product consumer")),
        }
    }
}
