//! Structural reification of frozen runtime metadata into raw CT data.
//! Budget callbacks supply no semantic authority. Every type is a real DAG node.
use super::*;
use crate::*;
pub trait CtDynamicReifier {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize)
    -> Result<(), Self::Error>;
    fn text(&mut self, text: &str, depth: usize) -> Result<String, Self::Error>;
    fn constant(&mut self, value: &Constant, depth: usize) -> Result<Constant, Self::Error>;
    fn store_type(
        &mut self,
        node: CtDynamicTypeNode,
        depth: usize,
    ) -> Result<CtDynamicTypeId, Self::Error>;
}
pub fn reify_dyn_interface_id<R: CtDynamicReifier>(
    value: &DynInterfaceId,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynInterfaceId, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynInterfaceId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        index: {
            reifier.node(depth + 1)?;
            value.index
        },
    })
}
pub fn reify_dyn_vtable_id<R: CtDynamicReifier>(
    value: &DynVtableId,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynVtableId, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynVtableId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        index: {
            reifier.node(depth + 1)?;
            value.index
        },
    })
}
pub fn reify_dyn_declaration<R: CtDynamicReifier>(
    value: &DynDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynDeclaration {
        implicit_lifetimes: match &value.implicit_lifetimes {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynImplicitLifeDeclaration>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_implicit_life_declaration(
                        item,
                        reifier,
                        depth + 1,
                    )?);
                }
                output
            }),
            None => None,
        },
        direct_supertraits: match &value.direct_supertraits {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynSelfPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_self_pattern(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        requirements: match &value.requirements {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynObligation>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_obligation(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        associated_definitions: match &value.associated_definitions {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynAssociatedDefinition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_associated_definition(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        method_signatures: match &value.method_signatures {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynMethodRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_method_recipe(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        method_definitions: match &value.method_definitions {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynDefinitionDeclaration>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_definition_declaration(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_pattern: match &value.self_pattern {
            Some(item) => Some(reify_dyn_type_pattern(item, reifier, depth + 1)?),
            None => None,
        },
        implemented_trait: match &value.implemented_trait {
            Some(item) => Some(reify_dyn_self_pattern(item, reifier, depth + 1)?),
            None => None,
        },
        id: reify_life_declaration_id(&value.id, reifier, depth + 1)?,
        identity: reify_life_declaration_identity(&value.identity, reifier, depth + 1)?,
        kind: reify_dyn_declaration_kind(&value.kind, reifier, depth + 1)?,
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtLifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_life_parameter_kind(item, reifier, depth + 1)?);
            }
            output
        },
        members: {
            reifier.rows(
                (value.members).len(),
                std::mem::size_of::<CtDynMemberDeclaration>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.members).len());
            for item in &value.members {
                output.push(reify_dyn_member_declaration(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_self_pattern<R: CtDynamicReifier>(
    value: &DynSelfPattern,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynSelfPattern, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynSelfPattern {
        declaration: reify_life_declaration_id(&value.declaration, reifier, depth + 1)?,
        arguments: {
            reifier.rows(
                (value.arguments).len(),
                std::mem::size_of::<CtDynPatternActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(reify_dyn_pattern_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_pattern_actual<R: CtDynamicReifier>(
    value: &DynPatternActual,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynPatternActual, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynPatternActual::Type(field_value_0) => {
            CtDynPatternActual::Type(reify_dyn_type_pattern(field_value_0, reifier, depth + 1)?)
        }
        DynPatternActual::Const(field_value_0) => {
            CtDynPatternActual::Const(reify_dyn_const_pattern(field_value_0, reifier, depth + 1)?)
        }
        DynPatternActual::Life(field_value_0) => {
            CtDynPatternActual::Life(reify_dyn_life_pattern(field_value_0, reifier, depth + 1)?)
        }
    })
}
pub fn reify_dyn_const_pattern<R: CtDynamicReifier>(
    value: &DynConstPattern,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynConstPattern, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynConstPattern::IntegerLiteral(field_value_0) => {
            CtDynConstPattern::IntegerLiteral(reifier.text(field_value_0, depth + 1)?)
        }
        DynConstPattern::FloatLiteral(field_value_0) => {
            CtDynConstPattern::FloatLiteral(reifier.text(field_value_0, depth + 1)?)
        }
        DynConstPattern::MethodParameter(field_value_0) => CtDynConstPattern::MethodParameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynConstPattern::Unary {
            op: field_op,
            value: field_value,
            ty: field_ty,
        } => CtDynConstPattern::Unary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            value: {
                reifier.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                Box::new(reify_dyn_const_pattern(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            ty: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynConstPattern::Binary {
            op: field_op,
            left: field_left,
            right: field_right,
            ty: field_ty,
        } => CtDynConstPattern::Binary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            left: {
                reifier.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                Box::new(reify_dyn_const_pattern(
                    (field_left).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            right: {
                reifier.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                Box::new(reify_dyn_const_pattern(
                    (field_right).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            ty: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynConstPattern::Convert {
            value: field_value,
            ty: field_ty,
        } => CtDynConstPattern::Convert {
            value: {
                reifier.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                Box::new(reify_dyn_const_pattern(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            ty: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynConstPattern::AssociatedProjection {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => CtDynConstPattern::AssociatedProjection {
            receiver: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_receiver).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            application: {
                reifier.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
                Box::new(reify_dyn_self_pattern(
                    (field_application).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
        DynConstPattern::Value(field_value_0) => {
            CtDynConstPattern::Value(reifier.constant(field_value_0, depth + 1)?)
        }
        DynConstPattern::Parameter(field_value_0) => CtDynConstPattern::Parameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_dyn_type_pattern<R: CtDynamicReifier>(
    value: &DynTypePattern,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynTypePattern, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynTypePattern::SelfType => CtDynTypePattern::SelfType,
        DynTypePattern::MethodParameter(field_value_0) => CtDynTypePattern::MethodParameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynTypePattern::AssociatedProjection {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => CtDynTypePattern::AssociatedProjection {
            receiver: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_receiver).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            application: {
                reifier.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
                Box::new(reify_dyn_self_pattern(
                    (field_application).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
        DynTypePattern::Concrete(field_value_0) => {
            CtDynTypePattern::Concrete(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
        DynTypePattern::Parameter(field_value_0) => CtDynTypePattern::Parameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynTypePattern::Nominal(field_value_0) => CtDynTypePattern::Nominal({
            reifier.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
            Box::new(reify_dyn_self_pattern(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        DynTypePattern::Address {
            mutable: field_mutable,
            lifetime: field_lifetime,
            inner: field_inner,
        } => CtDynTypePattern::Address {
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            lifetime: match field_lifetime {
                Some(item) => Some(reify_dyn_life_pattern(item, reifier, depth + 1)?),
                None => None,
            },
            inner: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_inner).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynTypePattern::Tuple(field_value_0) => CtDynTypePattern::Tuple({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtDynTypePattern>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_dyn_type_pattern(item, reifier, depth + 1)?);
            }
            output
        }),
        DynTypePattern::Array {
            element: field_element,
            length: field_length,
        } => CtDynTypePattern::Array {
            element: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_element).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            length: reify_dyn_const_pattern(field_length, reifier, depth + 1)?,
        },
        DynTypePattern::Slice(field_value_0) => CtDynTypePattern::Slice({
            reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
            Box::new(reify_dyn_type_pattern(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        DynTypePattern::Managed(field_value_0) => CtDynTypePattern::Managed({
            reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
            Box::new(reify_dyn_type_pattern(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        DynTypePattern::Desc {
            lifetime: field_lifetime,
            inner: field_inner,
        } => CtDynTypePattern::Desc {
            lifetime: match field_lifetime {
                Some(item) => Some(reify_dyn_life_pattern(item, reifier, depth + 1)?),
                None => None,
            },
            inner: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_inner).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynTypePattern::OwnedStore(field_value_0) => CtDynTypePattern::OwnedStore({
            reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
            Box::new(reify_dyn_type_pattern(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        DynTypePattern::ErrorUnion {
            error: field_error,
            result: field_result,
        } => CtDynTypePattern::ErrorUnion {
            error: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_error).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                Box::new(reify_dyn_type_pattern(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynTypePattern::Semantic {
            scope: field_scope,
            ty: field_ty,
        } => CtDynTypePattern::Semantic {
            scope: reify_semantic_definition_id(field_scope, reifier, depth + 1)?,
            ty: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
    })
}
pub fn reify_dyn_declaration_kind<R: CtDynamicReifier>(
    value: &DynDeclarationKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDeclarationKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynDeclarationKind::Trait => CtDynDeclarationKind::Trait,
        DynDeclarationKind::Impl => CtDynDeclarationKind::Impl,
    })
}
pub fn reify_dyn_member_declaration<R: CtDynamicReifier>(
    value: &DynMemberDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynMemberDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynMemberDeclaration {
        name: reifier.text(&value.name, depth + 1)?,
        kind: reify_dyn_member_kind(&value.kind, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_member_kind<R: CtDynamicReifier>(
    value: &DynMemberKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynMemberKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynMemberKind::Type => CtDynMemberKind::Type,
        DynMemberKind::Constant => CtDynMemberKind::Constant,
        DynMemberKind::Method(field_value_0) => CtDynMemberKind::Method({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtLifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_life_parameter_kind(item, reifier, depth + 1)?);
            }
            output
        }),
    })
}
pub fn reify_dyn_generic_actual<R: CtDynamicReifier>(
    value: &DynGenericActual,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynGenericActual, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynGenericActual::Type(field_value_0) => {
            CtDynGenericActual::Type(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
        DynGenericActual::QualifiedType(field_value_0) => CtDynGenericActual::QualifiedType(
            reify_dyn_type_value(field_value_0, reifier, depth + 1)?,
        ),
        DynGenericActual::Const(field_value_0) => {
            CtDynGenericActual::Const(reifier.constant(field_value_0, depth + 1)?)
        }
        DynGenericActual::Life(field_value_0) => CtDynGenericActual::Life({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_dyn_trait_application<R: CtDynamicReifier>(
    value: &DynTraitApplication,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynTraitApplication, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynTraitApplication {
        declaration: reify_life_declaration_id(&value.declaration, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_member<R: CtDynamicReifier>(
    value: &DynMember,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynMember, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynMember {
        application: reify_dyn_trait_application(&value.application, reifier, depth + 1)?,
        ordinal: {
            reifier.node(depth + 1)?;
            value.ordinal
        },
    })
}
pub fn reify_dyn_associated_type<R: CtDynamicReifier>(
    value: &DynAssociatedType,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedType, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynAssociatedType {
        member: reify_dyn_member(&value.member, reifier, depth + 1)?,
        ty: reify_type_value(&value.ty, reifier, depth + 1)?,
        life: reify_storage_life_shape(&value.life, reifier, depth + 1)?,
        classes: {
            reifier.rows(
                (value.classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn reify_dyn_associated_constant<R: CtDynamicReifier>(
    value: &DynAssociatedConstant,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedConstant, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynAssociatedConstant {
        member: reify_dyn_member(&value.member, reifier, depth + 1)?,
        ty: reify_type_value(&value.ty, reifier, depth + 1)?,
        value: reifier.constant(&value.value, depth + 1)?,
    })
}
pub fn reify_dyn_receiver<R: CtDynamicReifier>(
    value: &DynReceiver,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynReceiver, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynReceiver {
        mutable: {
            reifier.node(depth + 1)?;
            value.mutable
        },
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn reify_dyn_slot<R: CtDynamicReifier>(
    value: &DynSlot,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynSlot, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynSlot {
        member: reify_dyn_member(&value.member, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        receiver: reify_dyn_receiver(&value.receiver, reifier, depth + 1)?,
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtDynamicTypeId>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_type_value(item, reifier, depth + 1)?);
            }
            output
        },
        parameter_modes: {
            reifier.rows(
                (value.parameter_modes).len(),
                std::mem::size_of::<PassingMode>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameter_modes).len());
            for item in &value.parameter_modes {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        result: reify_type_value(&value.result, reifier, depth + 1)?,
        result_mode: {
            reifier.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            reifier.node(depth + 1)?;
            value.call_kind
        },
        storage_life: match &value.storage_life {
            Some(item) => Some(reify_storage_life_contract(item, reifier, depth + 1)?),
            None => None,
        },
        contract: reify_dyn_contract(&value.contract, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_interface<R: CtDynamicReifier>(
    value: &DynInterface,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynInterface, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynInterface {
        context: match &value.context {
            Some(item) => Some(reify_type_value(item, reifier, depth + 1)?),
            None => None,
        },
        owning_cleanup: match &value.owning_cleanup {
            Some(item) => Some(reify_dyn_cleanup_contract(item, reifier, depth + 1)?),
            None => None,
        },
        id: reify_dyn_interface_id(&value.id, reifier, depth + 1)?,
        application: reify_dyn_trait_application(&value.application, reifier, depth + 1)?,
        inherited: {
            reifier.rows(
                (value.inherited).len(),
                std::mem::size_of::<CtDynTraitApplication>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.inherited).len());
            for item in &value.inherited {
                output.push(reify_dyn_trait_application(item, reifier, depth + 1)?);
            }
            output
        },
        associated_types: {
            reifier.rows(
                (value.associated_types).len(),
                std::mem::size_of::<CtDynAssociatedType>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated_types).len());
            for item in &value.associated_types {
                output.push(reify_dyn_associated_type(item, reifier, depth + 1)?);
            }
            output
        },
        associated_constants: {
            reifier.rows(
                (value.associated_constants).len(),
                std::mem::size_of::<CtDynAssociatedConstant>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated_constants).len());
            for item in &value.associated_constants {
                output.push(reify_dyn_associated_constant(item, reifier, depth + 1)?);
            }
            output
        },
        retained_life: reify_retained_life_shape(&value.retained_life, reifier, depth + 1)?,
        slots: {
            reifier.rows(
                (value.slots).len(),
                std::mem::size_of::<CtDynSlot>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(reify_dyn_slot(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_core_dyn_entry<R: CtDynamicReifier>(
    value: &CoreDynEntry,
    reifier: &mut R,
    depth: usize,
) -> Result<CtCoreDynEntry, R::Error> {
    reifier.node(depth)?;
    Ok(CtCoreDynEntry {
        slot: {
            reifier.node(depth + 1)?;
            value.slot
        },
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        function: {
            reifier.node(depth + 1)?;
            value.function
        },
    })
}
pub fn reify_dyn_entry<R: CtDynamicReifier>(
    value: &DynEntry,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynEntry, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynEntry {
        slot: {
            reifier.node(depth + 1)?;
            value.slot
        },
        function: {
            reifier.node(depth + 1)?;
            value.function
        },
    })
}
pub fn reify_dyn_retained_port_binding<R: CtDynamicReifier>(
    value: &DynRetainedPortBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynRetainedPortBinding, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynRetainedPortBinding {
        port: {
            reifier.node(depth + 1)?;
            value.port
        },
        sources: {
            reifier.rows(
                (value.sources).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_core_dyn_vtable<R: CtDynamicReifier>(
    value: &CoreDynVtable,
    reifier: &mut R,
    depth: usize,
) -> Result<CtCoreDynVtable, R::Error> {
    reifier.node(depth)?;
    Ok(CtCoreDynVtable {
        self_life: match &value.self_life {
            Some(item) => Some(reify_storage_life_shape(item, reifier, depth + 1)?),
            None => None,
        },
        implementation_witnesses: match &value.implementation_witnesses {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynImplementationWitness>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_implementation_witness(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_classes: {
            reifier.rows(
                (value.self_classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.self_classes).len());
            for item in &value.self_classes {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        id: reify_dyn_vtable_id(&value.id, reifier, depth + 1)?,
        interface: reify_dyn_interface_id(&value.interface, reifier, depth + 1)?,
        self_type: reify_type_value(&value.self_type, reifier, depth + 1)?,
        implementation: reify_life_declaration_id(&value.implementation, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        ports: {
            reifier.rows(
                (value.ports).len(),
                std::mem::size_of::<CtDynRetainedPortBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(reify_dyn_retained_port_binding(item, reifier, depth + 1)?);
            }
            output
        },
        entries: {
            reifier.rows(
                (value.entries).len(),
                std::mem::size_of::<CtCoreDynEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entries).len());
            for item in &value.entries {
                output.push(reify_core_dyn_entry(item, reifier, depth + 1)?);
            }
            output
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(reify_core_dyn_entry(item, reifier, depth + 1)?),
            None => None,
        },
        owned_storage: match &value.owned_storage {
            Some(item) => Some(reify_dyn_owned_storage(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_dyn_vtable<R: CtDynamicReifier>(
    value: &DynVtable,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynVtable, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynVtable {
        self_life: match &value.self_life {
            Some(item) => Some(reify_storage_life_shape(item, reifier, depth + 1)?),
            None => None,
        },
        implementation_witnesses: match &value.implementation_witnesses {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynImplementationWitness>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_implementation_witness(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_classes: {
            reifier.rows(
                (value.self_classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.self_classes).len());
            for item in &value.self_classes {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        id: reify_dyn_vtable_id(&value.id, reifier, depth + 1)?,
        interface: reify_dyn_interface_id(&value.interface, reifier, depth + 1)?,
        self_type: reify_type_value(&value.self_type, reifier, depth + 1)?,
        implementation: reify_life_declaration_id(&value.implementation, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        ports: {
            reifier.rows(
                (value.ports).len(),
                std::mem::size_of::<CtDynRetainedPortBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(reify_dyn_retained_port_binding(item, reifier, depth + 1)?);
            }
            output
        },
        entries: {
            reifier.rows(
                (value.entries).len(),
                std::mem::size_of::<CtDynEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entries).len());
            for item in &value.entries {
                output.push(reify_dyn_entry(item, reifier, depth + 1)?);
            }
            output
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(reify_dyn_entry(item, reifier, depth + 1)?),
            None => None,
        },
        owned_storage: match &value.owned_storage {
            Some(item) => Some(reify_dyn_owned_storage(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_dyn_owned_storage<R: CtDynamicReifier>(
    value: &DynOwnedStorage,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynOwnedStorage, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynOwnedStorage::Heap => CtDynOwnedStorage::Heap,
    })
}
pub fn reify_dyn_requirement<R: CtDynamicReifier>(
    value: &DynRequirement,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynRequirement, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynRequirement::Read(field_value_0) => {
            CtDynRequirement::Read(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynRequirement::Write(field_value_0) => {
            CtDynRequirement::Write(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynRequirement::Manage(field_value_0) => {
            CtDynRequirement::Manage(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynRequirement::Apart(field_value_0, field_value_1) => CtDynRequirement::Apart(
            reify_storage_position(field_value_0, reifier, depth + 1)?,
            reify_storage_position(field_value_1, reifier, depth + 1)?,
        ),
        DynRequirement::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => CtDynRequirement::State {
            position: reify_storage_position(field_position, reifier, depth + 1)?,
            ty: reify_type_value(field_ty, reifier, depth + 1)?,
            variant: {
                reifier.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn reify_dyn_guarantee<R: CtDynamicReifier>(
    value: &DynGuarantee,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynGuarantee, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynGuarantee::StorageSources(field_value_0) => CtDynGuarantee::StorageSources(
            reify_storage_source_binding(field_value_0, reifier, depth + 1)?,
        ),
        DynGuarantee::ManageResult => CtDynGuarantee::ManageResult,
        DynGuarantee::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => CtDynGuarantee::State {
            position: reify_storage_position(field_position, reifier, depth + 1)?,
            ty: reify_type_value(field_ty, reifier, depth + 1)?,
            variant: {
                reifier.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn reify_dyn_effect<R: CtDynamicReifier>(
    value: &DynEffect,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynEffect, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynEffect::Alloc => CtDynEffect::Alloc,
        DynEffect::Io => CtDynEffect::Io,
        DynEffect::Block => CtDynEffect::Block,
        DynEffect::Suspend => CtDynEffect::Suspend,
        DynEffect::Spawn => CtDynEffect::Spawn,
        DynEffect::Ffi => CtDynEffect::Ffi,
        DynEffect::Reenter => CtDynEffect::Reenter,
        DynEffect::Panic => CtDynEffect::Panic,
        DynEffect::Call(field_value_0) => {
            CtDynEffect::Call(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynEffect::Cleanup(field_value_0) => {
            CtDynEffect::Cleanup(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
    })
}
pub fn reify_dyn_call_access<R: CtDynamicReifier>(
    value: &DynCallAccess,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynCallAccess, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynCallAccess {
        reads: {
            reifier.rows(
                (value.reads).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.reads).len());
            for item in &value.reads {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
        writes: {
            reifier.rows(
                (value.writes).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.writes).len());
            for item in &value.writes {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_result_source<R: CtDynamicReifier>(
    value: &DynResultSource,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynResultSource, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynResultSource::Value => CtDynResultSource::Value,
        DynResultSource::Create => CtDynResultSource::Create,
        DynResultSource::Give => CtDynResultSource::Give,
        DynResultSource::Borrow {
            sources: field_sources,
            mutable: field_mutable,
        } => CtDynResultSource::Borrow {
            sources: {
                reifier.rows(
                    (field_sources).len(),
                    std::mem::size_of::<CtStorageSource>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sources).len());
                for item in field_sources {
                    output.push(reify_storage_source(item, reifier, depth + 1)?);
                }
                output
            },
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
        },
    })
}
pub fn reify_dyn_cleanup_contract<R: CtDynamicReifier>(
    value: &DynCleanupContract,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynCleanupContract, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynCleanupContract {
        environment: {
            reifier.node(depth + 1)?;
            value.environment
        },
        positions: {
            reifier.rows(
                (value.positions).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.positions).len());
            for item in &value.positions {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
        access: reify_dyn_call_access(&value.access, reifier, depth + 1)?,
        effects: {
            reifier.rows(
                (value.effects).len(),
                std::mem::size_of::<CtDynEffect>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.effects).len());
            for item in &value.effects {
                output.push(reify_dyn_effect(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_resume_bound<R: CtDynamicReifier>(
    value: &DynResumeBound,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynResumeBound, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynResumeBound::Local => CtDynResumeBound::Local,
        DynResumeBound::Transfer => CtDynResumeBound::Transfer,
    })
}
pub fn reify_dyn_thread_requirements<R: CtDynamicReifier>(
    value: &DynThreadRequirements,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynThreadRequirements, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynThreadRequirements {
        transfer: {
            reifier.node(depth + 1)?;
            value.transfer
        },
        share: {
            reifier.node(depth + 1)?;
            value.share
        },
    })
}
pub fn reify_dyn_contract<R: CtDynamicReifier>(
    value: &DynContract,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynContract, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynContract {
        requires: match &value.requires {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynRequirement>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_requirement(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        guarantees: match &value.guarantees {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynGuarantee>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_guarantee(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        hold: match &value.hold {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<StoragePosition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_storage_position(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        call: match &value.call {
            Some(item) => Some(reify_dyn_call_access(item, reifier, depth + 1)?),
            None => None,
        },
        result: match &value.result {
            Some(item) => Some(reify_dyn_result_source(item, reifier, depth + 1)?),
            None => None,
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(reify_dyn_cleanup_contract(item, reifier, depth + 1)?),
            None => None,
        },
        effects: match &value.effects {
            Some(item) => Some({
                reifier.rows((item).len(), std::mem::size_of::<CtDynEffect>(), depth + 1)?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_effect(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        resume: match &value.resume {
            Some(item) => Some(reify_dyn_resume_bound(item, reifier, depth + 1)?),
            None => None,
        },
        threads: match &value.threads {
            Some(item) => Some(reify_dyn_thread_requirements(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_dyn_type_value<R: CtDynamicReifier>(
    value: &DynTypeValue,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynTypeValue, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynTypeValue {
        ty: reify_type_value(&value.ty, reifier, depth + 1)?,
        life: reify_storage_life_shape(&value.life, reifier, depth + 1)?,
        classes: {
            reifier.rows(
                (value.classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn reify_dyn_instantiation_scope<R: CtDynamicReifier>(
    value: &DynInstantiationScope,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynInstantiationScope, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynInstantiationScope {
        fixed_binder_count: {
            reifier.node(depth + 1)?;
            value.fixed_binder_count
        },
        invocation_binder_count: {
            reifier.node(depth + 1)?;
            value.invocation_binder_count
        },
    })
}
pub fn reify_dyn_life_embedding<R: CtDynamicReifier>(
    value: &DynLifeEmbedding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynLifeEmbedding, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynLifeEmbedding {
        from: {
            reifier.node(depth + 1)?;
            value.from
        },
        to: {
            reifier.node(depth + 1)?;
            value.to
        },
    })
}
pub fn reify_dyn_implicit_life_kind<R: CtDynamicReifier>(
    value: &DynImplicitLifeKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynImplicitLifeKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynImplicitLifeKind::Address => CtDynImplicitLifeKind::Address,
        DynImplicitLifeKind::Description => CtDynImplicitLifeKind::Description,
        DynImplicitLifeKind::LifetimeArgument => CtDynImplicitLifeKind::LifetimeArgument,
    })
}
pub fn reify_dyn_implicit_life_root<R: CtDynamicReifier>(
    value: &DynImplicitLifeRoot,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynImplicitLifeRoot, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynImplicitLifeRoot::SelfPattern => CtDynImplicitLifeRoot::SelfPattern,
        DynImplicitLifeRoot::ImplementedTrait => CtDynImplicitLifeRoot::ImplementedTrait,
        DynImplicitLifeRoot::Supertrait(field_value_0) => CtDynImplicitLifeRoot::Supertrait({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynImplicitLifeRoot::Requirement(field_value_0) => CtDynImplicitLifeRoot::Requirement({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynImplicitLifeRoot::AssociatedDefinition(field_value_0) => {
            CtDynImplicitLifeRoot::AssociatedDefinition({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        DynImplicitLifeRoot::MethodSignature(field_value_0) => {
            CtDynImplicitLifeRoot::MethodSignature({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        DynImplicitLifeRoot::MethodDefinition(field_value_0) => {
            CtDynImplicitLifeRoot::MethodDefinition({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
    })
}
pub fn reify_dyn_implicit_life_occurrence<R: CtDynamicReifier>(
    value: &DynImplicitLifeOccurrence,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynImplicitLifeOccurrence, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynImplicitLifeOccurrence {
        root: reify_dyn_implicit_life_root(&value.root, reifier, depth + 1)?,
        path: {
            reifier.rows((value.path).len(), std::mem::size_of::<u32>(), depth + 1)?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        kind: reify_dyn_implicit_life_kind(&value.kind, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_implicit_life_declaration<R: CtDynamicReifier>(
    value: &DynImplicitLifeDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynImplicitLifeDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynImplicitLifeDeclaration {
        occurrences: {
            reifier.rows(
                (value.occurrences).len(),
                std::mem::size_of::<CtDynImplicitLifeOccurrence>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrences).len());
            for item in &value.occurrences {
                output.push(reify_dyn_implicit_life_occurrence(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn reify_dyn_life_pattern<R: CtDynamicReifier>(
    value: &DynLifePattern,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynLifePattern, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynLifePattern::DeclarationParameter(field_value_0) => {
            CtDynLifePattern::DeclarationParameter({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        DynLifePattern::MethodParameter(field_value_0) => CtDynLifePattern::MethodParameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynLifePattern::Bound {
            depth: field_depth,
            index: field_index,
        } => CtDynLifePattern::Bound {
            depth: {
                reifier.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                reifier.node(depth + 1)?;
                *(field_index)
            },
        },
        DynLifePattern::Invocation(field_value_0) => CtDynLifePattern::Invocation({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        DynLifePattern::ImplicitDeclaration(field_value_0) => {
            CtDynLifePattern::ImplicitDeclaration({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
    })
}
pub fn reify_dyn_obligation<R: CtDynamicReifier>(
    value: &DynObligation,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynObligation, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynObligation::Structural {
            receiver: field_receiver,
            bound: field_bound,
        } => CtDynObligation::Structural {
            receiver: reify_dyn_type_pattern(field_receiver, reifier, depth + 1)?,
            bound: reify_dyn_structural_bound(field_bound, reifier, depth + 1)?,
        },
        DynObligation::Trait {
            receiver: field_receiver,
            application: field_application,
        } => CtDynObligation::Trait {
            receiver: reify_dyn_type_pattern(field_receiver, reifier, depth + 1)?,
            application: reify_dyn_self_pattern(field_application, reifier, depth + 1)?,
        },
        DynObligation::Callable {
            receiver: field_receiver,
            kind: field_kind,
            quantified_lifetimes: field_quantified_lifetimes,
            signature: field_signature,
        } => CtDynObligation::Callable {
            receiver: reify_dyn_type_pattern(field_receiver, reifier, depth + 1)?,
            kind: {
                reifier.node(depth + 1)?;
                *(field_kind)
            },
            quantified_lifetimes: {
                reifier.node(depth + 1)?;
                *(field_quantified_lifetimes)
            },
            signature: {
                reifier.rows(1, std::mem::size_of::<CtDynSignatureRecipe>(), depth + 1)?;
                Box::new(reify_dyn_signature_recipe(
                    (field_signature).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        DynObligation::TypeEqual {
            left: field_left,
            right: field_right,
        } => CtDynObligation::TypeEqual {
            left: reify_dyn_type_pattern(field_left, reifier, depth + 1)?,
            right: reify_dyn_type_pattern(field_right, reifier, depth + 1)?,
        },
        DynObligation::ConstEqual {
            ty: field_ty,
            left: field_left,
            right: field_right,
        } => CtDynObligation::ConstEqual {
            ty: reify_dyn_type_pattern(field_ty, reifier, depth + 1)?,
            left: reify_dyn_const_pattern(field_left, reifier, depth + 1)?,
            right: reify_dyn_const_pattern(field_right, reifier, depth + 1)?,
        },
    })
}
pub fn reify_dyn_parameter_recipe<R: CtDynamicReifier>(
    value: &DynParameterRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynParameterRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynParameterRecipe {
        ty: reify_dyn_type_pattern(&value.ty, reifier, depth + 1)?,
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn reify_dyn_signature_recipe<R: CtDynamicReifier>(
    value: &DynSignatureRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynSignatureRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynSignatureRecipe {
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtDynParameterRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_dyn_parameter_recipe(item, reifier, depth + 1)?);
            }
            output
        },
        result: reify_dyn_type_pattern(&value.result, reifier, depth + 1)?,
        result_mode: {
            reifier.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            reifier.node(depth + 1)?;
            value.call_kind
        },
        invocation_binder_count: {
            reifier.node(depth + 1)?;
            value.invocation_binder_count
        },
        contract: reify_dyn_contract_recipe(&value.contract, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_requirement_recipe<R: CtDynamicReifier>(
    value: &DynRequirementRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynRequirementRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynRequirementRecipe::Read(field_value_0) => {
            CtDynRequirementRecipe::Read(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynRequirementRecipe::Write(field_value_0) => CtDynRequirementRecipe::Write(
            reify_storage_position(field_value_0, reifier, depth + 1)?,
        ),
        DynRequirementRecipe::Manage(field_value_0) => CtDynRequirementRecipe::Manage(
            reify_storage_position(field_value_0, reifier, depth + 1)?,
        ),
        DynRequirementRecipe::Apart(field_value_0, field_value_1) => CtDynRequirementRecipe::Apart(
            reify_storage_position(field_value_0, reifier, depth + 1)?,
            reify_storage_position(field_value_1, reifier, depth + 1)?,
        ),
        DynRequirementRecipe::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => CtDynRequirementRecipe::State {
            position: reify_storage_position(field_position, reifier, depth + 1)?,
            ty: reify_dyn_type_pattern(field_ty, reifier, depth + 1)?,
            variant: {
                reifier.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn reify_dyn_guarantee_recipe<R: CtDynamicReifier>(
    value: &DynGuaranteeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynGuaranteeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynGuaranteeRecipe::StorageSources(field_value_0) => CtDynGuaranteeRecipe::StorageSources(
            reify_dyn_storage_source_binding_recipe(field_value_0, reifier, depth + 1)?,
        ),
        DynGuaranteeRecipe::ManageResult => CtDynGuaranteeRecipe::ManageResult,
        DynGuaranteeRecipe::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => CtDynGuaranteeRecipe::State {
            position: reify_storage_position(field_position, reifier, depth + 1)?,
            ty: reify_dyn_type_pattern(field_ty, reifier, depth + 1)?,
            variant: {
                reifier.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn reify_dyn_effect_recipe<R: CtDynamicReifier>(
    value: &DynEffectRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynEffectRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynEffectRecipe::Alloc => CtDynEffectRecipe::Alloc,
        DynEffectRecipe::Io => CtDynEffectRecipe::Io,
        DynEffectRecipe::Block => CtDynEffectRecipe::Block,
        DynEffectRecipe::Suspend => CtDynEffectRecipe::Suspend,
        DynEffectRecipe::Spawn => CtDynEffectRecipe::Spawn,
        DynEffectRecipe::Ffi => CtDynEffectRecipe::Ffi,
        DynEffectRecipe::Reenter => CtDynEffectRecipe::Reenter,
        DynEffectRecipe::Panic => CtDynEffectRecipe::Panic,
        DynEffectRecipe::Call(field_value_0) => {
            CtDynEffectRecipe::Call(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        DynEffectRecipe::Cleanup(field_value_0) => {
            CtDynEffectRecipe::Cleanup(reify_dyn_type_pattern(field_value_0, reifier, depth + 1)?)
        }
    })
}
pub fn reify_dyn_cleanup_recipe<R: CtDynamicReifier>(
    value: &DynCleanupRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynCleanupRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynCleanupRecipe {
        environment: {
            reifier.node(depth + 1)?;
            value.environment
        },
        positions: {
            reifier.rows(
                (value.positions).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.positions).len());
            for item in &value.positions {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
        access: reify_dyn_call_access(&value.access, reifier, depth + 1)?,
        effects: {
            reifier.rows(
                (value.effects).len(),
                std::mem::size_of::<CtDynEffectRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.effects).len());
            for item in &value.effects {
                output.push(reify_dyn_effect_recipe(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_contract_recipe<R: CtDynamicReifier>(
    value: &DynContractRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynContractRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynContractRecipe {
        requires: match &value.requires {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_requirement_recipe(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        guarantees: match &value.guarantees {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynGuaranteeRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_guarantee_recipe(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        hold: match &value.hold {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<StoragePosition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_storage_position(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        call: match &value.call {
            Some(item) => Some(reify_dyn_call_access(item, reifier, depth + 1)?),
            None => None,
        },
        result: match &value.result {
            Some(item) => Some(reify_dyn_result_source_recipe(item, reifier, depth + 1)?),
            None => None,
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(reify_dyn_cleanup_recipe(item, reifier, depth + 1)?),
            None => None,
        },
        effects: match &value.effects {
            Some(item) => Some({
                reifier.rows(
                    (item).len(),
                    std::mem::size_of::<CtDynEffectRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(reify_dyn_effect_recipe(item, reifier, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        resume: match &value.resume {
            Some(item) => Some(reify_dyn_resume_bound(item, reifier, depth + 1)?),
            None => None,
        },
        threads: match &value.threads {
            Some(item) => Some(reify_dyn_thread_requirements(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_dyn_storage_source_recipe<R: CtDynamicReifier>(
    value: &DynStorageSourceRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynStorageSourceRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynStorageSourceRecipe::Exact(field_value_0) => CtDynStorageSourceRecipe::Exact(
            reify_storage_position(field_value_0, reifier, depth + 1)?,
        ),
        DynStorageSourceRecipe::PlaceAddress {
            place: field_place,
            mode: field_mode,
        } => CtDynStorageSourceRecipe::PlaceAddress {
            place: reify_storage_position(field_place, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        DynStorageSourceRecipe::OwnedView {
            head: field_head,
            leaf: field_leaf,
            mode: field_mode,
        } => CtDynStorageSourceRecipe::OwnedView {
            head: reify_storage_position(field_head, reifier, depth + 1)?,
            leaf: reify_dyn_type_pattern(field_leaf, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        DynStorageSourceRecipe::ReborrowedLeaf {
            source: field_source,
            head: field_head,
        } => CtDynStorageSourceRecipe::ReborrowedLeaf {
            source: reify_storage_position(field_source, reifier, depth + 1)?,
            head: reify_storage_position(field_head, reifier, depth + 1)?,
        },
        DynStorageSourceRecipe::Static {
            leaf: field_leaf,
            mode: field_mode,
        } => CtDynStorageSourceRecipe::Static {
            leaf: reify_dyn_type_pattern(field_leaf, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
    })
}
pub fn reify_dyn_storage_source_binding_recipe<R: CtDynamicReifier>(
    value: &DynStorageSourceBindingRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynStorageSourceBindingRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynStorageSourceBindingRecipe {
        position: reify_storage_position(&value.position, reifier, depth + 1)?,
        sources: {
            reifier.rows(
                (value.sources).len(),
                std::mem::size_of::<CtDynStorageSourceRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(reify_dyn_storage_source_recipe(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_result_source_recipe<R: CtDynamicReifier>(
    value: &DynResultSourceRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynResultSourceRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynResultSourceRecipe::Value => CtDynResultSourceRecipe::Value,
        DynResultSourceRecipe::Create => CtDynResultSourceRecipe::Create,
        DynResultSourceRecipe::Give => CtDynResultSourceRecipe::Give,
        DynResultSourceRecipe::Borrow {
            sources: field_sources,
            mutable: field_mutable,
        } => CtDynResultSourceRecipe::Borrow {
            sources: {
                reifier.rows(
                    (field_sources).len(),
                    std::mem::size_of::<CtDynStorageSourceRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sources).len());
                for item in field_sources {
                    output.push(reify_dyn_storage_source_recipe(item, reifier, depth + 1)?);
                }
                output
            },
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
        },
    })
}
pub fn reify_dyn_definition_id<R: CtDynamicReifier>(
    value: &DynDefinitionId,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDefinitionId, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynDefinitionId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        source: {
            reifier.node(depth + 1)?;
            value.source
        },
    })
}
pub fn reify_dyn_definition_scope<R: CtDynamicReifier>(
    value: &DynDefinitionScope,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDefinitionScope, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynDefinitionScope::TraitDefault => CtDynDefinitionScope::TraitDefault,
        DynDefinitionScope::ImplOverride => CtDynDefinitionScope::ImplOverride,
    })
}
pub fn reify_dyn_definition_declaration<R: CtDynamicReifier>(
    value: &DynDefinitionDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDefinitionDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynDefinitionDeclaration {
        requirements: {
            reifier.rows(
                (value.requirements).len(),
                std::mem::size_of::<CtDynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(reify_dyn_obligation(item, reifier, depth + 1)?);
            }
            output
        },
        instances: {
            reifier.rows(
                (value.instances).len(),
                std::mem::size_of::<CtDynDefinitionSpecialization>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.instances).len());
            for item in &value.instances {
                output.push(reify_dyn_definition_specialization(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
        id: reify_dyn_definition_id(&value.id, reifier, depth + 1)?,
        identity: reify_life_declaration_identity(&value.identity, reifier, depth + 1)?,
        member: {
            reifier.node(depth + 1)?;
            value.member
        },
        scope: reify_dyn_definition_scope(&value.scope, reifier, depth + 1)?,
        signature: reify_dyn_signature_recipe(&value.signature, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_method_recipe<R: CtDynamicReifier>(
    value: &DynMethodRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynMethodRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynMethodRecipe {
        requirements: {
            reifier.rows(
                (value.requirements).len(),
                std::mem::size_of::<CtDynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(reify_dyn_obligation(item, reifier, depth + 1)?);
            }
            output
        },
        member: {
            reifier.node(depth + 1)?;
            value.member
        },
        signature: reify_dyn_signature_recipe(&value.signature, reifier, depth + 1)?,
        default_definition: match &value.default_definition {
            Some(item) => Some(reify_dyn_definition_id(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_dyn_function_instance<R: CtDynamicReifier>(
    value: &DynFunctionInstance,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynFunctionInstance, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynFunctionInstance {
        scope: reify_dyn_instantiation_scope(&value.scope, reifier, depth + 1)?,
        header_embedding: {
            reifier.rows(
                (value.header_embedding).len(),
                std::mem::size_of::<CtDynLifeEmbedding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.header_embedding).len());
            for item in &value.header_embedding {
                output.push(reify_dyn_life_embedding(item, reifier, depth + 1)?);
            }
            output
        },
        definition: reify_dyn_definition_id(&value.definition, reifier, depth + 1)?,
        receiver: reify_dyn_type_value(&value.receiver, reifier, depth + 1)?,
        declaration_actuals: {
            reifier.rows(
                (value.declaration_actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_actuals).len());
            for item in &value.declaration_actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        method_actuals: {
            reifier.rows(
                (value.method_actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.method_actuals).len());
            for item in &value.method_actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_associated_value_recipe<R: CtDynamicReifier>(
    value: &DynAssociatedValueRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedValueRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynAssociatedValueRecipe::Type(field_value_0) => CtDynAssociatedValueRecipe::Type(
            reify_dyn_type_pattern(field_value_0, reifier, depth + 1)?,
        ),
        DynAssociatedValueRecipe::Constant {
            ty: field_ty,
            value: field_value,
        } => CtDynAssociatedValueRecipe::Constant {
            ty: reify_dyn_type_pattern(field_ty, reifier, depth + 1)?,
            value: reify_dyn_const_pattern(field_value, reifier, depth + 1)?,
        },
    })
}
pub fn reify_dyn_associated_definition<R: CtDynamicReifier>(
    value: &DynAssociatedDefinition,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedDefinition, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynAssociatedDefinition {
        carrier: match &value.carrier {
            Some(item) => Some(reify_dyn_type_pattern(item, reifier, depth + 1)?),
            None => None,
        },
        member: {
            reifier.node(depth + 1)?;
            value.member
        },
        value: match &value.value {
            Some(item) => Some(reify_dyn_associated_value_recipe(item, reifier, depth + 1)?),
            None => None,
        },
        requirements: {
            reifier.rows(
                (value.requirements).len(),
                std::mem::size_of::<CtDynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(reify_dyn_obligation(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_associated_source<R: CtDynamicReifier>(
    value: &DynAssociatedSource,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedSource, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynAssociatedSource::TraitDefault {
            declaration: field_declaration,
            member: field_member,
        } => CtDynAssociatedSource::TraitDefault {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
        DynAssociatedSource::ImplOverride {
            declaration: field_declaration,
            member: field_member,
        } => CtDynAssociatedSource::ImplOverride {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
    })
}
pub fn reify_dyn_associated_value<R: CtDynamicReifier>(
    value: &DynAssociatedValue,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedValue, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynAssociatedValue::Type(field_value_0) => {
            CtDynAssociatedValue::Type(reify_dyn_type_value(field_value_0, reifier, depth + 1)?)
        }
        DynAssociatedValue::Constant {
            ty: field_ty,
            value: field_value,
        } => CtDynAssociatedValue::Constant {
            ty: reify_dyn_type_value(field_ty, reifier, depth + 1)?,
            value: reifier.constant(field_value, depth + 1)?,
        },
    })
}
pub fn reify_dyn_associated_resolution<R: CtDynamicReifier>(
    value: &DynAssociatedResolution,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynAssociatedResolution, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynAssociatedResolution {
        member: reify_dyn_member(&value.member, reifier, depth + 1)?,
        source: reify_dyn_associated_source(&value.source, reifier, depth + 1)?,
        value: reify_dyn_associated_value(&value.value, reifier, depth + 1)?,
    })
}
pub fn reify_dyn_implementation_witness<R: CtDynamicReifier>(
    value: &DynImplementationWitness,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynImplementationWitness, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynImplementationWitness {
        scope: reify_dyn_instantiation_scope(&value.scope, reifier, depth + 1)?,
        to_context: {
            reifier.rows(
                (value.to_context).len(),
                std::mem::size_of::<CtDynLifeEmbedding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.to_context).len());
            for item in &value.to_context {
                output.push(reify_dyn_life_embedding(item, reifier, depth + 1)?);
            }
            output
        },
        receiver: reify_dyn_type_value(&value.receiver, reifier, depth + 1)?,
        application: reify_dyn_trait_application(&value.application, reifier, depth + 1)?,
        implementation: reify_life_declaration_id(&value.implementation, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        associated: {
            reifier.rows(
                (value.associated).len(),
                std::mem::size_of::<CtDynAssociatedResolution>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated).len());
            for item in &value.associated {
                output.push(reify_dyn_associated_resolution(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_function_target<R: CtDynamicReifier>(
    value: &DynFunctionTarget,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynFunctionTarget, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynFunctionTarget::Core {
            module: field_module,
            source: field_source,
        } => CtDynFunctionTarget::Core {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            source: {
                reifier.node(depth + 1)?;
                *(field_source)
            },
        },
        DynFunctionTarget::Typed(field_value_0) => CtDynFunctionTarget::Typed({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_dyn_definition_specialization<R: CtDynamicReifier>(
    value: &DynDefinitionSpecialization,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynDefinitionSpecialization, R::Error> {
    reifier.node(depth)?;
    Ok(CtDynDefinitionSpecialization {
        function: reify_dyn_function_target(&value.function, reifier, depth + 1)?,
        receiver: reify_dyn_type_value(&value.receiver, reifier, depth + 1)?,
        declaration_actuals: {
            reifier.rows(
                (value.declaration_actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_actuals).len());
            for item in &value.declaration_actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        method_actuals: {
            reifier.rows(
                (value.method_actuals).len(),
                std::mem::size_of::<CtDynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.method_actuals).len());
            for item in &value.method_actuals {
                output.push(reify_dyn_generic_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_dyn_structural_bound<R: CtDynamicReifier>(
    value: &DynStructuralBound,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynStructuralBound, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        DynStructuralBound::Copy => CtDynStructuralBound::Copy,
        DynStructuralBound::Value => CtDynStructuralBound::Value,
        DynStructuralBound::SyncDrop => CtDynStructuralBound::SyncDrop,
        DynStructuralBound::StoreElement => CtDynStructuralBound::StoreElement,
    })
}
pub fn reify_semantic_definition_id<R: CtDynamicReifier>(
    value: &SemanticDefinitionId,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDefinitionId, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticDefinitionId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        index: {
            reifier.node(depth + 1)?;
            value.index
        },
    })
}
pub fn reify_semantic_instance_id<R: CtDynamicReifier>(
    value: &SemanticInstanceId,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticInstanceId, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticInstanceId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        index: {
            reifier.node(depth + 1)?;
            value.index
        },
    })
}
pub fn reify_semantic_module<R: CtDynamicReifier>(
    value: &SemanticModule,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticModule, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticModule {
        definitions: {
            reifier.rows(
                (value.definitions).len(),
                std::mem::size_of::<CtSemanticDefinition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.definitions).len());
            for item in &value.definitions {
                output.push(reify_semantic_definition(item, reifier, depth + 1)?);
            }
            output
        },
        instances: {
            reifier.rows(
                (value.instances).len(),
                std::mem::size_of::<CtSemanticInstance>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.instances).len());
            for item in &value.instances {
                output.push(reify_semantic_instance(item, reifier, depth + 1)?);
            }
            output
        },
        sites: {
            reifier.rows(
                (value.sites).len(),
                std::mem::size_of::<CtSemanticSite>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sites).len());
            for item in &value.sites {
                output.push(reify_semantic_site(item, reifier, depth + 1)?);
            }
            output
        },
        dyn_scopes: {
            reifier.rows(
                (value.dyn_scopes).len(),
                std::mem::size_of::<CtSemanticDynScopeBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.dyn_scopes).len());
            for item in &value.dyn_scopes {
                output.push(reify_semantic_dyn_scope_binding(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_dyn_scope_binding<R: CtDynamicReifier>(
    value: &SemanticDynScopeBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDynScopeBinding, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticDynScopeBinding {
        owner: reify_semantic_dyn_owner(&value.owner, reifier, depth + 1)?,
        scope: reify_semantic_definition_id(&value.scope, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_dyn_owner<R: CtDynamicReifier>(
    value: &SemanticDynOwner,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDynOwner, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticDynOwner::Declaration(field_value_0) => CtSemanticDynOwner::Declaration(
            reify_life_declaration_id(field_value_0, reifier, depth + 1)?,
        ),
        SemanticDynOwner::MethodSignature {
            declaration: field_declaration,
            member: field_member,
        } => CtSemanticDynOwner::MethodSignature {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
        SemanticDynOwner::MethodDefinition(field_value_0) => CtSemanticDynOwner::MethodDefinition(
            reify_dyn_definition_id(field_value_0, reifier, depth + 1)?,
        ),
    })
}
pub fn reify_semantic_definition<R: CtDynamicReifier>(
    value: &SemanticDefinition,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDefinition, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticDefinition {
        id: reify_semantic_definition_id(&value.id, reifier, depth + 1)?,
        identity: reify_life_declaration_identity(&value.identity, reifier, depth + 1)?,
        parent: match &value.parent {
            Some(item) => Some(reify_semantic_definition_id(item, reifier, depth + 1)?),
            None => None,
        },
        kind: reify_semantic_definition_kind(&value.kind, reifier, depth + 1)?,
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtSemanticFormal>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_semantic_formal(item, reifier, depth + 1)?);
            }
            output
        },
        implicit_lifetimes: {
            reifier.rows(
                (value.implicit_lifetimes).len(),
                std::mem::size_of::<CtSemanticImplicitLifetime>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.implicit_lifetimes).len());
            for item in &value.implicit_lifetimes {
                output.push(reify_semantic_implicit_lifetime(item, reifier, depth + 1)?);
            }
            output
        },
        requirements: {
            reifier.rows(
                (value.requirements).len(),
                std::mem::size_of::<CtSemanticRequirementRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(reify_semantic_requirement_recipe(item, reifier, depth + 1)?);
            }
            output
        },
        environment: {
            reifier.rows(
                (value.environment).len(),
                std::mem::size_of::<CtSemanticEnvironmentFormal>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.environment).len());
            for item in &value.environment {
                output.push(reify_semantic_environment_formal(item, reifier, depth + 1)?);
            }
            output
        },
        body: reify_semantic_definition_body(&value.body, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_definition_kind<R: CtDynamicReifier>(
    value: &SemanticDefinitionKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDefinitionKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticDefinitionKind::Function => CtSemanticDefinitionKind::Function,
        SemanticDefinitionKind::Method => CtSemanticDefinitionKind::Method,
        SemanticDefinitionKind::Closure => CtSemanticDefinitionKind::Closure,
        SemanticDefinitionKind::TaskEntry => CtSemanticDefinitionKind::TaskEntry,
        SemanticDefinitionKind::Nominal => CtSemanticDefinitionKind::Nominal,
        SemanticDefinitionKind::Scope => CtSemanticDefinitionKind::Scope,
    })
}
pub fn reify_semantic_definition_body<R: CtDynamicReifier>(
    value: &SemanticDefinitionBody,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDefinitionBody, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticDefinitionBody::Function {
            signature: field_signature,
            sites: field_sites,
        } => CtSemanticDefinitionBody::Function {
            signature: reify_semantic_signature_recipe(field_signature, reifier, depth + 1)?,
            sites: {
                reifier.rows(
                    (field_sites).len(),
                    std::mem::size_of::<CtSemanticSiteRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sites).len());
                for item in field_sites {
                    output.push(reify_semantic_site_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticDefinitionBody::Nominal {
            declaration: field_declaration,
            kind: field_kind,
            body: field_body,
        } => CtSemanticDefinitionBody::Nominal {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            kind: reify_life_nominal_kind(field_kind, reifier, depth + 1)?,
            body: reify_semantic_nominal_body(field_body, reifier, depth + 1)?,
        },
        SemanticDefinitionBody::Scope(field_value_0) => CtSemanticDefinitionBody::Scope(
            reify_semantic_scope_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticDefinitionBody::FunctionWithBody {
            signature: field_signature,
            protocol_contracts: field_protocol_contracts,
            foreign: field_foreign,
            resume_bound: field_resume_bound,
            sites: field_sites,
            parameters: field_parameters,
            receiver_parameter: field_receiver_parameter,
            names: field_names,
            site_nodes: field_site_nodes,
            body: field_body,
        } => CtSemanticDefinitionBody::FunctionWithBody {
            signature: reify_semantic_signature_recipe(field_signature, reifier, depth + 1)?,
            protocol_contracts: {
                reifier.rows(
                    (field_protocol_contracts).len(),
                    std::mem::size_of::<CtFunctionProtocolContract>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_protocol_contracts).len());
                for item in field_protocol_contracts {
                    output.push(reify_function_protocol_contract(item, reifier, depth + 1)?);
                }
                output
            },
            foreign: match field_foreign {
                Some(item) => Some(reify_foreign_declaration(item, reifier, depth + 1)?),
                None => None,
            },
            resume_bound: reify_task_resume_bound(field_resume_bound, reifier, depth + 1)?,
            sites: {
                reifier.rows(
                    (field_sites).len(),
                    std::mem::size_of::<CtSemanticSiteRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sites).len());
                for item in field_sites {
                    output.push(reify_semantic_site_recipe(item, reifier, depth + 1)?);
                }
                output
            },
            parameters: {
                reifier.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<CtSemanticOriginalClosureParameter>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(reify_semantic_original_closure_parameter(
                        item,
                        reifier,
                        depth + 1,
                    )?);
                }
                output
            },
            receiver_parameter: match field_receiver_parameter {
                Some(item) => Some({
                    reifier.node(depth + 1)?;
                    *(item)
                }),
                None => None,
            },
            names: {
                reifier.rows(
                    (field_names).len(),
                    std::mem::size_of::<CtSemanticOriginalNameBinding>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_names).len());
                for item in field_names {
                    output.push(reify_semantic_original_name_binding(
                        item,
                        reifier,
                        depth + 1,
                    )?);
                }
                output
            },
            site_nodes: {
                reifier.rows(
                    (field_site_nodes).len(),
                    std::mem::size_of::<CtSemanticOriginalSiteNode>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_site_nodes).len());
                for item in field_site_nodes {
                    output.push(reify_semantic_original_site_node(item, reifier, depth + 1)?);
                }
                output
            },
            body: reify_semantic_original_function_body(field_body, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_original_function_body<R: CtDynamicReifier>(
    value: &SemanticOriginalFunctionBody,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalFunctionBody, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalFunctionBody::Defined(field_value_0) => {
            CtSemanticOriginalFunctionBody::Defined(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalFunctionBody::External => CtSemanticOriginalFunctionBody::External,
        SemanticOriginalFunctionBody::Declaration => CtSemanticOriginalFunctionBody::Declaration,
    })
}
pub fn reify_semantic_original_type_source<R: CtDynamicReifier>(
    value: &SemanticOriginalTypeSource,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalTypeSource, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalTypeSource::Expression(field_value_0) => {
            CtSemanticOriginalTypeSource::Expression({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        SemanticOriginalTypeSource::Binding(field_value_0) => {
            CtSemanticOriginalTypeSource::Binding({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        SemanticOriginalTypeSource::Parameter(field_value_0) => {
            CtSemanticOriginalTypeSource::Parameter({
                reifier.node(depth + 1)?;
                *(field_value_0)
            })
        }
        SemanticOriginalTypeSource::Result => CtSemanticOriginalTypeSource::Result,
        SemanticOriginalTypeSource::CallConstCarrier {
            call: field_call,
            formal: field_formal,
        } => CtSemanticOriginalTypeSource::CallConstCarrier {
            call: {
                reifier.node(depth + 1)?;
                *(field_call)
            },
            formal: {
                reifier.node(depth + 1)?;
                *(field_formal)
            },
        },
        SemanticOriginalTypeSource::BindingProjection {
            statement: field_statement,
            pattern: field_pattern,
        } => CtSemanticOriginalTypeSource::BindingProjection {
            statement: {
                reifier.node(depth + 1)?;
                *(field_statement)
            },
            pattern: {
                reifier.rows((field_pattern).len(), std::mem::size_of::<u32>(), depth + 1)?;
                let mut output = Vec::with_capacity((field_pattern).len());
                for item in field_pattern {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
        },
        SemanticOriginalTypeSource::NameAt {
            at: field_at,
            name: field_name,
        } => CtSemanticOriginalTypeSource::NameAt {
            at: reify_semantic_original_node(field_at, reifier, depth + 1)?,
            name: reifier.text(field_name, depth + 1)?,
        },
    })
}
pub fn reify_semantic_original_namespace<R: CtDynamicReifier>(
    value: &SemanticOriginalNamespace,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalNamespace, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalNamespace::Value => CtSemanticOriginalNamespace::Value,
        SemanticOriginalNamespace::Type => CtSemanticOriginalNamespace::Type,
        SemanticOriginalNamespace::Lifetime => CtSemanticOriginalNamespace::Lifetime,
    })
}
pub fn reify_semantic_original_name_binding<R: CtDynamicReifier>(
    value: &SemanticOriginalNameBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalNameBinding, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalNameBinding {
        path: {
            reifier.rows((value.path).len(), std::mem::size_of::<String>(), depth + 1)?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push(reifier.text(item, depth + 1)?);
            }
            output
        },
        namespace: reify_semantic_original_namespace(&value.namespace, reifier, depth + 1)?,
        value: reify_semantic_original_name_value(&value.value, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_original_name_value<R: CtDynamicReifier>(
    value: &SemanticOriginalNameValue,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalNameValue, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalNameValue::Recipe(field_value_0) => CtSemanticOriginalNameValue::Recipe(
            reify_semantic_actual_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticOriginalNameValue::Definition(field_value_0) => {
            CtSemanticOriginalNameValue::Definition(reify_semantic_definition_id(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
    })
}
pub fn reify_semantic_original_node<R: CtDynamicReifier>(
    value: &SemanticOriginalNode,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalNode, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalNode::Expression(field_value_0) => CtSemanticOriginalNode::Expression({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOriginalNode::Statement(field_value_0) => CtSemanticOriginalNode::Statement({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_semantic_original_site_node<R: CtDynamicReifier>(
    value: &SemanticOriginalSiteNode,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalSiteNode, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalSiteNode {
        site: {
            reifier.node(depth + 1)?;
            value.site
        },
        node: reify_semantic_original_node(&value.node, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_scope_recipe<R: CtDynamicReifier>(
    value: &SemanticScopeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticScopeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticScopeRecipe::Trait {
            declaration: field_declaration,
        } => CtSemanticScopeRecipe::Trait {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
        },
        SemanticScopeRecipe::InherentImpl {
            declaration: field_declaration,
            self_type: field_self_type,
        } => CtSemanticScopeRecipe::InherentImpl {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            self_type: reify_semantic_type_recipe(field_self_type, reifier, depth + 1)?,
        },
        SemanticScopeRecipe::TraitImpl {
            declaration: field_declaration,
            self_type: field_self_type,
            trait_application: field_trait_application,
        } => CtSemanticScopeRecipe::TraitImpl {
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            self_type: reify_semantic_type_recipe(field_self_type, reifier, depth + 1)?,
            trait_application: reify_semantic_trait_recipe(
                field_trait_application,
                reifier,
                depth + 1,
            )?,
        },
        SemanticScopeRecipe::BuiltinImpl {
            self_type: field_self_type,
            trait_kind: field_trait_kind,
        } => CtSemanticScopeRecipe::BuiltinImpl {
            self_type: reify_semantic_type_recipe(field_self_type, reifier, depth + 1)?,
            trait_kind: reify_semantic_builtin_trait(field_trait_kind, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_builtin_trait<R: CtDynamicReifier>(
    value: &SemanticBuiltinTrait,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticBuiltinTrait, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticBuiltinTrait::Drop => CtSemanticBuiltinTrait::Drop,
        SemanticBuiltinTrait::Copy => CtSemanticBuiltinTrait::Copy,
        SemanticBuiltinTrait::Value => CtSemanticBuiltinTrait::Value,
        SemanticBuiltinTrait::SyncDrop => CtSemanticBuiltinTrait::SyncDrop,
        SemanticBuiltinTrait::StoreElement => CtSemanticBuiltinTrait::StoreElement,
        SemanticBuiltinTrait::Transfer => CtSemanticBuiltinTrait::Transfer,
        SemanticBuiltinTrait::Share => CtSemanticBuiltinTrait::Share,
        SemanticBuiltinTrait::ThreadCallable => CtSemanticBuiltinTrait::ThreadCallable,
        SemanticBuiltinTrait::ClosedEnvironment => CtSemanticBuiltinTrait::ClosedEnvironment,
        SemanticBuiltinTrait::Send => CtSemanticBuiltinTrait::Send,
        SemanticBuiltinTrait::Sync => CtSemanticBuiltinTrait::Sync,
    })
}
pub fn reify_semantic_nominal_body<R: CtDynamicReifier>(
    value: &SemanticNominalBody,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticNominalBody, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticNominalBody::Product {
            fields: field_fields,
        } => CtSemanticNominalBody::Product {
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<CtSemanticFieldRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(reify_semantic_field_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticNominalBody::Enum {
            variants: field_variants,
        } => CtSemanticNominalBody::Enum {
            variants: {
                reifier.rows(
                    (field_variants).len(),
                    std::mem::size_of::<CtSemanticVariantRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_variants).len());
                for item in field_variants {
                    output.push(reify_semantic_variant_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
    })
}
pub fn reify_semantic_field_recipe<R: CtDynamicReifier>(
    value: &SemanticFieldRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticFieldRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticFieldRecipe {
        name: reifier.text(&value.name, depth + 1)?,
        ty: reify_semantic_type_recipe(&value.ty, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_variant_recipe<R: CtDynamicReifier>(
    value: &SemanticVariantRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticVariantRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticVariantRecipe {
        name: reifier.text(&value.name, depth + 1)?,
        fields: {
            reifier.rows(
                (value.fields).len(),
                std::mem::size_of::<CtSemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.fields).len());
            for item in &value.fields {
                output.push(reify_semantic_type_recipe(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_formal<R: CtDynamicReifier>(
    value: &SemanticFormal,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticFormal, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticFormal::Type {
            requirements: field_requirements,
        } => CtSemanticFormal::Type {
            requirements: {
                reifier.rows(
                    (field_requirements).len(),
                    std::mem::size_of::<CtSemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_requirements).len());
                for item in field_requirements {
                    output.push(reify_semantic_requirement_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticFormal::Const {
            carrier: field_carrier,
        } => CtSemanticFormal::Const {
            carrier: reify_semantic_type_recipe(field_carrier, reifier, depth + 1)?,
        },
        SemanticFormal::Lifetime => CtSemanticFormal::Lifetime,
    })
}
pub fn reify_semantic_implicit_lifetime<R: CtDynamicReifier>(
    value: &SemanticImplicitLifetime,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticImplicitLifetime, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticImplicitLifetime {
        ordinal: {
            reifier.node(depth + 1)?;
            value.ordinal
        },
        occurrence_path: {
            reifier.rows(
                (value.occurrence_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrence_path).len());
            for item in &value.occurrence_path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn reify_semantic_environment_formal<R: CtDynamicReifier>(
    value: &SemanticEnvironmentFormal,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticEnvironmentFormal, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticEnvironmentFormal {
        kind: reify_semantic_formal(&value.kind, reifier, depth + 1)?,
        source: reify_semantic_actual_recipe(&value.source, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_instance<R: CtDynamicReifier>(
    value: &SemanticInstance,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticInstance, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticInstance {
        id: reify_semantic_instance_id(&value.id, reifier, depth + 1)?,
        definition: reify_semantic_definition_id(&value.definition, reifier, depth + 1)?,
        target: reify_semantic_instance_target(&value.target, reifier, depth + 1)?,
        lifetime_instantiation: reify_semantic_lifetime_instantiation(
            &value.lifetime_instantiation,
            reifier,
            depth + 1,
        )?,
        parent: match &value.parent {
            Some(item) => Some(reify_semantic_instance_id(item, reifier, depth + 1)?),
            None => None,
        },
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtSemanticActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_semantic_actual(item, reifier, depth + 1)?);
            }
            output
        },
        header_embedding: {
            reifier.rows(
                (value.header_embedding).len(),
                std::mem::size_of::<CtSemanticHeaderBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.header_embedding).len());
            for item in &value.header_embedding {
                output.push(reify_semantic_header_binding(item, reifier, depth + 1)?);
            }
            output
        },
        environment: match &value.environment {
            Some(item) => Some(reify_semantic_environment_instance(
                item,
                reifier,
                depth + 1,
            )?),
            None => None,
        },
        self_type: match &value.self_type {
            Some(item) => Some(reify_semantic_type_value(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_semantic_lifetime_instantiation<R: CtDynamicReifier>(
    value: &SemanticLifetimeInstantiation,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticLifetimeInstantiation, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticLifetimeInstantiation::Rigid => CtSemanticLifetimeInstantiation::Rigid,
        SemanticLifetimeInstantiation::Partition { rigid: field_rigid } => {
            CtSemanticLifetimeInstantiation::Partition {
                rigid: reify_semantic_instance_id(field_rigid, reifier, depth + 1)?,
            }
        }
    })
}
pub fn reify_semantic_function_target<R: CtDynamicReifier>(
    value: &SemanticFunctionTarget,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticFunctionTarget, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticFunctionTarget::Core {
            module: field_module,
            function: field_function,
        } => CtSemanticFunctionTarget::Core {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            function: {
                reifier.node(depth + 1)?;
                *(field_function)
            },
        },
        SemanticFunctionTarget::CoreClosure {
            module: field_module,
            creator: field_creator,
            expression: field_expression,
            occurrence: field_occurrence,
        } => CtSemanticFunctionTarget::CoreClosure {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            creator: {
                reifier.node(depth + 1)?;
                *(field_creator)
            },
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        SemanticFunctionTarget::Typed(field_value_0) => CtSemanticFunctionTarget::Typed({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_semantic_instance_target<R: CtDynamicReifier>(
    value: &SemanticInstanceTarget,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticInstanceTarget, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticInstanceTarget::Function(field_value_0) => CtSemanticInstanceTarget::Function(
            reify_semantic_function_target(field_value_0, reifier, depth + 1)?,
        ),
        SemanticInstanceTarget::Scope => CtSemanticInstanceTarget::Scope,
    })
}
pub fn reify_semantic_closure_target<R: CtDynamicReifier>(
    value: &SemanticClosureTarget,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticClosureTarget, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticClosureTarget::Core {
            module: field_module,
            creator: field_creator,
            expression: field_expression,
            occurrence: field_occurrence,
        } => CtSemanticClosureTarget::Core {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            creator: {
                reifier.node(depth + 1)?;
                *(field_creator)
            },
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        SemanticClosureTarget::Typed(field_value_0) => CtSemanticClosureTarget::Typed({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_semantic_header_binding<R: CtDynamicReifier>(
    value: &SemanticHeaderBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticHeaderBinding, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticHeaderBinding {
        header: {
            reifier.node(depth + 1)?;
            value.header
        },
        actual: reify_semantic_class_expr(&value.actual, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_environment_instance<R: CtDynamicReifier>(
    value: &SemanticEnvironmentInstance,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticEnvironmentInstance, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticEnvironmentInstance {
        closure: reify_semantic_closure_target(&value.closure, reifier, depth + 1)?,
        actuals: {
            reifier.rows(
                (value.actuals).len(),
                std::mem::size_of::<CtSemanticActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(reify_semantic_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_actual<R: CtDynamicReifier>(
    value: &SemanticActual,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticActual, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticActual::Type(field_value_0) => CtSemanticActual::Type(reify_semantic_type_value(
            field_value_0,
            reifier,
            depth + 1,
        )?),
        SemanticActual::Const {
            ty: field_ty,
            value: field_value,
        } => CtSemanticActual::Const {
            ty: reify_semantic_type_value(field_ty, reifier, depth + 1)?,
            value: reifier.constant(field_value, depth + 1)?,
        },
        SemanticActual::Life(field_value_0) => CtSemanticActual::Life(reify_semantic_class_expr(
            field_value_0,
            reifier,
            depth + 1,
        )?),
        SemanticActual::CoreRecipe {
            scope: field_scope,
            recipe: field_recipe,
        } => CtSemanticActual::CoreRecipe {
            scope: reify_semantic_instance_id(field_scope, reifier, depth + 1)?,
            recipe: {
                reifier.rows(1, std::mem::size_of::<CtSemanticActualRecipe>(), depth + 1)?;
                Box::new(reify_semantic_actual_recipe(
                    (field_recipe).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticActual::CoreInferred {
            scope: field_scope,
            call: field_call,
            formal: field_formal,
        } => CtSemanticActual::CoreInferred {
            scope: reify_semantic_instance_id(field_scope, reifier, depth + 1)?,
            call: {
                reifier.node(depth + 1)?;
                *(field_call)
            },
            formal: {
                reifier.node(depth + 1)?;
                *(field_formal)
            },
        },
    })
}
pub fn reify_semantic_core_type_source<R: CtDynamicReifier>(
    value: &SemanticCoreTypeSource,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticCoreTypeSource, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticCoreTypeSource::OriginalRecipe(field_value_0) => {
            CtSemanticCoreTypeSource::OriginalRecipe({
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_value_0).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            })
        }
        SemanticCoreTypeSource::Expression {
            module: field_module,
            function: field_function,
            expression: field_expression,
            occurrence: field_occurrence,
            recipe: field_recipe,
        } => CtSemanticCoreTypeSource::Expression {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            function: {
                reifier.node(depth + 1)?;
                *(field_function)
            },
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
            recipe: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_recipe).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticCoreTypeSource::OriginalExpression {
            module: field_module,
            function: field_function,
            expression: field_expression,
            occurrence: field_occurrence,
            source: field_source,
        } => CtSemanticCoreTypeSource::OriginalExpression {
            module: {
                reifier.node(depth + 1)?;
                *(field_module)
            },
            function: {
                reifier.node(depth + 1)?;
                *(field_function)
            },
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
            source: reify_semantic_original_type_source(field_source, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_type_value<R: CtDynamicReifier>(
    value: &SemanticTypeValue,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticTypeValue, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticTypeValue::Resolved {
            ty: field_ty,
            life: field_life,
            classes: field_classes,
        } => CtSemanticTypeValue::Resolved {
            ty: reify_type_value(field_ty, reifier, depth + 1)?,
            life: reify_storage_life_shape(field_life, reifier, depth + 1)?,
            classes: {
                reifier.rows(
                    (field_classes).len(),
                    std::mem::size_of::<CtSemanticClassExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_classes).len());
                for item in field_classes {
                    output.push(reify_semantic_class_expr(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticTypeValue::Core {
            scope: field_scope,
            source: field_source,
        } => CtSemanticTypeValue::Core {
            scope: reify_semantic_instance_id(field_scope, reifier, depth + 1)?,
            source: reify_semantic_core_type_source(field_source, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_scope_ref<R: CtDynamicReifier>(
    value: &SemanticScopeRef,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticScopeRef, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticScopeRef::Ancestor(field_value_0) => CtSemanticScopeRef::Ancestor({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_semantic_class_expr<R: CtDynamicReifier>(
    value: &SemanticClassExpr,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticClassExpr, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticClassExpr::Formal {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticClassExpr::Formal {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticClassExpr::Implicit {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticClassExpr::Implicit {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticClassExpr::Bound {
            depth: field_depth,
            index: field_index,
        } => CtSemanticClassExpr::Bound {
            depth: {
                reifier.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                reifier.node(depth + 1)?;
                *(field_index)
            },
        },
        SemanticClassExpr::OuterOccurrence {
            operand: field_operand,
            path: field_path,
            mode: field_mode,
        } => CtSemanticClassExpr::OuterOccurrence {
            operand: reify_semantic_operand(field_operand, reifier, depth + 1)?,
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        SemanticClassExpr::IntrinsicClass {
            operand: field_operand,
            path: field_path,
            class: field_class,
        } => CtSemanticClassExpr::IntrinsicClass {
            operand: reify_semantic_operand(field_operand, reifier, depth + 1)?,
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            class: {
                reifier.node(depth + 1)?;
                *(field_class)
            },
        },
        SemanticClassExpr::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticClassExpr::Environment {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
    })
}
pub fn reify_semantic_operand<R: CtDynamicReifier>(
    value: &SemanticOperand,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOperand, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOperand::Parameter(field_value_0) => CtSemanticOperand::Parameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOperand::Capture(field_value_0) => CtSemanticOperand::Capture({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOperand::Receiver => CtSemanticOperand::Receiver,
        SemanticOperand::Value(field_value_0) => CtSemanticOperand::Value({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOperand::Local(field_value_0) => CtSemanticOperand::Local({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOperand::TypeArgument {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticOperand::TypeArgument {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticOperand::Environment(field_value_0) => CtSemanticOperand::Environment({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOperand::CoreExpression {
            expression: field_expression,
            occurrence: field_occurrence,
        } => CtSemanticOperand::CoreExpression {
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
        },
    })
}
pub fn reify_semantic_site_recipe<R: CtDynamicReifier>(
    value: &SemanticSiteRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSiteRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticSiteRecipe {
        id: {
            reifier.node(depth + 1)?;
            value.id
        },
        occurrence_path: {
            reifier.rows(
                (value.occurrence_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrence_path).len());
            for item in &value.occurrence_path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        kind: reify_semantic_site_recipe_kind(&value.kind, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_site_recipe_kind<R: CtDynamicReifier>(
    value: &SemanticSiteRecipeKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSiteRecipeKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticSiteRecipeKind::Construct {
            result: field_result,
        } => CtSemanticSiteRecipeKind::Construct {
            result: reify_semantic_type_recipe(field_result, reifier, depth + 1)?,
        },
        SemanticSiteRecipeKind::Call {
            target: field_target,
            actuals: field_actuals,
        } => CtSemanticSiteRecipeKind::Call {
            target: reify_semantic_definition_id(field_target, reifier, depth + 1)?,
            actuals: {
                reifier.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticSiteRecipeKind::CallableCall {
            callee: field_callee,
        } => CtSemanticSiteRecipeKind::CallableCall {
            callee: reify_semantic_type_recipe(field_callee, reifier, depth + 1)?,
        },
        SemanticSiteRecipeKind::TypeUse {
            expected: field_expected,
        } => CtSemanticSiteRecipeKind::TypeUse {
            expected: reify_semantic_type_recipe(field_expected, reifier, depth + 1)?,
        },
        SemanticSiteRecipeKind::DeclareLocal {
            expected: field_expected,
        } => CtSemanticSiteRecipeKind::DeclareLocal {
            expected: reify_semantic_type_recipe(field_expected, reifier, depth + 1)?,
        },
        SemanticSiteRecipeKind::Closure {
            definition: field_definition,
            environment: field_environment,
        } => CtSemanticSiteRecipeKind::Closure {
            definition: reify_semantic_definition_id(field_definition, reifier, depth + 1)?,
            environment: {
                reifier.rows(
                    (field_environment).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticSiteRecipeKind::OriginalCall => CtSemanticSiteRecipeKind::OriginalCall,
    })
}
pub fn reify_semantic_site<R: CtDynamicReifier>(
    value: &SemanticSite,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSite, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticSite {
        instance: reify_semantic_instance_id(&value.instance, reifier, depth + 1)?,
        recipe: {
            reifier.node(depth + 1)?;
            value.recipe
        },
        at: reify_semantic_site_address(&value.at, reifier, depth + 1)?,
        action: reify_semantic_site_action(&value.action, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_site_address<R: CtDynamicReifier>(
    value: &SemanticSiteAddress,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSiteAddress, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticSiteAddress::Core {
            expression: field_expression,
            occurrence: field_occurrence,
            role: field_role,
        } => CtSemanticSiteAddress::Core {
            expression: {
                reifier.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
            role: reify_semantic_emission_role(field_role, reifier, depth + 1)?,
        },
        SemanticSiteAddress::Typed {
            block: field_block,
            instruction: field_instruction,
            edge: field_edge,
        } => CtSemanticSiteAddress::Typed {
            block: {
                reifier.node(depth + 1)?;
                *(field_block)
            },
            instruction: {
                reifier.node(depth + 1)?;
                *(field_instruction)
            },
            edge: reify_semantic_site_edge(field_edge, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_emission_role<R: CtDynamicReifier>(
    value: &SemanticEmissionRole,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticEmissionRole, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticEmissionRole::Construct => CtSemanticEmissionRole::Construct,
        SemanticEmissionRole::Call => CtSemanticEmissionRole::Call,
        SemanticEmissionRole::Ascription => CtSemanticEmissionRole::Ascription,
        SemanticEmissionRole::Binding => CtSemanticEmissionRole::Binding,
        SemanticEmissionRole::Closure => CtSemanticEmissionRole::Closure,
    })
}
pub fn reify_semantic_site_edge<R: CtDynamicReifier>(
    value: &SemanticSiteEdge,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSiteEdge, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticSiteEdge::Before => CtSemanticSiteEdge::Before,
        SemanticSiteEdge::After => CtSemanticSiteEdge::After,
    })
}
pub fn reify_semantic_site_action<R: CtDynamicReifier>(
    value: &SemanticSiteAction,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSiteAction, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticSiteAction::Construct {
            result: field_result,
            value: field_value,
        } => CtSemanticSiteAction::Construct {
            result: reify_semantic_operand(field_result, reifier, depth + 1)?,
            value: reify_semantic_type_value(field_value, reifier, depth + 1)?,
        },
        SemanticSiteAction::Call {
            target: field_target,
            actuals: field_actuals,
        } => CtSemanticSiteAction::Call {
            target: reify_semantic_function_target(field_target, reifier, depth + 1)?,
            actuals: {
                reifier.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<CtSemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(reify_semantic_actual(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticSiteAction::CallableCall {
            callee: field_callee,
        } => CtSemanticSiteAction::CallableCall {
            callee: reify_semantic_operand(field_callee, reifier, depth + 1)?,
        },
        SemanticSiteAction::TypeUse {
            subject: field_subject,
            expected: field_expected,
        } => CtSemanticSiteAction::TypeUse {
            subject: reify_semantic_operand(field_subject, reifier, depth + 1)?,
            expected: reify_semantic_type_value(field_expected, reifier, depth + 1)?,
        },
        SemanticSiteAction::DeclareLocal {
            local: field_local,
            expected: field_expected,
        } => CtSemanticSiteAction::DeclareLocal {
            local: reify_semantic_local_target(field_local, reifier, depth + 1)?,
            expected: reify_semantic_type_value(field_expected, reifier, depth + 1)?,
        },
        SemanticSiteAction::Closure {
            result: field_result,
            closure: field_closure,
            environment: field_environment,
        } => CtSemanticSiteAction::Closure {
            result: reify_semantic_operand(field_result, reifier, depth + 1)?,
            closure: reify_semantic_closure_target(field_closure, reifier, depth + 1)?,
            environment: {
                reifier.rows(
                    (field_environment).len(),
                    std::mem::size_of::<CtSemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(reify_semantic_actual(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticSiteAction::CoreBodyCall => CtSemanticSiteAction::CoreBodyCall,
        SemanticSiteAction::BodyCall {
            actuals: field_actuals,
        } => CtSemanticSiteAction::BodyCall {
            actuals: {
                reifier.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<CtSemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(reify_semantic_actual(item, reifier, depth + 1)?);
                }
                output
            },
        },
    })
}
pub fn reify_semantic_local_target<R: CtDynamicReifier>(
    value: &SemanticLocalTarget,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticLocalTarget, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticLocalTarget::Core {
            statement: field_statement,
            occurrence: field_occurrence,
        } => CtSemanticLocalTarget::Core {
            statement: {
                reifier.node(depth + 1)?;
                *(field_statement)
            },
            occurrence: {
                reifier.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        SemanticLocalTarget::Typed(field_value_0) => CtSemanticLocalTarget::Typed({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_semantic_recipe_operand<R: CtDynamicReifier>(
    value: &SemanticRecipeOperand,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticRecipeOperand, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticRecipeOperand::TypeFormal(field_value_0) => CtSemanticRecipeOperand::TypeFormal(
            reify_semantic_formal_ref(field_value_0, reifier, depth + 1)?,
        ),
        SemanticRecipeOperand::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticRecipeOperand::Environment {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticRecipeOperand::Parameter(field_value_0) => CtSemanticRecipeOperand::Parameter({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticRecipeOperand::Receiver => CtSemanticRecipeOperand::Receiver,
    })
}
pub fn reify_semantic_formal_ref<R: CtDynamicReifier>(
    value: &SemanticFormalRef,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticFormalRef, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticFormalRef {
        scope: reify_semantic_scope_ref(&value.scope, reifier, depth + 1)?,
        ordinal: {
            reifier.node(depth + 1)?;
            value.ordinal
        },
    })
}
pub fn reify_semantic_life_recipe<R: CtDynamicReifier>(
    value: &SemanticLifeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticLifeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticLifeRecipe::Formal(field_value_0) => CtSemanticLifeRecipe::Formal(
            reify_semantic_formal_ref(field_value_0, reifier, depth + 1)?,
        ),
        SemanticLifeRecipe::Implicit {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticLifeRecipe::Implicit {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticLifeRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticLifeRecipe::Environment {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticLifeRecipe::Bound {
            depth: field_depth,
            index: field_index,
        } => CtSemanticLifeRecipe::Bound {
            depth: {
                reifier.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                reifier.node(depth + 1)?;
                *(field_index)
            },
        },
        SemanticLifeRecipe::Invocation(field_value_0) => CtSemanticLifeRecipe::Invocation({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticLifeRecipe::OuterOccurrence {
            operand: field_operand,
            path: field_path,
            mode: field_mode,
        } => CtSemanticLifeRecipe::OuterOccurrence {
            operand: reify_semantic_recipe_operand(field_operand, reifier, depth + 1)?,
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        SemanticLifeRecipe::IntrinsicClass {
            operand: field_operand,
            path: field_path,
            class: field_class,
        } => CtSemanticLifeRecipe::IntrinsicClass {
            operand: reify_semantic_recipe_operand(field_operand, reifier, depth + 1)?,
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            class: {
                reifier.node(depth + 1)?;
                *(field_class)
            },
        },
    })
}
pub fn reify_semantic_outer_life_recipe<R: CtDynamicReifier>(
    value: &SemanticOuterLifeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOuterLifeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOuterLifeRecipe::Class(field_value_0) => CtSemanticOuterLifeRecipe::Class(
            reify_semantic_life_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticOuterLifeRecipe::Static => CtSemanticOuterLifeRecipe::Static,
    })
}
pub fn reify_semantic_type_recipe<R: CtDynamicReifier>(
    value: &SemanticTypeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticTypeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticTypeRecipe::Atom(field_value_0) => {
            CtSemanticTypeRecipe::Atom(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
        SemanticTypeRecipe::Exact(field_value_0) => CtSemanticTypeRecipe::Exact({
            reifier.rows(
                1,
                std::mem::size_of::<CtSemanticExactTypeRecipe>(),
                depth + 1,
            )?;
            Box::new(reify_semantic_exact_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::Formal(field_value_0) => CtSemanticTypeRecipe::Formal(
            reify_semantic_formal_ref(field_value_0, reifier, depth + 1)?,
        ),
        SemanticTypeRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticTypeRecipe::Environment {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticTypeRecipe::SelfType { scope: field_scope } => CtSemanticTypeRecipe::SelfType {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
        },
        SemanticTypeRecipe::Nominal {
            definition: field_definition,
            arguments: field_arguments,
        } => CtSemanticTypeRecipe::Nominal {
            definition: reify_semantic_definition_id(field_definition, reifier, depth + 1)?,
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticTypeRecipe::AssociatedType {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => CtSemanticTypeRecipe::AssociatedType {
            receiver: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_receiver).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            application: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTraitRecipe>(), depth + 1)?;
                Box::new(reify_semantic_trait_recipe(
                    (field_application).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
        SemanticTypeRecipe::Tuple(field_value_0) => CtSemanticTypeRecipe::Tuple({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtSemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_semantic_type_recipe(item, reifier, depth + 1)?);
            }
            output
        }),
        SemanticTypeRecipe::Array {
            element: field_element,
            length: field_length,
        } => CtSemanticTypeRecipe::Array {
            element: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_element).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            length: {
                reifier.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                Box::new(reify_semantic_const_recipe(
                    (field_length).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Slice(field_value_0) => CtSemanticTypeRecipe::Slice({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::Address {
            mutable: field_mutable,
            lifetime: field_lifetime,
            inner: field_inner,
        } => CtSemanticTypeRecipe::Address {
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            lifetime: reify_semantic_outer_life_recipe(field_lifetime, reifier, depth + 1)?,
            inner: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_inner).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Desc {
            lifetime: field_lifetime,
            inner: field_inner,
        } => CtSemanticTypeRecipe::Desc {
            lifetime: reify_semantic_outer_life_recipe(field_lifetime, reifier, depth + 1)?,
            inner: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_inner).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Managed(field_value_0) => CtSemanticTypeRecipe::Managed({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::OwnedStore(field_value_0) => CtSemanticTypeRecipe::OwnedStore({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::Mutex(field_value_0) => CtSemanticTypeRecipe::Mutex({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::MutexGuard {
            payload: field_payload,
            retained: field_retained,
        } => CtSemanticTypeRecipe::MutexGuard {
            payload: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_payload).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            retained: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_retained_shape_recipe(
                    (field_retained).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Registration(field_value_0) => CtSemanticTypeRecipe::Registration({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticTypeRecipe::AsyncRegistration(field_value_0) => {
            CtSemanticTypeRecipe::AsyncRegistration({
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_value_0).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            })
        }
        SemanticTypeRecipe::Task {
            result: field_result,
            retained: field_retained,
        } => CtSemanticTypeRecipe::Task {
            result: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            retained: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_retained_shape_recipe(
                    (field_retained).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::ErrorUnion {
            error: field_error,
            result: field_result,
        } => CtSemanticTypeRecipe::ErrorUnion {
            error: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_error).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Qualified {
            storage: field_storage,
            life: field_life,
        } => CtSemanticTypeRecipe::Qualified {
            storage: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_storage).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            life: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticStorageShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_storage_shape_recipe(
                    (field_life).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Code {
            kind: field_kind,
            signature: field_signature,
        } => CtSemanticTypeRecipe::Code {
            kind: reify_semantic_code_kind(field_kind, reifier, depth + 1)?,
            signature: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticSignatureRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_signature_recipe(
                    (field_signature).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Closure {
            definition: field_definition,
            arguments: field_arguments,
            environment: field_environment,
        } => CtSemanticTypeRecipe::Closure {
            definition: reify_semantic_definition_id(field_definition, reifier, depth + 1)?,
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
            environment: {
                reifier.rows(
                    (field_environment).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticTypeRecipe::Dyn {
            owned: field_owned,
            interface: field_interface,
        } => CtSemanticTypeRecipe::Dyn {
            owned: {
                reifier.node(depth + 1)?;
                *(field_owned)
            },
            interface: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticDynInterfaceRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_dyn_interface_recipe(
                    (field_interface).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::Opaque {
            definition: field_definition,
            arguments: field_arguments,
            hidden: field_hidden,
        } => CtSemanticTypeRecipe::Opaque {
            definition: reify_semantic_definition_id(field_definition, reifier, depth + 1)?,
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
                }
                output
            },
            hidden: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_hidden).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticTypeRecipe::AnonymousSum(field_value_0) => CtSemanticTypeRecipe::AnonymousSum({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtSemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_semantic_type_recipe(item, reifier, depth + 1)?);
            }
            output
        }),
        SemanticTypeRecipe::UnresolvedNamed(field_value_0) => {
            CtSemanticTypeRecipe::UnresolvedNamed(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticTypeRecipe::Unsupported(field_value_0) => {
            CtSemanticTypeRecipe::Unsupported(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticTypeRecipe::OriginalType {
            scope: field_scope,
            source: field_source,
        } => CtSemanticTypeRecipe::OriginalType {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            source: reify_semantic_original_type_source(field_source, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_exact_type_recipe<R: CtDynamicReifier>(
    value: &SemanticExactTypeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticExactTypeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticExactTypeRecipe {
        ty: reify_type_value(&value.ty, reifier, depth + 1)?,
        life: reify_storage_life_shape(&value.life, reifier, depth + 1)?,
        classes: {
            reifier.rows(
                (value.classes).len(),
                std::mem::size_of::<CtSemanticLifeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push(reify_semantic_life_recipe(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_code_kind<R: CtDynamicReifier>(
    value: &SemanticCodeKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticCodeKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticCodeKind::FunctionPointer => CtSemanticCodeKind::FunctionPointer,
        SemanticCodeKind::BorrowedFunctionPointer => CtSemanticCodeKind::BorrowedFunctionPointer,
        SemanticCodeKind::ForeignCallback => CtSemanticCodeKind::ForeignCallback,
    })
}
pub fn reify_semantic_actual_recipe<R: CtDynamicReifier>(
    value: &SemanticActualRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticActualRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticActualRecipe::Type(field_value_0) => CtSemanticActualRecipe::Type(
            reify_semantic_type_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticActualRecipe::Const {
            carrier: field_carrier,
            value: field_value,
        } => CtSemanticActualRecipe::Const {
            carrier: reify_semantic_type_recipe(field_carrier, reifier, depth + 1)?,
            value: reify_semantic_const_recipe(field_value, reifier, depth + 1)?,
        },
        SemanticActualRecipe::Life(field_value_0) => CtSemanticActualRecipe::Life(
            reify_semantic_life_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticActualRecipe::Infer => CtSemanticActualRecipe::Infer,
    })
}
pub fn reify_semantic_class_sequence_recipe<R: CtDynamicReifier>(
    value: &SemanticClassSequenceRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticClassSequenceRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticClassSequenceRecipe {
        parts: {
            reifier.rows(
                (value.parts).len(),
                std::mem::size_of::<CtSemanticClassSequencePart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parts).len());
            for item in &value.parts {
                output.push(reify_semantic_class_sequence_part(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn reify_semantic_class_sequence_part<R: CtDynamicReifier>(
    value: &SemanticClassSequencePart,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticClassSequencePart, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticClassSequencePart::One(field_value_0) => CtSemanticClassSequencePart::One(
            reify_semantic_life_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticClassSequencePart::AllClasses {
            ty: field_ty,
            path: field_path,
        } => CtSemanticClassSequencePart::AllClasses {
            ty: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
        },
    })
}
pub fn reify_semantic_storage_shape_recipe<R: CtDynamicReifier>(
    value: &SemanticStorageShapeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticStorageShapeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticStorageShapeRecipe {
        domain: reify_semantic_class_sequence_recipe(&value.domain, reifier, depth + 1)?,
        slots: {
            reifier.rows(
                (value.slots).len(),
                std::mem::size_of::<CtSemanticSlotRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(reify_semantic_slot_recipe(item, reifier, depth + 1)?);
            }
            output
        },
        retained: {
            reifier.rows(
                (value.retained).len(),
                std::mem::size_of::<CtSemanticRetainedPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(reify_semantic_retained_part(item, reifier, depth + 1)?);
            }
            output
        },
        nominal: {
            reifier.rows(
                (value.nominal).len(),
                std::mem::size_of::<CtSemanticNominalPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal).len());
            for item in &value.nominal {
                output.push(reify_semantic_nominal_part(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_slot_recipe<R: CtDynamicReifier>(
    value: &SemanticSlotRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSlotRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticSlotRecipe {
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<PlaceProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
        class: reify_semantic_life_recipe(&value.class, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_retained_part<R: CtDynamicReifier>(
    value: &SemanticRetainedPart,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticRetainedPart, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticRetainedPart::One {
            path: field_path,
            child_class: field_child_class,
            parent: field_parent,
        } => CtSemanticRetainedPart::One {
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            child_class: {
                reifier.node(depth + 1)?;
                *(field_child_class)
            },
            parent: reify_semantic_life_recipe(field_parent, reifier, depth + 1)?,
        },
        SemanticRetainedPart::All {
            path: field_path,
            parents: field_parents,
        } => CtSemanticRetainedPart::All {
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            parents: reify_semantic_class_sequence_recipe(field_parents, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_nominal_part<R: CtDynamicReifier>(
    value: &SemanticNominalPart,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticNominalPart, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticNominalPart::One {
            path: field_path,
            declaration: field_declaration,
            argument: field_argument,
            class: field_class,
        } => CtSemanticNominalPart::One {
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            declaration: reify_life_declaration_id(field_declaration, reifier, depth + 1)?,
            argument: {
                reifier.node(depth + 1)?;
                *(field_argument)
            },
            class: reify_semantic_life_recipe(field_class, reifier, depth + 1)?,
        },
        SemanticNominalPart::FromType {
            path: field_path,
            ty: field_ty,
            classes: field_classes,
        } => CtSemanticNominalPart::FromType {
            path: {
                reifier.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        reifier.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            ty: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            classes: reify_semantic_class_sequence_recipe(field_classes, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_retained_shape_recipe<R: CtDynamicReifier>(
    value: &SemanticRetainedShapeRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticRetainedShapeRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticRetainedShapeRecipe {
        domain: reify_semantic_class_sequence_recipe(&value.domain, reifier, depth + 1)?,
        ports: {
            reifier.rows(
                (value.ports).len(),
                std::mem::size_of::<CtSemanticPortPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(reify_semantic_port_part(item, reifier, depth + 1)?);
            }
            output
        },
        retained: {
            reifier.rows(
                (value.retained).len(),
                std::mem::size_of::<CtSemanticRetainedPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(reify_semantic_retained_part(item, reifier, depth + 1)?);
            }
            output
        },
        nominal: {
            reifier.rows(
                (value.nominal).len(),
                std::mem::size_of::<CtSemanticNominalPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal).len());
            for item in &value.nominal {
                output.push(reify_semantic_nominal_part(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_port_part<R: CtDynamicReifier>(
    value: &SemanticPortPart,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticPortPart, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticPortPart::One {
            leaf: field_leaf,
            mode: field_mode,
            class: field_class,
        } => CtSemanticPortPart::One {
            leaf: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_leaf).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
            class: reify_semantic_life_recipe(field_class, reifier, depth + 1)?,
        },
        SemanticPortPart::FromType {
            ty: field_ty,
            classes: field_classes,
        } => CtSemanticPortPart::FromType {
            ty: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_ty).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            classes: reify_semantic_class_sequence_recipe(field_classes, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_const_recipe<R: CtDynamicReifier>(
    value: &SemanticConstRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticConstRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticConstRecipe::Value(field_value_0) => {
            CtSemanticConstRecipe::Value(reifier.constant(field_value_0, depth + 1)?)
        }
        SemanticConstRecipe::IntegerLiteral(field_value_0) => {
            CtSemanticConstRecipe::IntegerLiteral(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticConstRecipe::FloatLiteral(field_value_0) => {
            CtSemanticConstRecipe::FloatLiteral(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticConstRecipe::Formal(field_value_0) => CtSemanticConstRecipe::Formal(
            reify_semantic_formal_ref(field_value_0, reifier, depth + 1)?,
        ),
        SemanticConstRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => CtSemanticConstRecipe::Environment {
            scope: reify_semantic_scope_ref(field_scope, reifier, depth + 1)?,
            ordinal: {
                reifier.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        SemanticConstRecipe::Unary {
            op: field_op,
            operand_type: field_operand_type,
            value: field_value,
        } => CtSemanticConstRecipe::Unary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            operand_type: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_operand_type).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                Box::new(reify_semantic_const_recipe(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticConstRecipe::Binary {
            op: field_op,
            operand_type: field_operand_type,
            left: field_left,
            right: field_right,
        } => CtSemanticConstRecipe::Binary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            operand_type: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_operand_type).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            left: {
                reifier.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                Box::new(reify_semantic_const_recipe(
                    (field_left).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            right: {
                reifier.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                Box::new(reify_semantic_const_recipe(
                    (field_right).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticConstRecipe::Convert {
            source_type: field_source_type,
            target_type: field_target_type,
            value: field_value,
        } => CtSemanticConstRecipe::Convert {
            source_type: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_source_type).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            target_type: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_target_type).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                Box::new(reify_semantic_const_recipe(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticConstRecipe::AssociatedConst {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => CtSemanticConstRecipe::AssociatedConst {
            receiver: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                Box::new(reify_semantic_type_recipe(
                    (field_receiver).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            application: {
                reifier.rows(1, std::mem::size_of::<CtSemanticTraitRecipe>(), depth + 1)?;
                Box::new(reify_semantic_trait_recipe(
                    (field_application).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            member: {
                reifier.node(depth + 1)?;
                *(field_member)
            },
        },
    })
}
pub fn reify_semantic_trait_recipe<R: CtDynamicReifier>(
    value: &SemanticTraitRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticTraitRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticTraitRecipe {
        declaration: reify_life_declaration_id(&value.declaration, reifier, depth + 1)?,
        arguments: {
            reifier.rows(
                (value.arguments).len(),
                std::mem::size_of::<CtSemanticActualRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(reify_semantic_actual_recipe(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_semantic_requirement_recipe<R: CtDynamicReifier>(
    value: &SemanticRequirementRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticRequirementRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticRequirementRecipe::Structural {
            ty: field_ty,
            bound: field_bound,
        } => CtSemanticRequirementRecipe::Structural {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            bound: reify_dyn_structural_bound(field_bound, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::Trait {
            ty: field_ty,
            application: field_application,
        } => CtSemanticRequirementRecipe::Trait {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            application: reify_semantic_trait_recipe(field_application, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::Callable {
            ty: field_ty,
            kind: field_kind,
            signature: field_signature,
        } => CtSemanticRequirementRecipe::Callable {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            kind: {
                reifier.node(depth + 1)?;
                *(field_kind)
            },
            signature: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticSignatureRecipe>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_signature_recipe(
                    (field_signature).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticRequirementRecipe::TypeEqual {
            left: field_left,
            right: field_right,
        } => CtSemanticRequirementRecipe::TypeEqual {
            left: reify_semantic_type_recipe(field_left, reifier, depth + 1)?,
            right: reify_semantic_type_recipe(field_right, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::ConstEqual {
            carrier: field_carrier,
            left: field_left,
            right: field_right,
        } => CtSemanticRequirementRecipe::ConstEqual {
            carrier: reify_semantic_type_recipe(field_carrier, reifier, depth + 1)?,
            left: reify_semantic_const_recipe(field_left, reifier, depth + 1)?,
            right: reify_semantic_const_recipe(field_right, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::LifeEqual {
            left: field_left,
            right: field_right,
        } => CtSemanticRequirementRecipe::LifeEqual {
            left: reify_semantic_life_recipe(field_left, reifier, depth + 1)?,
            right: reify_semantic_life_recipe(field_right, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::Outlives {
            longer: field_longer,
            shorter: field_shorter,
        } => CtSemanticRequirementRecipe::Outlives {
            longer: reify_semantic_life_recipe(field_longer, reifier, depth + 1)?,
            shorter: reify_semantic_life_recipe(field_shorter, reifier, depth + 1)?,
        },
        SemanticRequirementRecipe::ForAll {
            lifetimes: field_lifetimes,
            requirements: field_requirements,
        } => CtSemanticRequirementRecipe::ForAll {
            lifetimes: {
                reifier.node(depth + 1)?;
                *(field_lifetimes)
            },
            requirements: {
                reifier.rows(
                    (field_requirements).len(),
                    std::mem::size_of::<CtSemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_requirements).len());
                for item in field_requirements {
                    output.push(reify_semantic_requirement_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticRequirementRecipe::Builtin {
            ty: field_ty,
            trait_kind: field_trait_kind,
        } => CtSemanticRequirementRecipe::Builtin {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            trait_kind: reify_semantic_builtin_trait(field_trait_kind, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_signature_recipe<R: CtDynamicReifier>(
    value: &SemanticSignatureRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticSignatureRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticSignatureRecipe {
        bound_lifetimes: {
            reifier.node(depth + 1)?;
            value.bound_lifetimes
        },
        invocation_lifetimes: {
            reifier.node(depth + 1)?;
            value.invocation_lifetimes
        },
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtSemanticParameterRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_semantic_parameter_recipe(item, reifier, depth + 1)?);
            }
            output
        },
        result: reify_semantic_type_recipe(&value.result, reifier, depth + 1)?,
        result_mode: {
            reifier.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            reifier.node(depth + 1)?;
            value.call_kind
        },
    })
}
pub fn reify_semantic_parameter_recipe<R: CtDynamicReifier>(
    value: &SemanticParameterRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticParameterRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticParameterRecipe {
        ty: reify_semantic_type_recipe(&value.ty, reifier, depth + 1)?,
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn reify_semantic_dyn_context_recipe<R: CtDynamicReifier>(
    value: &SemanticDynContextRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDynContextRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticDynContextRecipe::Closed => CtSemanticDynContextRecipe::Closed,
        SemanticDynContextRecipe::Type(field_value_0) => CtSemanticDynContextRecipe::Type({
            reifier.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
            Box::new(reify_semantic_type_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticDynContextRecipe::Shape(field_value_0) => CtSemanticDynContextRecipe::Shape({
            reifier.rows(
                1,
                std::mem::size_of::<CtSemanticRetainedShapeRecipe>(),
                depth + 1,
            )?;
            Box::new(reify_semantic_retained_shape_recipe(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
    })
}
pub fn reify_semantic_dyn_interface_recipe<R: CtDynamicReifier>(
    value: &SemanticDynInterfaceRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticDynInterfaceRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticDynInterfaceRecipe {
        application: reify_semantic_trait_recipe(&value.application, reifier, depth + 1)?,
        context: reify_semantic_dyn_context_recipe(&value.context, reifier, depth + 1)?,
        associated: {
            reifier.rows(
                (value.associated).len(),
                std::mem::size_of::<CtSemanticAssociatedBindingRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated).len());
            for item in &value.associated {
                output.push(reify_semantic_associated_binding_recipe(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn reify_semantic_associated_binding_recipe<R: CtDynamicReifier>(
    value: &SemanticAssociatedBindingRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticAssociatedBindingRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticAssociatedBindingRecipe {
        application: reify_semantic_trait_recipe(&value.application, reifier, depth + 1)?,
        member: {
            reifier.node(depth + 1)?;
            value.member
        },
        value: reify_semantic_associated_recipe(&value.value, reifier, depth + 1)?,
    })
}
pub fn reify_semantic_associated_recipe<R: CtDynamicReifier>(
    value: &SemanticAssociatedRecipe,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticAssociatedRecipe, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticAssociatedRecipe::Type(field_value_0) => CtSemanticAssociatedRecipe::Type(
            reify_semantic_type_recipe(field_value_0, reifier, depth + 1)?,
        ),
        SemanticAssociatedRecipe::Const {
            carrier: field_carrier,
            value: field_value,
        } => CtSemanticAssociatedRecipe::Const {
            carrier: reify_semantic_type_recipe(field_carrier, reifier, depth + 1)?,
            value: reify_semantic_const_recipe(field_value, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_original_closure_parameter<R: CtDynamicReifier>(
    value: &SemanticOriginalClosureParameter,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalClosureParameter, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalClosureParameter {
        name: reifier.text(&value.name, depth + 1)?,
        ty: match &value.ty {
            Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
            None => None,
        },
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_pattern<R: CtDynamicReifier>(
    value: &SemanticOriginalPattern,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalPattern, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalPattern {
        kind: reify_semantic_original_pattern_kind(&value.kind, reifier, depth + 1)?,
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_pattern_kind<R: CtDynamicReifier>(
    value: &SemanticOriginalPatternKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalPatternKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalPatternKind::Name(field_value_0) => {
            CtSemanticOriginalPatternKind::Name(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalPatternKind::Wildcard => CtSemanticOriginalPatternKind::Wildcard,
        SemanticOriginalPatternKind::Tuple(field_value_0) => {
            CtSemanticOriginalPatternKind::Tuple({
                reifier.rows(
                    (field_value_0).len(),
                    std::mem::size_of::<CtSemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_value_0).len());
                for item in field_value_0 {
                    output.push(reify_semantic_original_pattern(item, reifier, depth + 1)?);
                }
                output
            })
        }
        SemanticOriginalPatternKind::Record {
            ty: field_ty,
            fields: field_fields,
        } => CtSemanticOriginalPatternKind::Record {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalPattern)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_pattern(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        SemanticOriginalPatternKind::Reference {
            mutable: field_mutable,
            pattern: field_pattern,
        } => CtSemanticOriginalPatternKind::Reference {
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            pattern: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticOriginalPattern>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_original_pattern(
                    (field_pattern).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalPatternKind::ErrorArm {
            is_error: field_is_error,
            pattern: field_pattern,
        } => CtSemanticOriginalPatternKind::ErrorArm {
            is_error: {
                reifier.node(depth + 1)?;
                *(field_is_error)
            },
            pattern: {
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticOriginalPattern>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_original_pattern(
                    (field_pattern).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalPatternKind::Variant {
            ty: field_ty,
            variant: field_variant,
            fields: field_fields,
        } => CtSemanticOriginalPatternKind::Variant {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            variant: reifier.text(field_variant, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<CtSemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(reify_semantic_original_pattern(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalPatternKind::Integer(field_value_0) => {
            CtSemanticOriginalPatternKind::Integer(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalPatternKind::Bool(field_value_0) => CtSemanticOriginalPatternKind::Bool({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOriginalPatternKind::Constructor {
            definition: field_definition,
            variant: field_variant,
            fields: field_fields,
        } => CtSemanticOriginalPatternKind::Constructor {
            definition: match field_definition {
                Some(item) => Some(reify_semantic_definition_id(item, reifier, depth + 1)?),
                None => None,
            },
            variant: reifier.text(field_variant, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<CtSemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(reify_semantic_original_pattern(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalPatternKind::RecordConstructor {
            definition: field_definition,
            fields: field_fields,
        } => CtSemanticOriginalPatternKind::RecordConstructor {
            definition: match field_definition {
                Some(item) => Some(reify_semantic_definition_id(item, reifier, depth + 1)?),
                None => None,
            },
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalPattern)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_pattern(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        SemanticOriginalPatternKind::Float(field_value_0) => {
            CtSemanticOriginalPatternKind::Float(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalPatternKind::String(field_value_0) => {
            CtSemanticOriginalPatternKind::String(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalPatternKind::Character(field_value_0) => {
            CtSemanticOriginalPatternKind::Character(reifier.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn reify_semantic_original_match_arm<R: CtDynamicReifier>(
    value: &SemanticOriginalMatchArm,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalMatchArm, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalMatchArm {
        pattern: reify_semantic_original_pattern(&value.pattern, reifier, depth + 1)?,
        body: reify_semantic_original_block(&value.body, reifier, depth + 1)?,
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_block<R: CtDynamicReifier>(
    value: &SemanticOriginalBlock,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalBlock, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalBlock {
        statements: {
            reifier.rows(
                (value.statements).len(),
                std::mem::size_of::<CtSemanticOriginalStatement>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.statements).len());
            for item in &value.statements {
                output.push(reify_semantic_original_statement(item, reifier, depth + 1)?);
            }
            output
        },
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_statement<R: CtDynamicReifier>(
    value: &SemanticOriginalStatement,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalStatement, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalStatement {
        id: {
            reifier.node(depth + 1)?;
            value.id
        },
        kind: reify_semantic_original_statement_kind(&value.kind, reifier, depth + 1)?,
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_statement_kind<R: CtDynamicReifier>(
    value: &SemanticOriginalStatementKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalStatementKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalStatementKind::Binding {
            name: field_name,
            mutable: field_mutable,
            ty: field_ty,
            initializer: field_initializer,
        } => CtSemanticOriginalStatementKind::Binding {
            name: reifier.text(field_name, depth + 1)?,
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            ty: match field_ty {
                Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
                None => None,
            },
            initializer: match field_initializer {
                Some(item) => Some(reify_semantic_original_expr(item, reifier, depth + 1)?),
                None => None,
            },
        },
        SemanticOriginalStatementKind::TupleBinding {
            pattern: field_pattern,
            mutable: field_mutable,
            ty: field_ty,
            initializer: field_initializer,
        } => CtSemanticOriginalStatementKind::TupleBinding {
            pattern: reify_semantic_original_pattern(field_pattern, reifier, depth + 1)?,
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            ty: match field_ty {
                Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
                None => None,
            },
            initializer: reify_semantic_original_expr(field_initializer, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::Assign {
            target: field_target,
            value: field_value,
        } => CtSemanticOriginalStatementKind::Assign {
            target: reify_semantic_original_expr(field_target, reifier, depth + 1)?,
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::Expression(field_value_0) => {
            CtSemanticOriginalStatementKind::Expression(reify_semantic_original_expr(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::Return(field_value_0) => {
            CtSemanticOriginalStatementKind::Return(match field_value_0 {
                Some(item) => Some(reify_semantic_original_expr(item, reifier, depth + 1)?),
                None => None,
            })
        }
        SemanticOriginalStatementKind::If {
            condition: field_condition,
            then_body: field_then_body,
            else_body: field_else_body,
        } => CtSemanticOriginalStatementKind::If {
            condition: reify_semantic_original_expr(field_condition, reifier, depth + 1)?,
            then_body: reify_semantic_original_block(field_then_body, reifier, depth + 1)?,
            else_body: match field_else_body {
                Some(item) => Some(reify_semantic_original_block(item, reifier, depth + 1)?),
                None => None,
            },
        },
        SemanticOriginalStatementKind::IfLet {
            pattern: field_pattern,
            value: field_value,
            then_body: field_then_body,
            else_body: field_else_body,
        } => CtSemanticOriginalStatementKind::IfLet {
            pattern: reify_semantic_original_pattern(field_pattern, reifier, depth + 1)?,
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
            then_body: reify_semantic_original_block(field_then_body, reifier, depth + 1)?,
            else_body: match field_else_body {
                Some(item) => Some(reify_semantic_original_block(item, reifier, depth + 1)?),
                None => None,
            },
        },
        SemanticOriginalStatementKind::While {
            condition: field_condition,
            body: field_body,
        } => CtSemanticOriginalStatementKind::While {
            condition: reify_semantic_original_expr(field_condition, reifier, depth + 1)?,
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::WhileLet {
            pattern: field_pattern,
            value: field_value,
            body: field_body,
        } => CtSemanticOriginalStatementKind::WhileLet {
            pattern: reify_semantic_original_pattern(field_pattern, reifier, depth + 1)?,
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::ForRange {
            name: field_name,
            start: field_start,
            end: field_end,
            body: field_body,
        } => CtSemanticOriginalStatementKind::ForRange {
            name: reifier.text(field_name, depth + 1)?,
            start: reify_semantic_original_expr(field_start, reifier, depth + 1)?,
            end: reify_semantic_original_expr(field_end, reifier, depth + 1)?,
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::Match {
            value: field_value,
            arms: field_arms,
        } => CtSemanticOriginalStatementKind::Match {
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
            arms: {
                reifier.rows(
                    (field_arms).len(),
                    std::mem::size_of::<CtSemanticOriginalMatchArm>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arms).len());
                for item in field_arms {
                    output.push(reify_semantic_original_match_arm(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalStatementKind::Block(field_value_0) => {
            CtSemanticOriginalStatementKind::Block(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::Defer(field_value_0) => {
            CtSemanticOriginalStatementKind::Defer(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::Unsafe(field_value_0) => {
            CtSemanticOriginalStatementKind::Unsafe(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::Break => CtSemanticOriginalStatementKind::Break,
        SemanticOriginalStatementKind::Continue => CtSemanticOriginalStatementKind::Continue,
        SemanticOriginalStatementKind::Unsupported(field_value_0) => {
            CtSemanticOriginalStatementKind::Unsupported(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalStatementKind::ForEach {
            pattern: field_pattern,
            iterator: field_iterator,
            body: field_body,
        } => CtSemanticOriginalStatementKind::ForEach {
            pattern: reify_semantic_original_pattern(field_pattern, reifier, depth + 1)?,
            iterator: reify_semantic_original_expr(field_iterator, reifier, depth + 1)?,
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::CompoundAssign {
            target: field_target,
            op: field_op,
            value: field_value,
        } => CtSemanticOriginalStatementKind::CompoundAssign {
            target: reify_semantic_original_expr(field_target, reifier, depth + 1)?,
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::Yield(field_value_0) => {
            CtSemanticOriginalStatementKind::Yield(reify_semantic_original_expr(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::Loop(field_value_0) => {
            CtSemanticOriginalStatementKind::Loop(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalStatementKind::With {
            name: field_name,
            value: field_value,
            body: field_body,
        } => CtSemanticOriginalStatementKind::With {
            name: reifier.text(field_name, depth + 1)?,
            value: reify_semantic_original_expr(field_value, reifier, depth + 1)?,
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalStatementKind::CompileTime(field_value_0) => {
            CtSemanticOriginalStatementKind::CompileTime({
                reifier.rows(
                    1,
                    std::mem::size_of::<CtSemanticOriginalStatement>(),
                    depth + 1,
                )?;
                Box::new(reify_semantic_original_statement(
                    (field_value_0).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            })
        }
    })
}
pub fn reify_semantic_original_expr<R: CtDynamicReifier>(
    value: &SemanticOriginalExpr,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalExpr, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalExpr {
        id: {
            reifier.node(depth + 1)?;
            value.id
        },
        kind: reify_semantic_original_expr_kind(&value.kind, reifier, depth + 1)?,
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_expr_kind<R: CtDynamicReifier>(
    value: &SemanticOriginalExprKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalExprKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalExprKind::MakeDynBorrow {
            interface: field_interface,
            vtable: field_vtable,
            source: field_source,
            mutable: field_mutable,
        } => CtSemanticOriginalExprKind::MakeDynBorrow {
            interface: reify_dyn_interface_id(field_interface, reifier, depth + 1)?,
            vtable: reify_dyn_vtable_id(field_vtable, reifier, depth + 1)?,
            source: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_source).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
        },
        SemanticOriginalExprKind::MakeDynOwned {
            interface: field_interface,
            vtable: field_vtable,
            source: field_source,
            route: field_route,
        } => CtSemanticOriginalExprKind::MakeDynOwned {
            interface: reify_dyn_interface_id(field_interface, reifier, depth + 1)?,
            vtable: reify_dyn_vtable_id(field_vtable, reifier, depth + 1)?,
            source: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_source).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            route: reify_dyn_owned_storage(field_route, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::CallDynSlot {
            callee: field_callee,
            interface: field_interface,
            slot: field_slot,
            arguments: field_arguments,
        } => CtSemanticOriginalExprKind::CallDynSlot {
            callee: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_callee).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            interface: reify_dyn_interface_id(field_interface, reifier, depth + 1)?,
            slot: {
                reifier.node(depth + 1)?;
                *(field_slot)
            },
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticOriginalExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_original_expr(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalExprKind::Literal(field_value_0) => {
            CtSemanticOriginalExprKind::Literal(reifier.constant(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Ascribed {
            ty: field_ty,
            value: field_value,
        } => CtSemanticOriginalExprKind::Ascribed {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::Integer(field_value_0) => {
            CtSemanticOriginalExprKind::Integer(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Float(field_value_0) => {
            CtSemanticOriginalExprKind::Float(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::NumericConvert {
            value: field_value,
            target: field_target,
        } => CtSemanticOriginalExprKind::NumericConvert {
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            target: reify_semantic_type_recipe(field_target, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::IntegerConvert {
            value: field_value,
            target: field_target,
        } => CtSemanticOriginalExprKind::IntegerConvert {
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            target: reify_semantic_type_recipe(field_target, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::Bool(field_value_0) => CtSemanticOriginalExprKind::Bool({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        SemanticOriginalExprKind::String(field_value_0) => {
            CtSemanticOriginalExprKind::String(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Unit => CtSemanticOriginalExprKind::Unit,
        SemanticOriginalExprKind::Name(field_value_0) => {
            CtSemanticOriginalExprKind::Name(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Unary {
            op: field_op,
            operand: field_operand,
        } => CtSemanticOriginalExprKind::Unary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            operand: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_operand).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::Binary {
            op: field_op,
            left: field_left,
            right: field_right,
        } => CtSemanticOriginalExprKind::Binary {
            op: {
                reifier.node(depth + 1)?;
                *(field_op)
            },
            left: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_left).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            right: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_right).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::TypeApply {
            callee: field_callee,
            type_arguments: field_type_arguments,
        } => CtSemanticOriginalExprKind::TypeApply {
            callee: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_callee).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            type_arguments: {
                reifier.rows(
                    (field_type_arguments).len(),
                    std::mem::size_of::<CtSemanticTypeRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_type_arguments).len());
                for item in field_type_arguments {
                    output.push(reify_semantic_type_recipe(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalExprKind::Call {
            callee: field_callee,
            arguments: field_arguments,
        } => CtSemanticOriginalExprKind::Call {
            callee: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_callee).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticOriginalExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_original_expr(item, reifier, depth + 1)?);
                }
                output
            },
        },
        SemanticOriginalExprKind::Tuple(field_value_0) => CtSemanticOriginalExprKind::Tuple({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtSemanticOriginalExpr>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_semantic_original_expr(item, reifier, depth + 1)?);
            }
            output
        }),
        SemanticOriginalExprKind::Array(field_value_0) => CtSemanticOriginalExprKind::Array({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtSemanticOriginalExpr>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_semantic_original_expr(item, reifier, depth + 1)?);
            }
            output
        }),
        SemanticOriginalExprKind::Index {
            object: field_object,
            index: field_index,
        } => CtSemanticOriginalExprKind::Index {
            object: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_object).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            index: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_index).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::Slice {
            object: field_object,
            start: field_start,
            end: field_end,
        } => CtSemanticOriginalExprKind::Slice {
            object: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_object).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            start: match field_start {
                Some(item) => Some({
                    reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                    Box::new(reify_semantic_original_expr(
                        (item).as_ref(),
                        reifier,
                        depth + 1,
                    )?)
                }),
                None => None,
            },
            end: match field_end {
                Some(item) => Some({
                    reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                    Box::new(reify_semantic_original_expr(
                        (item).as_ref(),
                        reifier,
                        depth + 1,
                    )?)
                }),
                None => None,
            },
        },
        SemanticOriginalExprKind::Member {
            object: field_object,
            member: field_member,
            owner: field_owner,
        } => CtSemanticOriginalExprKind::Member {
            object: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_object).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            member: reifier.text(field_member, depth + 1)?,
            owner: match field_owner {
                Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
                None => None,
            },
        },
        SemanticOriginalExprKind::ReceiverMember(field_value_0) => {
            CtSemanticOriginalExprKind::ReceiverMember(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Record {
            ty: field_ty,
            fields: field_fields,
        } => CtSemanticOriginalExprKind::Record {
            ty: reify_semantic_type_recipe(field_ty, reifier, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_expr(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        SemanticOriginalExprKind::RecordUpdate {
            ty: field_ty,
            base: field_base,
            fields: field_fields,
        } => CtSemanticOriginalExprKind::RecordUpdate {
            ty: match field_ty {
                Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
                None => None,
            },
            base: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_base).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_expr(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        SemanticOriginalExprKind::Sequence {
            bindings: field_bindings,
            value: field_value,
        } => CtSemanticOriginalExprKind::Sequence {
            bindings: {
                reifier.rows(
                    (field_bindings).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_bindings).len());
                for item in field_bindings {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_expr(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
            value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::CompileTime(field_value_0) => {
            CtSemanticOriginalExprKind::CompileTime({
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_value_0).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            })
        }
        SemanticOriginalExprKind::Try(field_value_0) => CtSemanticOriginalExprKind::Try({
            reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
            Box::new(reify_semantic_original_expr(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticOriginalExprKind::Closure {
            move_capture: field_move_capture,
            parameters: field_parameters,
            return_type: field_return_type,
            body: field_body,
        } => CtSemanticOriginalExprKind::Closure {
            move_capture: {
                reifier.node(depth + 1)?;
                *(field_move_capture)
            },
            parameters: {
                reifier.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<CtSemanticOriginalClosureParameter>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(reify_semantic_original_closure_parameter(
                        item,
                        reifier,
                        depth + 1,
                    )?);
                }
                output
            },
            return_type: match field_return_type {
                Some(item) => Some(reify_semantic_type_recipe(item, reifier, depth + 1)?),
                None => None,
            },
            body: reify_semantic_original_block(field_body, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::If {
            condition: field_condition,
            then_value: field_then_value,
            else_value: field_else_value,
        } => CtSemanticOriginalExprKind::If {
            condition: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_condition).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            then_value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_then_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            else_value: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_else_value).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::Unsupported(field_value_0) => {
            CtSemanticOriginalExprKind::Unsupported(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Spread(field_value_0) => CtSemanticOriginalExprKind::Spread({
            reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
            Box::new(reify_semantic_original_expr(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticOriginalExprKind::Group(field_value_0) => CtSemanticOriginalExprKind::Group({
            reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
            Box::new(reify_semantic_original_expr(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticOriginalExprKind::Character(field_value_0) => {
            CtSemanticOriginalExprKind::Character(reifier.text(field_value_0, depth + 1)?)
        }
        SemanticOriginalExprKind::Range {
            start: field_start,
            end: field_end,
        } => CtSemanticOriginalExprKind::Range {
            start: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_start).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            end: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_end).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::Pipeline {
            input: field_input,
            target: field_target,
        } => CtSemanticOriginalExprKind::Pipeline {
            input: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_input).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            target: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_target).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        SemanticOriginalExprKind::RecordConstructor {
            definition: field_definition,
            fields: field_fields,
        } => CtSemanticOriginalExprKind::RecordConstructor {
            definition: match field_definition {
                Some(item) => Some(reify_semantic_definition_id(item, reifier, depth + 1)?),
                None => None,
            },
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtSemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_semantic_original_expr(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        SemanticOriginalExprKind::Splice(field_value_0) => CtSemanticOriginalExprKind::Splice({
            reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
            Box::new(reify_semantic_original_expr(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        SemanticOriginalExprKind::CompileTimeBlock(field_value_0) => {
            CtSemanticOriginalExprKind::CompileTimeBlock(reify_semantic_original_block(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalExprKind::SyntaxQuote {
            category: field_category,
            template: field_template,
        } => CtSemanticOriginalExprKind::SyntaxQuote {
            category: reify_semantic_original_syntax_category(field_category, reifier, depth + 1)?,
            template: reify_semantic_original_quote_template(field_template, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::SyntaxIsland {
            path: field_path,
            tree: field_tree,
        } => CtSemanticOriginalExprKind::SyntaxIsland {
            path: {
                reifier.rows((field_path).len(), std::mem::size_of::<String>(), depth + 1)?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push(reifier.text(item, depth + 1)?);
                }
                output
            },
            tree: reify_semantic_original_syntax_tree(field_tree, reifier, depth + 1)?,
        },
        SemanticOriginalExprKind::GenericApply {
            callee: field_callee,
            arguments: field_arguments,
        } => CtSemanticOriginalExprKind::GenericApply {
            callee: {
                reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
                Box::new(reify_semantic_original_expr(
                    (field_callee).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            arguments: {
                reifier.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<CtSemanticOriginalGenericArgument>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(reify_semantic_original_generic_argument(
                        item,
                        reifier,
                        depth + 1,
                    )?);
                }
                output
            },
        },
    })
}
pub fn reify_semantic_original_quote_hole<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteHole,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteHole, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalQuoteHole {
        kind: reify_semantic_original_quote_hole_kind(&value.kind, reifier, depth + 1)?,
        value: {
            reifier.rows(1, std::mem::size_of::<CtSemanticOriginalExpr>(), depth + 1)?;
            Box::new(reify_semantic_original_expr(
                (value.value).as_ref(),
                reifier,
                depth + 1,
            )?)
        },
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_quote_template<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteTemplate,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteTemplate, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalQuoteTemplate {
        tree: reify_semantic_original_syntax_tree(&value.tree, reifier, depth + 1)?,
        holes: {
            reifier.rows(
                (value.holes).len(),
                std::mem::size_of::<CtSemanticOriginalQuoteHole>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.holes).len());
            for item in &value.holes {
                output.push(reify_semantic_original_quote_hole(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
        name_sites: {
            reifier.rows(
                (value.name_sites).len(),
                std::mem::size_of::<CtSemanticOriginalQuoteNameSite>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.name_sites).len());
            for item in &value.name_sites {
                output.push(reify_semantic_original_quote_name_site(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn reify_semantic_original_generic_argument<R: CtDynamicReifier>(
    value: &SemanticOriginalGenericArgument,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalGenericArgument, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalGenericArgument::Type(field_value_0) => {
            CtSemanticOriginalGenericArgument::Type(reify_semantic_type_recipe(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalGenericArgument::Const(field_value_0) => {
            CtSemanticOriginalGenericArgument::Const(reify_semantic_original_expr(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalGenericArgument::Lifetime(field_value_0) => {
            CtSemanticOriginalGenericArgument::Lifetime(reifier.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn reify_semantic_original_syntax_category<R: CtDynamicReifier>(
    value: &SemanticOriginalSyntaxCategory,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalSyntaxCategory, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalSyntaxCategory::Expr => CtSemanticOriginalSyntaxCategory::Expr,
        SemanticOriginalSyntaxCategory::Type => CtSemanticOriginalSyntaxCategory::Type,
        SemanticOriginalSyntaxCategory::StmtList => CtSemanticOriginalSyntaxCategory::StmtList,
        SemanticOriginalSyntaxCategory::ItemList => CtSemanticOriginalSyntaxCategory::ItemList,
    })
}
pub fn reify_semantic_original_syntax_delimiter<R: CtDynamicReifier>(
    value: &SemanticOriginalSyntaxDelimiter,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalSyntaxDelimiter, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalSyntaxDelimiter::Parenthesis => {
            CtSemanticOriginalSyntaxDelimiter::Parenthesis
        }
        SemanticOriginalSyntaxDelimiter::Bracket => CtSemanticOriginalSyntaxDelimiter::Bracket,
        SemanticOriginalSyntaxDelimiter::Brace => CtSemanticOriginalSyntaxDelimiter::Brace,
    })
}
pub fn reify_semantic_original_quote_name_namespace<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteNameNamespace,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteNameNamespace, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalQuoteNameNamespace::Value => CtSemanticOriginalQuoteNameNamespace::Value,
        SemanticOriginalQuoteNameNamespace::Type => CtSemanticOriginalQuoteNameNamespace::Type,
        SemanticOriginalQuoteNameNamespace::Member => CtSemanticOriginalQuoteNameNamespace::Member,
        SemanticOriginalQuoteNameNamespace::Module => CtSemanticOriginalQuoteNameNamespace::Module,
        SemanticOriginalQuoteNameNamespace::Label => CtSemanticOriginalQuoteNameNamespace::Label,
    })
}
pub fn reify_semantic_original_quote_name_role<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteNameRole,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteNameRole, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalQuoteNameRole::Binding => CtSemanticOriginalQuoteNameRole::Binding,
        SemanticOriginalQuoteNameRole::Reference => CtSemanticOriginalQuoteNameRole::Reference,
    })
}
pub fn reify_semantic_original_quote_hole_kind<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteHoleKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteHoleKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalQuoteHoleKind::Expr => CtSemanticOriginalQuoteHoleKind::Expr,
        SemanticOriginalQuoteHoleKind::Type => CtSemanticOriginalQuoteHoleKind::Type,
        SemanticOriginalQuoteHoleKind::StmtList => CtSemanticOriginalQuoteHoleKind::StmtList,
        SemanticOriginalQuoteHoleKind::ItemList => CtSemanticOriginalQuoteHoleKind::ItemList,
        SemanticOriginalQuoteHoleKind::Ident {
            namespace: field_namespace,
            role: field_role,
        } => CtSemanticOriginalQuoteHoleKind::Ident {
            namespace: reify_semantic_original_quote_name_namespace(
                field_namespace,
                reifier,
                depth + 1,
            )?,
            role: reify_semantic_original_quote_name_role(field_role, reifier, depth + 1)?,
        },
    })
}
pub fn reify_semantic_original_quote_name_site<R: CtDynamicReifier>(
    value: &SemanticOriginalQuoteNameSite,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalQuoteNameSite, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalQuoteNameSite {
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
        token_index: {
            reifier.node(depth + 1)?;
            value.token_index
        },
    })
}
pub fn reify_semantic_original_syntax_tree<R: CtDynamicReifier>(
    value: &SemanticOriginalSyntaxTree,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalSyntaxTree, R::Error> {
    reifier.node(depth)?;
    Ok(CtSemanticOriginalSyntaxTree {
        delimiter: reify_semantic_original_syntax_delimiter(&value.delimiter, reifier, depth + 1)?,
        children: {
            reifier.rows(
                (value.children).len(),
                std::mem::size_of::<CtSemanticOriginalSyntaxTreeEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.children).len());
            for item in &value.children {
                output.push(reify_semantic_original_syntax_tree_entry(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
        span: {
            reifier.node(depth + 1)?;
            value.span
        },
    })
}
pub fn reify_semantic_original_syntax_tree_entry<R: CtDynamicReifier>(
    value: &SemanticOriginalSyntaxTreeEntry,
    reifier: &mut R,
    depth: usize,
) -> Result<CtSemanticOriginalSyntaxTreeEntry, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        SemanticOriginalSyntaxTreeEntry::Token {
            text: field_text,
            span: field_span,
        } => CtSemanticOriginalSyntaxTreeEntry::Token {
            text: reifier.text(field_text, depth + 1)?,
            span: {
                reifier.node(depth + 1)?;
                *(field_span)
            },
        },
        SemanticOriginalSyntaxTreeEntry::Tree(field_value_0) => {
            CtSemanticOriginalSyntaxTreeEntry::Tree(reify_semantic_original_syntax_tree(
                field_value_0,
                reifier,
                depth + 1,
            )?)
        }
        SemanticOriginalSyntaxTreeEntry::Hole {
            index: field_index,
            span: field_span,
        } => CtSemanticOriginalSyntaxTreeEntry::Hole {
            index: {
                reifier.node(depth + 1)?;
                *(field_index)
            },
            span: {
                reifier.node(depth + 1)?;
                *(field_span)
            },
        },
    })
}
pub fn reify_task_resume_bound<R: CtDynamicReifier>(
    value: &TaskResumeBound,
    reifier: &mut R,
    depth: usize,
) -> Result<CtTaskResumeBound, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        TaskResumeBound::Local => CtTaskResumeBound::Local,
        TaskResumeBound::Transfer => CtTaskResumeBound::Transfer,
    })
}
pub fn reify_protocol_state<R: CtDynamicReifier>(
    value: &ProtocolState,
    reifier: &mut R,
    depth: usize,
) -> Result<CtProtocolState, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        ProtocolState::Open => CtProtocolState::Open,
        ProtocolState::Closing => CtProtocolState::Closing,
        ProtocolState::Closed => CtProtocolState::Closed,
    })
}
pub fn reify_protocol_kind<R: CtDynamicReifier>(
    value: &ProtocolKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtProtocolKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        ProtocolKind::AsyncRegistrationV1 => CtProtocolKind::AsyncRegistrationV1,
    })
}
pub fn reify_protocol_exit<R: CtDynamicReifier>(
    value: &ProtocolExit,
    reifier: &mut R,
    depth: usize,
) -> Result<CtProtocolExit, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        ProtocolExit::Return => CtProtocolExit::Return,
        ProtocolExit::ErrorOk => CtProtocolExit::ErrorOk,
        ProtocolExit::ErrorErr => CtProtocolExit::ErrorErr,
        ProtocolExit::PollPending => CtProtocolExit::PollPending,
        ProtocolExit::PollReady => CtProtocolExit::PollReady,
    })
}
pub fn reify_protocol_transition<R: CtDynamicReifier>(
    value: &ProtocolTransition,
    reifier: &mut R,
    depth: usize,
) -> Result<CtProtocolTransition, R::Error> {
    reifier.node(depth)?;
    Ok(CtProtocolTransition {
        input: reify_protocol_state(&value.input, reifier, depth + 1)?,
        exit: reify_protocol_exit(&value.exit, reifier, depth + 1)?,
        output: {
            reifier.rows(
                (value.output).len(),
                std::mem::size_of::<CtProtocolState>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.output).len());
            for item in &value.output {
                output.push(reify_protocol_state(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_function_protocol_contract<R: CtDynamicReifier>(
    value: &FunctionProtocolContract,
    reifier: &mut R,
    depth: usize,
) -> Result<CtFunctionProtocolContract, R::Error> {
    reifier.node(depth)?;
    Ok(CtFunctionProtocolContract {
        protocol: reify_protocol_kind(&value.protocol, reifier, depth + 1)?,
        parameter: {
            reifier.node(depth + 1)?;
            value.parameter
        },
        entry: {
            reifier.rows(
                (value.entry).len(),
                std::mem::size_of::<CtProtocolState>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entry).len());
            for item in &value.entry {
                output.push(reify_protocol_state(item, reifier, depth + 1)?);
            }
            output
        },
        exits: {
            reifier.rows(
                (value.exits).len(),
                std::mem::size_of::<CtProtocolTransition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.exits).len());
            for item in &value.exits {
                output.push(reify_protocol_transition(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_scoped_callback_role<R: CtDynamicReifier>(
    value: &ScopedCallbackRole,
    reifier: &mut R,
    depth: usize,
) -> Result<CtScopedCallbackRole, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        ScopedCallbackRole::Register => CtScopedCallbackRole::Register,
        ScopedCallbackRole::Close => CtScopedCallbackRole::Close,
        ScopedCallbackRole::AsyncRegister => CtScopedCallbackRole::AsyncRegister,
        ScopedCallbackRole::AsyncRequestClose => CtScopedCallbackRole::AsyncRequestClose,
        ScopedCallbackRole::AsyncPollDrain => CtScopedCallbackRole::AsyncPollDrain,
        ScopedCallbackRole::ThreadAsyncRegister => CtScopedCallbackRole::ThreadAsyncRegister,
        ScopedCallbackRole::ThreadAsyncRequestClose => {
            CtScopedCallbackRole::ThreadAsyncRequestClose
        }
        ScopedCallbackRole::ThreadAsyncPollDrain => CtScopedCallbackRole::ThreadAsyncPollDrain,
    })
}
pub fn reify_foreign_declaration<R: CtDynamicReifier>(
    value: &ForeignDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtForeignDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtForeignDeclaration {
        abi: reifier.text(&value.abi, depth + 1)?,
        symbol: reifier.text(&value.symbol, depth + 1)?,
        noescape_parameters: {
            reifier.rows(
                (value.noescape_parameters).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.noescape_parameters).len());
            for item in &value.noescape_parameters {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        static_callbacks: {
            reifier.node(depth + 1)?;
            value.static_callbacks
        },
        thread_driver_transfer: {
            reifier.node(depth + 1)?;
            value.thread_driver_transfer
        },
        scoped_callback: match &value.scoped_callback {
            Some(item) => Some(reify_scoped_callback_role(item, reifier, depth + 1)?),
            None => None,
        },
    })
}
pub fn reify_storage_source<R: CtDynamicReifier>(
    value: &StorageSource,
    reifier: &mut R,
    depth: usize,
) -> Result<CtStorageSource, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        StorageSource::Exact(field_value_0) => {
            CtStorageSource::Exact(reify_storage_position(field_value_0, reifier, depth + 1)?)
        }
        StorageSource::PlaceAddress {
            place: field_place,
            mode: field_mode,
        } => CtStorageSource::PlaceAddress {
            place: reify_storage_position(field_place, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        StorageSource::OwnedView {
            head: field_head,
            leaf: field_leaf,
            mode: field_mode,
        } => CtStorageSource::OwnedView {
            head: reify_storage_position(field_head, reifier, depth + 1)?,
            leaf: reify_type_value(field_leaf, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
        StorageSource::ReborrowedLeaf {
            source: field_source,
            head: field_head,
        } => CtStorageSource::ReborrowedLeaf {
            source: reify_storage_position(field_source, reifier, depth + 1)?,
            head: reify_storage_position(field_head, reifier, depth + 1)?,
        },
        StorageSource::Static {
            leaf: field_leaf,
            mode: field_mode,
        } => CtStorageSource::Static {
            leaf: reify_type_value(field_leaf, reifier, depth + 1)?,
            mode: {
                reifier.node(depth + 1)?;
                *(field_mode)
            },
        },
    })
}
pub fn reify_retained_borrow_port<R: CtDynamicReifier>(
    value: &RetainedBorrowPort,
    reifier: &mut R,
    depth: usize,
) -> Result<CtRetainedBorrowPort, R::Error> {
    reifier.node(depth)?;
    Ok(CtRetainedBorrowPort {
        leaf: reify_type_value(&value.leaf, reifier, depth + 1)?,
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn reify_retained_life_shape<R: CtDynamicReifier>(
    value: &RetainedLifeShape,
    reifier: &mut R,
    depth: usize,
) -> Result<CtRetainedLifeShape, R::Error> {
    reifier.node(depth)?;
    Ok(CtRetainedLifeShape {
        binder_count: {
            reifier.node(depth + 1)?;
            value.binder_count
        },
        ports: {
            reifier.rows(
                (value.ports).len(),
                std::mem::size_of::<CtRetainedBorrowPort>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(reify_retained_borrow_port(item, reifier, depth + 1)?);
            }
            output
        },
        retained: {
            reifier.rows(
                (value.retained).len(),
                std::mem::size_of::<StorageRetainedBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(reify_storage_retained_binding(item, reifier, depth + 1)?);
            }
            output
        },
        nominal_arguments: {
            reifier.rows(
                (value.nominal_arguments).len(),
                std::mem::size_of::<NominalLifeArgument>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal_arguments).len());
            for item in &value.nominal_arguments {
                output.push(reify_nominal_life_argument(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_storage_source_binding<R: CtDynamicReifier>(
    value: &StorageSourceBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<CtStorageSourceBinding, R::Error> {
    reifier.node(depth)?;
    Ok(CtStorageSourceBinding {
        position: reify_storage_position(&value.position, reifier, depth + 1)?,
        sources: {
            reifier.rows(
                (value.sources).len(),
                std::mem::size_of::<CtStorageSource>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(reify_storage_source(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_life_nominal_kind<R: CtDynamicReifier>(
    value: &LifeNominalKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtLifeNominalKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        LifeNominalKind::Record => CtLifeNominalKind::Record,
        LifeNominalKind::Package => CtLifeNominalKind::Package,
        LifeNominalKind::Resource => CtLifeNominalKind::Resource,
        LifeNominalKind::Enum => CtLifeNominalKind::Enum,
    })
}
pub fn reify_life_parameter_kind<R: CtDynamicReifier>(
    value: &LifeParameterKind,
    reifier: &mut R,
    depth: usize,
) -> Result<CtLifeParameterKind, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        LifeParameterKind::Type => CtLifeParameterKind::Type,
        LifeParameterKind::Const(field_value_0) => {
            CtLifeParameterKind::Const(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
        LifeParameterKind::Lifetime => CtLifeParameterKind::Lifetime,
    })
}
pub fn reify_life_nominal_declaration<R: CtDynamicReifier>(
    value: &LifeNominalDeclaration,
    reifier: &mut R,
    depth: usize,
) -> Result<CtLifeNominalDeclaration, R::Error> {
    reifier.node(depth)?;
    Ok(CtLifeNominalDeclaration {
        id: reify_life_declaration_id(&value.id, reifier, depth + 1)?,
        identity: reify_life_declaration_identity(&value.identity, reifier, depth + 1)?,
        kind: reify_life_nominal_kind(&value.kind, reifier, depth + 1)?,
        parameters: {
            reifier.rows(
                (value.parameters).len(),
                std::mem::size_of::<CtLifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(reify_life_parameter_kind(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_life_nominal_actual<R: CtDynamicReifier>(
    value: &LifeNominalActual,
    reifier: &mut R,
    depth: usize,
) -> Result<CtLifeNominalActual, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        LifeNominalActual::Type(field_value_0) => {
            CtLifeNominalActual::Type(reify_type_value(field_value_0, reifier, depth + 1)?)
        }
        LifeNominalActual::QualifiedType {
            ty: field_ty,
            life: field_life,
        } => CtLifeNominalActual::QualifiedType {
            ty: reify_type_value(field_ty, reifier, depth + 1)?,
            life: reify_storage_life_shape(field_life, reifier, depth + 1)?,
        },
        LifeNominalActual::Const(field_value_0) => {
            CtLifeNominalActual::Const(reifier.constant(field_value_0, depth + 1)?)
        }
        LifeNominalActual::Lifetime(field_value_0) => CtLifeNominalActual::Lifetime({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn reify_life_nominal_instance<R: CtDynamicReifier>(
    value: &LifeNominalInstance,
    reifier: &mut R,
    depth: usize,
) -> Result<CtLifeNominalInstance, R::Error> {
    reifier.node(depth)?;
    Ok(CtLifeNominalInstance {
        declaration: reify_life_declaration_id(&value.declaration, reifier, depth + 1)?,
        ty: reify_type_value(&value.ty, reifier, depth + 1)?,
        arguments: {
            reifier.rows(
                (value.arguments).len(),
                std::mem::size_of::<CtLifeNominalActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(reify_life_nominal_actual(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_type<R: CtDynamicReifier>(
    value: &Type,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynamicTypeNode, R::Error> {
    reifier.node(depth)?;
    Ok(match value {
        Type::Dyn {
            interface: field_interface,
        } => CtDynamicTypeNode::Dyn {
            interface: reify_dyn_interface_id(field_interface, reifier, depth + 1)?,
        },
        Type::OwnedDyn {
            interface: field_interface,
        } => CtDynamicTypeNode::OwnedDyn {
            interface: reify_dyn_interface_id(field_interface, reifier, depth + 1)?,
        },
        Type::BorrowedStorage {
            storage: field_storage,
            life: field_life,
        } => CtDynamicTypeNode::BorrowedStorage {
            storage: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_storage).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            life: reify_storage_life_shape(field_life, reifier, depth + 1)?,
        },
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
        Type::OwnedStore {
            element: field_element,
        } => CtDynamicTypeNode::OwnedStore {
            element: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_element).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::ForeignHandle => CtDynamicTypeNode::ForeignHandle,
        Type::SyncCell(field_value_0) => CtDynamicTypeNode::SyncCell({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        Type::Mutex {
            payload: field_payload,
        } => CtDynamicTypeNode::Mutex {
            payload: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_payload).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::MutexGuard {
            payload: field_payload,
            retained_life: field_retained_life,
        } => CtDynamicTypeNode::MutexGuard {
            payload: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_payload).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            retained_life: reify_retained_life_shape(field_retained_life, reifier, depth + 1)?,
        },
        Type::Registration {
            environment: field_environment,
        } => CtDynamicTypeNode::Registration {
            environment: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_environment).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::AsyncRegistration {
            environment: field_environment,
        } => CtDynamicTypeNode::AsyncRegistration {
            environment: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_environment).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::TaskI32 => CtDynamicTypeNode::TaskI32,
        Type::Task {
            result: field_result,
            retained_life: field_retained_life,
        } => CtDynamicTypeNode::Task {
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            retained_life: reify_retained_life_shape(field_retained_life, reifier, depth + 1)?,
        },
        Type::CWideString => CtDynamicTypeNode::CWideString,
        Type::ForeignCallback {
            parameters: field_parameters,
            result: field_result,
        } => CtDynamicTypeNode::ForeignCallback {
            parameters: {
                reifier.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(reify_type_value(item, reifier, depth + 1)?);
                }
                output
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::Tuple(field_value_0) => CtDynamicTypeNode::Tuple({
            reifier.rows(
                (field_value_0).len(),
                std::mem::size_of::<CtDynamicTypeId>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(reify_type_value(item, reifier, depth + 1)?);
            }
            output
        }),
        Type::Array {
            element: field_element,
            length: field_length,
        } => CtDynamicTypeNode::Array {
            element: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_element).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            length: {
                reifier.node(depth + 1)?;
                *(field_length)
            },
        },
        Type::Slice {
            element: field_element,
        } => CtDynamicTypeNode::Slice {
            element: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_element).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::Named(field_value_0) => {
            CtDynamicTypeNode::Named(reifier.text(field_value_0, depth + 1)?)
        }
        Type::Record {
            name: field_name,
            fields: field_fields,
        } => CtDynamicTypeNode::Record {
            name: reifier.text(field_name, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_type_value(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        Type::Package {
            name: field_name,
            fields: field_fields,
        } => CtDynamicTypeNode::Package {
            name: reifier.text(field_name, depth + 1)?,
            fields: {
                reifier.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        reifier.text(&(item).0, depth + 1)?,
                        reify_type_value(&(item).1, reifier, depth + 1)?,
                    ));
                }
                output
            },
        },
        Type::Enum {
            name: field_name,
            variants: field_variants,
        } => CtDynamicTypeNode::Enum {
            name: reifier.text(field_name, depth + 1)?,
            variants: {
                reifier.rows(
                    (field_variants).len(),
                    std::mem::size_of::<(String, Vec<CtDynamicTypeId>)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_variants).len());
                for item in field_variants {
                    output.push((reifier.text(&(item).0, depth + 1)?, {
                        reifier.rows(
                            ((item).1).len(),
                            std::mem::size_of::<CtDynamicTypeId>(),
                            depth + 1,
                        )?;
                        let mut output = Vec::with_capacity(((item).1).len());
                        for item in &(item).1 {
                            output.push(reify_type_value(item, reifier, depth + 1)?);
                        }
                        output
                    }));
                }
                output
            },
        },
        Type::ErrorUnion {
            error: field_error,
            result: field_result,
        } => CtDynamicTypeNode::ErrorUnion {
            error: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_error).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::Address {
            mutable: field_mutable,
            inner: field_inner,
        } => CtDynamicTypeNode::Address {
            mutable: {
                reifier.node(depth + 1)?;
                *(field_mutable)
            },
            inner: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_inner).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::Managed(field_value_0) => CtDynamicTypeNode::Managed({
            reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
            Box::new(reify_type_value(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        Type::Desc(field_value_0) => CtDynamicTypeNode::Desc({
            reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
            Box::new(reify_type_value(
                (field_value_0).as_ref(),
                reifier,
                depth + 1,
            )?)
        }),
        Type::FunctionPointer {
            parameters: field_parameters,
            result: field_result,
        } => CtDynamicTypeNode::FunctionPointer {
            parameters: {
                reifier.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(reify_type_value(item, reifier, depth + 1)?);
                }
                output
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::BorrowedFunctionPointer {
            parameters: field_parameters,
            result: field_result,
            life: field_life,
        } => CtDynamicTypeNode::BorrowedFunctionPointer {
            parameters: {
                reifier.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(reify_type_value(item, reifier, depth + 1)?);
                }
                output
            },
            result: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_result).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
            life: reify_storage_life_contract(field_life, reifier, depth + 1)?,
        },
        Type::Closure(field_value_0) => CtDynamicTypeNode::Closure({
            reifier.node(depth + 1)?;
            *(field_value_0)
        }),
        Type::Opaque {
            identity: field_identity,
            hidden: field_hidden,
        } => CtDynamicTypeNode::Opaque {
            identity: {
                reifier.node(depth + 1)?;
                *(field_identity)
            },
            hidden: {
                reifier.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                Box::new(reify_type_value(
                    (field_hidden).as_ref(),
                    reifier,
                    depth + 1,
                )?)
            },
        },
        Type::Unsupported(field_value_0) => {
            CtDynamicTypeNode::Unsupported(reifier.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn reify_storage_life_slot<R: CtDynamicReifier>(
    value: &StorageLifeSlot,
    reifier: &mut R,
    depth: usize,
) -> Result<StorageLifeSlot, R::Error> {
    reifier.node(depth)?;
    Ok(StorageLifeSlot {
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<PlaceProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
        mode: {
            reifier.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn reify_life_declaration_id<R: CtDynamicReifier>(
    value: &LifeDeclarationId,
    reifier: &mut R,
    depth: usize,
) -> Result<LifeDeclarationId, R::Error> {
    reifier.node(depth)?;
    Ok(LifeDeclarationId {
        module: {
            reifier.node(depth + 1)?;
            value.module
        },
        declaration: {
            reifier.node(depth + 1)?;
            value.declaration
        },
        compiler: {
            reifier.node(depth + 1)?;
            value.compiler
        },
    })
}
pub fn reify_storage_retained_binding<R: CtDynamicReifier>(
    value: &StorageRetainedBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<StorageRetainedBinding, R::Error> {
    reifier.node(depth)?;
    Ok(StorageRetainedBinding {
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        class: {
            reifier.node(depth + 1)?;
            value.class
        },
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn reify_nominal_life_argument<R: CtDynamicReifier>(
    value: &NominalLifeArgument,
    reifier: &mut R,
    depth: usize,
) -> Result<NominalLifeArgument, R::Error> {
    reifier.node(depth)?;
    Ok(NominalLifeArgument {
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        declaration: reify_life_declaration_id(&value.declaration, reifier, depth + 1)?,
        argument: {
            reifier.node(depth + 1)?;
            value.argument
        },
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn reify_storage_life_shape<R: CtDynamicReifier>(
    value: &StorageLifeShape,
    reifier: &mut R,
    depth: usize,
) -> Result<StorageLifeShape, R::Error> {
    reifier.node(depth)?;
    Ok(StorageLifeShape {
        binder_count: {
            reifier.node(depth + 1)?;
            value.binder_count
        },
        slots: {
            reifier.rows(
                (value.slots).len(),
                std::mem::size_of::<StorageLifeSlot>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(reify_storage_life_slot(item, reifier, depth + 1)?);
            }
            output
        },
        retained: {
            reifier.rows(
                (value.retained).len(),
                std::mem::size_of::<StorageRetainedBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(reify_storage_retained_binding(item, reifier, depth + 1)?);
            }
            output
        },
        nominal_arguments: {
            reifier.rows(
                (value.nominal_arguments).len(),
                std::mem::size_of::<NominalLifeArgument>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal_arguments).len());
            for item in &value.nominal_arguments {
                output.push(reify_nominal_life_argument(item, reifier, depth + 1)?);
            }
            output
        },
    })
}
pub fn reify_storage_position<R: CtDynamicReifier>(
    value: &StoragePosition,
    reifier: &mut R,
    depth: usize,
) -> Result<StoragePosition, R::Error> {
    reifier.node(depth)?;
    Ok(StoragePosition {
        base: {
            reifier.node(depth + 1)?;
            value.base
        },
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<ContractProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn reify_storage_life_binding<R: CtDynamicReifier>(
    value: &StorageLifeBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<StorageLifeBinding, R::Error> {
    reifier.node(depth)?;
    Ok(StorageLifeBinding {
        position: reify_storage_position(&value.position, reifier, depth + 1)?,
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn reify_signature_life_class<R: CtDynamicReifier>(
    value: &SignatureLifeClass,
    reifier: &mut R,
    depth: usize,
) -> Result<SignatureLifeClass, R::Error> {
    reifier.node(depth)?;
    Ok(SignatureLifeClass {
        base: {
            reifier.node(depth + 1)?;
            value.base
        },
        path: {
            reifier.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        class: {
            reifier.node(depth + 1)?;
            value.class
        },
    })
}
pub fn reify_signature_life_class_binding<R: CtDynamicReifier>(
    value: &SignatureLifeClassBinding,
    reifier: &mut R,
    depth: usize,
) -> Result<SignatureLifeClassBinding, R::Error> {
    reifier.node(depth)?;
    Ok(SignatureLifeClassBinding {
        position: reify_signature_life_class(&value.position, reifier, depth + 1)?,
        binder: {
            reifier.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn reify_storage_life_contract<R: CtDynamicReifier>(
    value: &StorageLifeContract,
    reifier: &mut R,
    depth: usize,
) -> Result<StorageLifeContract, R::Error> {
    reifier.node(depth)?;
    Ok(StorageLifeContract {
        fixed_binder_count: {
            reifier.node(depth + 1)?;
            value.fixed_binder_count
        },
        invocation_binder_count: {
            reifier.node(depth + 1)?;
            value.invocation_binder_count
        },
        bindings: {
            reifier.rows(
                (value.bindings).len(),
                std::mem::size_of::<StorageLifeBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.bindings).len());
            for item in &value.bindings {
                output.push(reify_storage_life_binding(item, reifier, depth + 1)?);
            }
            output
        },
        static_bindings: {
            reifier.rows(
                (value.static_bindings).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.static_bindings).len());
            for item in &value.static_bindings {
                output.push(reify_storage_position(item, reifier, depth + 1)?);
            }
            output
        },
        classes: {
            reifier.rows(
                (value.classes).len(),
                std::mem::size_of::<SignatureLifeClassBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push(reify_signature_life_class_binding(
                    item,
                    reifier,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn reify_life_declaration_identity<R: CtDynamicReifier>(
    value: &LifeDeclarationIdentity,
    reifier: &mut R,
    depth: usize,
) -> Result<LifeDeclarationIdentity, R::Error> {
    reifier.node(depth)?;
    Ok(LifeDeclarationIdentity {
        package: reifier.text(&value.package, depth + 1)?,
        module_path: {
            reifier.rows(
                (value.module_path).len(),
                std::mem::size_of::<String>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.module_path).len());
            for item in &value.module_path {
                output.push(reifier.text(item, depth + 1)?);
            }
            output
        },
        declaration_path: {
            reifier.rows(
                (value.declaration_path).len(),
                std::mem::size_of::<String>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_path).len());
            for item in &value.declaration_path {
                output.push(reifier.text(item, depth + 1)?);
            }
            output
        },
        generation_path: {
            reifier.rows(
                (value.generation_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.generation_path).len());
            for item in &value.generation_path {
                output.push({
                    reifier.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn reify_type_value<R: CtDynamicReifier>(
    value: &Type,
    reifier: &mut R,
    depth: usize,
) -> Result<CtDynamicTypeId, R::Error> {
    let node = reify_type(value, reifier, depth)?;
    reifier.store_type(node, depth)
}

/// Convert raw CT metadata to raw runtime data with caller-owned bounds and
/// Type-DAG resolution. No index, scope, visibility or permission is proved.
pub trait CtDynamicLifter {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize)
    -> Result<(), Self::Error>;
    fn text(&mut self, text: &str, depth: usize) -> Result<String, Self::Error>;
    fn constant(&mut self, value: &Constant, depth: usize) -> Result<Constant, Self::Error>;
    fn load_type(&mut self, id: CtDynamicTypeId, depth: usize) -> Result<Type, Self::Error>;
}
pub fn lift_dyn_interface_id<L: CtDynamicLifter>(
    value: &CtDynInterfaceId,
    lifter: &mut L,
    depth: usize,
) -> Result<DynInterfaceId, L::Error> {
    lifter.node(depth)?;
    Ok(DynInterfaceId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        index: {
            lifter.node(depth + 1)?;
            value.index
        },
    })
}
pub fn lift_dyn_vtable_id<L: CtDynamicLifter>(
    value: &CtDynVtableId,
    lifter: &mut L,
    depth: usize,
) -> Result<DynVtableId, L::Error> {
    lifter.node(depth)?;
    Ok(DynVtableId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        index: {
            lifter.node(depth + 1)?;
            value.index
        },
    })
}
pub fn lift_dyn_declaration<L: CtDynamicLifter>(
    value: &CtDynDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(DynDeclaration {
        implicit_lifetimes: match &value.implicit_lifetimes {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynImplicitLifeDeclaration>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_implicit_life_declaration(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        direct_supertraits: match &value.direct_supertraits {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynSelfPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_self_pattern(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        requirements: match &value.requirements {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynObligation>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_obligation(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        associated_definitions: match &value.associated_definitions {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynAssociatedDefinition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_associated_definition(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        method_signatures: match &value.method_signatures {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynMethodRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_method_recipe(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        method_definitions: match &value.method_definitions {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynDefinitionDeclaration>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_definition_declaration(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_pattern: match &value.self_pattern {
            Some(item) => Some(lift_dyn_type_pattern(item, lifter, depth + 1)?),
            None => None,
        },
        implemented_trait: match &value.implemented_trait {
            Some(item) => Some(lift_dyn_self_pattern(item, lifter, depth + 1)?),
            None => None,
        },
        id: lift_life_declaration_id(&value.id, lifter, depth + 1)?,
        identity: lift_life_declaration_identity(&value.identity, lifter, depth + 1)?,
        kind: lift_dyn_declaration_kind(&value.kind, lifter, depth + 1)?,
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<LifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(lift_life_parameter_kind(item, lifter, depth + 1)?);
            }
            output
        },
        members: {
            lifter.rows(
                (value.members).len(),
                std::mem::size_of::<DynMemberDeclaration>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.members).len());
            for item in &value.members {
                output.push(lift_dyn_member_declaration(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_self_pattern<L: CtDynamicLifter>(
    value: &CtDynSelfPattern,
    lifter: &mut L,
    depth: usize,
) -> Result<DynSelfPattern, L::Error> {
    lifter.node(depth)?;
    Ok(DynSelfPattern {
        declaration: lift_life_declaration_id(&value.declaration, lifter, depth + 1)?,
        arguments: {
            lifter.rows(
                (value.arguments).len(),
                std::mem::size_of::<DynPatternActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(lift_dyn_pattern_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_pattern_actual<L: CtDynamicLifter>(
    value: &CtDynPatternActual,
    lifter: &mut L,
    depth: usize,
) -> Result<DynPatternActual, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynPatternActual::Type(field_value_0) => {
            DynPatternActual::Type(lift_dyn_type_pattern(field_value_0, lifter, depth + 1)?)
        }
        CtDynPatternActual::Const(field_value_0) => {
            DynPatternActual::Const(lift_dyn_const_pattern(field_value_0, lifter, depth + 1)?)
        }
        CtDynPatternActual::Life(field_value_0) => {
            DynPatternActual::Life(lift_dyn_life_pattern(field_value_0, lifter, depth + 1)?)
        }
    })
}
pub fn lift_dyn_const_pattern<L: CtDynamicLifter>(
    value: &CtDynConstPattern,
    lifter: &mut L,
    depth: usize,
) -> Result<DynConstPattern, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynConstPattern::IntegerLiteral(field_value_0) => {
            DynConstPattern::IntegerLiteral(lifter.text(field_value_0, depth + 1)?)
        }
        CtDynConstPattern::FloatLiteral(field_value_0) => {
            DynConstPattern::FloatLiteral(lifter.text(field_value_0, depth + 1)?)
        }
        CtDynConstPattern::MethodParameter(field_value_0) => DynConstPattern::MethodParameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynConstPattern::Unary {
            op: field_op,
            value: field_value,
            ty: field_ty,
        } => DynConstPattern::Unary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            value: {
                lifter.rows(1, std::mem::size_of::<DynConstPattern>(), depth + 1)?;
                Box::new(lift_dyn_const_pattern(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            ty: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynConstPattern::Binary {
            op: field_op,
            left: field_left,
            right: field_right,
            ty: field_ty,
        } => DynConstPattern::Binary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            left: {
                lifter.rows(1, std::mem::size_of::<DynConstPattern>(), depth + 1)?;
                Box::new(lift_dyn_const_pattern(
                    (field_left).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            right: {
                lifter.rows(1, std::mem::size_of::<DynConstPattern>(), depth + 1)?;
                Box::new(lift_dyn_const_pattern(
                    (field_right).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            ty: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynConstPattern::Convert {
            value: field_value,
            ty: field_ty,
        } => DynConstPattern::Convert {
            value: {
                lifter.rows(1, std::mem::size_of::<DynConstPattern>(), depth + 1)?;
                Box::new(lift_dyn_const_pattern(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            ty: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynConstPattern::AssociatedProjection {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => DynConstPattern::AssociatedProjection {
            receiver: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_receiver).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            application: {
                lifter.rows(1, std::mem::size_of::<DynSelfPattern>(), depth + 1)?;
                Box::new(lift_dyn_self_pattern(
                    (field_application).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
        CtDynConstPattern::Value(field_value_0) => {
            DynConstPattern::Value(lifter.constant(field_value_0, depth + 1)?)
        }
        CtDynConstPattern::Parameter(field_value_0) => DynConstPattern::Parameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_dyn_type_pattern<L: CtDynamicLifter>(
    value: &CtDynTypePattern,
    lifter: &mut L,
    depth: usize,
) -> Result<DynTypePattern, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynTypePattern::SelfType => DynTypePattern::SelfType,
        CtDynTypePattern::MethodParameter(field_value_0) => DynTypePattern::MethodParameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynTypePattern::AssociatedProjection {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => DynTypePattern::AssociatedProjection {
            receiver: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_receiver).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            application: {
                lifter.rows(1, std::mem::size_of::<DynSelfPattern>(), depth + 1)?;
                Box::new(lift_dyn_self_pattern(
                    (field_application).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
        CtDynTypePattern::Concrete(field_value_0) => DynTypePattern::Concrete({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
        CtDynTypePattern::Parameter(field_value_0) => DynTypePattern::Parameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynTypePattern::Nominal(field_value_0) => DynTypePattern::Nominal({
            lifter.rows(1, std::mem::size_of::<DynSelfPattern>(), depth + 1)?;
            Box::new(lift_dyn_self_pattern(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtDynTypePattern::Address {
            mutable: field_mutable,
            lifetime: field_lifetime,
            inner: field_inner,
        } => DynTypePattern::Address {
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            lifetime: match field_lifetime {
                Some(item) => Some(lift_dyn_life_pattern(item, lifter, depth + 1)?),
                None => None,
            },
            inner: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_inner).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynTypePattern::Tuple(field_value_0) => DynTypePattern::Tuple({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<DynTypePattern>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_dyn_type_pattern(item, lifter, depth + 1)?);
            }
            output
        }),
        CtDynTypePattern::Array {
            element: field_element,
            length: field_length,
        } => DynTypePattern::Array {
            element: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_element).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            length: lift_dyn_const_pattern(field_length, lifter, depth + 1)?,
        },
        CtDynTypePattern::Slice(field_value_0) => DynTypePattern::Slice({
            lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
            Box::new(lift_dyn_type_pattern(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtDynTypePattern::Managed(field_value_0) => DynTypePattern::Managed({
            lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
            Box::new(lift_dyn_type_pattern(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtDynTypePattern::Desc {
            lifetime: field_lifetime,
            inner: field_inner,
        } => DynTypePattern::Desc {
            lifetime: match field_lifetime {
                Some(item) => Some(lift_dyn_life_pattern(item, lifter, depth + 1)?),
                None => None,
            },
            inner: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_inner).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynTypePattern::OwnedStore(field_value_0) => DynTypePattern::OwnedStore({
            lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
            Box::new(lift_dyn_type_pattern(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtDynTypePattern::ErrorUnion {
            error: field_error,
            result: field_result,
        } => DynTypePattern::ErrorUnion {
            error: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_error).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            result: {
                lifter.rows(1, std::mem::size_of::<DynTypePattern>(), depth + 1)?;
                Box::new(lift_dyn_type_pattern(
                    (field_result).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynTypePattern::Semantic {
            scope: field_scope,
            ty: field_ty,
        } => DynTypePattern::Semantic {
            scope: lift_semantic_definition_id(field_scope, lifter, depth + 1)?,
            ty: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
    })
}
pub fn lift_dyn_declaration_kind<L: CtDynamicLifter>(
    value: &CtDynDeclarationKind,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDeclarationKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynDeclarationKind::Trait => DynDeclarationKind::Trait,
        CtDynDeclarationKind::Impl => DynDeclarationKind::Impl,
    })
}
pub fn lift_dyn_member_declaration<L: CtDynamicLifter>(
    value: &CtDynMemberDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<DynMemberDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(DynMemberDeclaration {
        name: lifter.text(&value.name, depth + 1)?,
        kind: lift_dyn_member_kind(&value.kind, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_member_kind<L: CtDynamicLifter>(
    value: &CtDynMemberKind,
    lifter: &mut L,
    depth: usize,
) -> Result<DynMemberKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynMemberKind::Type => DynMemberKind::Type,
        CtDynMemberKind::Constant => DynMemberKind::Constant,
        CtDynMemberKind::Method(field_value_0) => DynMemberKind::Method({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<LifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_life_parameter_kind(item, lifter, depth + 1)?);
            }
            output
        }),
    })
}
pub fn lift_dyn_generic_actual<L: CtDynamicLifter>(
    value: &CtDynGenericActual,
    lifter: &mut L,
    depth: usize,
) -> Result<DynGenericActual, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynGenericActual::Type(field_value_0) => DynGenericActual::Type({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
        CtDynGenericActual::QualifiedType(field_value_0) => {
            DynGenericActual::QualifiedType(lift_dyn_type_value(field_value_0, lifter, depth + 1)?)
        }
        CtDynGenericActual::Const(field_value_0) => {
            DynGenericActual::Const(lifter.constant(field_value_0, depth + 1)?)
        }
        CtDynGenericActual::Life(field_value_0) => DynGenericActual::Life({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_dyn_trait_application<L: CtDynamicLifter>(
    value: &CtDynTraitApplication,
    lifter: &mut L,
    depth: usize,
) -> Result<DynTraitApplication, L::Error> {
    lifter.node(depth)?;
    Ok(DynTraitApplication {
        declaration: lift_life_declaration_id(&value.declaration, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_member<L: CtDynamicLifter>(
    value: &CtDynMember,
    lifter: &mut L,
    depth: usize,
) -> Result<DynMember, L::Error> {
    lifter.node(depth)?;
    Ok(DynMember {
        application: lift_dyn_trait_application(&value.application, lifter, depth + 1)?,
        ordinal: {
            lifter.node(depth + 1)?;
            value.ordinal
        },
    })
}
pub fn lift_dyn_associated_type<L: CtDynamicLifter>(
    value: &CtDynAssociatedType,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedType, L::Error> {
    lifter.node(depth)?;
    Ok(DynAssociatedType {
        member: lift_dyn_member(&value.member, lifter, depth + 1)?,
        ty: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.ty), depth + 1)?
        },
        life: lift_storage_life_shape(&value.life, lifter, depth + 1)?,
        classes: {
            lifter.rows(
                (value.classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn lift_dyn_associated_constant<L: CtDynamicLifter>(
    value: &CtDynAssociatedConstant,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedConstant, L::Error> {
    lifter.node(depth)?;
    Ok(DynAssociatedConstant {
        member: lift_dyn_member(&value.member, lifter, depth + 1)?,
        ty: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.ty), depth + 1)?
        },
        value: lifter.constant(&value.value, depth + 1)?,
    })
}
pub fn lift_dyn_receiver<L: CtDynamicLifter>(
    value: &CtDynReceiver,
    lifter: &mut L,
    depth: usize,
) -> Result<DynReceiver, L::Error> {
    lifter.node(depth)?;
    Ok(DynReceiver {
        mutable: {
            lifter.node(depth + 1)?;
            value.mutable
        },
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn lift_dyn_slot<L: CtDynamicLifter>(
    value: &CtDynSlot,
    lifter: &mut L,
    depth: usize,
) -> Result<DynSlot, L::Error> {
    lifter.node(depth)?;
    Ok(DynSlot {
        member: lift_dyn_member(&value.member, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        receiver: lift_dyn_receiver(&value.receiver, lifter, depth + 1)?,
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<Type>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*(item), depth + 1)?
                });
            }
            output
        },
        parameter_modes: {
            lifter.rows(
                (value.parameter_modes).len(),
                std::mem::size_of::<PassingMode>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameter_modes).len());
            for item in &value.parameter_modes {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        result: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.result), depth + 1)?
        },
        result_mode: {
            lifter.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            lifter.node(depth + 1)?;
            value.call_kind
        },
        storage_life: match &value.storage_life {
            Some(item) => Some(lift_storage_life_contract(item, lifter, depth + 1)?),
            None => None,
        },
        contract: lift_dyn_contract(&value.contract, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_interface<L: CtDynamicLifter>(
    value: &CtDynInterface,
    lifter: &mut L,
    depth: usize,
) -> Result<DynInterface, L::Error> {
    lifter.node(depth)?;
    Ok(DynInterface {
        context: match &value.context {
            Some(item) => Some({
                lifter.node(depth + 1)?;
                lifter.load_type(*(item), depth + 1)?
            }),
            None => None,
        },
        owning_cleanup: match &value.owning_cleanup {
            Some(item) => Some(lift_dyn_cleanup_contract(item, lifter, depth + 1)?),
            None => None,
        },
        id: lift_dyn_interface_id(&value.id, lifter, depth + 1)?,
        application: lift_dyn_trait_application(&value.application, lifter, depth + 1)?,
        inherited: {
            lifter.rows(
                (value.inherited).len(),
                std::mem::size_of::<DynTraitApplication>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.inherited).len());
            for item in &value.inherited {
                output.push(lift_dyn_trait_application(item, lifter, depth + 1)?);
            }
            output
        },
        associated_types: {
            lifter.rows(
                (value.associated_types).len(),
                std::mem::size_of::<DynAssociatedType>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated_types).len());
            for item in &value.associated_types {
                output.push(lift_dyn_associated_type(item, lifter, depth + 1)?);
            }
            output
        },
        associated_constants: {
            lifter.rows(
                (value.associated_constants).len(),
                std::mem::size_of::<DynAssociatedConstant>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated_constants).len());
            for item in &value.associated_constants {
                output.push(lift_dyn_associated_constant(item, lifter, depth + 1)?);
            }
            output
        },
        retained_life: lift_retained_life_shape(&value.retained_life, lifter, depth + 1)?,
        slots: {
            lifter.rows(
                (value.slots).len(),
                std::mem::size_of::<DynSlot>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(lift_dyn_slot(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_core_dyn_entry<L: CtDynamicLifter>(
    value: &CtCoreDynEntry,
    lifter: &mut L,
    depth: usize,
) -> Result<CoreDynEntry, L::Error> {
    lifter.node(depth)?;
    Ok(CoreDynEntry {
        slot: {
            lifter.node(depth + 1)?;
            value.slot
        },
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        function: {
            lifter.node(depth + 1)?;
            value.function
        },
    })
}
pub fn lift_dyn_entry<L: CtDynamicLifter>(
    value: &CtDynEntry,
    lifter: &mut L,
    depth: usize,
) -> Result<DynEntry, L::Error> {
    lifter.node(depth)?;
    Ok(DynEntry {
        slot: {
            lifter.node(depth + 1)?;
            value.slot
        },
        function: {
            lifter.node(depth + 1)?;
            value.function
        },
    })
}
pub fn lift_dyn_retained_port_binding<L: CtDynamicLifter>(
    value: &CtDynRetainedPortBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<DynRetainedPortBinding, L::Error> {
    lifter.node(depth)?;
    Ok(DynRetainedPortBinding {
        port: {
            lifter.node(depth + 1)?;
            value.port
        },
        sources: {
            lifter.rows(
                (value.sources).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_core_dyn_vtable<L: CtDynamicLifter>(
    value: &CtCoreDynVtable,
    lifter: &mut L,
    depth: usize,
) -> Result<CoreDynVtable, L::Error> {
    lifter.node(depth)?;
    Ok(CoreDynVtable {
        self_life: match &value.self_life {
            Some(item) => Some(lift_storage_life_shape(item, lifter, depth + 1)?),
            None => None,
        },
        implementation_witnesses: match &value.implementation_witnesses {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynImplementationWitness>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_implementation_witness(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_classes: {
            lifter.rows(
                (value.self_classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.self_classes).len());
            for item in &value.self_classes {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        id: lift_dyn_vtable_id(&value.id, lifter, depth + 1)?,
        interface: lift_dyn_interface_id(&value.interface, lifter, depth + 1)?,
        self_type: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.self_type), depth + 1)?
        },
        implementation: lift_life_declaration_id(&value.implementation, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        ports: {
            lifter.rows(
                (value.ports).len(),
                std::mem::size_of::<DynRetainedPortBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(lift_dyn_retained_port_binding(item, lifter, depth + 1)?);
            }
            output
        },
        entries: {
            lifter.rows(
                (value.entries).len(),
                std::mem::size_of::<CoreDynEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entries).len());
            for item in &value.entries {
                output.push(lift_core_dyn_entry(item, lifter, depth + 1)?);
            }
            output
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(lift_core_dyn_entry(item, lifter, depth + 1)?),
            None => None,
        },
        owned_storage: match &value.owned_storage {
            Some(item) => Some(lift_dyn_owned_storage(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_dyn_vtable<L: CtDynamicLifter>(
    value: &CtDynVtable,
    lifter: &mut L,
    depth: usize,
) -> Result<DynVtable, L::Error> {
    lifter.node(depth)?;
    Ok(DynVtable {
        self_life: match &value.self_life {
            Some(item) => Some(lift_storage_life_shape(item, lifter, depth + 1)?),
            None => None,
        },
        implementation_witnesses: match &value.implementation_witnesses {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynImplementationWitness>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_implementation_witness(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        self_classes: {
            lifter.rows(
                (value.self_classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.self_classes).len());
            for item in &value.self_classes {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        id: lift_dyn_vtable_id(&value.id, lifter, depth + 1)?,
        interface: lift_dyn_interface_id(&value.interface, lifter, depth + 1)?,
        self_type: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.self_type), depth + 1)?
        },
        implementation: lift_life_declaration_id(&value.implementation, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        ports: {
            lifter.rows(
                (value.ports).len(),
                std::mem::size_of::<DynRetainedPortBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(lift_dyn_retained_port_binding(item, lifter, depth + 1)?);
            }
            output
        },
        entries: {
            lifter.rows(
                (value.entries).len(),
                std::mem::size_of::<DynEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entries).len());
            for item in &value.entries {
                output.push(lift_dyn_entry(item, lifter, depth + 1)?);
            }
            output
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(lift_dyn_entry(item, lifter, depth + 1)?),
            None => None,
        },
        owned_storage: match &value.owned_storage {
            Some(item) => Some(lift_dyn_owned_storage(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_dyn_owned_storage<L: CtDynamicLifter>(
    value: &CtDynOwnedStorage,
    lifter: &mut L,
    depth: usize,
) -> Result<DynOwnedStorage, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynOwnedStorage::Heap => DynOwnedStorage::Heap,
    })
}
pub fn lift_dyn_requirement<L: CtDynamicLifter>(
    value: &CtDynRequirement,
    lifter: &mut L,
    depth: usize,
) -> Result<DynRequirement, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynRequirement::Read(field_value_0) => {
            DynRequirement::Read(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirement::Write(field_value_0) => {
            DynRequirement::Write(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirement::Manage(field_value_0) => {
            DynRequirement::Manage(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirement::Apart(field_value_0, field_value_1) => DynRequirement::Apart(
            lift_storage_position(field_value_0, lifter, depth + 1)?,
            lift_storage_position(field_value_1, lifter, depth + 1)?,
        ),
        CtDynRequirement::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => DynRequirement::State {
            position: lift_storage_position(field_position, lifter, depth + 1)?,
            ty: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_ty), depth + 1)?
            },
            variant: {
                lifter.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn lift_dyn_guarantee<L: CtDynamicLifter>(
    value: &CtDynGuarantee,
    lifter: &mut L,
    depth: usize,
) -> Result<DynGuarantee, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynGuarantee::StorageSources(field_value_0) => DynGuarantee::StorageSources(
            lift_storage_source_binding(field_value_0, lifter, depth + 1)?,
        ),
        CtDynGuarantee::ManageResult => DynGuarantee::ManageResult,
        CtDynGuarantee::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => DynGuarantee::State {
            position: lift_storage_position(field_position, lifter, depth + 1)?,
            ty: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_ty), depth + 1)?
            },
            variant: {
                lifter.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn lift_dyn_effect<L: CtDynamicLifter>(
    value: &CtDynEffect,
    lifter: &mut L,
    depth: usize,
) -> Result<DynEffect, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynEffect::Alloc => DynEffect::Alloc,
        CtDynEffect::Io => DynEffect::Io,
        CtDynEffect::Block => DynEffect::Block,
        CtDynEffect::Suspend => DynEffect::Suspend,
        CtDynEffect::Spawn => DynEffect::Spawn,
        CtDynEffect::Ffi => DynEffect::Ffi,
        CtDynEffect::Reenter => DynEffect::Reenter,
        CtDynEffect::Panic => DynEffect::Panic,
        CtDynEffect::Call(field_value_0) => {
            DynEffect::Call(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynEffect::Cleanup(field_value_0) => DynEffect::Cleanup({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
    })
}
pub fn lift_dyn_call_access<L: CtDynamicLifter>(
    value: &CtDynCallAccess,
    lifter: &mut L,
    depth: usize,
) -> Result<DynCallAccess, L::Error> {
    lifter.node(depth)?;
    Ok(DynCallAccess {
        reads: {
            lifter.rows(
                (value.reads).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.reads).len());
            for item in &value.reads {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
        writes: {
            lifter.rows(
                (value.writes).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.writes).len());
            for item in &value.writes {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_result_source<L: CtDynamicLifter>(
    value: &CtDynResultSource,
    lifter: &mut L,
    depth: usize,
) -> Result<DynResultSource, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynResultSource::Value => DynResultSource::Value,
        CtDynResultSource::Create => DynResultSource::Create,
        CtDynResultSource::Give => DynResultSource::Give,
        CtDynResultSource::Borrow {
            sources: field_sources,
            mutable: field_mutable,
        } => DynResultSource::Borrow {
            sources: {
                lifter.rows(
                    (field_sources).len(),
                    std::mem::size_of::<StorageSource>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sources).len());
                for item in field_sources {
                    output.push(lift_storage_source(item, lifter, depth + 1)?);
                }
                output
            },
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
        },
    })
}
pub fn lift_dyn_cleanup_contract<L: CtDynamicLifter>(
    value: &CtDynCleanupContract,
    lifter: &mut L,
    depth: usize,
) -> Result<DynCleanupContract, L::Error> {
    lifter.node(depth)?;
    Ok(DynCleanupContract {
        environment: {
            lifter.node(depth + 1)?;
            value.environment
        },
        positions: {
            lifter.rows(
                (value.positions).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.positions).len());
            for item in &value.positions {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
        access: lift_dyn_call_access(&value.access, lifter, depth + 1)?,
        effects: {
            lifter.rows(
                (value.effects).len(),
                std::mem::size_of::<DynEffect>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.effects).len());
            for item in &value.effects {
                output.push(lift_dyn_effect(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_resume_bound<L: CtDynamicLifter>(
    value: &CtDynResumeBound,
    lifter: &mut L,
    depth: usize,
) -> Result<DynResumeBound, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynResumeBound::Local => DynResumeBound::Local,
        CtDynResumeBound::Transfer => DynResumeBound::Transfer,
    })
}
pub fn lift_dyn_thread_requirements<L: CtDynamicLifter>(
    value: &CtDynThreadRequirements,
    lifter: &mut L,
    depth: usize,
) -> Result<DynThreadRequirements, L::Error> {
    lifter.node(depth)?;
    Ok(DynThreadRequirements {
        transfer: {
            lifter.node(depth + 1)?;
            value.transfer
        },
        share: {
            lifter.node(depth + 1)?;
            value.share
        },
    })
}
pub fn lift_dyn_contract<L: CtDynamicLifter>(
    value: &CtDynContract,
    lifter: &mut L,
    depth: usize,
) -> Result<DynContract, L::Error> {
    lifter.node(depth)?;
    Ok(DynContract {
        requires: match &value.requires {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynRequirement>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_requirement(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        guarantees: match &value.guarantees {
            Some(item) => Some({
                lifter.rows((item).len(), std::mem::size_of::<DynGuarantee>(), depth + 1)?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_guarantee(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        hold: match &value.hold {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<StoragePosition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_storage_position(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        call: match &value.call {
            Some(item) => Some(lift_dyn_call_access(item, lifter, depth + 1)?),
            None => None,
        },
        result: match &value.result {
            Some(item) => Some(lift_dyn_result_source(item, lifter, depth + 1)?),
            None => None,
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(lift_dyn_cleanup_contract(item, lifter, depth + 1)?),
            None => None,
        },
        effects: match &value.effects {
            Some(item) => Some({
                lifter.rows((item).len(), std::mem::size_of::<DynEffect>(), depth + 1)?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_effect(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        resume: match &value.resume {
            Some(item) => Some(lift_dyn_resume_bound(item, lifter, depth + 1)?),
            None => None,
        },
        threads: match &value.threads {
            Some(item) => Some(lift_dyn_thread_requirements(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_dyn_type_value<L: CtDynamicLifter>(
    value: &CtDynTypeValue,
    lifter: &mut L,
    depth: usize,
) -> Result<DynTypeValue, L::Error> {
    lifter.node(depth)?;
    Ok(DynTypeValue {
        ty: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.ty), depth + 1)?
        },
        life: lift_storage_life_shape(&value.life, lifter, depth + 1)?,
        classes: {
            lifter.rows(
                (value.classes).len(),
                std::mem::size_of::<StorageBinder>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn lift_dyn_instantiation_scope<L: CtDynamicLifter>(
    value: &CtDynInstantiationScope,
    lifter: &mut L,
    depth: usize,
) -> Result<DynInstantiationScope, L::Error> {
    lifter.node(depth)?;
    Ok(DynInstantiationScope {
        fixed_binder_count: {
            lifter.node(depth + 1)?;
            value.fixed_binder_count
        },
        invocation_binder_count: {
            lifter.node(depth + 1)?;
            value.invocation_binder_count
        },
    })
}
pub fn lift_dyn_life_embedding<L: CtDynamicLifter>(
    value: &CtDynLifeEmbedding,
    lifter: &mut L,
    depth: usize,
) -> Result<DynLifeEmbedding, L::Error> {
    lifter.node(depth)?;
    Ok(DynLifeEmbedding {
        from: {
            lifter.node(depth + 1)?;
            value.from
        },
        to: {
            lifter.node(depth + 1)?;
            value.to
        },
    })
}
pub fn lift_dyn_implicit_life_kind<L: CtDynamicLifter>(
    value: &CtDynImplicitLifeKind,
    lifter: &mut L,
    depth: usize,
) -> Result<DynImplicitLifeKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynImplicitLifeKind::Address => DynImplicitLifeKind::Address,
        CtDynImplicitLifeKind::Description => DynImplicitLifeKind::Description,
        CtDynImplicitLifeKind::LifetimeArgument => DynImplicitLifeKind::LifetimeArgument,
    })
}
pub fn lift_dyn_implicit_life_root<L: CtDynamicLifter>(
    value: &CtDynImplicitLifeRoot,
    lifter: &mut L,
    depth: usize,
) -> Result<DynImplicitLifeRoot, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynImplicitLifeRoot::SelfPattern => DynImplicitLifeRoot::SelfPattern,
        CtDynImplicitLifeRoot::ImplementedTrait => DynImplicitLifeRoot::ImplementedTrait,
        CtDynImplicitLifeRoot::Supertrait(field_value_0) => DynImplicitLifeRoot::Supertrait({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynImplicitLifeRoot::Requirement(field_value_0) => DynImplicitLifeRoot::Requirement({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynImplicitLifeRoot::AssociatedDefinition(field_value_0) => {
            DynImplicitLifeRoot::AssociatedDefinition({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtDynImplicitLifeRoot::MethodSignature(field_value_0) => {
            DynImplicitLifeRoot::MethodSignature({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtDynImplicitLifeRoot::MethodDefinition(field_value_0) => {
            DynImplicitLifeRoot::MethodDefinition({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
    })
}
pub fn lift_dyn_implicit_life_occurrence<L: CtDynamicLifter>(
    value: &CtDynImplicitLifeOccurrence,
    lifter: &mut L,
    depth: usize,
) -> Result<DynImplicitLifeOccurrence, L::Error> {
    lifter.node(depth)?;
    Ok(DynImplicitLifeOccurrence {
        root: lift_dyn_implicit_life_root(&value.root, lifter, depth + 1)?,
        path: {
            lifter.rows((value.path).len(), std::mem::size_of::<u32>(), depth + 1)?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        kind: lift_dyn_implicit_life_kind(&value.kind, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_implicit_life_declaration<L: CtDynamicLifter>(
    value: &CtDynImplicitLifeDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<DynImplicitLifeDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(DynImplicitLifeDeclaration {
        occurrences: {
            lifter.rows(
                (value.occurrences).len(),
                std::mem::size_of::<DynImplicitLifeOccurrence>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrences).len());
            for item in &value.occurrences {
                output.push(lift_dyn_implicit_life_occurrence(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_life_pattern<L: CtDynamicLifter>(
    value: &CtDynLifePattern,
    lifter: &mut L,
    depth: usize,
) -> Result<DynLifePattern, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynLifePattern::DeclarationParameter(field_value_0) => {
            DynLifePattern::DeclarationParameter({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtDynLifePattern::MethodParameter(field_value_0) => DynLifePattern::MethodParameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynLifePattern::Bound {
            depth: field_depth,
            index: field_index,
        } => DynLifePattern::Bound {
            depth: {
                lifter.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                lifter.node(depth + 1)?;
                *(field_index)
            },
        },
        CtDynLifePattern::Invocation(field_value_0) => DynLifePattern::Invocation({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynLifePattern::ImplicitDeclaration(field_value_0) => {
            DynLifePattern::ImplicitDeclaration({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
    })
}
pub fn lift_dyn_obligation<L: CtDynamicLifter>(
    value: &CtDynObligation,
    lifter: &mut L,
    depth: usize,
) -> Result<DynObligation, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynObligation::Structural {
            receiver: field_receiver,
            bound: field_bound,
        } => DynObligation::Structural {
            receiver: lift_dyn_type_pattern(field_receiver, lifter, depth + 1)?,
            bound: lift_dyn_structural_bound(field_bound, lifter, depth + 1)?,
        },
        CtDynObligation::Trait {
            receiver: field_receiver,
            application: field_application,
        } => DynObligation::Trait {
            receiver: lift_dyn_type_pattern(field_receiver, lifter, depth + 1)?,
            application: lift_dyn_self_pattern(field_application, lifter, depth + 1)?,
        },
        CtDynObligation::Callable {
            receiver: field_receiver,
            kind: field_kind,
            quantified_lifetimes: field_quantified_lifetimes,
            signature: field_signature,
        } => DynObligation::Callable {
            receiver: lift_dyn_type_pattern(field_receiver, lifter, depth + 1)?,
            kind: {
                lifter.node(depth + 1)?;
                *(field_kind)
            },
            quantified_lifetimes: {
                lifter.node(depth + 1)?;
                *(field_quantified_lifetimes)
            },
            signature: {
                lifter.rows(1, std::mem::size_of::<DynSignatureRecipe>(), depth + 1)?;
                Box::new(lift_dyn_signature_recipe(
                    (field_signature).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtDynObligation::TypeEqual {
            left: field_left,
            right: field_right,
        } => DynObligation::TypeEqual {
            left: lift_dyn_type_pattern(field_left, lifter, depth + 1)?,
            right: lift_dyn_type_pattern(field_right, lifter, depth + 1)?,
        },
        CtDynObligation::ConstEqual {
            ty: field_ty,
            left: field_left,
            right: field_right,
        } => DynObligation::ConstEqual {
            ty: lift_dyn_type_pattern(field_ty, lifter, depth + 1)?,
            left: lift_dyn_const_pattern(field_left, lifter, depth + 1)?,
            right: lift_dyn_const_pattern(field_right, lifter, depth + 1)?,
        },
    })
}
pub fn lift_dyn_parameter_recipe<L: CtDynamicLifter>(
    value: &CtDynParameterRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynParameterRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynParameterRecipe {
        ty: lift_dyn_type_pattern(&value.ty, lifter, depth + 1)?,
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn lift_dyn_signature_recipe<L: CtDynamicLifter>(
    value: &CtDynSignatureRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynSignatureRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynSignatureRecipe {
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<DynParameterRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(lift_dyn_parameter_recipe(item, lifter, depth + 1)?);
            }
            output
        },
        result: lift_dyn_type_pattern(&value.result, lifter, depth + 1)?,
        result_mode: {
            lifter.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            lifter.node(depth + 1)?;
            value.call_kind
        },
        invocation_binder_count: {
            lifter.node(depth + 1)?;
            value.invocation_binder_count
        },
        contract: lift_dyn_contract_recipe(&value.contract, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_requirement_recipe<L: CtDynamicLifter>(
    value: &CtDynRequirementRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynRequirementRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynRequirementRecipe::Read(field_value_0) => {
            DynRequirementRecipe::Read(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirementRecipe::Write(field_value_0) => {
            DynRequirementRecipe::Write(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirementRecipe::Manage(field_value_0) => {
            DynRequirementRecipe::Manage(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynRequirementRecipe::Apart(field_value_0, field_value_1) => DynRequirementRecipe::Apart(
            lift_storage_position(field_value_0, lifter, depth + 1)?,
            lift_storage_position(field_value_1, lifter, depth + 1)?,
        ),
        CtDynRequirementRecipe::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => DynRequirementRecipe::State {
            position: lift_storage_position(field_position, lifter, depth + 1)?,
            ty: lift_dyn_type_pattern(field_ty, lifter, depth + 1)?,
            variant: {
                lifter.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn lift_dyn_guarantee_recipe<L: CtDynamicLifter>(
    value: &CtDynGuaranteeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynGuaranteeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynGuaranteeRecipe::StorageSources(field_value_0) => DynGuaranteeRecipe::StorageSources(
            lift_dyn_storage_source_binding_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtDynGuaranteeRecipe::ManageResult => DynGuaranteeRecipe::ManageResult,
        CtDynGuaranteeRecipe::State {
            position: field_position,
            ty: field_ty,
            variant: field_variant,
        } => DynGuaranteeRecipe::State {
            position: lift_storage_position(field_position, lifter, depth + 1)?,
            ty: lift_dyn_type_pattern(field_ty, lifter, depth + 1)?,
            variant: {
                lifter.node(depth + 1)?;
                *(field_variant)
            },
        },
    })
}
pub fn lift_dyn_effect_recipe<L: CtDynamicLifter>(
    value: &CtDynEffectRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynEffectRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynEffectRecipe::Alloc => DynEffectRecipe::Alloc,
        CtDynEffectRecipe::Io => DynEffectRecipe::Io,
        CtDynEffectRecipe::Block => DynEffectRecipe::Block,
        CtDynEffectRecipe::Suspend => DynEffectRecipe::Suspend,
        CtDynEffectRecipe::Spawn => DynEffectRecipe::Spawn,
        CtDynEffectRecipe::Ffi => DynEffectRecipe::Ffi,
        CtDynEffectRecipe::Reenter => DynEffectRecipe::Reenter,
        CtDynEffectRecipe::Panic => DynEffectRecipe::Panic,
        CtDynEffectRecipe::Call(field_value_0) => {
            DynEffectRecipe::Call(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynEffectRecipe::Cleanup(field_value_0) => {
            DynEffectRecipe::Cleanup(lift_dyn_type_pattern(field_value_0, lifter, depth + 1)?)
        }
    })
}
pub fn lift_dyn_cleanup_recipe<L: CtDynamicLifter>(
    value: &CtDynCleanupRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynCleanupRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynCleanupRecipe {
        environment: {
            lifter.node(depth + 1)?;
            value.environment
        },
        positions: {
            lifter.rows(
                (value.positions).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.positions).len());
            for item in &value.positions {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
        access: lift_dyn_call_access(&value.access, lifter, depth + 1)?,
        effects: {
            lifter.rows(
                (value.effects).len(),
                std::mem::size_of::<DynEffectRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.effects).len());
            for item in &value.effects {
                output.push(lift_dyn_effect_recipe(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_contract_recipe<L: CtDynamicLifter>(
    value: &CtDynContractRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynContractRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynContractRecipe {
        requires: match &value.requires {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_requirement_recipe(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        guarantees: match &value.guarantees {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynGuaranteeRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_guarantee_recipe(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        hold: match &value.hold {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<StoragePosition>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_storage_position(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        call: match &value.call {
            Some(item) => Some(lift_dyn_call_access(item, lifter, depth + 1)?),
            None => None,
        },
        result: match &value.result {
            Some(item) => Some(lift_dyn_result_source_recipe(item, lifter, depth + 1)?),
            None => None,
        },
        cleanup: match &value.cleanup {
            Some(item) => Some(lift_dyn_cleanup_recipe(item, lifter, depth + 1)?),
            None => None,
        },
        effects: match &value.effects {
            Some(item) => Some({
                lifter.rows(
                    (item).len(),
                    std::mem::size_of::<DynEffectRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((item).len());
                for item in item {
                    output.push(lift_dyn_effect_recipe(item, lifter, depth + 1)?);
                }
                output
            }),
            None => None,
        },
        resume: match &value.resume {
            Some(item) => Some(lift_dyn_resume_bound(item, lifter, depth + 1)?),
            None => None,
        },
        threads: match &value.threads {
            Some(item) => Some(lift_dyn_thread_requirements(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_dyn_storage_source_recipe<L: CtDynamicLifter>(
    value: &CtDynStorageSourceRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynStorageSourceRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynStorageSourceRecipe::Exact(field_value_0) => {
            DynStorageSourceRecipe::Exact(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtDynStorageSourceRecipe::PlaceAddress {
            place: field_place,
            mode: field_mode,
        } => DynStorageSourceRecipe::PlaceAddress {
            place: lift_storage_position(field_place, lifter, depth + 1)?,
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtDynStorageSourceRecipe::OwnedView {
            head: field_head,
            leaf: field_leaf,
            mode: field_mode,
        } => DynStorageSourceRecipe::OwnedView {
            head: lift_storage_position(field_head, lifter, depth + 1)?,
            leaf: lift_dyn_type_pattern(field_leaf, lifter, depth + 1)?,
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtDynStorageSourceRecipe::ReborrowedLeaf {
            source: field_source,
            head: field_head,
        } => DynStorageSourceRecipe::ReborrowedLeaf {
            source: lift_storage_position(field_source, lifter, depth + 1)?,
            head: lift_storage_position(field_head, lifter, depth + 1)?,
        },
        CtDynStorageSourceRecipe::Static {
            leaf: field_leaf,
            mode: field_mode,
        } => DynStorageSourceRecipe::Static {
            leaf: lift_dyn_type_pattern(field_leaf, lifter, depth + 1)?,
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
    })
}
pub fn lift_dyn_storage_source_binding_recipe<L: CtDynamicLifter>(
    value: &CtDynStorageSourceBindingRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynStorageSourceBindingRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynStorageSourceBindingRecipe {
        position: lift_storage_position(&value.position, lifter, depth + 1)?,
        sources: {
            lifter.rows(
                (value.sources).len(),
                std::mem::size_of::<DynStorageSourceRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(lift_dyn_storage_source_recipe(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_result_source_recipe<L: CtDynamicLifter>(
    value: &CtDynResultSourceRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynResultSourceRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynResultSourceRecipe::Value => DynResultSourceRecipe::Value,
        CtDynResultSourceRecipe::Create => DynResultSourceRecipe::Create,
        CtDynResultSourceRecipe::Give => DynResultSourceRecipe::Give,
        CtDynResultSourceRecipe::Borrow {
            sources: field_sources,
            mutable: field_mutable,
        } => DynResultSourceRecipe::Borrow {
            sources: {
                lifter.rows(
                    (field_sources).len(),
                    std::mem::size_of::<DynStorageSourceRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sources).len());
                for item in field_sources {
                    output.push(lift_dyn_storage_source_recipe(item, lifter, depth + 1)?);
                }
                output
            },
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
        },
    })
}
pub fn lift_dyn_definition_id<L: CtDynamicLifter>(
    value: &CtDynDefinitionId,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDefinitionId, L::Error> {
    lifter.node(depth)?;
    Ok(DynDefinitionId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        source: {
            lifter.node(depth + 1)?;
            value.source
        },
    })
}
pub fn lift_dyn_definition_scope<L: CtDynamicLifter>(
    value: &CtDynDefinitionScope,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDefinitionScope, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynDefinitionScope::TraitDefault => DynDefinitionScope::TraitDefault,
        CtDynDefinitionScope::ImplOverride => DynDefinitionScope::ImplOverride,
    })
}
pub fn lift_dyn_definition_declaration<L: CtDynamicLifter>(
    value: &CtDynDefinitionDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDefinitionDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(DynDefinitionDeclaration {
        requirements: {
            lifter.rows(
                (value.requirements).len(),
                std::mem::size_of::<DynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(lift_dyn_obligation(item, lifter, depth + 1)?);
            }
            output
        },
        instances: {
            lifter.rows(
                (value.instances).len(),
                std::mem::size_of::<DynDefinitionSpecialization>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.instances).len());
            for item in &value.instances {
                output.push(lift_dyn_definition_specialization(item, lifter, depth + 1)?);
            }
            output
        },
        id: lift_dyn_definition_id(&value.id, lifter, depth + 1)?,
        identity: lift_life_declaration_identity(&value.identity, lifter, depth + 1)?,
        member: {
            lifter.node(depth + 1)?;
            value.member
        },
        scope: lift_dyn_definition_scope(&value.scope, lifter, depth + 1)?,
        signature: lift_dyn_signature_recipe(&value.signature, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_method_recipe<L: CtDynamicLifter>(
    value: &CtDynMethodRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynMethodRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(DynMethodRecipe {
        requirements: {
            lifter.rows(
                (value.requirements).len(),
                std::mem::size_of::<DynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(lift_dyn_obligation(item, lifter, depth + 1)?);
            }
            output
        },
        member: {
            lifter.node(depth + 1)?;
            value.member
        },
        signature: lift_dyn_signature_recipe(&value.signature, lifter, depth + 1)?,
        default_definition: match &value.default_definition {
            Some(item) => Some(lift_dyn_definition_id(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_dyn_function_instance<L: CtDynamicLifter>(
    value: &CtDynFunctionInstance,
    lifter: &mut L,
    depth: usize,
) -> Result<DynFunctionInstance, L::Error> {
    lifter.node(depth)?;
    Ok(DynFunctionInstance {
        scope: lift_dyn_instantiation_scope(&value.scope, lifter, depth + 1)?,
        header_embedding: {
            lifter.rows(
                (value.header_embedding).len(),
                std::mem::size_of::<DynLifeEmbedding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.header_embedding).len());
            for item in &value.header_embedding {
                output.push(lift_dyn_life_embedding(item, lifter, depth + 1)?);
            }
            output
        },
        definition: lift_dyn_definition_id(&value.definition, lifter, depth + 1)?,
        receiver: lift_dyn_type_value(&value.receiver, lifter, depth + 1)?,
        declaration_actuals: {
            lifter.rows(
                (value.declaration_actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_actuals).len());
            for item in &value.declaration_actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        method_actuals: {
            lifter.rows(
                (value.method_actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.method_actuals).len());
            for item in &value.method_actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_associated_value_recipe<L: CtDynamicLifter>(
    value: &CtDynAssociatedValueRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedValueRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynAssociatedValueRecipe::Type(field_value_0) => {
            DynAssociatedValueRecipe::Type(lift_dyn_type_pattern(field_value_0, lifter, depth + 1)?)
        }
        CtDynAssociatedValueRecipe::Constant {
            ty: field_ty,
            value: field_value,
        } => DynAssociatedValueRecipe::Constant {
            ty: lift_dyn_type_pattern(field_ty, lifter, depth + 1)?,
            value: lift_dyn_const_pattern(field_value, lifter, depth + 1)?,
        },
    })
}
pub fn lift_dyn_associated_definition<L: CtDynamicLifter>(
    value: &CtDynAssociatedDefinition,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedDefinition, L::Error> {
    lifter.node(depth)?;
    Ok(DynAssociatedDefinition {
        carrier: match &value.carrier {
            Some(item) => Some(lift_dyn_type_pattern(item, lifter, depth + 1)?),
            None => None,
        },
        member: {
            lifter.node(depth + 1)?;
            value.member
        },
        value: match &value.value {
            Some(item) => Some(lift_dyn_associated_value_recipe(item, lifter, depth + 1)?),
            None => None,
        },
        requirements: {
            lifter.rows(
                (value.requirements).len(),
                std::mem::size_of::<DynObligation>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(lift_dyn_obligation(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_associated_source<L: CtDynamicLifter>(
    value: &CtDynAssociatedSource,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedSource, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynAssociatedSource::TraitDefault {
            declaration: field_declaration,
            member: field_member,
        } => DynAssociatedSource::TraitDefault {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
        CtDynAssociatedSource::ImplOverride {
            declaration: field_declaration,
            member: field_member,
        } => DynAssociatedSource::ImplOverride {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
    })
}
pub fn lift_dyn_associated_value<L: CtDynamicLifter>(
    value: &CtDynAssociatedValue,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedValue, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynAssociatedValue::Type(field_value_0) => {
            DynAssociatedValue::Type(lift_dyn_type_value(field_value_0, lifter, depth + 1)?)
        }
        CtDynAssociatedValue::Constant {
            ty: field_ty,
            value: field_value,
        } => DynAssociatedValue::Constant {
            ty: lift_dyn_type_value(field_ty, lifter, depth + 1)?,
            value: lifter.constant(field_value, depth + 1)?,
        },
    })
}
pub fn lift_dyn_associated_resolution<L: CtDynamicLifter>(
    value: &CtDynAssociatedResolution,
    lifter: &mut L,
    depth: usize,
) -> Result<DynAssociatedResolution, L::Error> {
    lifter.node(depth)?;
    Ok(DynAssociatedResolution {
        member: lift_dyn_member(&value.member, lifter, depth + 1)?,
        source: lift_dyn_associated_source(&value.source, lifter, depth + 1)?,
        value: lift_dyn_associated_value(&value.value, lifter, depth + 1)?,
    })
}
pub fn lift_dyn_implementation_witness<L: CtDynamicLifter>(
    value: &CtDynImplementationWitness,
    lifter: &mut L,
    depth: usize,
) -> Result<DynImplementationWitness, L::Error> {
    lifter.node(depth)?;
    Ok(DynImplementationWitness {
        scope: lift_dyn_instantiation_scope(&value.scope, lifter, depth + 1)?,
        to_context: {
            lifter.rows(
                (value.to_context).len(),
                std::mem::size_of::<DynLifeEmbedding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.to_context).len());
            for item in &value.to_context {
                output.push(lift_dyn_life_embedding(item, lifter, depth + 1)?);
            }
            output
        },
        receiver: lift_dyn_type_value(&value.receiver, lifter, depth + 1)?,
        application: lift_dyn_trait_application(&value.application, lifter, depth + 1)?,
        implementation: lift_life_declaration_id(&value.implementation, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        associated: {
            lifter.rows(
                (value.associated).len(),
                std::mem::size_of::<DynAssociatedResolution>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated).len());
            for item in &value.associated {
                output.push(lift_dyn_associated_resolution(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_function_target<L: CtDynamicLifter>(
    value: &CtDynFunctionTarget,
    lifter: &mut L,
    depth: usize,
) -> Result<DynFunctionTarget, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynFunctionTarget::Core {
            module: field_module,
            source: field_source,
        } => DynFunctionTarget::Core {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            source: {
                lifter.node(depth + 1)?;
                *(field_source)
            },
        },
        CtDynFunctionTarget::Typed(field_value_0) => DynFunctionTarget::Typed({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_dyn_definition_specialization<L: CtDynamicLifter>(
    value: &CtDynDefinitionSpecialization,
    lifter: &mut L,
    depth: usize,
) -> Result<DynDefinitionSpecialization, L::Error> {
    lifter.node(depth)?;
    Ok(DynDefinitionSpecialization {
        function: lift_dyn_function_target(&value.function, lifter, depth + 1)?,
        receiver: lift_dyn_type_value(&value.receiver, lifter, depth + 1)?,
        declaration_actuals: {
            lifter.rows(
                (value.declaration_actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_actuals).len());
            for item in &value.declaration_actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        method_actuals: {
            lifter.rows(
                (value.method_actuals).len(),
                std::mem::size_of::<DynGenericActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.method_actuals).len());
            for item in &value.method_actuals {
                output.push(lift_dyn_generic_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_dyn_structural_bound<L: CtDynamicLifter>(
    value: &CtDynStructuralBound,
    lifter: &mut L,
    depth: usize,
) -> Result<DynStructuralBound, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynStructuralBound::Copy => DynStructuralBound::Copy,
        CtDynStructuralBound::Value => DynStructuralBound::Value,
        CtDynStructuralBound::SyncDrop => DynStructuralBound::SyncDrop,
        CtDynStructuralBound::StoreElement => DynStructuralBound::StoreElement,
    })
}
pub fn lift_semantic_definition_id<L: CtDynamicLifter>(
    value: &CtSemanticDefinitionId,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDefinitionId, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticDefinitionId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        index: {
            lifter.node(depth + 1)?;
            value.index
        },
    })
}
pub fn lift_semantic_instance_id<L: CtDynamicLifter>(
    value: &CtSemanticInstanceId,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticInstanceId, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticInstanceId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        index: {
            lifter.node(depth + 1)?;
            value.index
        },
    })
}
pub fn lift_semantic_module<L: CtDynamicLifter>(
    value: &CtSemanticModule,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticModule, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticModule {
        definitions: {
            lifter.rows(
                (value.definitions).len(),
                std::mem::size_of::<SemanticDefinition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.definitions).len());
            for item in &value.definitions {
                output.push(lift_semantic_definition(item, lifter, depth + 1)?);
            }
            output
        },
        instances: {
            lifter.rows(
                (value.instances).len(),
                std::mem::size_of::<SemanticInstance>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.instances).len());
            for item in &value.instances {
                output.push(lift_semantic_instance(item, lifter, depth + 1)?);
            }
            output
        },
        sites: {
            lifter.rows(
                (value.sites).len(),
                std::mem::size_of::<SemanticSite>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sites).len());
            for item in &value.sites {
                output.push(lift_semantic_site(item, lifter, depth + 1)?);
            }
            output
        },
        dyn_scopes: {
            lifter.rows(
                (value.dyn_scopes).len(),
                std::mem::size_of::<SemanticDynScopeBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.dyn_scopes).len());
            for item in &value.dyn_scopes {
                output.push(lift_semantic_dyn_scope_binding(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_dyn_scope_binding<L: CtDynamicLifter>(
    value: &CtSemanticDynScopeBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDynScopeBinding, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticDynScopeBinding {
        owner: lift_semantic_dyn_owner(&value.owner, lifter, depth + 1)?,
        scope: lift_semantic_definition_id(&value.scope, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_dyn_owner<L: CtDynamicLifter>(
    value: &CtSemanticDynOwner,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDynOwner, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticDynOwner::Declaration(field_value_0) => SemanticDynOwner::Declaration(
            lift_life_declaration_id(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticDynOwner::MethodSignature {
            declaration: field_declaration,
            member: field_member,
        } => SemanticDynOwner::MethodSignature {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
        CtSemanticDynOwner::MethodDefinition(field_value_0) => SemanticDynOwner::MethodDefinition(
            lift_dyn_definition_id(field_value_0, lifter, depth + 1)?,
        ),
    })
}
pub fn lift_semantic_definition<L: CtDynamicLifter>(
    value: &CtSemanticDefinition,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDefinition, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticDefinition {
        id: lift_semantic_definition_id(&value.id, lifter, depth + 1)?,
        identity: lift_life_declaration_identity(&value.identity, lifter, depth + 1)?,
        parent: match &value.parent {
            Some(item) => Some(lift_semantic_definition_id(item, lifter, depth + 1)?),
            None => None,
        },
        kind: lift_semantic_definition_kind(&value.kind, lifter, depth + 1)?,
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<SemanticFormal>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(lift_semantic_formal(item, lifter, depth + 1)?);
            }
            output
        },
        implicit_lifetimes: {
            lifter.rows(
                (value.implicit_lifetimes).len(),
                std::mem::size_of::<SemanticImplicitLifetime>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.implicit_lifetimes).len());
            for item in &value.implicit_lifetimes {
                output.push(lift_semantic_implicit_lifetime(item, lifter, depth + 1)?);
            }
            output
        },
        requirements: {
            lifter.rows(
                (value.requirements).len(),
                std::mem::size_of::<SemanticRequirementRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.requirements).len());
            for item in &value.requirements {
                output.push(lift_semantic_requirement_recipe(item, lifter, depth + 1)?);
            }
            output
        },
        environment: {
            lifter.rows(
                (value.environment).len(),
                std::mem::size_of::<SemanticEnvironmentFormal>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.environment).len());
            for item in &value.environment {
                output.push(lift_semantic_environment_formal(item, lifter, depth + 1)?);
            }
            output
        },
        body: lift_semantic_definition_body(&value.body, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_definition_kind<L: CtDynamicLifter>(
    value: &CtSemanticDefinitionKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDefinitionKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticDefinitionKind::Function => SemanticDefinitionKind::Function,
        CtSemanticDefinitionKind::Method => SemanticDefinitionKind::Method,
        CtSemanticDefinitionKind::Closure => SemanticDefinitionKind::Closure,
        CtSemanticDefinitionKind::TaskEntry => SemanticDefinitionKind::TaskEntry,
        CtSemanticDefinitionKind::Nominal => SemanticDefinitionKind::Nominal,
        CtSemanticDefinitionKind::Scope => SemanticDefinitionKind::Scope,
    })
}
pub fn lift_semantic_definition_body<L: CtDynamicLifter>(
    value: &CtSemanticDefinitionBody,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDefinitionBody, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticDefinitionBody::Function {
            signature: field_signature,
            sites: field_sites,
        } => SemanticDefinitionBody::Function {
            signature: lift_semantic_signature_recipe(field_signature, lifter, depth + 1)?,
            sites: {
                lifter.rows(
                    (field_sites).len(),
                    std::mem::size_of::<SemanticSiteRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sites).len());
                for item in field_sites {
                    output.push(lift_semantic_site_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticDefinitionBody::Nominal {
            declaration: field_declaration,
            kind: field_kind,
            body: field_body,
        } => SemanticDefinitionBody::Nominal {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            kind: lift_life_nominal_kind(field_kind, lifter, depth + 1)?,
            body: lift_semantic_nominal_body(field_body, lifter, depth + 1)?,
        },
        CtSemanticDefinitionBody::Scope(field_value_0) => SemanticDefinitionBody::Scope(
            lift_semantic_scope_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticDefinitionBody::FunctionWithBody {
            signature: field_signature,
            protocol_contracts: field_protocol_contracts,
            foreign: field_foreign,
            resume_bound: field_resume_bound,
            sites: field_sites,
            parameters: field_parameters,
            receiver_parameter: field_receiver_parameter,
            names: field_names,
            site_nodes: field_site_nodes,
            body: field_body,
        } => SemanticDefinitionBody::FunctionWithBody {
            signature: lift_semantic_signature_recipe(field_signature, lifter, depth + 1)?,
            protocol_contracts: {
                lifter.rows(
                    (field_protocol_contracts).len(),
                    std::mem::size_of::<FunctionProtocolContract>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_protocol_contracts).len());
                for item in field_protocol_contracts {
                    output.push(lift_function_protocol_contract(item, lifter, depth + 1)?);
                }
                output
            },
            foreign: match field_foreign {
                Some(item) => Some(lift_foreign_declaration(item, lifter, depth + 1)?),
                None => None,
            },
            resume_bound: lift_task_resume_bound(field_resume_bound, lifter, depth + 1)?,
            sites: {
                lifter.rows(
                    (field_sites).len(),
                    std::mem::size_of::<SemanticSiteRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_sites).len());
                for item in field_sites {
                    output.push(lift_semantic_site_recipe(item, lifter, depth + 1)?);
                }
                output
            },
            parameters: {
                lifter.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<SemanticOriginalClosureParameter>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(lift_semantic_original_closure_parameter(
                        item,
                        lifter,
                        depth + 1,
                    )?);
                }
                output
            },
            receiver_parameter: match field_receiver_parameter {
                Some(item) => Some({
                    lifter.node(depth + 1)?;
                    *(item)
                }),
                None => None,
            },
            names: {
                lifter.rows(
                    (field_names).len(),
                    std::mem::size_of::<SemanticOriginalNameBinding>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_names).len());
                for item in field_names {
                    output.push(lift_semantic_original_name_binding(
                        item,
                        lifter,
                        depth + 1,
                    )?);
                }
                output
            },
            site_nodes: {
                lifter.rows(
                    (field_site_nodes).len(),
                    std::mem::size_of::<SemanticOriginalSiteNode>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_site_nodes).len());
                for item in field_site_nodes {
                    output.push(lift_semantic_original_site_node(item, lifter, depth + 1)?);
                }
                output
            },
            body: lift_semantic_original_function_body(field_body, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_original_function_body<L: CtDynamicLifter>(
    value: &CtSemanticOriginalFunctionBody,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalFunctionBody, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalFunctionBody::Defined(field_value_0) => {
            SemanticOriginalFunctionBody::Defined(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalFunctionBody::External => SemanticOriginalFunctionBody::External,
        CtSemanticOriginalFunctionBody::Declaration => SemanticOriginalFunctionBody::Declaration,
    })
}
pub fn lift_semantic_original_type_source<L: CtDynamicLifter>(
    value: &CtSemanticOriginalTypeSource,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalTypeSource, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalTypeSource::Expression(field_value_0) => {
            SemanticOriginalTypeSource::Expression({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtSemanticOriginalTypeSource::Binding(field_value_0) => {
            SemanticOriginalTypeSource::Binding({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtSemanticOriginalTypeSource::Parameter(field_value_0) => {
            SemanticOriginalTypeSource::Parameter({
                lifter.node(depth + 1)?;
                *(field_value_0)
            })
        }
        CtSemanticOriginalTypeSource::Result => SemanticOriginalTypeSource::Result,
        CtSemanticOriginalTypeSource::CallConstCarrier {
            call: field_call,
            formal: field_formal,
        } => SemanticOriginalTypeSource::CallConstCarrier {
            call: {
                lifter.node(depth + 1)?;
                *(field_call)
            },
            formal: {
                lifter.node(depth + 1)?;
                *(field_formal)
            },
        },
        CtSemanticOriginalTypeSource::BindingProjection {
            statement: field_statement,
            pattern: field_pattern,
        } => SemanticOriginalTypeSource::BindingProjection {
            statement: {
                lifter.node(depth + 1)?;
                *(field_statement)
            },
            pattern: {
                lifter.rows((field_pattern).len(), std::mem::size_of::<u32>(), depth + 1)?;
                let mut output = Vec::with_capacity((field_pattern).len());
                for item in field_pattern {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
        },
        CtSemanticOriginalTypeSource::NameAt {
            at: field_at,
            name: field_name,
        } => SemanticOriginalTypeSource::NameAt {
            at: lift_semantic_original_node(field_at, lifter, depth + 1)?,
            name: lifter.text(field_name, depth + 1)?,
        },
    })
}
pub fn lift_semantic_original_namespace<L: CtDynamicLifter>(
    value: &CtSemanticOriginalNamespace,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalNamespace, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalNamespace::Value => SemanticOriginalNamespace::Value,
        CtSemanticOriginalNamespace::Type => SemanticOriginalNamespace::Type,
        CtSemanticOriginalNamespace::Lifetime => SemanticOriginalNamespace::Lifetime,
    })
}
pub fn lift_semantic_original_name_binding<L: CtDynamicLifter>(
    value: &CtSemanticOriginalNameBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalNameBinding, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalNameBinding {
        path: {
            lifter.rows((value.path).len(), std::mem::size_of::<String>(), depth + 1)?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push(lifter.text(item, depth + 1)?);
            }
            output
        },
        namespace: lift_semantic_original_namespace(&value.namespace, lifter, depth + 1)?,
        value: lift_semantic_original_name_value(&value.value, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_original_name_value<L: CtDynamicLifter>(
    value: &CtSemanticOriginalNameValue,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalNameValue, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalNameValue::Recipe(field_value_0) => SemanticOriginalNameValue::Recipe(
            lift_semantic_actual_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticOriginalNameValue::Definition(field_value_0) => {
            SemanticOriginalNameValue::Definition(lift_semantic_definition_id(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
    })
}
pub fn lift_semantic_original_node<L: CtDynamicLifter>(
    value: &CtSemanticOriginalNode,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalNode, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalNode::Expression(field_value_0) => SemanticOriginalNode::Expression({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOriginalNode::Statement(field_value_0) => SemanticOriginalNode::Statement({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_semantic_original_site_node<L: CtDynamicLifter>(
    value: &CtSemanticOriginalSiteNode,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalSiteNode, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalSiteNode {
        site: {
            lifter.node(depth + 1)?;
            value.site
        },
        node: lift_semantic_original_node(&value.node, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_scope_recipe<L: CtDynamicLifter>(
    value: &CtSemanticScopeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticScopeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticScopeRecipe::Trait {
            declaration: field_declaration,
        } => SemanticScopeRecipe::Trait {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
        },
        CtSemanticScopeRecipe::InherentImpl {
            declaration: field_declaration,
            self_type: field_self_type,
        } => SemanticScopeRecipe::InherentImpl {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            self_type: lift_semantic_type_recipe(field_self_type, lifter, depth + 1)?,
        },
        CtSemanticScopeRecipe::TraitImpl {
            declaration: field_declaration,
            self_type: field_self_type,
            trait_application: field_trait_application,
        } => SemanticScopeRecipe::TraitImpl {
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            self_type: lift_semantic_type_recipe(field_self_type, lifter, depth + 1)?,
            trait_application: lift_semantic_trait_recipe(
                field_trait_application,
                lifter,
                depth + 1,
            )?,
        },
        CtSemanticScopeRecipe::BuiltinImpl {
            self_type: field_self_type,
            trait_kind: field_trait_kind,
        } => SemanticScopeRecipe::BuiltinImpl {
            self_type: lift_semantic_type_recipe(field_self_type, lifter, depth + 1)?,
            trait_kind: lift_semantic_builtin_trait(field_trait_kind, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_builtin_trait<L: CtDynamicLifter>(
    value: &CtSemanticBuiltinTrait,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticBuiltinTrait, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticBuiltinTrait::Drop => SemanticBuiltinTrait::Drop,
        CtSemanticBuiltinTrait::Copy => SemanticBuiltinTrait::Copy,
        CtSemanticBuiltinTrait::Value => SemanticBuiltinTrait::Value,
        CtSemanticBuiltinTrait::SyncDrop => SemanticBuiltinTrait::SyncDrop,
        CtSemanticBuiltinTrait::StoreElement => SemanticBuiltinTrait::StoreElement,
        CtSemanticBuiltinTrait::Transfer => SemanticBuiltinTrait::Transfer,
        CtSemanticBuiltinTrait::Share => SemanticBuiltinTrait::Share,
        CtSemanticBuiltinTrait::ThreadCallable => SemanticBuiltinTrait::ThreadCallable,
        CtSemanticBuiltinTrait::ClosedEnvironment => SemanticBuiltinTrait::ClosedEnvironment,
        CtSemanticBuiltinTrait::Send => SemanticBuiltinTrait::Send,
        CtSemanticBuiltinTrait::Sync => SemanticBuiltinTrait::Sync,
    })
}
pub fn lift_semantic_nominal_body<L: CtDynamicLifter>(
    value: &CtSemanticNominalBody,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticNominalBody, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticNominalBody::Product {
            fields: field_fields,
        } => SemanticNominalBody::Product {
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<SemanticFieldRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(lift_semantic_field_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticNominalBody::Enum {
            variants: field_variants,
        } => SemanticNominalBody::Enum {
            variants: {
                lifter.rows(
                    (field_variants).len(),
                    std::mem::size_of::<SemanticVariantRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_variants).len());
                for item in field_variants {
                    output.push(lift_semantic_variant_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
    })
}
pub fn lift_semantic_field_recipe<L: CtDynamicLifter>(
    value: &CtSemanticFieldRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticFieldRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticFieldRecipe {
        name: lifter.text(&value.name, depth + 1)?,
        ty: lift_semantic_type_recipe(&value.ty, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_variant_recipe<L: CtDynamicLifter>(
    value: &CtSemanticVariantRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticVariantRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticVariantRecipe {
        name: lifter.text(&value.name, depth + 1)?,
        fields: {
            lifter.rows(
                (value.fields).len(),
                std::mem::size_of::<SemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.fields).len());
            for item in &value.fields {
                output.push(lift_semantic_type_recipe(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_formal<L: CtDynamicLifter>(
    value: &CtSemanticFormal,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticFormal, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticFormal::Type {
            requirements: field_requirements,
        } => SemanticFormal::Type {
            requirements: {
                lifter.rows(
                    (field_requirements).len(),
                    std::mem::size_of::<SemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_requirements).len());
                for item in field_requirements {
                    output.push(lift_semantic_requirement_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticFormal::Const {
            carrier: field_carrier,
        } => SemanticFormal::Const {
            carrier: lift_semantic_type_recipe(field_carrier, lifter, depth + 1)?,
        },
        CtSemanticFormal::Lifetime => SemanticFormal::Lifetime,
    })
}
pub fn lift_semantic_implicit_lifetime<L: CtDynamicLifter>(
    value: &CtSemanticImplicitLifetime,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticImplicitLifetime, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticImplicitLifetime {
        ordinal: {
            lifter.node(depth + 1)?;
            value.ordinal
        },
        occurrence_path: {
            lifter.rows(
                (value.occurrence_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrence_path).len());
            for item in &value.occurrence_path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn lift_semantic_environment_formal<L: CtDynamicLifter>(
    value: &CtSemanticEnvironmentFormal,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticEnvironmentFormal, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticEnvironmentFormal {
        kind: lift_semantic_formal(&value.kind, lifter, depth + 1)?,
        source: lift_semantic_actual_recipe(&value.source, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_instance<L: CtDynamicLifter>(
    value: &CtSemanticInstance,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticInstance, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticInstance {
        id: lift_semantic_instance_id(&value.id, lifter, depth + 1)?,
        definition: lift_semantic_definition_id(&value.definition, lifter, depth + 1)?,
        target: lift_semantic_instance_target(&value.target, lifter, depth + 1)?,
        lifetime_instantiation: lift_semantic_lifetime_instantiation(
            &value.lifetime_instantiation,
            lifter,
            depth + 1,
        )?,
        parent: match &value.parent {
            Some(item) => Some(lift_semantic_instance_id(item, lifter, depth + 1)?),
            None => None,
        },
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<SemanticActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_semantic_actual(item, lifter, depth + 1)?);
            }
            output
        },
        header_embedding: {
            lifter.rows(
                (value.header_embedding).len(),
                std::mem::size_of::<SemanticHeaderBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.header_embedding).len());
            for item in &value.header_embedding {
                output.push(lift_semantic_header_binding(item, lifter, depth + 1)?);
            }
            output
        },
        environment: match &value.environment {
            Some(item) => Some(lift_semantic_environment_instance(item, lifter, depth + 1)?),
            None => None,
        },
        self_type: match &value.self_type {
            Some(item) => Some(lift_semantic_type_value(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_semantic_lifetime_instantiation<L: CtDynamicLifter>(
    value: &CtSemanticLifetimeInstantiation,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticLifetimeInstantiation, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticLifetimeInstantiation::Rigid => SemanticLifetimeInstantiation::Rigid,
        CtSemanticLifetimeInstantiation::Partition { rigid: field_rigid } => {
            SemanticLifetimeInstantiation::Partition {
                rigid: lift_semantic_instance_id(field_rigid, lifter, depth + 1)?,
            }
        }
    })
}
pub fn lift_semantic_function_target<L: CtDynamicLifter>(
    value: &CtSemanticFunctionTarget,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticFunctionTarget, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticFunctionTarget::Core {
            module: field_module,
            function: field_function,
        } => SemanticFunctionTarget::Core {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            function: {
                lifter.node(depth + 1)?;
                *(field_function)
            },
        },
        CtSemanticFunctionTarget::CoreClosure {
            module: field_module,
            creator: field_creator,
            expression: field_expression,
            occurrence: field_occurrence,
        } => SemanticFunctionTarget::CoreClosure {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            creator: {
                lifter.node(depth + 1)?;
                *(field_creator)
            },
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        CtSemanticFunctionTarget::Typed(field_value_0) => SemanticFunctionTarget::Typed({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_semantic_instance_target<L: CtDynamicLifter>(
    value: &CtSemanticInstanceTarget,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticInstanceTarget, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticInstanceTarget::Function(field_value_0) => SemanticInstanceTarget::Function(
            lift_semantic_function_target(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticInstanceTarget::Scope => SemanticInstanceTarget::Scope,
    })
}
pub fn lift_semantic_closure_target<L: CtDynamicLifter>(
    value: &CtSemanticClosureTarget,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticClosureTarget, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticClosureTarget::Core {
            module: field_module,
            creator: field_creator,
            expression: field_expression,
            occurrence: field_occurrence,
        } => SemanticClosureTarget::Core {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            creator: {
                lifter.node(depth + 1)?;
                *(field_creator)
            },
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        CtSemanticClosureTarget::Typed(field_value_0) => SemanticClosureTarget::Typed({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_semantic_header_binding<L: CtDynamicLifter>(
    value: &CtSemanticHeaderBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticHeaderBinding, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticHeaderBinding {
        header: {
            lifter.node(depth + 1)?;
            value.header
        },
        actual: lift_semantic_class_expr(&value.actual, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_environment_instance<L: CtDynamicLifter>(
    value: &CtSemanticEnvironmentInstance,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticEnvironmentInstance, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticEnvironmentInstance {
        closure: lift_semantic_closure_target(&value.closure, lifter, depth + 1)?,
        actuals: {
            lifter.rows(
                (value.actuals).len(),
                std::mem::size_of::<SemanticActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.actuals).len());
            for item in &value.actuals {
                output.push(lift_semantic_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_actual<L: CtDynamicLifter>(
    value: &CtSemanticActual,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticActual, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticActual::Type(field_value_0) => {
            SemanticActual::Type(lift_semantic_type_value(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticActual::Const {
            ty: field_ty,
            value: field_value,
        } => SemanticActual::Const {
            ty: lift_semantic_type_value(field_ty, lifter, depth + 1)?,
            value: lifter.constant(field_value, depth + 1)?,
        },
        CtSemanticActual::Life(field_value_0) => {
            SemanticActual::Life(lift_semantic_class_expr(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticActual::CoreRecipe {
            scope: field_scope,
            recipe: field_recipe,
        } => SemanticActual::CoreRecipe {
            scope: lift_semantic_instance_id(field_scope, lifter, depth + 1)?,
            recipe: {
                lifter.rows(1, std::mem::size_of::<SemanticActualRecipe>(), depth + 1)?;
                Box::new(lift_semantic_actual_recipe(
                    (field_recipe).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticActual::CoreInferred {
            scope: field_scope,
            call: field_call,
            formal: field_formal,
        } => SemanticActual::CoreInferred {
            scope: lift_semantic_instance_id(field_scope, lifter, depth + 1)?,
            call: {
                lifter.node(depth + 1)?;
                *(field_call)
            },
            formal: {
                lifter.node(depth + 1)?;
                *(field_formal)
            },
        },
    })
}
pub fn lift_semantic_core_type_source<L: CtDynamicLifter>(
    value: &CtSemanticCoreTypeSource,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticCoreTypeSource, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticCoreTypeSource::OriginalRecipe(field_value_0) => {
            SemanticCoreTypeSource::OriginalRecipe({
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_value_0).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            })
        }
        CtSemanticCoreTypeSource::Expression {
            module: field_module,
            function: field_function,
            expression: field_expression,
            occurrence: field_occurrence,
            recipe: field_recipe,
        } => SemanticCoreTypeSource::Expression {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            function: {
                lifter.node(depth + 1)?;
                *(field_function)
            },
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
            recipe: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_recipe).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticCoreTypeSource::OriginalExpression {
            module: field_module,
            function: field_function,
            expression: field_expression,
            occurrence: field_occurrence,
            source: field_source,
        } => SemanticCoreTypeSource::OriginalExpression {
            module: {
                lifter.node(depth + 1)?;
                *(field_module)
            },
            function: {
                lifter.node(depth + 1)?;
                *(field_function)
            },
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
            source: lift_semantic_original_type_source(field_source, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_type_value<L: CtDynamicLifter>(
    value: &CtSemanticTypeValue,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticTypeValue, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticTypeValue::Resolved {
            ty: field_ty,
            life: field_life,
            classes: field_classes,
        } => SemanticTypeValue::Resolved {
            ty: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_ty), depth + 1)?
            },
            life: lift_storage_life_shape(field_life, lifter, depth + 1)?,
            classes: {
                lifter.rows(
                    (field_classes).len(),
                    std::mem::size_of::<SemanticClassExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_classes).len());
                for item in field_classes {
                    output.push(lift_semantic_class_expr(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticTypeValue::Core {
            scope: field_scope,
            source: field_source,
        } => SemanticTypeValue::Core {
            scope: lift_semantic_instance_id(field_scope, lifter, depth + 1)?,
            source: lift_semantic_core_type_source(field_source, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_scope_ref<L: CtDynamicLifter>(
    value: &CtSemanticScopeRef,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticScopeRef, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticScopeRef::Ancestor(field_value_0) => SemanticScopeRef::Ancestor({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_semantic_class_expr<L: CtDynamicLifter>(
    value: &CtSemanticClassExpr,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticClassExpr, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticClassExpr::Formal {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticClassExpr::Formal {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticClassExpr::Implicit {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticClassExpr::Implicit {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticClassExpr::Bound {
            depth: field_depth,
            index: field_index,
        } => SemanticClassExpr::Bound {
            depth: {
                lifter.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                lifter.node(depth + 1)?;
                *(field_index)
            },
        },
        CtSemanticClassExpr::OuterOccurrence {
            operand: field_operand,
            path: field_path,
            mode: field_mode,
        } => SemanticClassExpr::OuterOccurrence {
            operand: lift_semantic_operand(field_operand, lifter, depth + 1)?,
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtSemanticClassExpr::IntrinsicClass {
            operand: field_operand,
            path: field_path,
            class: field_class,
        } => SemanticClassExpr::IntrinsicClass {
            operand: lift_semantic_operand(field_operand, lifter, depth + 1)?,
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            class: {
                lifter.node(depth + 1)?;
                *(field_class)
            },
        },
        CtSemanticClassExpr::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticClassExpr::Environment {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
    })
}
pub fn lift_semantic_operand<L: CtDynamicLifter>(
    value: &CtSemanticOperand,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOperand, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOperand::Parameter(field_value_0) => SemanticOperand::Parameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOperand::Capture(field_value_0) => SemanticOperand::Capture({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOperand::Receiver => SemanticOperand::Receiver,
        CtSemanticOperand::Value(field_value_0) => SemanticOperand::Value({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOperand::Local(field_value_0) => SemanticOperand::Local({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOperand::TypeArgument {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticOperand::TypeArgument {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticOperand::Environment(field_value_0) => SemanticOperand::Environment({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOperand::CoreExpression {
            expression: field_expression,
            occurrence: field_occurrence,
        } => SemanticOperand::CoreExpression {
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
        },
    })
}
pub fn lift_semantic_site_recipe<L: CtDynamicLifter>(
    value: &CtSemanticSiteRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSiteRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticSiteRecipe {
        id: {
            lifter.node(depth + 1)?;
            value.id
        },
        occurrence_path: {
            lifter.rows(
                (value.occurrence_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.occurrence_path).len());
            for item in &value.occurrence_path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        kind: lift_semantic_site_recipe_kind(&value.kind, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_site_recipe_kind<L: CtDynamicLifter>(
    value: &CtSemanticSiteRecipeKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSiteRecipeKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticSiteRecipeKind::Construct {
            result: field_result,
        } => SemanticSiteRecipeKind::Construct {
            result: lift_semantic_type_recipe(field_result, lifter, depth + 1)?,
        },
        CtSemanticSiteRecipeKind::Call {
            target: field_target,
            actuals: field_actuals,
        } => SemanticSiteRecipeKind::Call {
            target: lift_semantic_definition_id(field_target, lifter, depth + 1)?,
            actuals: {
                lifter.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticSiteRecipeKind::CallableCall {
            callee: field_callee,
        } => SemanticSiteRecipeKind::CallableCall {
            callee: lift_semantic_type_recipe(field_callee, lifter, depth + 1)?,
        },
        CtSemanticSiteRecipeKind::TypeUse {
            expected: field_expected,
        } => SemanticSiteRecipeKind::TypeUse {
            expected: lift_semantic_type_recipe(field_expected, lifter, depth + 1)?,
        },
        CtSemanticSiteRecipeKind::DeclareLocal {
            expected: field_expected,
        } => SemanticSiteRecipeKind::DeclareLocal {
            expected: lift_semantic_type_recipe(field_expected, lifter, depth + 1)?,
        },
        CtSemanticSiteRecipeKind::Closure {
            definition: field_definition,
            environment: field_environment,
        } => SemanticSiteRecipeKind::Closure {
            definition: lift_semantic_definition_id(field_definition, lifter, depth + 1)?,
            environment: {
                lifter.rows(
                    (field_environment).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticSiteRecipeKind::OriginalCall => SemanticSiteRecipeKind::OriginalCall,
    })
}
pub fn lift_semantic_site<L: CtDynamicLifter>(
    value: &CtSemanticSite,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSite, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticSite {
        instance: lift_semantic_instance_id(&value.instance, lifter, depth + 1)?,
        recipe: {
            lifter.node(depth + 1)?;
            value.recipe
        },
        at: lift_semantic_site_address(&value.at, lifter, depth + 1)?,
        action: lift_semantic_site_action(&value.action, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_site_address<L: CtDynamicLifter>(
    value: &CtSemanticSiteAddress,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSiteAddress, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticSiteAddress::Core {
            expression: field_expression,
            occurrence: field_occurrence,
            role: field_role,
        } => SemanticSiteAddress::Core {
            expression: {
                lifter.node(depth + 1)?;
                *(field_expression)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
            role: lift_semantic_emission_role(field_role, lifter, depth + 1)?,
        },
        CtSemanticSiteAddress::Typed {
            block: field_block,
            instruction: field_instruction,
            edge: field_edge,
        } => SemanticSiteAddress::Typed {
            block: {
                lifter.node(depth + 1)?;
                *(field_block)
            },
            instruction: {
                lifter.node(depth + 1)?;
                *(field_instruction)
            },
            edge: lift_semantic_site_edge(field_edge, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_emission_role<L: CtDynamicLifter>(
    value: &CtSemanticEmissionRole,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticEmissionRole, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticEmissionRole::Construct => SemanticEmissionRole::Construct,
        CtSemanticEmissionRole::Call => SemanticEmissionRole::Call,
        CtSemanticEmissionRole::Ascription => SemanticEmissionRole::Ascription,
        CtSemanticEmissionRole::Binding => SemanticEmissionRole::Binding,
        CtSemanticEmissionRole::Closure => SemanticEmissionRole::Closure,
    })
}
pub fn lift_semantic_site_edge<L: CtDynamicLifter>(
    value: &CtSemanticSiteEdge,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSiteEdge, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticSiteEdge::Before => SemanticSiteEdge::Before,
        CtSemanticSiteEdge::After => SemanticSiteEdge::After,
    })
}
pub fn lift_semantic_site_action<L: CtDynamicLifter>(
    value: &CtSemanticSiteAction,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSiteAction, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticSiteAction::Construct {
            result: field_result,
            value: field_value,
        } => SemanticSiteAction::Construct {
            result: lift_semantic_operand(field_result, lifter, depth + 1)?,
            value: lift_semantic_type_value(field_value, lifter, depth + 1)?,
        },
        CtSemanticSiteAction::Call {
            target: field_target,
            actuals: field_actuals,
        } => SemanticSiteAction::Call {
            target: lift_semantic_function_target(field_target, lifter, depth + 1)?,
            actuals: {
                lifter.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<SemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(lift_semantic_actual(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticSiteAction::CallableCall {
            callee: field_callee,
        } => SemanticSiteAction::CallableCall {
            callee: lift_semantic_operand(field_callee, lifter, depth + 1)?,
        },
        CtSemanticSiteAction::TypeUse {
            subject: field_subject,
            expected: field_expected,
        } => SemanticSiteAction::TypeUse {
            subject: lift_semantic_operand(field_subject, lifter, depth + 1)?,
            expected: lift_semantic_type_value(field_expected, lifter, depth + 1)?,
        },
        CtSemanticSiteAction::DeclareLocal {
            local: field_local,
            expected: field_expected,
        } => SemanticSiteAction::DeclareLocal {
            local: lift_semantic_local_target(field_local, lifter, depth + 1)?,
            expected: lift_semantic_type_value(field_expected, lifter, depth + 1)?,
        },
        CtSemanticSiteAction::Closure {
            result: field_result,
            closure: field_closure,
            environment: field_environment,
        } => SemanticSiteAction::Closure {
            result: lift_semantic_operand(field_result, lifter, depth + 1)?,
            closure: lift_semantic_closure_target(field_closure, lifter, depth + 1)?,
            environment: {
                lifter.rows(
                    (field_environment).len(),
                    std::mem::size_of::<SemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(lift_semantic_actual(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticSiteAction::CoreBodyCall => SemanticSiteAction::CoreBodyCall,
        CtSemanticSiteAction::BodyCall {
            actuals: field_actuals,
        } => SemanticSiteAction::BodyCall {
            actuals: {
                lifter.rows(
                    (field_actuals).len(),
                    std::mem::size_of::<SemanticActual>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_actuals).len());
                for item in field_actuals {
                    output.push(lift_semantic_actual(item, lifter, depth + 1)?);
                }
                output
            },
        },
    })
}
pub fn lift_semantic_local_target<L: CtDynamicLifter>(
    value: &CtSemanticLocalTarget,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticLocalTarget, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticLocalTarget::Core {
            statement: field_statement,
            occurrence: field_occurrence,
        } => SemanticLocalTarget::Core {
            statement: {
                lifter.node(depth + 1)?;
                *(field_statement)
            },
            occurrence: {
                lifter.node(depth + 1)?;
                *(field_occurrence)
            },
        },
        CtSemanticLocalTarget::Typed(field_value_0) => SemanticLocalTarget::Typed({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_semantic_recipe_operand<L: CtDynamicLifter>(
    value: &CtSemanticRecipeOperand,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticRecipeOperand, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticRecipeOperand::TypeFormal(field_value_0) => SemanticRecipeOperand::TypeFormal(
            lift_semantic_formal_ref(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticRecipeOperand::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticRecipeOperand::Environment {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticRecipeOperand::Parameter(field_value_0) => SemanticRecipeOperand::Parameter({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticRecipeOperand::Receiver => SemanticRecipeOperand::Receiver,
    })
}
pub fn lift_semantic_formal_ref<L: CtDynamicLifter>(
    value: &CtSemanticFormalRef,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticFormalRef, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticFormalRef {
        scope: lift_semantic_scope_ref(&value.scope, lifter, depth + 1)?,
        ordinal: {
            lifter.node(depth + 1)?;
            value.ordinal
        },
    })
}
pub fn lift_semantic_life_recipe<L: CtDynamicLifter>(
    value: &CtSemanticLifeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticLifeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticLifeRecipe::Formal(field_value_0) => {
            SemanticLifeRecipe::Formal(lift_semantic_formal_ref(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticLifeRecipe::Implicit {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticLifeRecipe::Implicit {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticLifeRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticLifeRecipe::Environment {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticLifeRecipe::Bound {
            depth: field_depth,
            index: field_index,
        } => SemanticLifeRecipe::Bound {
            depth: {
                lifter.node(depth + 1)?;
                *(field_depth)
            },
            index: {
                lifter.node(depth + 1)?;
                *(field_index)
            },
        },
        CtSemanticLifeRecipe::Invocation(field_value_0) => SemanticLifeRecipe::Invocation({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticLifeRecipe::OuterOccurrence {
            operand: field_operand,
            path: field_path,
            mode: field_mode,
        } => SemanticLifeRecipe::OuterOccurrence {
            operand: lift_semantic_recipe_operand(field_operand, lifter, depth + 1)?,
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtSemanticLifeRecipe::IntrinsicClass {
            operand: field_operand,
            path: field_path,
            class: field_class,
        } => SemanticLifeRecipe::IntrinsicClass {
            operand: lift_semantic_recipe_operand(field_operand, lifter, depth + 1)?,
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            class: {
                lifter.node(depth + 1)?;
                *(field_class)
            },
        },
    })
}
pub fn lift_semantic_outer_life_recipe<L: CtDynamicLifter>(
    value: &CtSemanticOuterLifeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOuterLifeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOuterLifeRecipe::Class(field_value_0) => SemanticOuterLifeRecipe::Class(
            lift_semantic_life_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticOuterLifeRecipe::Static => SemanticOuterLifeRecipe::Static,
    })
}
pub fn lift_semantic_type_recipe<L: CtDynamicLifter>(
    value: &CtSemanticTypeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticTypeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticTypeRecipe::Atom(field_value_0) => SemanticTypeRecipe::Atom({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
        CtSemanticTypeRecipe::Exact(field_value_0) => SemanticTypeRecipe::Exact({
            lifter.rows(1, std::mem::size_of::<SemanticExactTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_exact_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::Formal(field_value_0) => {
            SemanticTypeRecipe::Formal(lift_semantic_formal_ref(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticTypeRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticTypeRecipe::Environment {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticTypeRecipe::SelfType { scope: field_scope } => SemanticTypeRecipe::SelfType {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
        },
        CtSemanticTypeRecipe::Nominal {
            definition: field_definition,
            arguments: field_arguments,
        } => SemanticTypeRecipe::Nominal {
            definition: lift_semantic_definition_id(field_definition, lifter, depth + 1)?,
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticTypeRecipe::AssociatedType {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => SemanticTypeRecipe::AssociatedType {
            receiver: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_receiver).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            application: {
                lifter.rows(1, std::mem::size_of::<SemanticTraitRecipe>(), depth + 1)?;
                Box::new(lift_semantic_trait_recipe(
                    (field_application).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
        CtSemanticTypeRecipe::Tuple(field_value_0) => SemanticTypeRecipe::Tuple({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<SemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_semantic_type_recipe(item, lifter, depth + 1)?);
            }
            output
        }),
        CtSemanticTypeRecipe::Array {
            element: field_element,
            length: field_length,
        } => SemanticTypeRecipe::Array {
            element: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_element).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            length: {
                lifter.rows(1, std::mem::size_of::<SemanticConstRecipe>(), depth + 1)?;
                Box::new(lift_semantic_const_recipe(
                    (field_length).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Slice(field_value_0) => SemanticTypeRecipe::Slice({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::Address {
            mutable: field_mutable,
            lifetime: field_lifetime,
            inner: field_inner,
        } => SemanticTypeRecipe::Address {
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            lifetime: lift_semantic_outer_life_recipe(field_lifetime, lifter, depth + 1)?,
            inner: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_inner).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Desc {
            lifetime: field_lifetime,
            inner: field_inner,
        } => SemanticTypeRecipe::Desc {
            lifetime: lift_semantic_outer_life_recipe(field_lifetime, lifter, depth + 1)?,
            inner: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_inner).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Managed(field_value_0) => SemanticTypeRecipe::Managed({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::OwnedStore(field_value_0) => SemanticTypeRecipe::OwnedStore({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::Mutex(field_value_0) => SemanticTypeRecipe::Mutex({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::MutexGuard {
            payload: field_payload,
            retained: field_retained,
        } => SemanticTypeRecipe::MutexGuard {
            payload: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_payload).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            retained: {
                lifter.rows(
                    1,
                    std::mem::size_of::<SemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(lift_semantic_retained_shape_recipe(
                    (field_retained).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Registration(field_value_0) => SemanticTypeRecipe::Registration({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticTypeRecipe::AsyncRegistration(field_value_0) => {
            SemanticTypeRecipe::AsyncRegistration({
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_value_0).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            })
        }
        CtSemanticTypeRecipe::Task {
            result: field_result,
            retained: field_retained,
        } => SemanticTypeRecipe::Task {
            result: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_result).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            retained: {
                lifter.rows(
                    1,
                    std::mem::size_of::<SemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(lift_semantic_retained_shape_recipe(
                    (field_retained).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::ErrorUnion {
            error: field_error,
            result: field_result,
        } => SemanticTypeRecipe::ErrorUnion {
            error: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_error).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            result: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_result).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Qualified {
            storage: field_storage,
            life: field_life,
        } => SemanticTypeRecipe::Qualified {
            storage: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_storage).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            life: {
                lifter.rows(
                    1,
                    std::mem::size_of::<SemanticStorageShapeRecipe>(),
                    depth + 1,
                )?;
                Box::new(lift_semantic_storage_shape_recipe(
                    (field_life).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Code {
            kind: field_kind,
            signature: field_signature,
        } => SemanticTypeRecipe::Code {
            kind: lift_semantic_code_kind(field_kind, lifter, depth + 1)?,
            signature: {
                lifter.rows(1, std::mem::size_of::<SemanticSignatureRecipe>(), depth + 1)?;
                Box::new(lift_semantic_signature_recipe(
                    (field_signature).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Closure {
            definition: field_definition,
            arguments: field_arguments,
            environment: field_environment,
        } => SemanticTypeRecipe::Closure {
            definition: lift_semantic_definition_id(field_definition, lifter, depth + 1)?,
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
            environment: {
                lifter.rows(
                    (field_environment).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_environment).len());
                for item in field_environment {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticTypeRecipe::Dyn {
            owned: field_owned,
            interface: field_interface,
        } => SemanticTypeRecipe::Dyn {
            owned: {
                lifter.node(depth + 1)?;
                *(field_owned)
            },
            interface: {
                lifter.rows(
                    1,
                    std::mem::size_of::<SemanticDynInterfaceRecipe>(),
                    depth + 1,
                )?;
                Box::new(lift_semantic_dyn_interface_recipe(
                    (field_interface).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::Opaque {
            definition: field_definition,
            arguments: field_arguments,
            hidden: field_hidden,
        } => SemanticTypeRecipe::Opaque {
            definition: lift_semantic_definition_id(field_definition, lifter, depth + 1)?,
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticActualRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
                }
                output
            },
            hidden: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_hidden).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticTypeRecipe::AnonymousSum(field_value_0) => SemanticTypeRecipe::AnonymousSum({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<SemanticTypeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_semantic_type_recipe(item, lifter, depth + 1)?);
            }
            output
        }),
        CtSemanticTypeRecipe::UnresolvedNamed(field_value_0) => {
            SemanticTypeRecipe::UnresolvedNamed(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticTypeRecipe::Unsupported(field_value_0) => {
            SemanticTypeRecipe::Unsupported(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticTypeRecipe::OriginalType {
            scope: field_scope,
            source: field_source,
        } => SemanticTypeRecipe::OriginalType {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            source: lift_semantic_original_type_source(field_source, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_exact_type_recipe<L: CtDynamicLifter>(
    value: &CtSemanticExactTypeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticExactTypeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticExactTypeRecipe {
        ty: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.ty), depth + 1)?
        },
        life: lift_storage_life_shape(&value.life, lifter, depth + 1)?,
        classes: {
            lifter.rows(
                (value.classes).len(),
                std::mem::size_of::<SemanticLifeRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push(lift_semantic_life_recipe(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_code_kind<L: CtDynamicLifter>(
    value: &CtSemanticCodeKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticCodeKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticCodeKind::FunctionPointer => SemanticCodeKind::FunctionPointer,
        CtSemanticCodeKind::BorrowedFunctionPointer => SemanticCodeKind::BorrowedFunctionPointer,
        CtSemanticCodeKind::ForeignCallback => SemanticCodeKind::ForeignCallback,
    })
}
pub fn lift_semantic_actual_recipe<L: CtDynamicLifter>(
    value: &CtSemanticActualRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticActualRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticActualRecipe::Type(field_value_0) => {
            SemanticActualRecipe::Type(lift_semantic_type_recipe(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticActualRecipe::Const {
            carrier: field_carrier,
            value: field_value,
        } => SemanticActualRecipe::Const {
            carrier: lift_semantic_type_recipe(field_carrier, lifter, depth + 1)?,
            value: lift_semantic_const_recipe(field_value, lifter, depth + 1)?,
        },
        CtSemanticActualRecipe::Life(field_value_0) => {
            SemanticActualRecipe::Life(lift_semantic_life_recipe(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticActualRecipe::Infer => SemanticActualRecipe::Infer,
    })
}
pub fn lift_semantic_class_sequence_recipe<L: CtDynamicLifter>(
    value: &CtSemanticClassSequenceRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticClassSequenceRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticClassSequenceRecipe {
        parts: {
            lifter.rows(
                (value.parts).len(),
                std::mem::size_of::<SemanticClassSequencePart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parts).len());
            for item in &value.parts {
                output.push(lift_semantic_class_sequence_part(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_class_sequence_part<L: CtDynamicLifter>(
    value: &CtSemanticClassSequencePart,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticClassSequencePart, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticClassSequencePart::One(field_value_0) => SemanticClassSequencePart::One(
            lift_semantic_life_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticClassSequencePart::AllClasses {
            ty: field_ty,
            path: field_path,
        } => SemanticClassSequencePart::AllClasses {
            ty: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
        },
    })
}
pub fn lift_semantic_storage_shape_recipe<L: CtDynamicLifter>(
    value: &CtSemanticStorageShapeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticStorageShapeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticStorageShapeRecipe {
        domain: lift_semantic_class_sequence_recipe(&value.domain, lifter, depth + 1)?,
        slots: {
            lifter.rows(
                (value.slots).len(),
                std::mem::size_of::<SemanticSlotRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(lift_semantic_slot_recipe(item, lifter, depth + 1)?);
            }
            output
        },
        retained: {
            lifter.rows(
                (value.retained).len(),
                std::mem::size_of::<SemanticRetainedPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(lift_semantic_retained_part(item, lifter, depth + 1)?);
            }
            output
        },
        nominal: {
            lifter.rows(
                (value.nominal).len(),
                std::mem::size_of::<SemanticNominalPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal).len());
            for item in &value.nominal {
                output.push(lift_semantic_nominal_part(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_slot_recipe<L: CtDynamicLifter>(
    value: &CtSemanticSlotRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSlotRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticSlotRecipe {
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<PlaceProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
        class: lift_semantic_life_recipe(&value.class, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_retained_part<L: CtDynamicLifter>(
    value: &CtSemanticRetainedPart,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticRetainedPart, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticRetainedPart::One {
            path: field_path,
            child_class: field_child_class,
            parent: field_parent,
        } => SemanticRetainedPart::One {
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            child_class: {
                lifter.node(depth + 1)?;
                *(field_child_class)
            },
            parent: lift_semantic_life_recipe(field_parent, lifter, depth + 1)?,
        },
        CtSemanticRetainedPart::All {
            path: field_path,
            parents: field_parents,
        } => SemanticRetainedPart::All {
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            parents: lift_semantic_class_sequence_recipe(field_parents, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_nominal_part<L: CtDynamicLifter>(
    value: &CtSemanticNominalPart,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticNominalPart, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticNominalPart::One {
            path: field_path,
            declaration: field_declaration,
            argument: field_argument,
            class: field_class,
        } => SemanticNominalPart::One {
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            declaration: lift_life_declaration_id(field_declaration, lifter, depth + 1)?,
            argument: {
                lifter.node(depth + 1)?;
                *(field_argument)
            },
            class: lift_semantic_life_recipe(field_class, lifter, depth + 1)?,
        },
        CtSemanticNominalPart::FromType {
            path: field_path,
            ty: field_ty,
            classes: field_classes,
        } => SemanticNominalPart::FromType {
            path: {
                lifter.rows(
                    (field_path).len(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push({
                        lifter.node(depth + 1)?;
                        *(item)
                    });
                }
                output
            },
            ty: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            classes: lift_semantic_class_sequence_recipe(field_classes, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_retained_shape_recipe<L: CtDynamicLifter>(
    value: &CtSemanticRetainedShapeRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticRetainedShapeRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticRetainedShapeRecipe {
        domain: lift_semantic_class_sequence_recipe(&value.domain, lifter, depth + 1)?,
        ports: {
            lifter.rows(
                (value.ports).len(),
                std::mem::size_of::<SemanticPortPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(lift_semantic_port_part(item, lifter, depth + 1)?);
            }
            output
        },
        retained: {
            lifter.rows(
                (value.retained).len(),
                std::mem::size_of::<SemanticRetainedPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(lift_semantic_retained_part(item, lifter, depth + 1)?);
            }
            output
        },
        nominal: {
            lifter.rows(
                (value.nominal).len(),
                std::mem::size_of::<SemanticNominalPart>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal).len());
            for item in &value.nominal {
                output.push(lift_semantic_nominal_part(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_port_part<L: CtDynamicLifter>(
    value: &CtSemanticPortPart,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticPortPart, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticPortPart::One {
            leaf: field_leaf,
            mode: field_mode,
            class: field_class,
        } => SemanticPortPart::One {
            leaf: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_leaf).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
            class: lift_semantic_life_recipe(field_class, lifter, depth + 1)?,
        },
        CtSemanticPortPart::FromType {
            ty: field_ty,
            classes: field_classes,
        } => SemanticPortPart::FromType {
            ty: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_ty).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            classes: lift_semantic_class_sequence_recipe(field_classes, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_const_recipe<L: CtDynamicLifter>(
    value: &CtSemanticConstRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticConstRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticConstRecipe::Value(field_value_0) => {
            SemanticConstRecipe::Value(lifter.constant(field_value_0, depth + 1)?)
        }
        CtSemanticConstRecipe::IntegerLiteral(field_value_0) => {
            SemanticConstRecipe::IntegerLiteral(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticConstRecipe::FloatLiteral(field_value_0) => {
            SemanticConstRecipe::FloatLiteral(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticConstRecipe::Formal(field_value_0) => {
            SemanticConstRecipe::Formal(lift_semantic_formal_ref(field_value_0, lifter, depth + 1)?)
        }
        CtSemanticConstRecipe::Environment {
            scope: field_scope,
            ordinal: field_ordinal,
        } => SemanticConstRecipe::Environment {
            scope: lift_semantic_scope_ref(field_scope, lifter, depth + 1)?,
            ordinal: {
                lifter.node(depth + 1)?;
                *(field_ordinal)
            },
        },
        CtSemanticConstRecipe::Unary {
            op: field_op,
            operand_type: field_operand_type,
            value: field_value,
        } => SemanticConstRecipe::Unary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            operand_type: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_operand_type).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticConstRecipe>(), depth + 1)?;
                Box::new(lift_semantic_const_recipe(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticConstRecipe::Binary {
            op: field_op,
            operand_type: field_operand_type,
            left: field_left,
            right: field_right,
        } => SemanticConstRecipe::Binary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            operand_type: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_operand_type).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            left: {
                lifter.rows(1, std::mem::size_of::<SemanticConstRecipe>(), depth + 1)?;
                Box::new(lift_semantic_const_recipe(
                    (field_left).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            right: {
                lifter.rows(1, std::mem::size_of::<SemanticConstRecipe>(), depth + 1)?;
                Box::new(lift_semantic_const_recipe(
                    (field_right).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticConstRecipe::Convert {
            source_type: field_source_type,
            target_type: field_target_type,
            value: field_value,
        } => SemanticConstRecipe::Convert {
            source_type: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_source_type).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            target_type: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_target_type).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticConstRecipe>(), depth + 1)?;
                Box::new(lift_semantic_const_recipe(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticConstRecipe::AssociatedConst {
            receiver: field_receiver,
            application: field_application,
            member: field_member,
        } => SemanticConstRecipe::AssociatedConst {
            receiver: {
                lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
                Box::new(lift_semantic_type_recipe(
                    (field_receiver).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            application: {
                lifter.rows(1, std::mem::size_of::<SemanticTraitRecipe>(), depth + 1)?;
                Box::new(lift_semantic_trait_recipe(
                    (field_application).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            member: {
                lifter.node(depth + 1)?;
                *(field_member)
            },
        },
    })
}
pub fn lift_semantic_trait_recipe<L: CtDynamicLifter>(
    value: &CtSemanticTraitRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticTraitRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticTraitRecipe {
        declaration: lift_life_declaration_id(&value.declaration, lifter, depth + 1)?,
        arguments: {
            lifter.rows(
                (value.arguments).len(),
                std::mem::size_of::<SemanticActualRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(lift_semantic_actual_recipe(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_semantic_requirement_recipe<L: CtDynamicLifter>(
    value: &CtSemanticRequirementRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticRequirementRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticRequirementRecipe::Structural {
            ty: field_ty,
            bound: field_bound,
        } => SemanticRequirementRecipe::Structural {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            bound: lift_dyn_structural_bound(field_bound, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::Trait {
            ty: field_ty,
            application: field_application,
        } => SemanticRequirementRecipe::Trait {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            application: lift_semantic_trait_recipe(field_application, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::Callable {
            ty: field_ty,
            kind: field_kind,
            signature: field_signature,
        } => SemanticRequirementRecipe::Callable {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            kind: {
                lifter.node(depth + 1)?;
                *(field_kind)
            },
            signature: {
                lifter.rows(1, std::mem::size_of::<SemanticSignatureRecipe>(), depth + 1)?;
                Box::new(lift_semantic_signature_recipe(
                    (field_signature).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticRequirementRecipe::TypeEqual {
            left: field_left,
            right: field_right,
        } => SemanticRequirementRecipe::TypeEqual {
            left: lift_semantic_type_recipe(field_left, lifter, depth + 1)?,
            right: lift_semantic_type_recipe(field_right, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::ConstEqual {
            carrier: field_carrier,
            left: field_left,
            right: field_right,
        } => SemanticRequirementRecipe::ConstEqual {
            carrier: lift_semantic_type_recipe(field_carrier, lifter, depth + 1)?,
            left: lift_semantic_const_recipe(field_left, lifter, depth + 1)?,
            right: lift_semantic_const_recipe(field_right, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::LifeEqual {
            left: field_left,
            right: field_right,
        } => SemanticRequirementRecipe::LifeEqual {
            left: lift_semantic_life_recipe(field_left, lifter, depth + 1)?,
            right: lift_semantic_life_recipe(field_right, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::Outlives {
            longer: field_longer,
            shorter: field_shorter,
        } => SemanticRequirementRecipe::Outlives {
            longer: lift_semantic_life_recipe(field_longer, lifter, depth + 1)?,
            shorter: lift_semantic_life_recipe(field_shorter, lifter, depth + 1)?,
        },
        CtSemanticRequirementRecipe::ForAll {
            lifetimes: field_lifetimes,
            requirements: field_requirements,
        } => SemanticRequirementRecipe::ForAll {
            lifetimes: {
                lifter.node(depth + 1)?;
                *(field_lifetimes)
            },
            requirements: {
                lifter.rows(
                    (field_requirements).len(),
                    std::mem::size_of::<SemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_requirements).len());
                for item in field_requirements {
                    output.push(lift_semantic_requirement_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticRequirementRecipe::Builtin {
            ty: field_ty,
            trait_kind: field_trait_kind,
        } => SemanticRequirementRecipe::Builtin {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            trait_kind: lift_semantic_builtin_trait(field_trait_kind, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_signature_recipe<L: CtDynamicLifter>(
    value: &CtSemanticSignatureRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticSignatureRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticSignatureRecipe {
        bound_lifetimes: {
            lifter.node(depth + 1)?;
            value.bound_lifetimes
        },
        invocation_lifetimes: {
            lifter.node(depth + 1)?;
            value.invocation_lifetimes
        },
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<SemanticParameterRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(lift_semantic_parameter_recipe(item, lifter, depth + 1)?);
            }
            output
        },
        result: lift_semantic_type_recipe(&value.result, lifter, depth + 1)?,
        result_mode: {
            lifter.node(depth + 1)?;
            value.result_mode
        },
        call_kind: {
            lifter.node(depth + 1)?;
            value.call_kind
        },
    })
}
pub fn lift_semantic_parameter_recipe<L: CtDynamicLifter>(
    value: &CtSemanticParameterRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticParameterRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticParameterRecipe {
        ty: lift_semantic_type_recipe(&value.ty, lifter, depth + 1)?,
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn lift_semantic_dyn_context_recipe<L: CtDynamicLifter>(
    value: &CtSemanticDynContextRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDynContextRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticDynContextRecipe::Closed => SemanticDynContextRecipe::Closed,
        CtSemanticDynContextRecipe::Type(field_value_0) => SemanticDynContextRecipe::Type({
            lifter.rows(1, std::mem::size_of::<SemanticTypeRecipe>(), depth + 1)?;
            Box::new(lift_semantic_type_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticDynContextRecipe::Shape(field_value_0) => SemanticDynContextRecipe::Shape({
            lifter.rows(
                1,
                std::mem::size_of::<SemanticRetainedShapeRecipe>(),
                depth + 1,
            )?;
            Box::new(lift_semantic_retained_shape_recipe(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
    })
}
pub fn lift_semantic_dyn_interface_recipe<L: CtDynamicLifter>(
    value: &CtSemanticDynInterfaceRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticDynInterfaceRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticDynInterfaceRecipe {
        application: lift_semantic_trait_recipe(&value.application, lifter, depth + 1)?,
        context: lift_semantic_dyn_context_recipe(&value.context, lifter, depth + 1)?,
        associated: {
            lifter.rows(
                (value.associated).len(),
                std::mem::size_of::<SemanticAssociatedBindingRecipe>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.associated).len());
            for item in &value.associated {
                output.push(lift_semantic_associated_binding_recipe(
                    item,
                    lifter,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn lift_semantic_associated_binding_recipe<L: CtDynamicLifter>(
    value: &CtSemanticAssociatedBindingRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticAssociatedBindingRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticAssociatedBindingRecipe {
        application: lift_semantic_trait_recipe(&value.application, lifter, depth + 1)?,
        member: {
            lifter.node(depth + 1)?;
            value.member
        },
        value: lift_semantic_associated_recipe(&value.value, lifter, depth + 1)?,
    })
}
pub fn lift_semantic_associated_recipe<L: CtDynamicLifter>(
    value: &CtSemanticAssociatedRecipe,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticAssociatedRecipe, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticAssociatedRecipe::Type(field_value_0) => SemanticAssociatedRecipe::Type(
            lift_semantic_type_recipe(field_value_0, lifter, depth + 1)?,
        ),
        CtSemanticAssociatedRecipe::Const {
            carrier: field_carrier,
            value: field_value,
        } => SemanticAssociatedRecipe::Const {
            carrier: lift_semantic_type_recipe(field_carrier, lifter, depth + 1)?,
            value: lift_semantic_const_recipe(field_value, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_original_closure_parameter<L: CtDynamicLifter>(
    value: &CtSemanticOriginalClosureParameter,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalClosureParameter, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalClosureParameter {
        name: lifter.text(&value.name, depth + 1)?,
        ty: match &value.ty {
            Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
            None => None,
        },
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_pattern<L: CtDynamicLifter>(
    value: &CtSemanticOriginalPattern,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalPattern, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalPattern {
        kind: lift_semantic_original_pattern_kind(&value.kind, lifter, depth + 1)?,
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_pattern_kind<L: CtDynamicLifter>(
    value: &CtSemanticOriginalPatternKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalPatternKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalPatternKind::Name(field_value_0) => {
            SemanticOriginalPatternKind::Name(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalPatternKind::Wildcard => SemanticOriginalPatternKind::Wildcard,
        CtSemanticOriginalPatternKind::Tuple(field_value_0) => {
            SemanticOriginalPatternKind::Tuple({
                lifter.rows(
                    (field_value_0).len(),
                    std::mem::size_of::<SemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_value_0).len());
                for item in field_value_0 {
                    output.push(lift_semantic_original_pattern(item, lifter, depth + 1)?);
                }
                output
            })
        }
        CtSemanticOriginalPatternKind::Record {
            ty: field_ty,
            fields: field_fields,
        } => SemanticOriginalPatternKind::Record {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, SemanticOriginalPattern)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_pattern(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
        },
        CtSemanticOriginalPatternKind::Reference {
            mutable: field_mutable,
            pattern: field_pattern,
        } => SemanticOriginalPatternKind::Reference {
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            pattern: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalPattern>(), depth + 1)?;
                Box::new(lift_semantic_original_pattern(
                    (field_pattern).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalPatternKind::ErrorArm {
            is_error: field_is_error,
            pattern: field_pattern,
        } => SemanticOriginalPatternKind::ErrorArm {
            is_error: {
                lifter.node(depth + 1)?;
                *(field_is_error)
            },
            pattern: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalPattern>(), depth + 1)?;
                Box::new(lift_semantic_original_pattern(
                    (field_pattern).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalPatternKind::Variant {
            ty: field_ty,
            variant: field_variant,
            fields: field_fields,
        } => SemanticOriginalPatternKind::Variant {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            variant: lifter.text(field_variant, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<SemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(lift_semantic_original_pattern(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalPatternKind::Integer(field_value_0) => {
            SemanticOriginalPatternKind::Integer(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalPatternKind::Bool(field_value_0) => SemanticOriginalPatternKind::Bool({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOriginalPatternKind::Constructor {
            definition: field_definition,
            variant: field_variant,
            fields: field_fields,
        } => SemanticOriginalPatternKind::Constructor {
            definition: match field_definition {
                Some(item) => Some(lift_semantic_definition_id(item, lifter, depth + 1)?),
                None => None,
            },
            variant: lifter.text(field_variant, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<SemanticOriginalPattern>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push(lift_semantic_original_pattern(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalPatternKind::RecordConstructor {
            definition: field_definition,
            fields: field_fields,
        } => SemanticOriginalPatternKind::RecordConstructor {
            definition: match field_definition {
                Some(item) => Some(lift_semantic_definition_id(item, lifter, depth + 1)?),
                None => None,
            },
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, SemanticOriginalPattern)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_pattern(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
        },
        CtSemanticOriginalPatternKind::Float(field_value_0) => {
            SemanticOriginalPatternKind::Float(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalPatternKind::String(field_value_0) => {
            SemanticOriginalPatternKind::String(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalPatternKind::Character(field_value_0) => {
            SemanticOriginalPatternKind::Character(lifter.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn lift_semantic_original_match_arm<L: CtDynamicLifter>(
    value: &CtSemanticOriginalMatchArm,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalMatchArm, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalMatchArm {
        pattern: lift_semantic_original_pattern(&value.pattern, lifter, depth + 1)?,
        body: lift_semantic_original_block(&value.body, lifter, depth + 1)?,
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_block<L: CtDynamicLifter>(
    value: &CtSemanticOriginalBlock,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalBlock, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalBlock {
        statements: {
            lifter.rows(
                (value.statements).len(),
                std::mem::size_of::<SemanticOriginalStatement>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.statements).len());
            for item in &value.statements {
                output.push(lift_semantic_original_statement(item, lifter, depth + 1)?);
            }
            output
        },
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_statement<L: CtDynamicLifter>(
    value: &CtSemanticOriginalStatement,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalStatement, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalStatement {
        id: {
            lifter.node(depth + 1)?;
            value.id
        },
        kind: lift_semantic_original_statement_kind(&value.kind, lifter, depth + 1)?,
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_statement_kind<L: CtDynamicLifter>(
    value: &CtSemanticOriginalStatementKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalStatementKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalStatementKind::Binding {
            name: field_name,
            mutable: field_mutable,
            ty: field_ty,
            initializer: field_initializer,
        } => SemanticOriginalStatementKind::Binding {
            name: lifter.text(field_name, depth + 1)?,
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            ty: match field_ty {
                Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
                None => None,
            },
            initializer: match field_initializer {
                Some(item) => Some(lift_semantic_original_expr(item, lifter, depth + 1)?),
                None => None,
            },
        },
        CtSemanticOriginalStatementKind::TupleBinding {
            pattern: field_pattern,
            mutable: field_mutable,
            ty: field_ty,
            initializer: field_initializer,
        } => SemanticOriginalStatementKind::TupleBinding {
            pattern: lift_semantic_original_pattern(field_pattern, lifter, depth + 1)?,
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            ty: match field_ty {
                Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
                None => None,
            },
            initializer: lift_semantic_original_expr(field_initializer, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::Assign {
            target: field_target,
            value: field_value,
        } => SemanticOriginalStatementKind::Assign {
            target: lift_semantic_original_expr(field_target, lifter, depth + 1)?,
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::Expression(field_value_0) => {
            SemanticOriginalStatementKind::Expression(lift_semantic_original_expr(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::Return(field_value_0) => {
            SemanticOriginalStatementKind::Return(match field_value_0 {
                Some(item) => Some(lift_semantic_original_expr(item, lifter, depth + 1)?),
                None => None,
            })
        }
        CtSemanticOriginalStatementKind::If {
            condition: field_condition,
            then_body: field_then_body,
            else_body: field_else_body,
        } => SemanticOriginalStatementKind::If {
            condition: lift_semantic_original_expr(field_condition, lifter, depth + 1)?,
            then_body: lift_semantic_original_block(field_then_body, lifter, depth + 1)?,
            else_body: match field_else_body {
                Some(item) => Some(lift_semantic_original_block(item, lifter, depth + 1)?),
                None => None,
            },
        },
        CtSemanticOriginalStatementKind::IfLet {
            pattern: field_pattern,
            value: field_value,
            then_body: field_then_body,
            else_body: field_else_body,
        } => SemanticOriginalStatementKind::IfLet {
            pattern: lift_semantic_original_pattern(field_pattern, lifter, depth + 1)?,
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
            then_body: lift_semantic_original_block(field_then_body, lifter, depth + 1)?,
            else_body: match field_else_body {
                Some(item) => Some(lift_semantic_original_block(item, lifter, depth + 1)?),
                None => None,
            },
        },
        CtSemanticOriginalStatementKind::While {
            condition: field_condition,
            body: field_body,
        } => SemanticOriginalStatementKind::While {
            condition: lift_semantic_original_expr(field_condition, lifter, depth + 1)?,
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::WhileLet {
            pattern: field_pattern,
            value: field_value,
            body: field_body,
        } => SemanticOriginalStatementKind::WhileLet {
            pattern: lift_semantic_original_pattern(field_pattern, lifter, depth + 1)?,
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::ForRange {
            name: field_name,
            start: field_start,
            end: field_end,
            body: field_body,
        } => SemanticOriginalStatementKind::ForRange {
            name: lifter.text(field_name, depth + 1)?,
            start: lift_semantic_original_expr(field_start, lifter, depth + 1)?,
            end: lift_semantic_original_expr(field_end, lifter, depth + 1)?,
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::Match {
            value: field_value,
            arms: field_arms,
        } => SemanticOriginalStatementKind::Match {
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
            arms: {
                lifter.rows(
                    (field_arms).len(),
                    std::mem::size_of::<SemanticOriginalMatchArm>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arms).len());
                for item in field_arms {
                    output.push(lift_semantic_original_match_arm(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalStatementKind::Block(field_value_0) => {
            SemanticOriginalStatementKind::Block(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::Defer(field_value_0) => {
            SemanticOriginalStatementKind::Defer(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::Unsafe(field_value_0) => {
            SemanticOriginalStatementKind::Unsafe(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::Break => SemanticOriginalStatementKind::Break,
        CtSemanticOriginalStatementKind::Continue => SemanticOriginalStatementKind::Continue,
        CtSemanticOriginalStatementKind::Unsupported(field_value_0) => {
            SemanticOriginalStatementKind::Unsupported(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalStatementKind::ForEach {
            pattern: field_pattern,
            iterator: field_iterator,
            body: field_body,
        } => SemanticOriginalStatementKind::ForEach {
            pattern: lift_semantic_original_pattern(field_pattern, lifter, depth + 1)?,
            iterator: lift_semantic_original_expr(field_iterator, lifter, depth + 1)?,
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::CompoundAssign {
            target: field_target,
            op: field_op,
            value: field_value,
        } => SemanticOriginalStatementKind::CompoundAssign {
            target: lift_semantic_original_expr(field_target, lifter, depth + 1)?,
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::Yield(field_value_0) => {
            SemanticOriginalStatementKind::Yield(lift_semantic_original_expr(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::Loop(field_value_0) => {
            SemanticOriginalStatementKind::Loop(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalStatementKind::With {
            name: field_name,
            value: field_value,
            body: field_body,
        } => SemanticOriginalStatementKind::With {
            name: lifter.text(field_name, depth + 1)?,
            value: lift_semantic_original_expr(field_value, lifter, depth + 1)?,
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalStatementKind::CompileTime(field_value_0) => {
            SemanticOriginalStatementKind::CompileTime({
                lifter.rows(
                    1,
                    std::mem::size_of::<SemanticOriginalStatement>(),
                    depth + 1,
                )?;
                Box::new(lift_semantic_original_statement(
                    (field_value_0).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            })
        }
    })
}
pub fn lift_semantic_original_expr<L: CtDynamicLifter>(
    value: &CtSemanticOriginalExpr,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalExpr, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalExpr {
        id: {
            lifter.node(depth + 1)?;
            value.id
        },
        kind: lift_semantic_original_expr_kind(&value.kind, lifter, depth + 1)?,
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_expr_kind<L: CtDynamicLifter>(
    value: &CtSemanticOriginalExprKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalExprKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalExprKind::MakeDynBorrow {
            interface: field_interface,
            vtable: field_vtable,
            source: field_source,
            mutable: field_mutable,
        } => SemanticOriginalExprKind::MakeDynBorrow {
            interface: lift_dyn_interface_id(field_interface, lifter, depth + 1)?,
            vtable: lift_dyn_vtable_id(field_vtable, lifter, depth + 1)?,
            source: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_source).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
        },
        CtSemanticOriginalExprKind::MakeDynOwned {
            interface: field_interface,
            vtable: field_vtable,
            source: field_source,
            route: field_route,
        } => SemanticOriginalExprKind::MakeDynOwned {
            interface: lift_dyn_interface_id(field_interface, lifter, depth + 1)?,
            vtable: lift_dyn_vtable_id(field_vtable, lifter, depth + 1)?,
            source: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_source).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            route: lift_dyn_owned_storage(field_route, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::CallDynSlot {
            callee: field_callee,
            interface: field_interface,
            slot: field_slot,
            arguments: field_arguments,
        } => SemanticOriginalExprKind::CallDynSlot {
            callee: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_callee).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            interface: lift_dyn_interface_id(field_interface, lifter, depth + 1)?,
            slot: {
                lifter.node(depth + 1)?;
                *(field_slot)
            },
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticOriginalExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_original_expr(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalExprKind::Literal(field_value_0) => {
            SemanticOriginalExprKind::Literal(lifter.constant(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Ascribed {
            ty: field_ty,
            value: field_value,
        } => SemanticOriginalExprKind::Ascribed {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::Integer(field_value_0) => {
            SemanticOriginalExprKind::Integer(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Float(field_value_0) => {
            SemanticOriginalExprKind::Float(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::NumericConvert {
            value: field_value,
            target: field_target,
        } => SemanticOriginalExprKind::NumericConvert {
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            target: lift_semantic_type_recipe(field_target, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::IntegerConvert {
            value: field_value,
            target: field_target,
        } => SemanticOriginalExprKind::IntegerConvert {
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            target: lift_semantic_type_recipe(field_target, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::Bool(field_value_0) => SemanticOriginalExprKind::Bool({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtSemanticOriginalExprKind::String(field_value_0) => {
            SemanticOriginalExprKind::String(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Unit => SemanticOriginalExprKind::Unit,
        CtSemanticOriginalExprKind::Name(field_value_0) => {
            SemanticOriginalExprKind::Name(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Unary {
            op: field_op,
            operand: field_operand,
        } => SemanticOriginalExprKind::Unary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            operand: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_operand).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::Binary {
            op: field_op,
            left: field_left,
            right: field_right,
        } => SemanticOriginalExprKind::Binary {
            op: {
                lifter.node(depth + 1)?;
                *(field_op)
            },
            left: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_left).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            right: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_right).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::TypeApply {
            callee: field_callee,
            type_arguments: field_type_arguments,
        } => SemanticOriginalExprKind::TypeApply {
            callee: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_callee).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            type_arguments: {
                lifter.rows(
                    (field_type_arguments).len(),
                    std::mem::size_of::<SemanticTypeRecipe>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_type_arguments).len());
                for item in field_type_arguments {
                    output.push(lift_semantic_type_recipe(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalExprKind::Call {
            callee: field_callee,
            arguments: field_arguments,
        } => SemanticOriginalExprKind::Call {
            callee: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_callee).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticOriginalExpr>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_original_expr(item, lifter, depth + 1)?);
                }
                output
            },
        },
        CtSemanticOriginalExprKind::Tuple(field_value_0) => SemanticOriginalExprKind::Tuple({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<SemanticOriginalExpr>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_semantic_original_expr(item, lifter, depth + 1)?);
            }
            output
        }),
        CtSemanticOriginalExprKind::Array(field_value_0) => SemanticOriginalExprKind::Array({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<SemanticOriginalExpr>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push(lift_semantic_original_expr(item, lifter, depth + 1)?);
            }
            output
        }),
        CtSemanticOriginalExprKind::Index {
            object: field_object,
            index: field_index,
        } => SemanticOriginalExprKind::Index {
            object: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_object).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            index: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_index).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::Slice {
            object: field_object,
            start: field_start,
            end: field_end,
        } => SemanticOriginalExprKind::Slice {
            object: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_object).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            start: match field_start {
                Some(item) => Some({
                    lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                    Box::new(lift_semantic_original_expr(
                        (item).as_ref(),
                        lifter,
                        depth + 1,
                    )?)
                }),
                None => None,
            },
            end: match field_end {
                Some(item) => Some({
                    lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                    Box::new(lift_semantic_original_expr(
                        (item).as_ref(),
                        lifter,
                        depth + 1,
                    )?)
                }),
                None => None,
            },
        },
        CtSemanticOriginalExprKind::Member {
            object: field_object,
            member: field_member,
            owner: field_owner,
        } => SemanticOriginalExprKind::Member {
            object: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_object).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            member: lifter.text(field_member, depth + 1)?,
            owner: match field_owner {
                Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
                None => None,
            },
        },
        CtSemanticOriginalExprKind::ReceiverMember(field_value_0) => {
            SemanticOriginalExprKind::ReceiverMember(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Record {
            ty: field_ty,
            fields: field_fields,
        } => SemanticOriginalExprKind::Record {
            ty: lift_semantic_type_recipe(field_ty, lifter, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, SemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_expr(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
        },
        CtSemanticOriginalExprKind::RecordUpdate {
            ty: field_ty,
            base: field_base,
            fields: field_fields,
        } => SemanticOriginalExprKind::RecordUpdate {
            ty: match field_ty {
                Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
                None => None,
            },
            base: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_base).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, SemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_expr(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
        },
        CtSemanticOriginalExprKind::Sequence {
            bindings: field_bindings,
            value: field_value,
        } => SemanticOriginalExprKind::Sequence {
            bindings: {
                lifter.rows(
                    (field_bindings).len(),
                    std::mem::size_of::<(String, SemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_bindings).len());
                for item in field_bindings {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_expr(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
            value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::CompileTime(field_value_0) => {
            SemanticOriginalExprKind::CompileTime({
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_value_0).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            })
        }
        CtSemanticOriginalExprKind::Try(field_value_0) => SemanticOriginalExprKind::Try({
            lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
            Box::new(lift_semantic_original_expr(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticOriginalExprKind::Closure {
            move_capture: field_move_capture,
            parameters: field_parameters,
            return_type: field_return_type,
            body: field_body,
        } => SemanticOriginalExprKind::Closure {
            move_capture: {
                lifter.node(depth + 1)?;
                *(field_move_capture)
            },
            parameters: {
                lifter.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<SemanticOriginalClosureParameter>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push(lift_semantic_original_closure_parameter(
                        item,
                        lifter,
                        depth + 1,
                    )?);
                }
                output
            },
            return_type: match field_return_type {
                Some(item) => Some(lift_semantic_type_recipe(item, lifter, depth + 1)?),
                None => None,
            },
            body: lift_semantic_original_block(field_body, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::If {
            condition: field_condition,
            then_value: field_then_value,
            else_value: field_else_value,
        } => SemanticOriginalExprKind::If {
            condition: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_condition).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            then_value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_then_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            else_value: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_else_value).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::Unsupported(field_value_0) => {
            SemanticOriginalExprKind::Unsupported(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Spread(field_value_0) => SemanticOriginalExprKind::Spread({
            lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
            Box::new(lift_semantic_original_expr(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticOriginalExprKind::Group(field_value_0) => SemanticOriginalExprKind::Group({
            lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
            Box::new(lift_semantic_original_expr(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticOriginalExprKind::Character(field_value_0) => {
            SemanticOriginalExprKind::Character(lifter.text(field_value_0, depth + 1)?)
        }
        CtSemanticOriginalExprKind::Range {
            start: field_start,
            end: field_end,
        } => SemanticOriginalExprKind::Range {
            start: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_start).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            end: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_end).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::Pipeline {
            input: field_input,
            target: field_target,
        } => SemanticOriginalExprKind::Pipeline {
            input: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_input).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            target: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_target).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
        },
        CtSemanticOriginalExprKind::RecordConstructor {
            definition: field_definition,
            fields: field_fields,
        } => SemanticOriginalExprKind::RecordConstructor {
            definition: match field_definition {
                Some(item) => Some(lift_semantic_definition_id(item, lifter, depth + 1)?),
                None => None,
            },
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, SemanticOriginalExpr)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((
                        lifter.text(&(item).0, depth + 1)?,
                        lift_semantic_original_expr(&(item).1, lifter, depth + 1)?,
                    ));
                }
                output
            },
        },
        CtSemanticOriginalExprKind::Splice(field_value_0) => SemanticOriginalExprKind::Splice({
            lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
            Box::new(lift_semantic_original_expr(
                (field_value_0).as_ref(),
                lifter,
                depth + 1,
            )?)
        }),
        CtSemanticOriginalExprKind::CompileTimeBlock(field_value_0) => {
            SemanticOriginalExprKind::CompileTimeBlock(lift_semantic_original_block(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalExprKind::SyntaxQuote {
            category: field_category,
            template: field_template,
        } => SemanticOriginalExprKind::SyntaxQuote {
            category: lift_semantic_original_syntax_category(field_category, lifter, depth + 1)?,
            template: lift_semantic_original_quote_template(field_template, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::SyntaxIsland {
            path: field_path,
            tree: field_tree,
        } => SemanticOriginalExprKind::SyntaxIsland {
            path: {
                lifter.rows((field_path).len(), std::mem::size_of::<String>(), depth + 1)?;
                let mut output = Vec::with_capacity((field_path).len());
                for item in field_path {
                    output.push(lifter.text(item, depth + 1)?);
                }
                output
            },
            tree: lift_semantic_original_syntax_tree(field_tree, lifter, depth + 1)?,
        },
        CtSemanticOriginalExprKind::GenericApply {
            callee: field_callee,
            arguments: field_arguments,
        } => SemanticOriginalExprKind::GenericApply {
            callee: {
                lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
                Box::new(lift_semantic_original_expr(
                    (field_callee).as_ref(),
                    lifter,
                    depth + 1,
                )?)
            },
            arguments: {
                lifter.rows(
                    (field_arguments).len(),
                    std::mem::size_of::<SemanticOriginalGenericArgument>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_arguments).len());
                for item in field_arguments {
                    output.push(lift_semantic_original_generic_argument(
                        item,
                        lifter,
                        depth + 1,
                    )?);
                }
                output
            },
        },
    })
}
pub fn lift_semantic_original_quote_hole<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteHole,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteHole, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalQuoteHole {
        kind: lift_semantic_original_quote_hole_kind(&value.kind, lifter, depth + 1)?,
        value: {
            lifter.rows(1, std::mem::size_of::<SemanticOriginalExpr>(), depth + 1)?;
            Box::new(lift_semantic_original_expr(
                (value.value).as_ref(),
                lifter,
                depth + 1,
            )?)
        },
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_quote_template<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteTemplate,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteTemplate, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalQuoteTemplate {
        tree: lift_semantic_original_syntax_tree(&value.tree, lifter, depth + 1)?,
        holes: {
            lifter.rows(
                (value.holes).len(),
                std::mem::size_of::<SemanticOriginalQuoteHole>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.holes).len());
            for item in &value.holes {
                output.push(lift_semantic_original_quote_hole(item, lifter, depth + 1)?);
            }
            output
        },
        name_sites: {
            lifter.rows(
                (value.name_sites).len(),
                std::mem::size_of::<SemanticOriginalQuoteNameSite>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.name_sites).len());
            for item in &value.name_sites {
                output.push(lift_semantic_original_quote_name_site(
                    item,
                    lifter,
                    depth + 1,
                )?);
            }
            output
        },
    })
}
pub fn lift_semantic_original_generic_argument<L: CtDynamicLifter>(
    value: &CtSemanticOriginalGenericArgument,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalGenericArgument, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalGenericArgument::Type(field_value_0) => {
            SemanticOriginalGenericArgument::Type(lift_semantic_type_recipe(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalGenericArgument::Const(field_value_0) => {
            SemanticOriginalGenericArgument::Const(lift_semantic_original_expr(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalGenericArgument::Lifetime(field_value_0) => {
            SemanticOriginalGenericArgument::Lifetime(lifter.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn lift_semantic_original_syntax_category<L: CtDynamicLifter>(
    value: &CtSemanticOriginalSyntaxCategory,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalSyntaxCategory, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalSyntaxCategory::Expr => SemanticOriginalSyntaxCategory::Expr,
        CtSemanticOriginalSyntaxCategory::Type => SemanticOriginalSyntaxCategory::Type,
        CtSemanticOriginalSyntaxCategory::StmtList => SemanticOriginalSyntaxCategory::StmtList,
        CtSemanticOriginalSyntaxCategory::ItemList => SemanticOriginalSyntaxCategory::ItemList,
    })
}
pub fn lift_semantic_original_syntax_delimiter<L: CtDynamicLifter>(
    value: &CtSemanticOriginalSyntaxDelimiter,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalSyntaxDelimiter, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalSyntaxDelimiter::Parenthesis => {
            SemanticOriginalSyntaxDelimiter::Parenthesis
        }
        CtSemanticOriginalSyntaxDelimiter::Bracket => SemanticOriginalSyntaxDelimiter::Bracket,
        CtSemanticOriginalSyntaxDelimiter::Brace => SemanticOriginalSyntaxDelimiter::Brace,
    })
}
pub fn lift_semantic_original_quote_name_namespace<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteNameNamespace,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteNameNamespace, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalQuoteNameNamespace::Value => SemanticOriginalQuoteNameNamespace::Value,
        CtSemanticOriginalQuoteNameNamespace::Type => SemanticOriginalQuoteNameNamespace::Type,
        CtSemanticOriginalQuoteNameNamespace::Member => SemanticOriginalQuoteNameNamespace::Member,
        CtSemanticOriginalQuoteNameNamespace::Module => SemanticOriginalQuoteNameNamespace::Module,
        CtSemanticOriginalQuoteNameNamespace::Label => SemanticOriginalQuoteNameNamespace::Label,
    })
}
pub fn lift_semantic_original_quote_name_role<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteNameRole,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteNameRole, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalQuoteNameRole::Binding => SemanticOriginalQuoteNameRole::Binding,
        CtSemanticOriginalQuoteNameRole::Reference => SemanticOriginalQuoteNameRole::Reference,
    })
}
pub fn lift_semantic_original_quote_hole_kind<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteHoleKind,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteHoleKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalQuoteHoleKind::Expr => SemanticOriginalQuoteHoleKind::Expr,
        CtSemanticOriginalQuoteHoleKind::Type => SemanticOriginalQuoteHoleKind::Type,
        CtSemanticOriginalQuoteHoleKind::StmtList => SemanticOriginalQuoteHoleKind::StmtList,
        CtSemanticOriginalQuoteHoleKind::ItemList => SemanticOriginalQuoteHoleKind::ItemList,
        CtSemanticOriginalQuoteHoleKind::Ident {
            namespace: field_namespace,
            role: field_role,
        } => SemanticOriginalQuoteHoleKind::Ident {
            namespace: lift_semantic_original_quote_name_namespace(
                field_namespace,
                lifter,
                depth + 1,
            )?,
            role: lift_semantic_original_quote_name_role(field_role, lifter, depth + 1)?,
        },
    })
}
pub fn lift_semantic_original_quote_name_site<L: CtDynamicLifter>(
    value: &CtSemanticOriginalQuoteNameSite,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalQuoteNameSite, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalQuoteNameSite {
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
        token_index: {
            lifter.node(depth + 1)?;
            value.token_index
        },
    })
}
pub fn lift_semantic_original_syntax_tree<L: CtDynamicLifter>(
    value: &CtSemanticOriginalSyntaxTree,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalSyntaxTree, L::Error> {
    lifter.node(depth)?;
    Ok(SemanticOriginalSyntaxTree {
        delimiter: lift_semantic_original_syntax_delimiter(&value.delimiter, lifter, depth + 1)?,
        children: {
            lifter.rows(
                (value.children).len(),
                std::mem::size_of::<SemanticOriginalSyntaxTreeEntry>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.children).len());
            for item in &value.children {
                output.push(lift_semantic_original_syntax_tree_entry(
                    item,
                    lifter,
                    depth + 1,
                )?);
            }
            output
        },
        span: {
            lifter.node(depth + 1)?;
            value.span
        },
    })
}
pub fn lift_semantic_original_syntax_tree_entry<L: CtDynamicLifter>(
    value: &CtSemanticOriginalSyntaxTreeEntry,
    lifter: &mut L,
    depth: usize,
) -> Result<SemanticOriginalSyntaxTreeEntry, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtSemanticOriginalSyntaxTreeEntry::Token {
            text: field_text,
            span: field_span,
        } => SemanticOriginalSyntaxTreeEntry::Token {
            text: lifter.text(field_text, depth + 1)?,
            span: {
                lifter.node(depth + 1)?;
                *(field_span)
            },
        },
        CtSemanticOriginalSyntaxTreeEntry::Tree(field_value_0) => {
            SemanticOriginalSyntaxTreeEntry::Tree(lift_semantic_original_syntax_tree(
                field_value_0,
                lifter,
                depth + 1,
            )?)
        }
        CtSemanticOriginalSyntaxTreeEntry::Hole {
            index: field_index,
            span: field_span,
        } => SemanticOriginalSyntaxTreeEntry::Hole {
            index: {
                lifter.node(depth + 1)?;
                *(field_index)
            },
            span: {
                lifter.node(depth + 1)?;
                *(field_span)
            },
        },
    })
}
pub fn lift_task_resume_bound<L: CtDynamicLifter>(
    value: &CtTaskResumeBound,
    lifter: &mut L,
    depth: usize,
) -> Result<TaskResumeBound, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtTaskResumeBound::Local => TaskResumeBound::Local,
        CtTaskResumeBound::Transfer => TaskResumeBound::Transfer,
    })
}
pub fn lift_protocol_state<L: CtDynamicLifter>(
    value: &CtProtocolState,
    lifter: &mut L,
    depth: usize,
) -> Result<ProtocolState, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtProtocolState::Open => ProtocolState::Open,
        CtProtocolState::Closing => ProtocolState::Closing,
        CtProtocolState::Closed => ProtocolState::Closed,
    })
}
pub fn lift_protocol_kind<L: CtDynamicLifter>(
    value: &CtProtocolKind,
    lifter: &mut L,
    depth: usize,
) -> Result<ProtocolKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtProtocolKind::AsyncRegistrationV1 => ProtocolKind::AsyncRegistrationV1,
    })
}
pub fn lift_protocol_exit<L: CtDynamicLifter>(
    value: &CtProtocolExit,
    lifter: &mut L,
    depth: usize,
) -> Result<ProtocolExit, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtProtocolExit::Return => ProtocolExit::Return,
        CtProtocolExit::ErrorOk => ProtocolExit::ErrorOk,
        CtProtocolExit::ErrorErr => ProtocolExit::ErrorErr,
        CtProtocolExit::PollPending => ProtocolExit::PollPending,
        CtProtocolExit::PollReady => ProtocolExit::PollReady,
    })
}
pub fn lift_protocol_transition<L: CtDynamicLifter>(
    value: &CtProtocolTransition,
    lifter: &mut L,
    depth: usize,
) -> Result<ProtocolTransition, L::Error> {
    lifter.node(depth)?;
    Ok(ProtocolTransition {
        input: lift_protocol_state(&value.input, lifter, depth + 1)?,
        exit: lift_protocol_exit(&value.exit, lifter, depth + 1)?,
        output: {
            lifter.rows(
                (value.output).len(),
                std::mem::size_of::<ProtocolState>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.output).len());
            for item in &value.output {
                output.push(lift_protocol_state(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_function_protocol_contract<L: CtDynamicLifter>(
    value: &CtFunctionProtocolContract,
    lifter: &mut L,
    depth: usize,
) -> Result<FunctionProtocolContract, L::Error> {
    lifter.node(depth)?;
    Ok(FunctionProtocolContract {
        protocol: lift_protocol_kind(&value.protocol, lifter, depth + 1)?,
        parameter: {
            lifter.node(depth + 1)?;
            value.parameter
        },
        entry: {
            lifter.rows(
                (value.entry).len(),
                std::mem::size_of::<ProtocolState>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.entry).len());
            for item in &value.entry {
                output.push(lift_protocol_state(item, lifter, depth + 1)?);
            }
            output
        },
        exits: {
            lifter.rows(
                (value.exits).len(),
                std::mem::size_of::<ProtocolTransition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.exits).len());
            for item in &value.exits {
                output.push(lift_protocol_transition(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_scoped_callback_role<L: CtDynamicLifter>(
    value: &CtScopedCallbackRole,
    lifter: &mut L,
    depth: usize,
) -> Result<ScopedCallbackRole, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtScopedCallbackRole::Register => ScopedCallbackRole::Register,
        CtScopedCallbackRole::Close => ScopedCallbackRole::Close,
        CtScopedCallbackRole::AsyncRegister => ScopedCallbackRole::AsyncRegister,
        CtScopedCallbackRole::AsyncRequestClose => ScopedCallbackRole::AsyncRequestClose,
        CtScopedCallbackRole::AsyncPollDrain => ScopedCallbackRole::AsyncPollDrain,
        CtScopedCallbackRole::ThreadAsyncRegister => ScopedCallbackRole::ThreadAsyncRegister,
        CtScopedCallbackRole::ThreadAsyncRequestClose => {
            ScopedCallbackRole::ThreadAsyncRequestClose
        }
        CtScopedCallbackRole::ThreadAsyncPollDrain => ScopedCallbackRole::ThreadAsyncPollDrain,
    })
}
pub fn lift_foreign_declaration<L: CtDynamicLifter>(
    value: &CtForeignDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<ForeignDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(ForeignDeclaration {
        abi: lifter.text(&value.abi, depth + 1)?,
        symbol: lifter.text(&value.symbol, depth + 1)?,
        noescape_parameters: {
            lifter.rows(
                (value.noescape_parameters).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.noescape_parameters).len());
            for item in &value.noescape_parameters {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        static_callbacks: {
            lifter.node(depth + 1)?;
            value.static_callbacks
        },
        thread_driver_transfer: {
            lifter.node(depth + 1)?;
            value.thread_driver_transfer
        },
        scoped_callback: match &value.scoped_callback {
            Some(item) => Some(lift_scoped_callback_role(item, lifter, depth + 1)?),
            None => None,
        },
    })
}
pub fn lift_storage_source<L: CtDynamicLifter>(
    value: &CtStorageSource,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageSource, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtStorageSource::Exact(field_value_0) => {
            StorageSource::Exact(lift_storage_position(field_value_0, lifter, depth + 1)?)
        }
        CtStorageSource::PlaceAddress {
            place: field_place,
            mode: field_mode,
        } => StorageSource::PlaceAddress {
            place: lift_storage_position(field_place, lifter, depth + 1)?,
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtStorageSource::OwnedView {
            head: field_head,
            leaf: field_leaf,
            mode: field_mode,
        } => StorageSource::OwnedView {
            head: lift_storage_position(field_head, lifter, depth + 1)?,
            leaf: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_leaf), depth + 1)?
            },
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
        CtStorageSource::ReborrowedLeaf {
            source: field_source,
            head: field_head,
        } => StorageSource::ReborrowedLeaf {
            source: lift_storage_position(field_source, lifter, depth + 1)?,
            head: lift_storage_position(field_head, lifter, depth + 1)?,
        },
        CtStorageSource::Static {
            leaf: field_leaf,
            mode: field_mode,
        } => StorageSource::Static {
            leaf: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_leaf), depth + 1)?
            },
            mode: {
                lifter.node(depth + 1)?;
                *(field_mode)
            },
        },
    })
}
pub fn lift_retained_borrow_port<L: CtDynamicLifter>(
    value: &CtRetainedBorrowPort,
    lifter: &mut L,
    depth: usize,
) -> Result<RetainedBorrowPort, L::Error> {
    lifter.node(depth)?;
    Ok(RetainedBorrowPort {
        leaf: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.leaf), depth + 1)?
        },
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn lift_retained_life_shape<L: CtDynamicLifter>(
    value: &CtRetainedLifeShape,
    lifter: &mut L,
    depth: usize,
) -> Result<RetainedLifeShape, L::Error> {
    lifter.node(depth)?;
    Ok(RetainedLifeShape {
        binder_count: {
            lifter.node(depth + 1)?;
            value.binder_count
        },
        ports: {
            lifter.rows(
                (value.ports).len(),
                std::mem::size_of::<RetainedBorrowPort>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.ports).len());
            for item in &value.ports {
                output.push(lift_retained_borrow_port(item, lifter, depth + 1)?);
            }
            output
        },
        retained: {
            lifter.rows(
                (value.retained).len(),
                std::mem::size_of::<StorageRetainedBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(lift_storage_retained_binding(item, lifter, depth + 1)?);
            }
            output
        },
        nominal_arguments: {
            lifter.rows(
                (value.nominal_arguments).len(),
                std::mem::size_of::<NominalLifeArgument>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal_arguments).len());
            for item in &value.nominal_arguments {
                output.push(lift_nominal_life_argument(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_storage_source_binding<L: CtDynamicLifter>(
    value: &CtStorageSourceBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageSourceBinding, L::Error> {
    lifter.node(depth)?;
    Ok(StorageSourceBinding {
        position: lift_storage_position(&value.position, lifter, depth + 1)?,
        sources: {
            lifter.rows(
                (value.sources).len(),
                std::mem::size_of::<StorageSource>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.sources).len());
            for item in &value.sources {
                output.push(lift_storage_source(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_life_nominal_kind<L: CtDynamicLifter>(
    value: &CtLifeNominalKind,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeNominalKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtLifeNominalKind::Record => LifeNominalKind::Record,
        CtLifeNominalKind::Package => LifeNominalKind::Package,
        CtLifeNominalKind::Resource => LifeNominalKind::Resource,
        CtLifeNominalKind::Enum => LifeNominalKind::Enum,
    })
}
pub fn lift_life_parameter_kind<L: CtDynamicLifter>(
    value: &CtLifeParameterKind,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeParameterKind, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtLifeParameterKind::Type => LifeParameterKind::Type,
        CtLifeParameterKind::Const(field_value_0) => LifeParameterKind::Const({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
        CtLifeParameterKind::Lifetime => LifeParameterKind::Lifetime,
    })
}
pub fn lift_life_nominal_declaration<L: CtDynamicLifter>(
    value: &CtLifeNominalDeclaration,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeNominalDeclaration, L::Error> {
    lifter.node(depth)?;
    Ok(LifeNominalDeclaration {
        id: lift_life_declaration_id(&value.id, lifter, depth + 1)?,
        identity: lift_life_declaration_identity(&value.identity, lifter, depth + 1)?,
        kind: lift_life_nominal_kind(&value.kind, lifter, depth + 1)?,
        parameters: {
            lifter.rows(
                (value.parameters).len(),
                std::mem::size_of::<LifeParameterKind>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.parameters).len());
            for item in &value.parameters {
                output.push(lift_life_parameter_kind(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_life_nominal_actual<L: CtDynamicLifter>(
    value: &CtLifeNominalActual,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeNominalActual, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtLifeNominalActual::Type(field_value_0) => LifeNominalActual::Type({
            lifter.node(depth + 1)?;
            lifter.load_type(*(field_value_0), depth + 1)?
        }),
        CtLifeNominalActual::QualifiedType {
            ty: field_ty,
            life: field_life,
        } => LifeNominalActual::QualifiedType {
            ty: {
                lifter.node(depth + 1)?;
                lifter.load_type(*(field_ty), depth + 1)?
            },
            life: lift_storage_life_shape(field_life, lifter, depth + 1)?,
        },
        CtLifeNominalActual::Const(field_value_0) => {
            LifeNominalActual::Const(lifter.constant(field_value_0, depth + 1)?)
        }
        CtLifeNominalActual::Lifetime(field_value_0) => LifeNominalActual::Lifetime({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
    })
}
pub fn lift_life_nominal_instance<L: CtDynamicLifter>(
    value: &CtLifeNominalInstance,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeNominalInstance, L::Error> {
    lifter.node(depth)?;
    Ok(LifeNominalInstance {
        declaration: lift_life_declaration_id(&value.declaration, lifter, depth + 1)?,
        ty: {
            lifter.node(depth + 1)?;
            lifter.load_type(*(&value.ty), depth + 1)?
        },
        arguments: {
            lifter.rows(
                (value.arguments).len(),
                std::mem::size_of::<LifeNominalActual>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.arguments).len());
            for item in &value.arguments {
                output.push(lift_life_nominal_actual(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_type<L: CtDynamicLifter>(
    value: &CtDynamicTypeNode,
    lifter: &mut L,
    depth: usize,
) -> Result<Type, L::Error> {
    lifter.node(depth)?;
    Ok(match value {
        CtDynamicTypeNode::Dyn {
            interface: field_interface,
        } => Type::Dyn {
            interface: lift_dyn_interface_id(field_interface, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::OwnedDyn {
            interface: field_interface,
        } => Type::OwnedDyn {
            interface: lift_dyn_interface_id(field_interface, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::BorrowedStorage {
            storage: field_storage,
            life: field_life,
        } => Type::BorrowedStorage {
            storage: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_storage).as_ref()), depth + 1)?
                })
            },
            life: lift_storage_life_shape(field_life, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::Unit => Type::Unit,
        CtDynamicTypeNode::Bool => Type::Bool,
        CtDynamicTypeNode::I32 => Type::I32,
        CtDynamicTypeNode::I64 => Type::I64,
        CtDynamicTypeNode::U64 => Type::U64,
        CtDynamicTypeNode::Usize => Type::Usize,
        CtDynamicTypeNode::F32 => Type::F32,
        CtDynamicTypeNode::F64 => Type::F64,
        CtDynamicTypeNode::U8 => Type::U8,
        CtDynamicTypeNode::U16 => Type::U16,
        CtDynamicTypeNode::U32 => Type::U32,
        CtDynamicTypeNode::I8 => Type::I8,
        CtDynamicTypeNode::I16 => Type::I16,
        CtDynamicTypeNode::Isize => Type::Isize,
        CtDynamicTypeNode::ByteBuffer => Type::ByteBuffer,
        CtDynamicTypeNode::OwnedStore {
            element: field_element,
        } => Type::OwnedStore {
            element: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_element).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::ForeignHandle => Type::ForeignHandle,
        CtDynamicTypeNode::SyncCell(field_value_0) => Type::SyncCell({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynamicTypeNode::Mutex {
            payload: field_payload,
        } => Type::Mutex {
            payload: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_payload).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::MutexGuard {
            payload: field_payload,
            retained_life: field_retained_life,
        } => Type::MutexGuard {
            payload: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_payload).as_ref()), depth + 1)?
                })
            },
            retained_life: lift_retained_life_shape(field_retained_life, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::Registration {
            environment: field_environment,
        } => Type::Registration {
            environment: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_environment).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::AsyncRegistration {
            environment: field_environment,
        } => Type::AsyncRegistration {
            environment: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_environment).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::TaskI32 => Type::TaskI32,
        CtDynamicTypeNode::Task {
            result: field_result,
            retained_life: field_retained_life,
        } => Type::Task {
            result: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_result).as_ref()), depth + 1)?
                })
            },
            retained_life: lift_retained_life_shape(field_retained_life, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::CWideString => Type::CWideString,
        CtDynamicTypeNode::ForeignCallback {
            parameters: field_parameters,
            result: field_result,
        } => Type::ForeignCallback {
            parameters: {
                lifter.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<Type>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push({
                        lifter.node(depth + 1)?;
                        lifter.load_type(*(item), depth + 1)?
                    });
                }
                output
            },
            result: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_result).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::Tuple(field_value_0) => Type::Tuple({
            lifter.rows(
                (field_value_0).len(),
                std::mem::size_of::<Type>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((field_value_0).len());
            for item in field_value_0 {
                output.push({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*(item), depth + 1)?
                });
            }
            output
        }),
        CtDynamicTypeNode::Array {
            element: field_element,
            length: field_length,
        } => Type::Array {
            element: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_element).as_ref()), depth + 1)?
                })
            },
            length: {
                lifter.node(depth + 1)?;
                *(field_length)
            },
        },
        CtDynamicTypeNode::Slice {
            element: field_element,
        } => Type::Slice {
            element: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_element).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::Named(field_value_0) => {
            Type::Named(lifter.text(field_value_0, depth + 1)?)
        }
        CtDynamicTypeNode::Record {
            name: field_name,
            fields: field_fields,
        } => Type::Record {
            name: lifter.text(field_name, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, Type)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((lifter.text(&(item).0, depth + 1)?, {
                        lifter.node(depth + 1)?;
                        lifter.load_type(*(&(item).1), depth + 1)?
                    }));
                }
                output
            },
        },
        CtDynamicTypeNode::Package {
            name: field_name,
            fields: field_fields,
        } => Type::Package {
            name: lifter.text(field_name, depth + 1)?,
            fields: {
                lifter.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, Type)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_fields).len());
                for item in field_fields {
                    output.push((lifter.text(&(item).0, depth + 1)?, {
                        lifter.node(depth + 1)?;
                        lifter.load_type(*(&(item).1), depth + 1)?
                    }));
                }
                output
            },
        },
        CtDynamicTypeNode::Enum {
            name: field_name,
            variants: field_variants,
        } => Type::Enum {
            name: lifter.text(field_name, depth + 1)?,
            variants: {
                lifter.rows(
                    (field_variants).len(),
                    std::mem::size_of::<(String, Vec<Type>)>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_variants).len());
                for item in field_variants {
                    output.push((lifter.text(&(item).0, depth + 1)?, {
                        lifter.rows(((item).1).len(), std::mem::size_of::<Type>(), depth + 1)?;
                        let mut output = Vec::with_capacity(((item).1).len());
                        for item in &(item).1 {
                            output.push({
                                lifter.node(depth + 1)?;
                                lifter.load_type(*(item), depth + 1)?
                            });
                        }
                        output
                    }));
                }
                output
            },
        },
        CtDynamicTypeNode::ErrorUnion {
            error: field_error,
            result: field_result,
        } => Type::ErrorUnion {
            error: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_error).as_ref()), depth + 1)?
                })
            },
            result: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_result).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::Address {
            mutable: field_mutable,
            inner: field_inner,
        } => Type::Address {
            mutable: {
                lifter.node(depth + 1)?;
                *(field_mutable)
            },
            inner: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_inner).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::Managed(field_value_0) => Type::Managed({
            lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
            Box::new({
                lifter.node(depth + 1)?;
                lifter.load_type(*((field_value_0).as_ref()), depth + 1)?
            })
        }),
        CtDynamicTypeNode::Desc(field_value_0) => Type::Desc({
            lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
            Box::new({
                lifter.node(depth + 1)?;
                lifter.load_type(*((field_value_0).as_ref()), depth + 1)?
            })
        }),
        CtDynamicTypeNode::FunctionPointer {
            parameters: field_parameters,
            result: field_result,
        } => Type::FunctionPointer {
            parameters: {
                lifter.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<Type>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push({
                        lifter.node(depth + 1)?;
                        lifter.load_type(*(item), depth + 1)?
                    });
                }
                output
            },
            result: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_result).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::BorrowedFunctionPointer {
            parameters: field_parameters,
            result: field_result,
            life: field_life,
        } => Type::BorrowedFunctionPointer {
            parameters: {
                lifter.rows(
                    (field_parameters).len(),
                    std::mem::size_of::<Type>(),
                    depth + 1,
                )?;
                let mut output = Vec::with_capacity((field_parameters).len());
                for item in field_parameters {
                    output.push({
                        lifter.node(depth + 1)?;
                        lifter.load_type(*(item), depth + 1)?
                    });
                }
                output
            },
            result: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_result).as_ref()), depth + 1)?
                })
            },
            life: lift_storage_life_contract(field_life, lifter, depth + 1)?,
        },
        CtDynamicTypeNode::Closure(field_value_0) => Type::Closure({
            lifter.node(depth + 1)?;
            *(field_value_0)
        }),
        CtDynamicTypeNode::Opaque {
            identity: field_identity,
            hidden: field_hidden,
        } => Type::Opaque {
            identity: {
                lifter.node(depth + 1)?;
                *(field_identity)
            },
            hidden: {
                lifter.rows(1, std::mem::size_of::<Type>(), depth + 1)?;
                Box::new({
                    lifter.node(depth + 1)?;
                    lifter.load_type(*((field_hidden).as_ref()), depth + 1)?
                })
            },
        },
        CtDynamicTypeNode::Unsupported(field_value_0) => {
            Type::Unsupported(lifter.text(field_value_0, depth + 1)?)
        }
    })
}
pub fn lift_storage_life_slot<L: CtDynamicLifter>(
    value: &StorageLifeSlot,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageLifeSlot, L::Error> {
    lifter.node(depth)?;
    Ok(StorageLifeSlot {
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<PlaceProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
        mode: {
            lifter.node(depth + 1)?;
            value.mode
        },
    })
}
pub fn lift_life_declaration_id<L: CtDynamicLifter>(
    value: &LifeDeclarationId,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeDeclarationId, L::Error> {
    lifter.node(depth)?;
    Ok(LifeDeclarationId {
        module: {
            lifter.node(depth + 1)?;
            value.module
        },
        declaration: {
            lifter.node(depth + 1)?;
            value.declaration
        },
        compiler: {
            lifter.node(depth + 1)?;
            value.compiler
        },
    })
}
pub fn lift_storage_retained_binding<L: CtDynamicLifter>(
    value: &StorageRetainedBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageRetainedBinding, L::Error> {
    lifter.node(depth)?;
    Ok(StorageRetainedBinding {
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        class: {
            lifter.node(depth + 1)?;
            value.class
        },
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn lift_nominal_life_argument<L: CtDynamicLifter>(
    value: &NominalLifeArgument,
    lifter: &mut L,
    depth: usize,
) -> Result<NominalLifeArgument, L::Error> {
    lifter.node(depth)?;
    Ok(NominalLifeArgument {
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        declaration: lift_life_declaration_id(&value.declaration, lifter, depth + 1)?,
        argument: {
            lifter.node(depth + 1)?;
            value.argument
        },
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn lift_storage_life_shape<L: CtDynamicLifter>(
    value: &StorageLifeShape,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageLifeShape, L::Error> {
    lifter.node(depth)?;
    Ok(StorageLifeShape {
        binder_count: {
            lifter.node(depth + 1)?;
            value.binder_count
        },
        slots: {
            lifter.rows(
                (value.slots).len(),
                std::mem::size_of::<StorageLifeSlot>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.slots).len());
            for item in &value.slots {
                output.push(lift_storage_life_slot(item, lifter, depth + 1)?);
            }
            output
        },
        retained: {
            lifter.rows(
                (value.retained).len(),
                std::mem::size_of::<StorageRetainedBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.retained).len());
            for item in &value.retained {
                output.push(lift_storage_retained_binding(item, lifter, depth + 1)?);
            }
            output
        },
        nominal_arguments: {
            lifter.rows(
                (value.nominal_arguments).len(),
                std::mem::size_of::<NominalLifeArgument>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.nominal_arguments).len());
            for item in &value.nominal_arguments {
                output.push(lift_nominal_life_argument(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_storage_position<L: CtDynamicLifter>(
    value: &StoragePosition,
    lifter: &mut L,
    depth: usize,
) -> Result<StoragePosition, L::Error> {
    lifter.node(depth)?;
    Ok(StoragePosition {
        base: {
            lifter.node(depth + 1)?;
            value.base
        },
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<ContractProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
pub fn lift_storage_life_binding<L: CtDynamicLifter>(
    value: &StorageLifeBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageLifeBinding, L::Error> {
    lifter.node(depth)?;
    Ok(StorageLifeBinding {
        position: lift_storage_position(&value.position, lifter, depth + 1)?,
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn lift_signature_life_class<L: CtDynamicLifter>(
    value: &SignatureLifeClass,
    lifter: &mut L,
    depth: usize,
) -> Result<SignatureLifeClass, L::Error> {
    lifter.node(depth)?;
    Ok(SignatureLifeClass {
        base: {
            lifter.node(depth + 1)?;
            value.base
        },
        path: {
            lifter.rows(
                (value.path).len(),
                std::mem::size_of::<LifeTypeProjection>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.path).len());
            for item in &value.path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
        class: {
            lifter.node(depth + 1)?;
            value.class
        },
    })
}
pub fn lift_signature_life_class_binding<L: CtDynamicLifter>(
    value: &SignatureLifeClassBinding,
    lifter: &mut L,
    depth: usize,
) -> Result<SignatureLifeClassBinding, L::Error> {
    lifter.node(depth)?;
    Ok(SignatureLifeClassBinding {
        position: lift_signature_life_class(&value.position, lifter, depth + 1)?,
        binder: {
            lifter.node(depth + 1)?;
            value.binder
        },
    })
}
pub fn lift_storage_life_contract<L: CtDynamicLifter>(
    value: &StorageLifeContract,
    lifter: &mut L,
    depth: usize,
) -> Result<StorageLifeContract, L::Error> {
    lifter.node(depth)?;
    Ok(StorageLifeContract {
        fixed_binder_count: {
            lifter.node(depth + 1)?;
            value.fixed_binder_count
        },
        invocation_binder_count: {
            lifter.node(depth + 1)?;
            value.invocation_binder_count
        },
        bindings: {
            lifter.rows(
                (value.bindings).len(),
                std::mem::size_of::<StorageLifeBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.bindings).len());
            for item in &value.bindings {
                output.push(lift_storage_life_binding(item, lifter, depth + 1)?);
            }
            output
        },
        static_bindings: {
            lifter.rows(
                (value.static_bindings).len(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.static_bindings).len());
            for item in &value.static_bindings {
                output.push(lift_storage_position(item, lifter, depth + 1)?);
            }
            output
        },
        classes: {
            lifter.rows(
                (value.classes).len(),
                std::mem::size_of::<SignatureLifeClassBinding>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.classes).len());
            for item in &value.classes {
                output.push(lift_signature_life_class_binding(item, lifter, depth + 1)?);
            }
            output
        },
    })
}
pub fn lift_life_declaration_identity<L: CtDynamicLifter>(
    value: &LifeDeclarationIdentity,
    lifter: &mut L,
    depth: usize,
) -> Result<LifeDeclarationIdentity, L::Error> {
    lifter.node(depth)?;
    Ok(LifeDeclarationIdentity {
        package: lifter.text(&value.package, depth + 1)?,
        module_path: {
            lifter.rows(
                (value.module_path).len(),
                std::mem::size_of::<String>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.module_path).len());
            for item in &value.module_path {
                output.push(lifter.text(item, depth + 1)?);
            }
            output
        },
        declaration_path: {
            lifter.rows(
                (value.declaration_path).len(),
                std::mem::size_of::<String>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.declaration_path).len());
            for item in &value.declaration_path {
                output.push(lifter.text(item, depth + 1)?);
            }
            output
        },
        generation_path: {
            lifter.rows(
                (value.generation_path).len(),
                std::mem::size_of::<u32>(),
                depth + 1,
            )?;
            let mut output = Vec::with_capacity((value.generation_path).len());
            for item in &value.generation_path {
                output.push({
                    lifter.node(depth + 1)?;
                    *(item)
                });
            }
            output
        },
    })
}
