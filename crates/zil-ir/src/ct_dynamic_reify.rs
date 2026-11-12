//! Structural reification of frozen runtime metadata into raw CT data.
//! Budget callbacks supply no semantic authority. Every type is a real DAG node.
use super::*;
use crate::*;
pub trait CtDynamicReifier {
    type Error;
    fn node(&mut self,depth:usize)->Result<(),Self::Error>;
    fn rows(&mut self,count:usize,element_bytes:usize,depth:usize)->Result<(),Self::Error>;
    fn text(&mut self,text:&str,depth:usize)->Result<String,Self::Error>;
    fn constant(&mut self,value:&Constant,depth:usize)->Result<Constant,Self::Error>;
    fn store_type(&mut self,node:CtDynamicTypeNode,depth:usize)->Result<CtDynamicTypeId,Self::Error>;
}
pub fn reify_dyn_interface_id<R:CtDynamicReifier>(value:&DynInterfaceId,reifier:&mut R,depth:usize)->Result<CtDynInterfaceId,R::Error>{ reifier.node(depth)?;
Ok(CtDynInterfaceId { module: { reifier.node(depth+1)?; value.module }, index: { reifier.node(depth+1)?; value.index } })
}
pub fn reify_dyn_vtable_id<R:CtDynamicReifier>(value:&DynVtableId,reifier:&mut R,depth:usize)->Result<CtDynVtableId,R::Error>{ reifier.node(depth)?;
Ok(CtDynVtableId { module: { reifier.node(depth+1)?; value.module }, index: { reifier.node(depth+1)?; value.index } })
}
pub fn reify_dyn_declaration<R:CtDynamicReifier>(value:&DynDeclaration,reifier:&mut R,depth:usize)->Result<CtDynDeclaration,R::Error>{ reifier.node(depth)?;
Ok(CtDynDeclaration { implicit_lifetimes: match &value.implicit_lifetimes { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynImplicitLifeDeclaration>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_implicit_life_declaration(item,reifier,depth+1)?); } output }), None=>None }, direct_supertraits: match &value.direct_supertraits { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynSelfPattern>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_self_pattern(item,reifier,depth+1)?); } output }), None=>None }, requirements: match &value.requirements { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynObligation>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_obligation(item,reifier,depth+1)?); } output }), None=>None }, associated_definitions: match &value.associated_definitions { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynAssociatedDefinition>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_associated_definition(item,reifier,depth+1)?); } output }), None=>None }, method_signatures: match &value.method_signatures { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynMethodRecipe>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_method_recipe(item,reifier,depth+1)?); } output }), None=>None }, method_definitions: match &value.method_definitions { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynDefinitionDeclaration>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_definition_declaration(item,reifier,depth+1)?); } output }), None=>None }, self_pattern: match &value.self_pattern { Some(item)=>Some(reify_dyn_type_pattern(item,reifier,depth+1)?), None=>None }, implemented_trait: match &value.implemented_trait { Some(item)=>Some(reify_dyn_self_pattern(item,reifier,depth+1)?), None=>None }, id: reify_life_declaration_id(&value.id,reifier,depth+1)?, identity: reify_life_declaration_identity(&value.identity,reifier,depth+1)?, kind: reify_dyn_declaration_kind(&value.kind,reifier,depth+1)?, parameters: { reifier.rows((value.parameters).len(),std::mem::size_of::<CtLifeParameterKind>(),depth+1)?; let mut output=Vec::with_capacity((value.parameters).len()); for item in &value.parameters { output.push(reify_life_parameter_kind(item,reifier,depth+1)?); } output }, members: { reifier.rows((value.members).len(),std::mem::size_of::<CtDynMemberDeclaration>(),depth+1)?; let mut output=Vec::with_capacity((value.members).len()); for item in &value.members { output.push(reify_dyn_member_declaration(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_self_pattern<R:CtDynamicReifier>(value:&DynSelfPattern,reifier:&mut R,depth:usize)->Result<CtDynSelfPattern,R::Error>{ reifier.node(depth)?;
Ok(CtDynSelfPattern { declaration: reify_life_declaration_id(&value.declaration,reifier,depth+1)?, arguments: { reifier.rows((value.arguments).len(),std::mem::size_of::<CtDynPatternActual>(),depth+1)?; let mut output=Vec::with_capacity((value.arguments).len()); for item in &value.arguments { output.push(reify_dyn_pattern_actual(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_pattern_actual<R:CtDynamicReifier>(value:&DynPatternActual,reifier:&mut R,depth:usize)->Result<CtDynPatternActual,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynPatternActual::Type(field_value_0) => CtDynPatternActual::Type(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?),
DynPatternActual::Const(field_value_0) => CtDynPatternActual::Const(reify_dyn_const_pattern(field_value_0,reifier,depth+1)?),
DynPatternActual::Life(field_value_0) => CtDynPatternActual::Life(reify_dyn_life_pattern(field_value_0,reifier,depth+1)?),
})
}
pub fn reify_dyn_const_pattern<R:CtDynamicReifier>(value:&DynConstPattern,reifier:&mut R,depth:usize)->Result<CtDynConstPattern,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynConstPattern::IntegerLiteral(field_value_0) => CtDynConstPattern::IntegerLiteral(reifier.text(field_value_0,depth+1)?),
DynConstPattern::FloatLiteral(field_value_0) => CtDynConstPattern::FloatLiteral(reifier.text(field_value_0,depth+1)?),
DynConstPattern::MethodParameter(field_value_0) => CtDynConstPattern::MethodParameter({ reifier.node(depth+1)?; *(field_value_0) }),
DynConstPattern::Unary { op: field_op, value: field_value, ty: field_ty } => CtDynConstPattern::Unary { op: { reifier.node(depth+1)?; *(field_op) }, value: { reifier.rows(1,std::mem::size_of::<CtDynConstPattern>(),depth+1)?; Box::new(reify_dyn_const_pattern(field_value,reifier,depth+1)?) }, ty: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_ty,reifier,depth+1)?) } },
DynConstPattern::Binary { op: field_op, left: field_left, right: field_right, ty: field_ty } => CtDynConstPattern::Binary { op: { reifier.node(depth+1)?; *(field_op) }, left: { reifier.rows(1,std::mem::size_of::<CtDynConstPattern>(),depth+1)?; Box::new(reify_dyn_const_pattern(field_left,reifier,depth+1)?) }, right: { reifier.rows(1,std::mem::size_of::<CtDynConstPattern>(),depth+1)?; Box::new(reify_dyn_const_pattern(field_right,reifier,depth+1)?) }, ty: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_ty,reifier,depth+1)?) } },
DynConstPattern::Convert { value: field_value, ty: field_ty } => CtDynConstPattern::Convert { value: { reifier.rows(1,std::mem::size_of::<CtDynConstPattern>(),depth+1)?; Box::new(reify_dyn_const_pattern(field_value,reifier,depth+1)?) }, ty: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_ty,reifier,depth+1)?) } },
DynConstPattern::AssociatedProjection { receiver: field_receiver, application: field_application, member: field_member } => CtDynConstPattern::AssociatedProjection { receiver: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_receiver,reifier,depth+1)?) }, application: { reifier.rows(1,std::mem::size_of::<CtDynSelfPattern>(),depth+1)?; Box::new(reify_dyn_self_pattern(field_application,reifier,depth+1)?) }, member: { reifier.node(depth+1)?; *(field_member) } },
DynConstPattern::Value(field_value_0) => CtDynConstPattern::Value(reifier.constant(field_value_0,depth+1)?),
DynConstPattern::Parameter(field_value_0) => CtDynConstPattern::Parameter({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_dyn_type_pattern<R:CtDynamicReifier>(value:&DynTypePattern,reifier:&mut R,depth:usize)->Result<CtDynTypePattern,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynTypePattern::SelfType => CtDynTypePattern::SelfType,
DynTypePattern::MethodParameter(field_value_0) => CtDynTypePattern::MethodParameter({ reifier.node(depth+1)?; *(field_value_0) }),
DynTypePattern::AssociatedProjection { receiver: field_receiver, application: field_application, member: field_member } => CtDynTypePattern::AssociatedProjection { receiver: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_receiver,reifier,depth+1)?) }, application: { reifier.rows(1,std::mem::size_of::<CtDynSelfPattern>(),depth+1)?; Box::new(reify_dyn_self_pattern(field_application,reifier,depth+1)?) }, member: { reifier.node(depth+1)?; *(field_member) } },
DynTypePattern::Concrete(field_value_0) => CtDynTypePattern::Concrete(reify_type_value(field_value_0,reifier,depth+1)?),
DynTypePattern::Parameter(field_value_0) => CtDynTypePattern::Parameter({ reifier.node(depth+1)?; *(field_value_0) }),
DynTypePattern::Nominal(field_value_0) => CtDynTypePattern::Nominal({ reifier.rows(1,std::mem::size_of::<CtDynSelfPattern>(),depth+1)?; Box::new(reify_dyn_self_pattern(field_value_0,reifier,depth+1)?) }),
DynTypePattern::Address { mutable: field_mutable, lifetime: field_lifetime, inner: field_inner } => CtDynTypePattern::Address { mutable: { reifier.node(depth+1)?; *(field_mutable) }, lifetime: match field_lifetime { Some(item)=>Some(reify_dyn_life_pattern(item,reifier,depth+1)?), None=>None }, inner: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_inner,reifier,depth+1)?) } },
DynTypePattern::Tuple(field_value_0) => CtDynTypePattern::Tuple({ reifier.rows((field_value_0).len(),std::mem::size_of::<CtDynTypePattern>(),depth+1)?; let mut output=Vec::with_capacity((field_value_0).len()); for item in field_value_0 { output.push(reify_dyn_type_pattern(item,reifier,depth+1)?); } output }),
DynTypePattern::Array { element: field_element, length: field_length } => CtDynTypePattern::Array { element: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_element,reifier,depth+1)?) }, length: reify_dyn_const_pattern(field_length,reifier,depth+1)? },
DynTypePattern::Slice(field_value_0) => CtDynTypePattern::Slice({ reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?) }),
DynTypePattern::Managed(field_value_0) => CtDynTypePattern::Managed({ reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?) }),
DynTypePattern::Desc { lifetime: field_lifetime, inner: field_inner } => CtDynTypePattern::Desc { lifetime: match field_lifetime { Some(item)=>Some(reify_dyn_life_pattern(item,reifier,depth+1)?), None=>None }, inner: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_inner,reifier,depth+1)?) } },
DynTypePattern::OwnedStore(field_value_0) => CtDynTypePattern::OwnedStore({ reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?) }),
DynTypePattern::ErrorUnion { error: field_error, result: field_result } => CtDynTypePattern::ErrorUnion { error: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_error,reifier,depth+1)?) }, result: { reifier.rows(1,std::mem::size_of::<CtDynTypePattern>(),depth+1)?; Box::new(reify_dyn_type_pattern(field_result,reifier,depth+1)?) } },
})
}
pub fn reify_dyn_declaration_kind<R:CtDynamicReifier>(value:&DynDeclarationKind,reifier:&mut R,depth:usize)->Result<CtDynDeclarationKind,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynDeclarationKind::Trait => CtDynDeclarationKind::Trait,
DynDeclarationKind::Impl => CtDynDeclarationKind::Impl,
})
}
pub fn reify_dyn_member_declaration<R:CtDynamicReifier>(value:&DynMemberDeclaration,reifier:&mut R,depth:usize)->Result<CtDynMemberDeclaration,R::Error>{ reifier.node(depth)?;
Ok(CtDynMemberDeclaration { name: reifier.text(&value.name,depth+1)?, kind: reify_dyn_member_kind(&value.kind,reifier,depth+1)? })
}
pub fn reify_dyn_member_kind<R:CtDynamicReifier>(value:&DynMemberKind,reifier:&mut R,depth:usize)->Result<CtDynMemberKind,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynMemberKind::Type => CtDynMemberKind::Type,
DynMemberKind::Constant => CtDynMemberKind::Constant,
DynMemberKind::Method(field_value_0) => CtDynMemberKind::Method({ reifier.rows((field_value_0).len(),std::mem::size_of::<CtLifeParameterKind>(),depth+1)?; let mut output=Vec::with_capacity((field_value_0).len()); for item in field_value_0 { output.push(reify_life_parameter_kind(item,reifier,depth+1)?); } output }),
})
}
pub fn reify_dyn_generic_actual<R:CtDynamicReifier>(value:&DynGenericActual,reifier:&mut R,depth:usize)->Result<CtDynGenericActual,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynGenericActual::Type(field_value_0) => CtDynGenericActual::Type(reify_type_value(field_value_0,reifier,depth+1)?),
DynGenericActual::QualifiedType(field_value_0) => CtDynGenericActual::QualifiedType(reify_dyn_type_value(field_value_0,reifier,depth+1)?),
DynGenericActual::Const(field_value_0) => CtDynGenericActual::Const(reifier.constant(field_value_0,depth+1)?),
DynGenericActual::Life(field_value_0) => CtDynGenericActual::Life({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_dyn_trait_application<R:CtDynamicReifier>(value:&DynTraitApplication,reifier:&mut R,depth:usize)->Result<CtDynTraitApplication,R::Error>{ reifier.node(depth)?;
Ok(CtDynTraitApplication { declaration: reify_life_declaration_id(&value.declaration,reifier,depth+1)?, actuals: { reifier.rows((value.actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.actuals).len()); for item in &value.actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_member<R:CtDynamicReifier>(value:&DynMember,reifier:&mut R,depth:usize)->Result<CtDynMember,R::Error>{ reifier.node(depth)?;
Ok(CtDynMember { application: reify_dyn_trait_application(&value.application,reifier,depth+1)?, ordinal: { reifier.node(depth+1)?; value.ordinal } })
}
pub fn reify_dyn_associated_type<R:CtDynamicReifier>(value:&DynAssociatedType,reifier:&mut R,depth:usize)->Result<CtDynAssociatedType,R::Error>{ reifier.node(depth)?;
Ok(CtDynAssociatedType { member: reify_dyn_member(&value.member,reifier,depth+1)?, ty: reify_type_value(&value.ty,reifier,depth+1)?, life: reify_storage_life_shape(&value.life,reifier,depth+1)?, classes: { reifier.rows((value.classes).len(),std::mem::size_of::<StorageBinder>(),depth+1)?; let mut output=Vec::with_capacity((value.classes).len()); for item in &value.classes { output.push({ reifier.node(depth+1)?; *(item) }); } output } })
}
pub fn reify_dyn_associated_constant<R:CtDynamicReifier>(value:&DynAssociatedConstant,reifier:&mut R,depth:usize)->Result<CtDynAssociatedConstant,R::Error>{ reifier.node(depth)?;
Ok(CtDynAssociatedConstant { member: reify_dyn_member(&value.member,reifier,depth+1)?, ty: reify_type_value(&value.ty,reifier,depth+1)?, value: reifier.constant(&value.value,depth+1)? })
}
pub fn reify_dyn_receiver<R:CtDynamicReifier>(value:&DynReceiver,reifier:&mut R,depth:usize)->Result<CtDynReceiver,R::Error>{ reifier.node(depth)?;
Ok(CtDynReceiver { mutable: { reifier.node(depth+1)?; value.mutable }, mode: { reifier.node(depth+1)?; value.mode } })
}
pub fn reify_dyn_slot<R:CtDynamicReifier>(value:&DynSlot,reifier:&mut R,depth:usize)->Result<CtDynSlot,R::Error>{ reifier.node(depth)?;
Ok(CtDynSlot { member: reify_dyn_member(&value.member,reifier,depth+1)?, actuals: { reifier.rows((value.actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.actuals).len()); for item in &value.actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, receiver: reify_dyn_receiver(&value.receiver,reifier,depth+1)?, parameters: { reifier.rows((value.parameters).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity((value.parameters).len()); for item in &value.parameters { output.push(reify_type_value(item,reifier,depth+1)?); } output }, parameter_modes: { reifier.rows((value.parameter_modes).len(),std::mem::size_of::<PassingMode>(),depth+1)?; let mut output=Vec::with_capacity((value.parameter_modes).len()); for item in &value.parameter_modes { output.push({ reifier.node(depth+1)?; *(item) }); } output }, result: reify_type_value(&value.result,reifier,depth+1)?, result_mode: { reifier.node(depth+1)?; value.result_mode }, call_kind: { reifier.node(depth+1)?; value.call_kind }, storage_life: match &value.storage_life { Some(item)=>Some(reify_storage_life_contract(item,reifier,depth+1)?), None=>None }, contract: reify_dyn_contract(&value.contract,reifier,depth+1)? })
}
pub fn reify_dyn_interface<R:CtDynamicReifier>(value:&DynInterface,reifier:&mut R,depth:usize)->Result<CtDynInterface,R::Error>{ reifier.node(depth)?;
Ok(CtDynInterface { context: match &value.context { Some(item)=>Some(reify_type_value(item,reifier,depth+1)?), None=>None }, owning_cleanup: match &value.owning_cleanup { Some(item)=>Some(reify_dyn_cleanup_contract(item,reifier,depth+1)?), None=>None }, id: reify_dyn_interface_id(&value.id,reifier,depth+1)?, application: reify_dyn_trait_application(&value.application,reifier,depth+1)?, inherited: { reifier.rows((value.inherited).len(),std::mem::size_of::<CtDynTraitApplication>(),depth+1)?; let mut output=Vec::with_capacity((value.inherited).len()); for item in &value.inherited { output.push(reify_dyn_trait_application(item,reifier,depth+1)?); } output }, associated_types: { reifier.rows((value.associated_types).len(),std::mem::size_of::<CtDynAssociatedType>(),depth+1)?; let mut output=Vec::with_capacity((value.associated_types).len()); for item in &value.associated_types { output.push(reify_dyn_associated_type(item,reifier,depth+1)?); } output }, associated_constants: { reifier.rows((value.associated_constants).len(),std::mem::size_of::<CtDynAssociatedConstant>(),depth+1)?; let mut output=Vec::with_capacity((value.associated_constants).len()); for item in &value.associated_constants { output.push(reify_dyn_associated_constant(item,reifier,depth+1)?); } output }, retained_life: reify_retained_life_shape(&value.retained_life,reifier,depth+1)?, slots: { reifier.rows((value.slots).len(),std::mem::size_of::<CtDynSlot>(),depth+1)?; let mut output=Vec::with_capacity((value.slots).len()); for item in &value.slots { output.push(reify_dyn_slot(item,reifier,depth+1)?); } output } })
}
pub fn reify_core_dyn_entry<R:CtDynamicReifier>(value:&CoreDynEntry,reifier:&mut R,depth:usize)->Result<CtCoreDynEntry,R::Error>{ reifier.node(depth)?;
Ok(CtCoreDynEntry { slot: { reifier.node(depth+1)?; value.slot }, module: { reifier.node(depth+1)?; value.module }, function: { reifier.node(depth+1)?; value.function } })
}
pub fn reify_dyn_entry<R:CtDynamicReifier>(value:&DynEntry,reifier:&mut R,depth:usize)->Result<CtDynEntry,R::Error>{ reifier.node(depth)?;
Ok(CtDynEntry { slot: { reifier.node(depth+1)?; value.slot }, function: { reifier.node(depth+1)?; value.function } })
}
pub fn reify_dyn_retained_port_binding<R:CtDynamicReifier>(value:&DynRetainedPortBinding,reifier:&mut R,depth:usize)->Result<CtDynRetainedPortBinding,R::Error>{ reifier.node(depth)?;
Ok(CtDynRetainedPortBinding { port: { reifier.node(depth+1)?; value.port }, sources: { reifier.rows((value.sources).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.sources).len()); for item in &value.sources { output.push(reify_storage_position(item,reifier,depth+1)?); } output } })
}
pub fn reify_core_dyn_vtable<R:CtDynamicReifier>(value:&CoreDynVtable,reifier:&mut R,depth:usize)->Result<CtCoreDynVtable,R::Error>{ reifier.node(depth)?;
Ok(CtCoreDynVtable { self_life: match &value.self_life { Some(item)=>Some(reify_storage_life_shape(item,reifier,depth+1)?), None=>None }, implementation_witnesses: match &value.implementation_witnesses { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynImplementationWitness>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_implementation_witness(item,reifier,depth+1)?); } output }), None=>None }, self_classes: { reifier.rows((value.self_classes).len(),std::mem::size_of::<StorageBinder>(),depth+1)?; let mut output=Vec::with_capacity((value.self_classes).len()); for item in &value.self_classes { output.push({ reifier.node(depth+1)?; *(item) }); } output }, id: reify_dyn_vtable_id(&value.id,reifier,depth+1)?, interface: reify_dyn_interface_id(&value.interface,reifier,depth+1)?, self_type: reify_type_value(&value.self_type,reifier,depth+1)?, implementation: reify_life_declaration_id(&value.implementation,reifier,depth+1)?, actuals: { reifier.rows((value.actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.actuals).len()); for item in &value.actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, ports: { reifier.rows((value.ports).len(),std::mem::size_of::<CtDynRetainedPortBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.ports).len()); for item in &value.ports { output.push(reify_dyn_retained_port_binding(item,reifier,depth+1)?); } output }, entries: { reifier.rows((value.entries).len(),std::mem::size_of::<CtCoreDynEntry>(),depth+1)?; let mut output=Vec::with_capacity((value.entries).len()); for item in &value.entries { output.push(reify_core_dyn_entry(item,reifier,depth+1)?); } output }, cleanup: match &value.cleanup { Some(item)=>Some(reify_core_dyn_entry(item,reifier,depth+1)?), None=>None }, owned_storage: match &value.owned_storage { Some(item)=>Some(reify_dyn_owned_storage(item,reifier,depth+1)?), None=>None } })
}
pub fn reify_dyn_vtable<R:CtDynamicReifier>(value:&DynVtable,reifier:&mut R,depth:usize)->Result<CtDynVtable,R::Error>{ reifier.node(depth)?;
Ok(CtDynVtable { self_life: match &value.self_life { Some(item)=>Some(reify_storage_life_shape(item,reifier,depth+1)?), None=>None }, implementation_witnesses: match &value.implementation_witnesses { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynImplementationWitness>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_implementation_witness(item,reifier,depth+1)?); } output }), None=>None }, self_classes: { reifier.rows((value.self_classes).len(),std::mem::size_of::<StorageBinder>(),depth+1)?; let mut output=Vec::with_capacity((value.self_classes).len()); for item in &value.self_classes { output.push({ reifier.node(depth+1)?; *(item) }); } output }, id: reify_dyn_vtable_id(&value.id,reifier,depth+1)?, interface: reify_dyn_interface_id(&value.interface,reifier,depth+1)?, self_type: reify_type_value(&value.self_type,reifier,depth+1)?, implementation: reify_life_declaration_id(&value.implementation,reifier,depth+1)?, actuals: { reifier.rows((value.actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.actuals).len()); for item in &value.actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, ports: { reifier.rows((value.ports).len(),std::mem::size_of::<CtDynRetainedPortBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.ports).len()); for item in &value.ports { output.push(reify_dyn_retained_port_binding(item,reifier,depth+1)?); } output }, entries: { reifier.rows((value.entries).len(),std::mem::size_of::<CtDynEntry>(),depth+1)?; let mut output=Vec::with_capacity((value.entries).len()); for item in &value.entries { output.push(reify_dyn_entry(item,reifier,depth+1)?); } output }, cleanup: match &value.cleanup { Some(item)=>Some(reify_dyn_entry(item,reifier,depth+1)?), None=>None }, owned_storage: match &value.owned_storage { Some(item)=>Some(reify_dyn_owned_storage(item,reifier,depth+1)?), None=>None } })
}
pub fn reify_dyn_owned_storage<R:CtDynamicReifier>(value:&DynOwnedStorage,reifier:&mut R,depth:usize)->Result<CtDynOwnedStorage,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynOwnedStorage::Heap => CtDynOwnedStorage::Heap,
})
}
pub fn reify_dyn_requirement<R:CtDynamicReifier>(value:&DynRequirement,reifier:&mut R,depth:usize)->Result<CtDynRequirement,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynRequirement::Read(field_value_0) => CtDynRequirement::Read(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirement::Write(field_value_0) => CtDynRequirement::Write(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirement::Manage(field_value_0) => CtDynRequirement::Manage(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirement::Apart(field_value_0, field_value_1) => CtDynRequirement::Apart(reify_storage_position(field_value_0,reifier,depth+1)?, reify_storage_position(field_value_1,reifier,depth+1)?),
DynRequirement::State { position: field_position, ty: field_ty, variant: field_variant } => CtDynRequirement::State { position: reify_storage_position(field_position,reifier,depth+1)?, ty: reify_type_value(field_ty,reifier,depth+1)?, variant: { reifier.node(depth+1)?; *(field_variant) } },
})
}
pub fn reify_dyn_guarantee<R:CtDynamicReifier>(value:&DynGuarantee,reifier:&mut R,depth:usize)->Result<CtDynGuarantee,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynGuarantee::StorageSources(field_value_0) => CtDynGuarantee::StorageSources(reify_storage_source_binding(field_value_0,reifier,depth+1)?),
DynGuarantee::ManageResult => CtDynGuarantee::ManageResult,
DynGuarantee::State { position: field_position, ty: field_ty, variant: field_variant } => CtDynGuarantee::State { position: reify_storage_position(field_position,reifier,depth+1)?, ty: reify_type_value(field_ty,reifier,depth+1)?, variant: { reifier.node(depth+1)?; *(field_variant) } },
})
}
pub fn reify_dyn_effect<R:CtDynamicReifier>(value:&DynEffect,reifier:&mut R,depth:usize)->Result<CtDynEffect,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynEffect::Alloc => CtDynEffect::Alloc,
DynEffect::Io => CtDynEffect::Io,
DynEffect::Block => CtDynEffect::Block,
DynEffect::Suspend => CtDynEffect::Suspend,
DynEffect::Spawn => CtDynEffect::Spawn,
DynEffect::Ffi => CtDynEffect::Ffi,
DynEffect::Reenter => CtDynEffect::Reenter,
DynEffect::Panic => CtDynEffect::Panic,
DynEffect::Call(field_value_0) => CtDynEffect::Call(reify_storage_position(field_value_0,reifier,depth+1)?),
DynEffect::Cleanup(field_value_0) => CtDynEffect::Cleanup(reify_type_value(field_value_0,reifier,depth+1)?),
})
}
pub fn reify_dyn_call_access<R:CtDynamicReifier>(value:&DynCallAccess,reifier:&mut R,depth:usize)->Result<CtDynCallAccess,R::Error>{ reifier.node(depth)?;
Ok(CtDynCallAccess { reads: { reifier.rows((value.reads).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.reads).len()); for item in &value.reads { output.push(reify_storage_position(item,reifier,depth+1)?); } output }, writes: { reifier.rows((value.writes).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.writes).len()); for item in &value.writes { output.push(reify_storage_position(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_result_source<R:CtDynamicReifier>(value:&DynResultSource,reifier:&mut R,depth:usize)->Result<CtDynResultSource,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynResultSource::Value => CtDynResultSource::Value,
DynResultSource::Create => CtDynResultSource::Create,
DynResultSource::Give => CtDynResultSource::Give,
DynResultSource::Borrow { sources: field_sources, mutable: field_mutable } => CtDynResultSource::Borrow { sources: { reifier.rows((field_sources).len(),std::mem::size_of::<CtStorageSource>(),depth+1)?; let mut output=Vec::with_capacity((field_sources).len()); for item in field_sources { output.push(reify_storage_source(item,reifier,depth+1)?); } output }, mutable: { reifier.node(depth+1)?; *(field_mutable) } },
})
}
pub fn reify_dyn_cleanup_contract<R:CtDynamicReifier>(value:&DynCleanupContract,reifier:&mut R,depth:usize)->Result<CtDynCleanupContract,R::Error>{ reifier.node(depth)?;
Ok(CtDynCleanupContract { environment: { reifier.node(depth+1)?; value.environment }, positions: { reifier.rows((value.positions).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.positions).len()); for item in &value.positions { output.push(reify_storage_position(item,reifier,depth+1)?); } output }, access: reify_dyn_call_access(&value.access,reifier,depth+1)?, effects: { reifier.rows((value.effects).len(),std::mem::size_of::<CtDynEffect>(),depth+1)?; let mut output=Vec::with_capacity((value.effects).len()); for item in &value.effects { output.push(reify_dyn_effect(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_resume_bound<R:CtDynamicReifier>(value:&DynResumeBound,reifier:&mut R,depth:usize)->Result<CtDynResumeBound,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynResumeBound::Local => CtDynResumeBound::Local,
DynResumeBound::Transfer => CtDynResumeBound::Transfer,
})
}
pub fn reify_dyn_thread_requirements<R:CtDynamicReifier>(value:&DynThreadRequirements,reifier:&mut R,depth:usize)->Result<CtDynThreadRequirements,R::Error>{ reifier.node(depth)?;
Ok(CtDynThreadRequirements { transfer: { reifier.node(depth+1)?; value.transfer }, share: { reifier.node(depth+1)?; value.share } })
}
pub fn reify_dyn_contract<R:CtDynamicReifier>(value:&DynContract,reifier:&mut R,depth:usize)->Result<CtDynContract,R::Error>{ reifier.node(depth)?;
Ok(CtDynContract { requires: match &value.requires { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynRequirement>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_requirement(item,reifier,depth+1)?); } output }), None=>None }, guarantees: match &value.guarantees { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynGuarantee>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_guarantee(item,reifier,depth+1)?); } output }), None=>None }, hold: match &value.hold { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_storage_position(item,reifier,depth+1)?); } output }), None=>None }, call: match &value.call { Some(item)=>Some(reify_dyn_call_access(item,reifier,depth+1)?), None=>None }, result: match &value.result { Some(item)=>Some(reify_dyn_result_source(item,reifier,depth+1)?), None=>None }, cleanup: match &value.cleanup { Some(item)=>Some(reify_dyn_cleanup_contract(item,reifier,depth+1)?), None=>None }, effects: match &value.effects { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynEffect>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_effect(item,reifier,depth+1)?); } output }), None=>None }, resume: match &value.resume { Some(item)=>Some(reify_dyn_resume_bound(item,reifier,depth+1)?), None=>None }, threads: match &value.threads { Some(item)=>Some(reify_dyn_thread_requirements(item,reifier,depth+1)?), None=>None } })
}
pub fn reify_dyn_type_value<R:CtDynamicReifier>(value:&DynTypeValue,reifier:&mut R,depth:usize)->Result<CtDynTypeValue,R::Error>{ reifier.node(depth)?;
Ok(CtDynTypeValue { ty: reify_type_value(&value.ty,reifier,depth+1)?, life: reify_storage_life_shape(&value.life,reifier,depth+1)?, classes: { reifier.rows((value.classes).len(),std::mem::size_of::<StorageBinder>(),depth+1)?; let mut output=Vec::with_capacity((value.classes).len()); for item in &value.classes { output.push({ reifier.node(depth+1)?; *(item) }); } output } })
}
pub fn reify_dyn_instantiation_scope<R:CtDynamicReifier>(value:&DynInstantiationScope,reifier:&mut R,depth:usize)->Result<CtDynInstantiationScope,R::Error>{ reifier.node(depth)?;
Ok(CtDynInstantiationScope { fixed_binder_count: { reifier.node(depth+1)?; value.fixed_binder_count }, invocation_binder_count: { reifier.node(depth+1)?; value.invocation_binder_count } })
}
pub fn reify_dyn_life_embedding<R:CtDynamicReifier>(value:&DynLifeEmbedding,reifier:&mut R,depth:usize)->Result<CtDynLifeEmbedding,R::Error>{ reifier.node(depth)?;
Ok(CtDynLifeEmbedding { from: { reifier.node(depth+1)?; value.from }, to: { reifier.node(depth+1)?; value.to } })
}
pub fn reify_dyn_implicit_life_kind<R:CtDynamicReifier>(value:&DynImplicitLifeKind,reifier:&mut R,depth:usize)->Result<CtDynImplicitLifeKind,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynImplicitLifeKind::Address => CtDynImplicitLifeKind::Address,
DynImplicitLifeKind::Description => CtDynImplicitLifeKind::Description,
DynImplicitLifeKind::LifetimeArgument => CtDynImplicitLifeKind::LifetimeArgument,
})
}
pub fn reify_dyn_implicit_life_root<R:CtDynamicReifier>(value:&DynImplicitLifeRoot,reifier:&mut R,depth:usize)->Result<CtDynImplicitLifeRoot,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynImplicitLifeRoot::SelfPattern => CtDynImplicitLifeRoot::SelfPattern,
DynImplicitLifeRoot::ImplementedTrait => CtDynImplicitLifeRoot::ImplementedTrait,
DynImplicitLifeRoot::Supertrait(field_value_0) => CtDynImplicitLifeRoot::Supertrait({ reifier.node(depth+1)?; *(field_value_0) }),
DynImplicitLifeRoot::Requirement(field_value_0) => CtDynImplicitLifeRoot::Requirement({ reifier.node(depth+1)?; *(field_value_0) }),
DynImplicitLifeRoot::AssociatedDefinition(field_value_0) => CtDynImplicitLifeRoot::AssociatedDefinition({ reifier.node(depth+1)?; *(field_value_0) }),
DynImplicitLifeRoot::MethodSignature(field_value_0) => CtDynImplicitLifeRoot::MethodSignature({ reifier.node(depth+1)?; *(field_value_0) }),
DynImplicitLifeRoot::MethodDefinition(field_value_0) => CtDynImplicitLifeRoot::MethodDefinition({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_dyn_implicit_life_occurrence<R:CtDynamicReifier>(value:&DynImplicitLifeOccurrence,reifier:&mut R,depth:usize)->Result<CtDynImplicitLifeOccurrence,R::Error>{ reifier.node(depth)?;
Ok(CtDynImplicitLifeOccurrence { root: reify_dyn_implicit_life_root(&value.root,reifier,depth+1)?, path: { reifier.rows((value.path).len(),std::mem::size_of::<u32>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output }, kind: reify_dyn_implicit_life_kind(&value.kind,reifier,depth+1)? })
}
pub fn reify_dyn_implicit_life_declaration<R:CtDynamicReifier>(value:&DynImplicitLifeDeclaration,reifier:&mut R,depth:usize)->Result<CtDynImplicitLifeDeclaration,R::Error>{ reifier.node(depth)?;
Ok(CtDynImplicitLifeDeclaration { occurrences: { reifier.rows((value.occurrences).len(),std::mem::size_of::<CtDynImplicitLifeOccurrence>(),depth+1)?; let mut output=Vec::with_capacity((value.occurrences).len()); for item in &value.occurrences { output.push(reify_dyn_implicit_life_occurrence(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_life_pattern<R:CtDynamicReifier>(value:&DynLifePattern,reifier:&mut R,depth:usize)->Result<CtDynLifePattern,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynLifePattern::DeclarationParameter(field_value_0) => CtDynLifePattern::DeclarationParameter({ reifier.node(depth+1)?; *(field_value_0) }),
DynLifePattern::MethodParameter(field_value_0) => CtDynLifePattern::MethodParameter({ reifier.node(depth+1)?; *(field_value_0) }),
DynLifePattern::Bound { depth: field_depth, index: field_index } => CtDynLifePattern::Bound { depth: { reifier.node(depth+1)?; *(field_depth) }, index: { reifier.node(depth+1)?; *(field_index) } },
DynLifePattern::Invocation(field_value_0) => CtDynLifePattern::Invocation({ reifier.node(depth+1)?; *(field_value_0) }),
DynLifePattern::ImplicitDeclaration(field_value_0) => CtDynLifePattern::ImplicitDeclaration({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_dyn_obligation<R:CtDynamicReifier>(value:&DynObligation,reifier:&mut R,depth:usize)->Result<CtDynObligation,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynObligation::Structural { receiver: field_receiver, bound: field_bound } => CtDynObligation::Structural { receiver: reify_dyn_type_pattern(field_receiver,reifier,depth+1)?, bound: reify_dyn_structural_bound(field_bound,reifier,depth+1)? },
DynObligation::Trait { receiver: field_receiver, application: field_application } => CtDynObligation::Trait { receiver: reify_dyn_type_pattern(field_receiver,reifier,depth+1)?, application: reify_dyn_self_pattern(field_application,reifier,depth+1)? },
DynObligation::Callable { receiver: field_receiver, kind: field_kind, quantified_lifetimes: field_quantified_lifetimes, signature: field_signature } => CtDynObligation::Callable { receiver: reify_dyn_type_pattern(field_receiver,reifier,depth+1)?, kind: { reifier.node(depth+1)?; *(field_kind) }, quantified_lifetimes: { reifier.node(depth+1)?; *(field_quantified_lifetimes) }, signature: { reifier.rows(1,std::mem::size_of::<CtDynSignatureRecipe>(),depth+1)?; Box::new(reify_dyn_signature_recipe(field_signature,reifier,depth+1)?) } },
DynObligation::TypeEqual { left: field_left, right: field_right } => CtDynObligation::TypeEqual { left: reify_dyn_type_pattern(field_left,reifier,depth+1)?, right: reify_dyn_type_pattern(field_right,reifier,depth+1)? },
DynObligation::ConstEqual { ty: field_ty, left: field_left, right: field_right } => CtDynObligation::ConstEqual { ty: reify_dyn_type_pattern(field_ty,reifier,depth+1)?, left: reify_dyn_const_pattern(field_left,reifier,depth+1)?, right: reify_dyn_const_pattern(field_right,reifier,depth+1)? },
})
}
pub fn reify_dyn_parameter_recipe<R:CtDynamicReifier>(value:&DynParameterRecipe,reifier:&mut R,depth:usize)->Result<CtDynParameterRecipe,R::Error>{ reifier.node(depth)?;
Ok(CtDynParameterRecipe { ty: reify_dyn_type_pattern(&value.ty,reifier,depth+1)?, mode: { reifier.node(depth+1)?; value.mode } })
}
pub fn reify_dyn_signature_recipe<R:CtDynamicReifier>(value:&DynSignatureRecipe,reifier:&mut R,depth:usize)->Result<CtDynSignatureRecipe,R::Error>{ reifier.node(depth)?;
Ok(CtDynSignatureRecipe { parameters: { reifier.rows((value.parameters).len(),std::mem::size_of::<CtDynParameterRecipe>(),depth+1)?; let mut output=Vec::with_capacity((value.parameters).len()); for item in &value.parameters { output.push(reify_dyn_parameter_recipe(item,reifier,depth+1)?); } output }, result: reify_dyn_type_pattern(&value.result,reifier,depth+1)?, result_mode: { reifier.node(depth+1)?; value.result_mode }, call_kind: { reifier.node(depth+1)?; value.call_kind }, invocation_binder_count: { reifier.node(depth+1)?; value.invocation_binder_count }, contract: reify_dyn_contract_recipe(&value.contract,reifier,depth+1)? })
}
pub fn reify_dyn_requirement_recipe<R:CtDynamicReifier>(value:&DynRequirementRecipe,reifier:&mut R,depth:usize)->Result<CtDynRequirementRecipe,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynRequirementRecipe::Read(field_value_0) => CtDynRequirementRecipe::Read(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirementRecipe::Write(field_value_0) => CtDynRequirementRecipe::Write(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirementRecipe::Manage(field_value_0) => CtDynRequirementRecipe::Manage(reify_storage_position(field_value_0,reifier,depth+1)?),
DynRequirementRecipe::Apart(field_value_0, field_value_1) => CtDynRequirementRecipe::Apart(reify_storage_position(field_value_0,reifier,depth+1)?, reify_storage_position(field_value_1,reifier,depth+1)?),
DynRequirementRecipe::State { position: field_position, ty: field_ty, variant: field_variant } => CtDynRequirementRecipe::State { position: reify_storage_position(field_position,reifier,depth+1)?, ty: reify_dyn_type_pattern(field_ty,reifier,depth+1)?, variant: { reifier.node(depth+1)?; *(field_variant) } },
})
}
pub fn reify_dyn_guarantee_recipe<R:CtDynamicReifier>(value:&DynGuaranteeRecipe,reifier:&mut R,depth:usize)->Result<CtDynGuaranteeRecipe,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynGuaranteeRecipe::StorageSources(field_value_0) => CtDynGuaranteeRecipe::StorageSources(reify_storage_source_binding(field_value_0,reifier,depth+1)?),
DynGuaranteeRecipe::ManageResult => CtDynGuaranteeRecipe::ManageResult,
DynGuaranteeRecipe::State { position: field_position, ty: field_ty, variant: field_variant } => CtDynGuaranteeRecipe::State { position: reify_storage_position(field_position,reifier,depth+1)?, ty: reify_dyn_type_pattern(field_ty,reifier,depth+1)?, variant: { reifier.node(depth+1)?; *(field_variant) } },
})
}
pub fn reify_dyn_effect_recipe<R:CtDynamicReifier>(value:&DynEffectRecipe,reifier:&mut R,depth:usize)->Result<CtDynEffectRecipe,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynEffectRecipe::Alloc => CtDynEffectRecipe::Alloc,
DynEffectRecipe::Io => CtDynEffectRecipe::Io,
DynEffectRecipe::Block => CtDynEffectRecipe::Block,
DynEffectRecipe::Suspend => CtDynEffectRecipe::Suspend,
DynEffectRecipe::Spawn => CtDynEffectRecipe::Spawn,
DynEffectRecipe::Ffi => CtDynEffectRecipe::Ffi,
DynEffectRecipe::Reenter => CtDynEffectRecipe::Reenter,
DynEffectRecipe::Panic => CtDynEffectRecipe::Panic,
DynEffectRecipe::Call(field_value_0) => CtDynEffectRecipe::Call(reify_storage_position(field_value_0,reifier,depth+1)?),
DynEffectRecipe::Cleanup(field_value_0) => CtDynEffectRecipe::Cleanup(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?),
})
}
pub fn reify_dyn_cleanup_recipe<R:CtDynamicReifier>(value:&DynCleanupRecipe,reifier:&mut R,depth:usize)->Result<CtDynCleanupRecipe,R::Error>{ reifier.node(depth)?;
Ok(CtDynCleanupRecipe { environment: { reifier.node(depth+1)?; value.environment }, positions: { reifier.rows((value.positions).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.positions).len()); for item in &value.positions { output.push(reify_storage_position(item,reifier,depth+1)?); } output }, access: reify_dyn_call_access(&value.access,reifier,depth+1)?, effects: { reifier.rows((value.effects).len(),std::mem::size_of::<CtDynEffectRecipe>(),depth+1)?; let mut output=Vec::with_capacity((value.effects).len()); for item in &value.effects { output.push(reify_dyn_effect_recipe(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_contract_recipe<R:CtDynamicReifier>(value:&DynContractRecipe,reifier:&mut R,depth:usize)->Result<CtDynContractRecipe,R::Error>{ reifier.node(depth)?;
Ok(CtDynContractRecipe { requires: match &value.requires { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynRequirementRecipe>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_requirement_recipe(item,reifier,depth+1)?); } output }), None=>None }, guarantees: match &value.guarantees { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynGuaranteeRecipe>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_guarantee_recipe(item,reifier,depth+1)?); } output }), None=>None }, hold: match &value.hold { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_storage_position(item,reifier,depth+1)?); } output }), None=>None }, call: match &value.call { Some(item)=>Some(reify_dyn_call_access(item,reifier,depth+1)?), None=>None }, result: match &value.result { Some(item)=>Some(reify_dyn_result_source(item,reifier,depth+1)?), None=>None }, cleanup: match &value.cleanup { Some(item)=>Some(reify_dyn_cleanup_recipe(item,reifier,depth+1)?), None=>None }, effects: match &value.effects { Some(item)=>Some({ reifier.rows((item).len(),std::mem::size_of::<CtDynEffectRecipe>(),depth+1)?; let mut output=Vec::with_capacity((item).len()); for item in item { output.push(reify_dyn_effect_recipe(item,reifier,depth+1)?); } output }), None=>None }, resume: match &value.resume { Some(item)=>Some(reify_dyn_resume_bound(item,reifier,depth+1)?), None=>None }, threads: match &value.threads { Some(item)=>Some(reify_dyn_thread_requirements(item,reifier,depth+1)?), None=>None } })
}
pub fn reify_dyn_definition_id<R:CtDynamicReifier>(value:&DynDefinitionId,reifier:&mut R,depth:usize)->Result<CtDynDefinitionId,R::Error>{ reifier.node(depth)?;
Ok(CtDynDefinitionId { module: { reifier.node(depth+1)?; value.module }, source: { reifier.node(depth+1)?; value.source } })
}
pub fn reify_dyn_definition_scope<R:CtDynamicReifier>(value:&DynDefinitionScope,reifier:&mut R,depth:usize)->Result<CtDynDefinitionScope,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynDefinitionScope::TraitDefault => CtDynDefinitionScope::TraitDefault,
DynDefinitionScope::ImplOverride => CtDynDefinitionScope::ImplOverride,
})
}
pub fn reify_dyn_definition_declaration<R:CtDynamicReifier>(value:&DynDefinitionDeclaration,reifier:&mut R,depth:usize)->Result<CtDynDefinitionDeclaration,R::Error>{ reifier.node(depth)?;
Ok(CtDynDefinitionDeclaration { requirements: { reifier.rows((value.requirements).len(),std::mem::size_of::<CtDynObligation>(),depth+1)?; let mut output=Vec::with_capacity((value.requirements).len()); for item in &value.requirements { output.push(reify_dyn_obligation(item,reifier,depth+1)?); } output }, instances: { reifier.rows((value.instances).len(),std::mem::size_of::<CtDynDefinitionSpecialization>(),depth+1)?; let mut output=Vec::with_capacity((value.instances).len()); for item in &value.instances { output.push(reify_dyn_definition_specialization(item,reifier,depth+1)?); } output }, id: reify_dyn_definition_id(&value.id,reifier,depth+1)?, identity: reify_life_declaration_identity(&value.identity,reifier,depth+1)?, member: { reifier.node(depth+1)?; value.member }, scope: reify_dyn_definition_scope(&value.scope,reifier,depth+1)?, signature: reify_dyn_signature_recipe(&value.signature,reifier,depth+1)? })
}
pub fn reify_dyn_method_recipe<R:CtDynamicReifier>(value:&DynMethodRecipe,reifier:&mut R,depth:usize)->Result<CtDynMethodRecipe,R::Error>{ reifier.node(depth)?;
Ok(CtDynMethodRecipe { requirements: { reifier.rows((value.requirements).len(),std::mem::size_of::<CtDynObligation>(),depth+1)?; let mut output=Vec::with_capacity((value.requirements).len()); for item in &value.requirements { output.push(reify_dyn_obligation(item,reifier,depth+1)?); } output }, member: { reifier.node(depth+1)?; value.member }, signature: reify_dyn_signature_recipe(&value.signature,reifier,depth+1)?, default_definition: match &value.default_definition { Some(item)=>Some(reify_dyn_definition_id(item,reifier,depth+1)?), None=>None } })
}
pub fn reify_dyn_function_instance<R:CtDynamicReifier>(value:&DynFunctionInstance,reifier:&mut R,depth:usize)->Result<CtDynFunctionInstance,R::Error>{ reifier.node(depth)?;
Ok(CtDynFunctionInstance { scope: reify_dyn_instantiation_scope(&value.scope,reifier,depth+1)?, header_embedding: { reifier.rows((value.header_embedding).len(),std::mem::size_of::<CtDynLifeEmbedding>(),depth+1)?; let mut output=Vec::with_capacity((value.header_embedding).len()); for item in &value.header_embedding { output.push(reify_dyn_life_embedding(item,reifier,depth+1)?); } output }, definition: reify_dyn_definition_id(&value.definition,reifier,depth+1)?, receiver: reify_dyn_type_value(&value.receiver,reifier,depth+1)?, declaration_actuals: { reifier.rows((value.declaration_actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.declaration_actuals).len()); for item in &value.declaration_actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, method_actuals: { reifier.rows((value.method_actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.method_actuals).len()); for item in &value.method_actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_associated_value_recipe<R:CtDynamicReifier>(value:&DynAssociatedValueRecipe,reifier:&mut R,depth:usize)->Result<CtDynAssociatedValueRecipe,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynAssociatedValueRecipe::Type(field_value_0) => CtDynAssociatedValueRecipe::Type(reify_dyn_type_pattern(field_value_0,reifier,depth+1)?),
DynAssociatedValueRecipe::Constant { ty: field_ty, value: field_value } => CtDynAssociatedValueRecipe::Constant { ty: reify_dyn_type_pattern(field_ty,reifier,depth+1)?, value: reify_dyn_const_pattern(field_value,reifier,depth+1)? },
})
}
pub fn reify_dyn_associated_definition<R:CtDynamicReifier>(value:&DynAssociatedDefinition,reifier:&mut R,depth:usize)->Result<CtDynAssociatedDefinition,R::Error>{ reifier.node(depth)?;
Ok(CtDynAssociatedDefinition { carrier: match &value.carrier { Some(item)=>Some(reify_dyn_type_pattern(item,reifier,depth+1)?), None=>None }, member: { reifier.node(depth+1)?; value.member }, value: match &value.value { Some(item)=>Some(reify_dyn_associated_value_recipe(item,reifier,depth+1)?), None=>None }, requirements: { reifier.rows((value.requirements).len(),std::mem::size_of::<CtDynObligation>(),depth+1)?; let mut output=Vec::with_capacity((value.requirements).len()); for item in &value.requirements { output.push(reify_dyn_obligation(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_associated_source<R:CtDynamicReifier>(value:&DynAssociatedSource,reifier:&mut R,depth:usize)->Result<CtDynAssociatedSource,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynAssociatedSource::TraitDefault { declaration: field_declaration, member: field_member } => CtDynAssociatedSource::TraitDefault { declaration: reify_life_declaration_id(field_declaration,reifier,depth+1)?, member: { reifier.node(depth+1)?; *(field_member) } },
DynAssociatedSource::ImplOverride { declaration: field_declaration, member: field_member } => CtDynAssociatedSource::ImplOverride { declaration: reify_life_declaration_id(field_declaration,reifier,depth+1)?, member: { reifier.node(depth+1)?; *(field_member) } },
})
}
pub fn reify_dyn_associated_value<R:CtDynamicReifier>(value:&DynAssociatedValue,reifier:&mut R,depth:usize)->Result<CtDynAssociatedValue,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynAssociatedValue::Type(field_value_0) => CtDynAssociatedValue::Type(reify_dyn_type_value(field_value_0,reifier,depth+1)?),
DynAssociatedValue::Constant { ty: field_ty, value: field_value } => CtDynAssociatedValue::Constant { ty: reify_dyn_type_value(field_ty,reifier,depth+1)?, value: reifier.constant(field_value,depth+1)? },
})
}
pub fn reify_dyn_associated_resolution<R:CtDynamicReifier>(value:&DynAssociatedResolution,reifier:&mut R,depth:usize)->Result<CtDynAssociatedResolution,R::Error>{ reifier.node(depth)?;
Ok(CtDynAssociatedResolution { member: reify_dyn_member(&value.member,reifier,depth+1)?, source: reify_dyn_associated_source(&value.source,reifier,depth+1)?, value: reify_dyn_associated_value(&value.value,reifier,depth+1)? })
}
pub fn reify_dyn_implementation_witness<R:CtDynamicReifier>(value:&DynImplementationWitness,reifier:&mut R,depth:usize)->Result<CtDynImplementationWitness,R::Error>{ reifier.node(depth)?;
Ok(CtDynImplementationWitness { scope: reify_dyn_instantiation_scope(&value.scope,reifier,depth+1)?, to_context: { reifier.rows((value.to_context).len(),std::mem::size_of::<CtDynLifeEmbedding>(),depth+1)?; let mut output=Vec::with_capacity((value.to_context).len()); for item in &value.to_context { output.push(reify_dyn_life_embedding(item,reifier,depth+1)?); } output }, receiver: reify_dyn_type_value(&value.receiver,reifier,depth+1)?, application: reify_dyn_trait_application(&value.application,reifier,depth+1)?, implementation: reify_life_declaration_id(&value.implementation,reifier,depth+1)?, actuals: { reifier.rows((value.actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.actuals).len()); for item in &value.actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, associated: { reifier.rows((value.associated).len(),std::mem::size_of::<CtDynAssociatedResolution>(),depth+1)?; let mut output=Vec::with_capacity((value.associated).len()); for item in &value.associated { output.push(reify_dyn_associated_resolution(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_function_target<R:CtDynamicReifier>(value:&DynFunctionTarget,reifier:&mut R,depth:usize)->Result<CtDynFunctionTarget,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynFunctionTarget::Core { module: field_module, source: field_source } => CtDynFunctionTarget::Core { module: { reifier.node(depth+1)?; *(field_module) }, source: { reifier.node(depth+1)?; *(field_source) } },
DynFunctionTarget::Typed(field_value_0) => CtDynFunctionTarget::Typed({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_dyn_definition_specialization<R:CtDynamicReifier>(value:&DynDefinitionSpecialization,reifier:&mut R,depth:usize)->Result<CtDynDefinitionSpecialization,R::Error>{ reifier.node(depth)?;
Ok(CtDynDefinitionSpecialization { function: reify_dyn_function_target(&value.function,reifier,depth+1)?, receiver: reify_dyn_type_value(&value.receiver,reifier,depth+1)?, declaration_actuals: { reifier.rows((value.declaration_actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.declaration_actuals).len()); for item in &value.declaration_actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output }, method_actuals: { reifier.rows((value.method_actuals).len(),std::mem::size_of::<CtDynGenericActual>(),depth+1)?; let mut output=Vec::with_capacity((value.method_actuals).len()); for item in &value.method_actuals { output.push(reify_dyn_generic_actual(item,reifier,depth+1)?); } output } })
}
pub fn reify_dyn_structural_bound<R:CtDynamicReifier>(value:&DynStructuralBound,reifier:&mut R,depth:usize)->Result<CtDynStructuralBound,R::Error>{ reifier.node(depth)?;
Ok(match value {
DynStructuralBound::Copy => CtDynStructuralBound::Copy,
DynStructuralBound::Value => CtDynStructuralBound::Value,
DynStructuralBound::SyncDrop => CtDynStructuralBound::SyncDrop,
DynStructuralBound::StoreElement => CtDynStructuralBound::StoreElement,
})
}
pub fn reify_storage_source<R:CtDynamicReifier>(value:&StorageSource,reifier:&mut R,depth:usize)->Result<CtStorageSource,R::Error>{ reifier.node(depth)?;
Ok(match value {
StorageSource::Exact(field_value_0) => CtStorageSource::Exact(reify_storage_position(field_value_0,reifier,depth+1)?),
StorageSource::PlaceAddress { place: field_place, mode: field_mode } => CtStorageSource::PlaceAddress { place: reify_storage_position(field_place,reifier,depth+1)?, mode: { reifier.node(depth+1)?; *(field_mode) } },
StorageSource::OwnedView { head: field_head, leaf: field_leaf, mode: field_mode } => CtStorageSource::OwnedView { head: reify_storage_position(field_head,reifier,depth+1)?, leaf: reify_type_value(field_leaf,reifier,depth+1)?, mode: { reifier.node(depth+1)?; *(field_mode) } },
StorageSource::ReborrowedLeaf { source: field_source, head: field_head } => CtStorageSource::ReborrowedLeaf { source: reify_storage_position(field_source,reifier,depth+1)?, head: reify_storage_position(field_head,reifier,depth+1)? },
StorageSource::Static { leaf: field_leaf, mode: field_mode } => CtStorageSource::Static { leaf: reify_type_value(field_leaf,reifier,depth+1)?, mode: { reifier.node(depth+1)?; *(field_mode) } },
})
}
pub fn reify_retained_borrow_port<R:CtDynamicReifier>(value:&RetainedBorrowPort,reifier:&mut R,depth:usize)->Result<CtRetainedBorrowPort,R::Error>{ reifier.node(depth)?;
Ok(CtRetainedBorrowPort { leaf: reify_type_value(&value.leaf,reifier,depth+1)?, mode: { reifier.node(depth+1)?; value.mode }, binder: { reifier.node(depth+1)?; value.binder } })
}
pub fn reify_retained_life_shape<R:CtDynamicReifier>(value:&RetainedLifeShape,reifier:&mut R,depth:usize)->Result<CtRetainedLifeShape,R::Error>{ reifier.node(depth)?;
Ok(CtRetainedLifeShape { binder_count: { reifier.node(depth+1)?; value.binder_count }, ports: { reifier.rows((value.ports).len(),std::mem::size_of::<CtRetainedBorrowPort>(),depth+1)?; let mut output=Vec::with_capacity((value.ports).len()); for item in &value.ports { output.push(reify_retained_borrow_port(item,reifier,depth+1)?); } output }, retained: { reifier.rows((value.retained).len(),std::mem::size_of::<StorageRetainedBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.retained).len()); for item in &value.retained { output.push(reify_storage_retained_binding(item,reifier,depth+1)?); } output }, nominal_arguments: { reifier.rows((value.nominal_arguments).len(),std::mem::size_of::<NominalLifeArgument>(),depth+1)?; let mut output=Vec::with_capacity((value.nominal_arguments).len()); for item in &value.nominal_arguments { output.push(reify_nominal_life_argument(item,reifier,depth+1)?); } output } })
}
pub fn reify_storage_source_binding<R:CtDynamicReifier>(value:&StorageSourceBinding,reifier:&mut R,depth:usize)->Result<CtStorageSourceBinding,R::Error>{ reifier.node(depth)?;
Ok(CtStorageSourceBinding { position: reify_storage_position(&value.position,reifier,depth+1)?, sources: { reifier.rows((value.sources).len(),std::mem::size_of::<CtStorageSource>(),depth+1)?; let mut output=Vec::with_capacity((value.sources).len()); for item in &value.sources { output.push(reify_storage_source(item,reifier,depth+1)?); } output } })
}
pub fn reify_life_nominal_kind<R:CtDynamicReifier>(value:&LifeNominalKind,reifier:&mut R,depth:usize)->Result<CtLifeNominalKind,R::Error>{ reifier.node(depth)?;
Ok(match value {
LifeNominalKind::Record => CtLifeNominalKind::Record,
LifeNominalKind::Package => CtLifeNominalKind::Package,
LifeNominalKind::Resource => CtLifeNominalKind::Resource,
LifeNominalKind::Enum => CtLifeNominalKind::Enum,
})
}
pub fn reify_life_parameter_kind<R:CtDynamicReifier>(value:&LifeParameterKind,reifier:&mut R,depth:usize)->Result<CtLifeParameterKind,R::Error>{ reifier.node(depth)?;
Ok(match value {
LifeParameterKind::Type => CtLifeParameterKind::Type,
LifeParameterKind::Const(field_value_0) => CtLifeParameterKind::Const(reify_type_value(field_value_0,reifier,depth+1)?),
LifeParameterKind::Lifetime => CtLifeParameterKind::Lifetime,
})
}
pub fn reify_life_nominal_declaration<R:CtDynamicReifier>(value:&LifeNominalDeclaration,reifier:&mut R,depth:usize)->Result<CtLifeNominalDeclaration,R::Error>{ reifier.node(depth)?;
Ok(CtLifeNominalDeclaration { id: reify_life_declaration_id(&value.id,reifier,depth+1)?, identity: reify_life_declaration_identity(&value.identity,reifier,depth+1)?, kind: reify_life_nominal_kind(&value.kind,reifier,depth+1)?, parameters: { reifier.rows((value.parameters).len(),std::mem::size_of::<CtLifeParameterKind>(),depth+1)?; let mut output=Vec::with_capacity((value.parameters).len()); for item in &value.parameters { output.push(reify_life_parameter_kind(item,reifier,depth+1)?); } output } })
}
pub fn reify_life_nominal_actual<R:CtDynamicReifier>(value:&LifeNominalActual,reifier:&mut R,depth:usize)->Result<CtLifeNominalActual,R::Error>{ reifier.node(depth)?;
Ok(match value {
LifeNominalActual::Type(field_value_0) => CtLifeNominalActual::Type(reify_type_value(field_value_0,reifier,depth+1)?),
LifeNominalActual::QualifiedType { ty: field_ty, life: field_life } => CtLifeNominalActual::QualifiedType { ty: reify_type_value(field_ty,reifier,depth+1)?, life: reify_storage_life_shape(field_life,reifier,depth+1)? },
LifeNominalActual::Const(field_value_0) => CtLifeNominalActual::Const(reifier.constant(field_value_0,depth+1)?),
LifeNominalActual::Lifetime(field_value_0) => CtLifeNominalActual::Lifetime({ reifier.node(depth+1)?; *(field_value_0) }),
})
}
pub fn reify_life_nominal_instance<R:CtDynamicReifier>(value:&LifeNominalInstance,reifier:&mut R,depth:usize)->Result<CtLifeNominalInstance,R::Error>{ reifier.node(depth)?;
Ok(CtLifeNominalInstance { declaration: reify_life_declaration_id(&value.declaration,reifier,depth+1)?, ty: reify_type_value(&value.ty,reifier,depth+1)?, arguments: { reifier.rows((value.arguments).len(),std::mem::size_of::<CtLifeNominalActual>(),depth+1)?; let mut output=Vec::with_capacity((value.arguments).len()); for item in &value.arguments { output.push(reify_life_nominal_actual(item,reifier,depth+1)?); } output } })
}
pub fn reify_type<R:CtDynamicReifier>(value:&Type,reifier:&mut R,depth:usize)->Result<CtDynamicTypeNode,R::Error>{ reifier.node(depth)?;
Ok(match value {
Type::Dyn { interface: field_interface } => CtDynamicTypeNode::Dyn { interface: reify_dyn_interface_id(field_interface,reifier,depth+1)? },
Type::OwnedDyn { interface: field_interface } => CtDynamicTypeNode::OwnedDyn { interface: reify_dyn_interface_id(field_interface,reifier,depth+1)? },
Type::BorrowedStorage { storage: field_storage, life: field_life } => CtDynamicTypeNode::BorrowedStorage { storage: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_storage,reifier,depth+1)?) }, life: reify_storage_life_shape(field_life,reifier,depth+1)? },
Type::Unit => CtDynamicTypeNode::Unit,
Type::Bool => CtDynamicTypeNode::Bool,
Type::I32 => CtDynamicTypeNode::I32,
Type::I64 => CtDynamicTypeNode::I64,
Type::U64 => CtDynamicTypeNode::U64,
Type::Usize => CtDynamicTypeNode::Usize,
Type::F32 => CtDynamicTypeNode::F32,
Type::F64 => CtDynamicTypeNode::F64,
Type::U8 => CtDynamicTypeNode::U8,
Type::U16 => CtDynamicTypeNode::U16,
Type::U32 => CtDynamicTypeNode::U32,
Type::I8 => CtDynamicTypeNode::I8,
Type::I16 => CtDynamicTypeNode::I16,
Type::Isize => CtDynamicTypeNode::Isize,
Type::ByteBuffer => CtDynamicTypeNode::ByteBuffer,
Type::OwnedStore { element: field_element } => CtDynamicTypeNode::OwnedStore { element: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_element,reifier,depth+1)?) } },
Type::ForeignHandle => CtDynamicTypeNode::ForeignHandle,
Type::SyncCell(field_value_0) => CtDynamicTypeNode::SyncCell({ reifier.node(depth+1)?; *(field_value_0) }),
Type::Mutex { payload: field_payload } => CtDynamicTypeNode::Mutex { payload: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_payload,reifier,depth+1)?) } },
Type::MutexGuard { payload: field_payload, retained_life: field_retained_life } => CtDynamicTypeNode::MutexGuard { payload: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_payload,reifier,depth+1)?) }, retained_life: reify_retained_life_shape(field_retained_life,reifier,depth+1)? },
Type::Registration { environment: field_environment } => CtDynamicTypeNode::Registration { environment: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_environment,reifier,depth+1)?) } },
Type::AsyncRegistration { environment: field_environment } => CtDynamicTypeNode::AsyncRegistration { environment: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_environment,reifier,depth+1)?) } },
Type::TaskI32 => CtDynamicTypeNode::TaskI32,
Type::Task { result: field_result, retained_life: field_retained_life } => CtDynamicTypeNode::Task { result: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_result,reifier,depth+1)?) }, retained_life: reify_retained_life_shape(field_retained_life,reifier,depth+1)? },
Type::CWideString => CtDynamicTypeNode::CWideString,
Type::ForeignCallback { parameters: field_parameters, result: field_result } => CtDynamicTypeNode::ForeignCallback { parameters: { reifier.rows((field_parameters).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity((field_parameters).len()); for item in field_parameters { output.push(reify_type_value(item,reifier,depth+1)?); } output }, result: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_result,reifier,depth+1)?) } },
Type::Tuple(field_value_0) => CtDynamicTypeNode::Tuple({ reifier.rows((field_value_0).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity((field_value_0).len()); for item in field_value_0 { output.push(reify_type_value(item,reifier,depth+1)?); } output }),
Type::Array { element: field_element, length: field_length } => CtDynamicTypeNode::Array { element: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_element,reifier,depth+1)?) }, length: { reifier.node(depth+1)?; *(field_length) } },
Type::Slice { element: field_element } => CtDynamicTypeNode::Slice { element: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_element,reifier,depth+1)?) } },
Type::Named(field_value_0) => CtDynamicTypeNode::Named(reifier.text(field_value_0,depth+1)?),
Type::Record { name: field_name, fields: field_fields } => CtDynamicTypeNode::Record { name: reifier.text(field_name,depth+1)?, fields: { reifier.rows((field_fields).len(),std::mem::size_of::<(String, CtDynamicTypeId)>(),depth+1)?; let mut output=Vec::with_capacity((field_fields).len()); for item in field_fields { output.push((reifier.text(&(item).0,depth+1)?, reify_type_value(&(item).1,reifier,depth+1)?)); } output } },
Type::Package { name: field_name, fields: field_fields } => CtDynamicTypeNode::Package { name: reifier.text(field_name,depth+1)?, fields: { reifier.rows((field_fields).len(),std::mem::size_of::<(String, CtDynamicTypeId)>(),depth+1)?; let mut output=Vec::with_capacity((field_fields).len()); for item in field_fields { output.push((reifier.text(&(item).0,depth+1)?, reify_type_value(&(item).1,reifier,depth+1)?)); } output } },
Type::Enum { name: field_name, variants: field_variants } => CtDynamicTypeNode::Enum { name: reifier.text(field_name,depth+1)?, variants: { reifier.rows((field_variants).len(),std::mem::size_of::<(String, Vec<CtDynamicTypeId>)>(),depth+1)?; let mut output=Vec::with_capacity((field_variants).len()); for item in field_variants { output.push((reifier.text(&(item).0,depth+1)?, { reifier.rows(((item).1).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity(((item).1).len()); for item in &(item).1 { output.push(reify_type_value(item,reifier,depth+1)?); } output })); } output } },
Type::ErrorUnion { error: field_error, result: field_result } => CtDynamicTypeNode::ErrorUnion { error: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_error,reifier,depth+1)?) }, result: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_result,reifier,depth+1)?) } },
Type::Address { mutable: field_mutable, inner: field_inner } => CtDynamicTypeNode::Address { mutable: { reifier.node(depth+1)?; *(field_mutable) }, inner: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_inner,reifier,depth+1)?) } },
Type::Managed(field_value_0) => CtDynamicTypeNode::Managed({ reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_value_0,reifier,depth+1)?) }),
Type::Desc(field_value_0) => CtDynamicTypeNode::Desc({ reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_value_0,reifier,depth+1)?) }),
Type::FunctionPointer { parameters: field_parameters, result: field_result } => CtDynamicTypeNode::FunctionPointer { parameters: { reifier.rows((field_parameters).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity((field_parameters).len()); for item in field_parameters { output.push(reify_type_value(item,reifier,depth+1)?); } output }, result: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_result,reifier,depth+1)?) } },
Type::BorrowedFunctionPointer { parameters: field_parameters, result: field_result, life: field_life } => CtDynamicTypeNode::BorrowedFunctionPointer { parameters: { reifier.rows((field_parameters).len(),std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; let mut output=Vec::with_capacity((field_parameters).len()); for item in field_parameters { output.push(reify_type_value(item,reifier,depth+1)?); } output }, result: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_result,reifier,depth+1)?) }, life: reify_storage_life_contract(field_life,reifier,depth+1)? },
Type::Closure(field_value_0) => CtDynamicTypeNode::Closure({ reifier.node(depth+1)?; *(field_value_0) }),
Type::Opaque { identity: field_identity, hidden: field_hidden } => CtDynamicTypeNode::Opaque { identity: { reifier.node(depth+1)?; *(field_identity) }, hidden: { reifier.rows(1,std::mem::size_of::<CtDynamicTypeId>(),depth+1)?; Box::new(reify_type_value(field_hidden,reifier,depth+1)?) } },
Type::Unsupported(field_value_0) => CtDynamicTypeNode::Unsupported(reifier.text(field_value_0,depth+1)?),
})
}
pub fn reify_storage_life_slot<R:CtDynamicReifier>(value:&StorageLifeSlot,reifier:&mut R,depth:usize)->Result<StorageLifeSlot,R::Error>{ reifier.node(depth)?;
Ok(StorageLifeSlot { path: { reifier.rows((value.path).len(),std::mem::size_of::<PlaceProjection>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output }, binder: { reifier.node(depth+1)?; value.binder }, mode: { reifier.node(depth+1)?; value.mode } })
}
pub fn reify_life_declaration_id<R:CtDynamicReifier>(value:&LifeDeclarationId,reifier:&mut R,depth:usize)->Result<LifeDeclarationId,R::Error>{ reifier.node(depth)?;
Ok(LifeDeclarationId { module: { reifier.node(depth+1)?; value.module }, declaration: { reifier.node(depth+1)?; value.declaration }, compiler: { reifier.node(depth+1)?; value.compiler } })
}
pub fn reify_storage_retained_binding<R:CtDynamicReifier>(value:&StorageRetainedBinding,reifier:&mut R,depth:usize)->Result<StorageRetainedBinding,R::Error>{ reifier.node(depth)?;
Ok(StorageRetainedBinding { path: { reifier.rows((value.path).len(),std::mem::size_of::<LifeTypeProjection>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output }, class: { reifier.node(depth+1)?; value.class }, binder: { reifier.node(depth+1)?; value.binder } })
}
pub fn reify_nominal_life_argument<R:CtDynamicReifier>(value:&NominalLifeArgument,reifier:&mut R,depth:usize)->Result<NominalLifeArgument,R::Error>{ reifier.node(depth)?;
Ok(NominalLifeArgument { path: { reifier.rows((value.path).len(),std::mem::size_of::<LifeTypeProjection>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output }, declaration: reify_life_declaration_id(&value.declaration,reifier,depth+1)?, argument: { reifier.node(depth+1)?; value.argument }, binder: { reifier.node(depth+1)?; value.binder } })
}
pub fn reify_storage_life_shape<R:CtDynamicReifier>(value:&StorageLifeShape,reifier:&mut R,depth:usize)->Result<StorageLifeShape,R::Error>{ reifier.node(depth)?;
Ok(StorageLifeShape { binder_count: { reifier.node(depth+1)?; value.binder_count }, slots: { reifier.rows((value.slots).len(),std::mem::size_of::<StorageLifeSlot>(),depth+1)?; let mut output=Vec::with_capacity((value.slots).len()); for item in &value.slots { output.push(reify_storage_life_slot(item,reifier,depth+1)?); } output }, retained: { reifier.rows((value.retained).len(),std::mem::size_of::<StorageRetainedBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.retained).len()); for item in &value.retained { output.push(reify_storage_retained_binding(item,reifier,depth+1)?); } output }, nominal_arguments: { reifier.rows((value.nominal_arguments).len(),std::mem::size_of::<NominalLifeArgument>(),depth+1)?; let mut output=Vec::with_capacity((value.nominal_arguments).len()); for item in &value.nominal_arguments { output.push(reify_nominal_life_argument(item,reifier,depth+1)?); } output } })
}
pub fn reify_storage_position<R:CtDynamicReifier>(value:&StoragePosition,reifier:&mut R,depth:usize)->Result<StoragePosition,R::Error>{ reifier.node(depth)?;
Ok(StoragePosition { base: { reifier.node(depth+1)?; value.base }, path: { reifier.rows((value.path).len(),std::mem::size_of::<ContractProjection>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output } })
}
pub fn reify_storage_life_binding<R:CtDynamicReifier>(value:&StorageLifeBinding,reifier:&mut R,depth:usize)->Result<StorageLifeBinding,R::Error>{ reifier.node(depth)?;
Ok(StorageLifeBinding { position: reify_storage_position(&value.position,reifier,depth+1)?, binder: { reifier.node(depth+1)?; value.binder } })
}
pub fn reify_signature_life_class<R:CtDynamicReifier>(value:&SignatureLifeClass,reifier:&mut R,depth:usize)->Result<SignatureLifeClass,R::Error>{ reifier.node(depth)?;
Ok(SignatureLifeClass { base: { reifier.node(depth+1)?; value.base }, path: { reifier.rows((value.path).len(),std::mem::size_of::<LifeTypeProjection>(),depth+1)?; let mut output=Vec::with_capacity((value.path).len()); for item in &value.path { output.push({ reifier.node(depth+1)?; *(item) }); } output }, class: { reifier.node(depth+1)?; value.class } })
}
pub fn reify_signature_life_class_binding<R:CtDynamicReifier>(value:&SignatureLifeClassBinding,reifier:&mut R,depth:usize)->Result<SignatureLifeClassBinding,R::Error>{ reifier.node(depth)?;
Ok(SignatureLifeClassBinding { position: reify_signature_life_class(&value.position,reifier,depth+1)?, binder: { reifier.node(depth+1)?; value.binder } })
}
pub fn reify_storage_life_contract<R:CtDynamicReifier>(value:&StorageLifeContract,reifier:&mut R,depth:usize)->Result<StorageLifeContract,R::Error>{ reifier.node(depth)?;
Ok(StorageLifeContract { fixed_binder_count: { reifier.node(depth+1)?; value.fixed_binder_count }, invocation_binder_count: { reifier.node(depth+1)?; value.invocation_binder_count }, bindings: { reifier.rows((value.bindings).len(),std::mem::size_of::<StorageLifeBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.bindings).len()); for item in &value.bindings { output.push(reify_storage_life_binding(item,reifier,depth+1)?); } output }, static_bindings: { reifier.rows((value.static_bindings).len(),std::mem::size_of::<StoragePosition>(),depth+1)?; let mut output=Vec::with_capacity((value.static_bindings).len()); for item in &value.static_bindings { output.push(reify_storage_position(item,reifier,depth+1)?); } output }, classes: { reifier.rows((value.classes).len(),std::mem::size_of::<SignatureLifeClassBinding>(),depth+1)?; let mut output=Vec::with_capacity((value.classes).len()); for item in &value.classes { output.push(reify_signature_life_class_binding(item,reifier,depth+1)?); } output } })
}
pub fn reify_life_declaration_identity<R:CtDynamicReifier>(value:&LifeDeclarationIdentity,reifier:&mut R,depth:usize)->Result<LifeDeclarationIdentity,R::Error>{ reifier.node(depth)?;
Ok(LifeDeclarationIdentity { package: reifier.text(&value.package,depth+1)?, module_path: { reifier.rows((value.module_path).len(),std::mem::size_of::<String>(),depth+1)?; let mut output=Vec::with_capacity((value.module_path).len()); for item in &value.module_path { output.push(reifier.text(item,depth+1)?); } output }, declaration_path: { reifier.rows((value.declaration_path).len(),std::mem::size_of::<String>(),depth+1)?; let mut output=Vec::with_capacity((value.declaration_path).len()); for item in &value.declaration_path { output.push(reifier.text(item,depth+1)?); } output }, generation_path: { reifier.rows((value.generation_path).len(),std::mem::size_of::<u32>(),depth+1)?; let mut output=Vec::with_capacity((value.generation_path).len()); for item in &value.generation_path { output.push({ reifier.node(depth+1)?; *(item) }); } output } })
}
pub fn reify_type_value<R:CtDynamicReifier>(value:&Type,reifier:&mut R,depth:usize)->Result<CtDynamicTypeId,R::Error>{
 let node=reify_type(value,reifier,depth)?;
 reifier.store_type(node,depth)
}
