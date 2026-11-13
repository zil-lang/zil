//! Structural CT metadata traversal; callbacks carry the caller's own budget.
use super::*;
use crate::*;
pub trait CtDynamicVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize)
    -> Result<(), Self::Error>;
    /// Allocation-aware traversal defaults to the existing structural work.
    fn vector_storage(
        &mut self,
        count: usize,
        _capacity: usize,
        element_bytes: usize,
        depth: usize,
    ) -> Result<(), Self::Error> {
        self.rows(count, element_bytes, depth)
    }
    fn string_storage(
        &mut self,
        value: &str,
        _capacity: usize,
        depth: usize,
    ) -> Result<(), Self::Error> {
        self.text(value, depth)
    }
    fn type_node(
        &mut self,
        index: usize,
        node: &CtDynamicTypeNode,
        depth: usize,
    ) -> Result<(), Self::Error>;
    fn type_reference(&mut self, id: CtDynamicTypeId, depth: usize) -> Result<(), Self::Error>;
    fn type_handle(&mut self, value: &CtTypeRef, depth: usize) -> Result<(), Self::Error>;
    fn constant(&mut self, value: &Constant, depth: usize) -> Result<(), Self::Error>;
    fn text(&mut self, value: &str, depth: usize) -> Result<(), Self::Error>;
}
pub trait CtDynamicMetadata {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error>;
}
impl CtDynamicMetadata for CtDynInterfaceId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.index;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynVtableId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.index;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDeclaration {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.implicit_lifetimes {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynImplicitLifeDeclaration>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.direct_supertraits {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynSelfPattern>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.requirements {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynObligation>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.associated_definitions {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynAssociatedDefinition>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.method_signatures {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynMethodRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.method_definitions {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynDefinitionDeclaration>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.self_pattern {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.implemented_trait {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtLifeParameterKind>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.members).len(),
            (self.members).capacity(),
            std::mem::size_of::<CtDynMemberDeclaration>(),
            depth + 1,
        )?;
        for item in &self.members {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynSelfPattern {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.arguments).len(),
            (self.arguments).capacity(),
            std::mem::size_of::<CtDynPatternActual>(),
            depth + 1,
        )?;
        for item in &self.arguments {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynPatternActual {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Life(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynConstPattern {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::IntegerLiteral(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::FloatLiteral(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::MethodParameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Unary {
                op: field_op,
                value: field_value,
                ty: field_ty,
            } => {
                let _ = field_op;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                ((field_value).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Binary {
                op: field_op,
                left: field_left,
                right: field_right,
                ty: field_ty,
            } => {
                let _ = field_op;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                ((field_left).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                ((field_right).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Convert {
                value: field_value,
                ty: field_ty,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynConstPattern>(), depth + 1)?;
                ((field_value).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AssociatedProjection {
                receiver: field_receiver,
                application: field_application,
                member: field_member,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_receiver).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
                ((field_application).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
            Self::Value(field_value_0) => {
                visitor.constant(field_value_0, depth + 1)?;
            }
            Self::Parameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynTypePattern {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::SelfType => {}
            Self::MethodParameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::AssociatedProjection {
                receiver: field_receiver,
                application: field_application,
                member: field_member,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_receiver).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
                ((field_application).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
            Self::Concrete(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
            Self::Parameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Nominal(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynSelfPattern>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Address {
                mutable: field_mutable,
                lifetime: field_lifetime,
                inner: field_inner,
            } => {
                let _ = field_mutable;
                visitor.node(depth + 1)?;
                visitor.node(depth + 1)?;
                if let Some(item) = field_lifetime {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_inner).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Tuple(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtDynTypePattern>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Array {
                element: field_element,
                length: field_length,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_element).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                (field_length).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Slice(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Managed(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Desc {
                lifetime: field_lifetime,
                inner: field_inner,
            } => {
                visitor.node(depth + 1)?;
                if let Some(item) = field_lifetime {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_inner).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::OwnedStore(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ErrorUnion {
                error: field_error,
                result: field_result,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_error).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynTypePattern>(), depth + 1)?;
                ((field_result).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Semantic {
                scope: field_scope,
                ty: field_ty,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDeclarationKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Trait => {}
            Self::Impl => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynMemberDeclaration {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.string_storage((self.name).as_str(), (self.name).capacity(), depth + 1)?;
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynMemberKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type => {}
            Self::Constant => {}
            Self::Method(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtLifeParameterKind>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynGenericActual {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
            Self::QualifiedType(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(field_value_0) => {
                visitor.constant(field_value_0, depth + 1)?;
            }
            Self::Life(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynTraitApplication {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynMember {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.ordinal;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedType {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.member).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(self.ty, depth + 1)?;
        (self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.classes).len(),
            (self.classes).capacity(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedConstant {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.member).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(self.ty, depth + 1)?;
        visitor.constant(&self.value, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynReceiver {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.mutable;
        visitor.node(depth + 1)?;
        (self.mode).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynSlot {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.member).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtDynamicTypeId>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            visitor.type_reference(*item, depth + 1)?;
        }
        visitor.vector_storage(
            (self.parameter_modes).len(),
            (self.parameter_modes).capacity(),
            std::mem::size_of::<PassingMode>(),
            depth + 1,
        )?;
        for item in &self.parameter_modes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.type_reference(self.result, depth + 1)?;
        (self.result_mode).visit_ct_dynamic(visitor, depth + 1)?;
        (self.call_kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.storage_life {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.contract).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynInterface {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.context {
            visitor.type_reference(*item, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.owning_cleanup {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.inherited).len(),
            (self.inherited).capacity(),
            std::mem::size_of::<CtDynTraitApplication>(),
            depth + 1,
        )?;
        for item in &self.inherited {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.associated_types).len(),
            (self.associated_types).capacity(),
            std::mem::size_of::<CtDynAssociatedType>(),
            depth + 1,
        )?;
        for item in &self.associated_types {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.associated_constants).len(),
            (self.associated_constants).capacity(),
            std::mem::size_of::<CtDynAssociatedConstant>(),
            depth + 1,
        )?;
        for item in &self.associated_constants {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.retained_life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.slots).len(),
            (self.slots).capacity(),
            std::mem::size_of::<CtDynSlot>(),
            depth + 1,
        )?;
        for item in &self.slots {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtCoreDynEntry {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.slot;
        visitor.node(depth + 1)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.function;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynEntry {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.slot;
        visitor.node(depth + 1)?;
        let _ = &self.function;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynRetainedPortBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.port;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.sources).len(),
            (self.sources).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.sources {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtCoreDynVtable {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.self_life {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.implementation_witnesses {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynImplementationWitness>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.vector_storage(
            (self.self_classes).len(),
            (self.self_classes).capacity(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(self.self_type, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.ports).len(),
            (self.ports).capacity(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.entries).len(),
            (self.entries).capacity(),
            std::mem::size_of::<CtCoreDynEntry>(),
            depth + 1,
        )?;
        for item in &self.entries {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.cleanup {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.owned_storage {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynVtable {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.self_life {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.implementation_witnesses {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynImplementationWitness>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.vector_storage(
            (self.self_classes).len(),
            (self.self_classes).capacity(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(self.self_type, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.ports).len(),
            (self.ports).capacity(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.entries).len(),
            (self.entries).capacity(),
            std::mem::size_of::<CtDynEntry>(),
            depth + 1,
        )?;
        for item in &self.entries {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.cleanup {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.owned_storage {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynOwnedStorage {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Heap => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynRequirement {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Read(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Write(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Manage(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Apart(field_value_0, field_value_1) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value_1).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::State {
                position: field_position,
                ty: field_ty,
                variant: field_variant,
            } => {
                (field_position).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*field_ty, depth + 1)?;
                let _ = field_variant;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynGuarantee {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::StorageSources(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ManageResult => {}
            Self::State {
                position: field_position,
                ty: field_ty,
                variant: field_variant,
            } => {
                (field_position).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*field_ty, depth + 1)?;
                let _ = field_variant;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynEffect {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Alloc => {}
            Self::Io => {}
            Self::Block => {}
            Self::Suspend => {}
            Self::Spawn => {}
            Self::Ffi => {}
            Self::Reenter => {}
            Self::Panic => {}
            Self::Call(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Cleanup(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynCallAccess {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.reads).len(),
            (self.reads).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.reads {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.writes).len(),
            (self.writes).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.writes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynResultSource {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Value => {}
            Self::Create => {}
            Self::Give => {}
            Self::Borrow {
                sources: field_sources,
                mutable: field_mutable,
            } => {
                visitor.vector_storage(
                    (field_sources).len(),
                    (field_sources).capacity(),
                    std::mem::size_of::<CtStorageSource>(),
                    depth + 1,
                )?;
                for item in field_sources {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                let _ = field_mutable;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynCleanupContract {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.environment;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.positions).len(),
            (self.positions).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.effects).len(),
            (self.effects).capacity(),
            std::mem::size_of::<CtDynEffect>(),
            depth + 1,
        )?;
        for item in &self.effects {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynResumeBound {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Local => {}
            Self::Transfer => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynThreadRequirements {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.transfer;
        visitor.node(depth + 1)?;
        let _ = &self.share;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynContract {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.requires {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynRequirement>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.guarantees {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynGuarantee>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.hold {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.call {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.result {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.cleanup {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.effects {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynEffect>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.resume {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.threads {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynTypeValue {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.type_reference(self.ty, depth + 1)?;
        (self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.classes).len(),
            (self.classes).capacity(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynInstantiationScope {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.fixed_binder_count;
        visitor.node(depth + 1)?;
        let _ = &self.invocation_binder_count;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynLifeEmbedding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.from;
        visitor.node(depth + 1)?;
        let _ = &self.to;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynImplicitLifeKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Address => {}
            Self::Description => {}
            Self::LifetimeArgument => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynImplicitLifeRoot {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::SelfPattern => {}
            Self::ImplementedTrait => {}
            Self::Supertrait(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Requirement(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::AssociatedDefinition(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::MethodSignature(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::MethodDefinition(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynImplicitLifeOccurrence {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.root).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<u32>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynImplicitLifeDeclaration {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.occurrences).len(),
            (self.occurrences).capacity(),
            std::mem::size_of::<CtDynImplicitLifeOccurrence>(),
            depth + 1,
        )?;
        for item in &self.occurrences {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynLifePattern {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::DeclarationParameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::MethodParameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Bound {
                depth: field_depth,
                index: field_index,
            } => {
                let _ = field_depth;
                visitor.node(depth + 1)?;
                let _ = field_index;
                visitor.node(depth + 1)?;
            }
            Self::Invocation(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::ImplicitDeclaration(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynObligation {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Structural {
                receiver: field_receiver,
                bound: field_bound,
            } => {
                (field_receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (field_bound).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Trait {
                receiver: field_receiver,
                application: field_application,
            } => {
                (field_receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (field_application).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Callable {
                receiver: field_receiver,
                kind: field_kind,
                quantified_lifetimes: field_quantified_lifetimes,
                signature: field_signature,
            } => {
                (field_receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (field_kind).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_quantified_lifetimes;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynSignatureRecipe>(), depth + 1)?;
                ((field_signature).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TypeEqual {
                left: field_left,
                right: field_right,
            } => {
                (field_left).visit_ct_dynamic(visitor, depth + 1)?;
                (field_right).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ConstEqual {
                ty: field_ty,
                left: field_left,
                right: field_right,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_left).visit_ct_dynamic(visitor, depth + 1)?;
                (field_right).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynParameterRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.ty).visit_ct_dynamic(visitor, depth + 1)?;
        (self.mode).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynSignatureRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtDynParameterRecipe>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.result).visit_ct_dynamic(visitor, depth + 1)?;
        (self.result_mode).visit_ct_dynamic(visitor, depth + 1)?;
        (self.call_kind).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.invocation_binder_count;
        visitor.node(depth + 1)?;
        (self.contract).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynRequirementRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Read(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Write(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Manage(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Apart(field_value_0, field_value_1) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value_1).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::State {
                position: field_position,
                ty: field_ty,
                variant: field_variant,
            } => {
                (field_position).visit_ct_dynamic(visitor, depth + 1)?;
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_variant;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynGuaranteeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::StorageSources(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ManageResult => {}
            Self::State {
                position: field_position,
                ty: field_ty,
                variant: field_variant,
            } => {
                (field_position).visit_ct_dynamic(visitor, depth + 1)?;
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_variant;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynEffectRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Alloc => {}
            Self::Io => {}
            Self::Block => {}
            Self::Suspend => {}
            Self::Spawn => {}
            Self::Ffi => {}
            Self::Reenter => {}
            Self::Panic => {}
            Self::Call(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Cleanup(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynCleanupRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.environment;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.positions).len(),
            (self.positions).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.effects).len(),
            (self.effects).capacity(),
            std::mem::size_of::<CtDynEffectRecipe>(),
            depth + 1,
        )?;
        for item in &self.effects {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynContractRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.requires {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynRequirementRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.guarantees {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynGuaranteeRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.hold {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<StoragePosition>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.call {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.result {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.cleanup {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.effects {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtDynEffectRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.resume {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.threads {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynStorageSourceRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Exact(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::PlaceAddress {
                place: field_place,
                mode: field_mode,
            } => {
                (field_place).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::OwnedView {
                head: field_head,
                leaf: field_leaf,
                mode: field_mode,
            } => {
                (field_head).visit_ct_dynamic(visitor, depth + 1)?;
                (field_leaf).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::ReborrowedLeaf {
                source: field_source,
                head: field_head,
            } => {
                (field_source).visit_ct_dynamic(visitor, depth + 1)?;
                (field_head).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Static {
                leaf: field_leaf,
                mode: field_mode,
            } => {
                (field_leaf).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynStorageSourceBindingRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.position).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.sources).len(),
            (self.sources).capacity(),
            std::mem::size_of::<CtDynStorageSourceRecipe>(),
            depth + 1,
        )?;
        for item in &self.sources {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynResultSourceRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Value => {}
            Self::Create => {}
            Self::Give => {}
            Self::Borrow {
                sources: field_sources,
                mutable: field_mutable,
            } => {
                visitor.vector_storage(
                    (field_sources).len(),
                    (field_sources).capacity(),
                    std::mem::size_of::<CtDynStorageSourceRecipe>(),
                    depth + 1,
                )?;
                for item in field_sources {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                let _ = field_mutable;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDefinitionId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.source;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDefinitionScope {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::TraitDefault => {}
            Self::ImplOverride => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDefinitionDeclaration {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.requirements).len(),
            (self.requirements).capacity(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.instances).len(),
            (self.instances).capacity(),
            std::mem::size_of::<CtDynDefinitionSpecialization>(),
            depth + 1,
        )?;
        for item in &self.instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.member;
        visitor.node(depth + 1)?;
        (self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        (self.signature).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynMethodRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.requirements).len(),
            (self.requirements).capacity(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        let _ = &self.member;
        visitor.node(depth + 1)?;
        (self.signature).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.default_definition {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynFunctionInstance {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.header_embedding).len(),
            (self.header_embedding).capacity(),
            std::mem::size_of::<CtDynLifeEmbedding>(),
            depth + 1,
        )?;
        for item in &self.header_embedding {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.definition).visit_ct_dynamic(visitor, depth + 1)?;
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.declaration_actuals).len(),
            (self.declaration_actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.method_actuals).len(),
            (self.method_actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.method_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedValueRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Constant {
                ty: field_ty,
                value: field_value,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedDefinition {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.carrier {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        let _ = &self.member;
        visitor.node(depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.value {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.requirements).len(),
            (self.requirements).capacity(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedSource {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::TraitDefault {
                declaration: field_declaration,
                member: field_member,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
            Self::ImplOverride {
                declaration: field_declaration,
                member: field_member,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedValue {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Constant {
                ty: field_ty,
                value: field_value,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.constant(field_value, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynAssociatedResolution {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.member).visit_ct_dynamic(visitor, depth + 1)?;
        (self.source).visit_ct_dynamic(visitor, depth + 1)?;
        (self.value).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynImplementationWitness {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.to_context).len(),
            (self.to_context).capacity(),
            std::mem::size_of::<CtDynLifeEmbedding>(),
            depth + 1,
        )?;
        for item in &self.to_context {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.associated).len(),
            (self.associated).capacity(),
            std::mem::size_of::<CtDynAssociatedResolution>(),
            depth + 1,
        )?;
        for item in &self.associated {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynFunctionTarget {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Core {
                module: field_module,
                source: field_source,
            } => {
                let _ = field_module;
                visitor.node(depth + 1)?;
                let _ = field_source;
                visitor.node(depth + 1)?;
            }
            Self::Typed(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynDefinitionSpecialization {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.function).visit_ct_dynamic(visitor, depth + 1)?;
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.declaration_actuals).len(),
            (self.declaration_actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.method_actuals).len(),
            (self.method_actuals).capacity(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.method_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynStructuralBound {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Copy => {}
            Self::Value => {}
            Self::SyncDrop => {}
            Self::StoreElement => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDefinitionId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.index;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticInstanceId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.index;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticModule {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.definitions).len(),
            (self.definitions).capacity(),
            std::mem::size_of::<CtSemanticDefinition>(),
            depth + 1,
        )?;
        for item in &self.definitions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.instances).len(),
            (self.instances).capacity(),
            std::mem::size_of::<CtSemanticInstance>(),
            depth + 1,
        )?;
        for item in &self.instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.sites).len(),
            (self.sites).capacity(),
            std::mem::size_of::<CtSemanticSite>(),
            depth + 1,
        )?;
        for item in &self.sites {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.dyn_scopes).len(),
            (self.dyn_scopes).capacity(),
            std::mem::size_of::<CtSemanticDynScopeBinding>(),
            depth + 1,
        )?;
        for item in &self.dyn_scopes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDynScopeBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.owner).visit_ct_dynamic(visitor, depth + 1)?;
        (self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDynOwner {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Declaration(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::MethodSignature {
                declaration: field_declaration,
                member: field_member,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
            Self::MethodDefinition(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDefinition {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.parent {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtSemanticFormal>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.implicit_lifetimes).len(),
            (self.implicit_lifetimes).capacity(),
            std::mem::size_of::<CtSemanticImplicitLifetime>(),
            depth + 1,
        )?;
        for item in &self.implicit_lifetimes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.requirements).len(),
            (self.requirements).capacity(),
            std::mem::size_of::<CtSemanticRequirementRecipe>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.environment).len(),
            (self.environment).capacity(),
            std::mem::size_of::<CtSemanticEnvironmentFormal>(),
            depth + 1,
        )?;
        for item in &self.environment {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.body).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDefinitionKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Function => {}
            Self::Method => {}
            Self::Closure => {}
            Self::TaskEntry => {}
            Self::Nominal => {}
            Self::Scope => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDefinitionBody {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Function {
                signature: field_signature,
                sites: field_sites,
            } => {
                (field_signature).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_sites).len(),
                    (field_sites).capacity(),
                    std::mem::size_of::<CtSemanticSiteRecipe>(),
                    depth + 1,
                )?;
                for item in field_sites {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Nominal {
                declaration: field_declaration,
                kind: field_kind,
                body: field_body,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                (field_kind).visit_ct_dynamic(visitor, depth + 1)?;
                (field_body).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Scope(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticScopeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Trait {
                declaration: field_declaration,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::InherentImpl {
                declaration: field_declaration,
                self_type: field_self_type,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                (field_self_type).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TraitImpl {
                declaration: field_declaration,
                self_type: field_self_type,
                trait_application: field_trait_application,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                (field_self_type).visit_ct_dynamic(visitor, depth + 1)?;
                (field_trait_application).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::BuiltinImpl {
                self_type: field_self_type,
                trait_kind: field_trait_kind,
            } => {
                (field_self_type).visit_ct_dynamic(visitor, depth + 1)?;
                (field_trait_kind).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticBuiltinTrait {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Drop => {}
            Self::Copy => {}
            Self::Value => {}
            Self::SyncDrop => {}
            Self::StoreElement => {}
            Self::Transfer => {}
            Self::Share => {}
            Self::ThreadCallable => {}
            Self::ClosedEnvironment => {}
            Self::Send => {}
            Self::Sync => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticNominalBody {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Product {
                fields: field_fields,
            } => {
                visitor.vector_storage(
                    (field_fields).len(),
                    (field_fields).capacity(),
                    std::mem::size_of::<CtSemanticFieldRecipe>(),
                    depth + 1,
                )?;
                for item in field_fields {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Enum {
                variants: field_variants,
            } => {
                visitor.vector_storage(
                    (field_variants).len(),
                    (field_variants).capacity(),
                    std::mem::size_of::<CtSemanticVariantRecipe>(),
                    depth + 1,
                )?;
                for item in field_variants {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticFieldRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.string_storage((self.name).as_str(), (self.name).capacity(), depth + 1)?;
        (self.ty).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticVariantRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.string_storage((self.name).as_str(), (self.name).capacity(), depth + 1)?;
        visitor.vector_storage(
            (self.fields).len(),
            (self.fields).capacity(),
            std::mem::size_of::<CtSemanticTypeRecipe>(),
            depth + 1,
        )?;
        for item in &self.fields {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticFormal {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type {
                requirements: field_requirements,
            } => {
                visitor.vector_storage(
                    (field_requirements).len(),
                    (field_requirements).capacity(),
                    std::mem::size_of::<CtSemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                for item in field_requirements {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Const {
                carrier: field_carrier,
            } => {
                (field_carrier).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Lifetime => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticImplicitLifetime {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.ordinal;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.occurrence_path).len(),
            (self.occurrence_path).capacity(),
            std::mem::size_of::<u32>(),
            depth + 1,
        )?;
        for item in &self.occurrence_path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticEnvironmentFormal {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        (self.source).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticInstance {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.definition).visit_ct_dynamic(visitor, depth + 1)?;
        (self.target).visit_ct_dynamic(visitor, depth + 1)?;
        (self.lifetime_instantiation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.parent {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtSemanticActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.header_embedding).len(),
            (self.header_embedding).capacity(),
            std::mem::size_of::<CtSemanticHeaderBinding>(),
            depth + 1,
        )?;
        for item in &self.header_embedding {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.environment {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.self_type {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticLifetimeInstantiation {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Rigid => {}
            Self::Partition { rigid: field_rigid } => {
                (field_rigid).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticFunctionTarget {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Core {
                module: field_module,
                function: field_function,
            } => {
                let _ = field_module;
                visitor.node(depth + 1)?;
                let _ = field_function;
                visitor.node(depth + 1)?;
            }
            Self::CoreClosure {
                module: field_module,
                creator: field_creator,
                expression: field_expression,
                occurrence: field_occurrence,
            } => {
                let _ = field_module;
                visitor.node(depth + 1)?;
                let _ = field_creator;
                visitor.node(depth + 1)?;
                let _ = field_expression;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
            }
            Self::Typed(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticInstanceTarget {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Function(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Scope => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticClosureTarget {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Core {
                module: field_module,
                creator: field_creator,
                expression: field_expression,
                occurrence: field_occurrence,
            } => {
                let _ = field_module;
                visitor.node(depth + 1)?;
                let _ = field_creator;
                visitor.node(depth + 1)?;
                let _ = field_expression;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
            }
            Self::Typed(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticHeaderBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.header;
        visitor.node(depth + 1)?;
        (self.actual).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticEnvironmentInstance {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.closure).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.actuals).len(),
            (self.actuals).capacity(),
            std::mem::size_of::<CtSemanticActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticActual {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const {
                ty: field_ty,
                value: field_value,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.constant(field_value, depth + 1)?;
            }
            Self::Life(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::CoreRecipe {
                scope: field_scope,
                recipe: field_recipe,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticActualRecipe>(), depth + 1)?;
                ((field_recipe).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticCoreTypeSource {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::OriginalRecipe(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Expression {
                module: field_module,
                function: field_function,
                expression: field_expression,
                occurrence: field_occurrence,
                recipe: field_recipe,
            } => {
                let _ = field_module;
                visitor.node(depth + 1)?;
                let _ = field_function;
                visitor.node(depth + 1)?;
                let _ = field_expression;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_recipe).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticTypeValue {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Resolved {
                ty: field_ty,
                life: field_life,
                classes: field_classes,
            } => {
                visitor.type_reference(*field_ty, depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_classes).len(),
                    (field_classes).capacity(),
                    std::mem::size_of::<CtSemanticClassExpr>(),
                    depth + 1,
                )?;
                for item in field_classes {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Core {
                scope: field_scope,
                source: field_source,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                (field_source).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticScopeRef {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Ancestor(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticClassExpr {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Formal {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Implicit {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Bound {
                depth: field_depth,
                index: field_index,
            } => {
                let _ = field_depth;
                visitor.node(depth + 1)?;
                let _ = field_index;
                visitor.node(depth + 1)?;
            }
            Self::OuterOccurrence {
                operand: field_operand,
                path: field_path,
                mode: field_mode,
            } => {
                (field_operand).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::IntrinsicClass {
                operand: field_operand,
                path: field_path,
                class: field_class,
            } => {
                (field_operand).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_class;
                visitor.node(depth + 1)?;
            }
            Self::Environment {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticOperand {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Parameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Capture(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Receiver => {}
            Self::Value(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Local(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::TypeArgument {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Environment(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::CoreExpression {
                expression: field_expression,
                occurrence: field_occurrence,
            } => {
                let _ = field_expression;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSiteRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.id;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.occurrence_path).len(),
            (self.occurrence_path).capacity(),
            std::mem::size_of::<u32>(),
            depth + 1,
        )?;
        for item in &self.occurrence_path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSiteRecipeKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Construct {
                result: field_result,
            } => {
                (field_result).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Call {
                target: field_target,
                actuals: field_actuals,
            } => {
                (field_target).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_actuals).len(),
                    (field_actuals).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_actuals {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::CallableCall {
                callee: field_callee,
            } => {
                (field_callee).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TypeUse {
                expected: field_expected,
            } => {
                (field_expected).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::DeclareLocal {
                expected: field_expected,
            } => {
                (field_expected).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Closure {
                definition: field_definition,
                environment: field_environment,
            } => {
                (field_definition).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_environment).len(),
                    (field_environment).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_environment {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSite {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.instance).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.recipe;
        visitor.node(depth + 1)?;
        (self.at).visit_ct_dynamic(visitor, depth + 1)?;
        (self.action).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSiteAddress {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Core {
                expression: field_expression,
                occurrence: field_occurrence,
                role: field_role,
            } => {
                let _ = field_expression;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
                (field_role).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Typed {
                block: field_block,
                instruction: field_instruction,
                edge: field_edge,
            } => {
                let _ = field_block;
                visitor.node(depth + 1)?;
                let _ = field_instruction;
                visitor.node(depth + 1)?;
                (field_edge).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticEmissionRole {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Construct => {}
            Self::Call => {}
            Self::Ascription => {}
            Self::Binding => {}
            Self::Closure => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSiteEdge {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Before => {}
            Self::After => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSiteAction {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Construct {
                result: field_result,
                value: field_value,
            } => {
                (field_result).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Call {
                target: field_target,
                actuals: field_actuals,
            } => {
                (field_target).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_actuals).len(),
                    (field_actuals).capacity(),
                    std::mem::size_of::<CtSemanticActual>(),
                    depth + 1,
                )?;
                for item in field_actuals {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::CallableCall {
                callee: field_callee,
            } => {
                (field_callee).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TypeUse {
                subject: field_subject,
                expected: field_expected,
            } => {
                (field_subject).visit_ct_dynamic(visitor, depth + 1)?;
                (field_expected).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::DeclareLocal {
                local: field_local,
                expected: field_expected,
            } => {
                (field_local).visit_ct_dynamic(visitor, depth + 1)?;
                (field_expected).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Closure {
                result: field_result,
                closure: field_closure,
                environment: field_environment,
            } => {
                (field_result).visit_ct_dynamic(visitor, depth + 1)?;
                (field_closure).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_environment).len(),
                    (field_environment).capacity(),
                    std::mem::size_of::<CtSemanticActual>(),
                    depth + 1,
                )?;
                for item in field_environment {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticLocalTarget {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Core {
                statement: field_statement,
                occurrence: field_occurrence,
            } => {
                let _ = field_statement;
                visitor.node(depth + 1)?;
                let _ = field_occurrence;
                visitor.node(depth + 1)?;
            }
            Self::Typed(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticRecipeOperand {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::TypeFormal(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Environment {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Parameter(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Receiver => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticFormalRef {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.ordinal;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticLifeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Formal(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Implicit {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Environment {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Bound {
                depth: field_depth,
                index: field_index,
            } => {
                let _ = field_depth;
                visitor.node(depth + 1)?;
                let _ = field_index;
                visitor.node(depth + 1)?;
            }
            Self::Invocation(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::OuterOccurrence {
                operand: field_operand,
                path: field_path,
                mode: field_mode,
            } => {
                (field_operand).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::IntrinsicClass {
                operand: field_operand,
                path: field_path,
                class: field_class,
            } => {
                (field_operand).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_class;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticOuterLifeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Class(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Static => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticTypeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Atom(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
            Self::Exact(field_value_0) => {
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticExactTypeRecipe>(),
                    depth + 1,
                )?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Formal(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Environment {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::SelfType { scope: field_scope } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Nominal {
                definition: field_definition,
                arguments: field_arguments,
            } => {
                (field_definition).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_arguments).len(),
                    (field_arguments).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_arguments {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::AssociatedType {
                receiver: field_receiver,
                application: field_application,
                member: field_member,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_receiver).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTraitRecipe>(), depth + 1)?;
                ((field_application).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
            Self::Tuple(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtSemanticTypeRecipe>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Array {
                element: field_element,
                length: field_length,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_element).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                ((field_length).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Slice(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Address {
                mutable: field_mutable,
                lifetime: field_lifetime,
                inner: field_inner,
            } => {
                let _ = field_mutable;
                visitor.node(depth + 1)?;
                (field_lifetime).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_inner).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Desc {
                lifetime: field_lifetime,
                inner: field_inner,
            } => {
                (field_lifetime).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_inner).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Managed(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::OwnedStore(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Mutex(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::MutexGuard {
                payload: field_payload,
                retained: field_retained,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_payload).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                ((field_retained).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Registration(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AsyncRegistration(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Task {
                result: field_result,
                retained: field_retained,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_result).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticRetainedShapeRecipe>(),
                    depth + 1,
                )?;
                ((field_retained).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ErrorUnion {
                error: field_error,
                result: field_result,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_error).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_result).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Qualified {
                storage: field_storage,
                life: field_life,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_storage).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticStorageShapeRecipe>(),
                    depth + 1,
                )?;
                ((field_life).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Code {
                kind: field_kind,
                signature: field_signature,
            } => {
                (field_kind).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticSignatureRecipe>(),
                    depth + 1,
                )?;
                ((field_signature).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Closure {
                definition: field_definition,
                arguments: field_arguments,
                environment: field_environment,
            } => {
                (field_definition).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_arguments).len(),
                    (field_arguments).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_arguments {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                visitor.vector_storage(
                    (field_environment).len(),
                    (field_environment).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_environment {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Dyn {
                owned: field_owned,
                interface: field_interface,
            } => {
                let _ = field_owned;
                visitor.node(depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticDynInterfaceRecipe>(),
                    depth + 1,
                )?;
                ((field_interface).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Opaque {
                definition: field_definition,
                arguments: field_arguments,
                hidden: field_hidden,
            } => {
                (field_definition).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_arguments).len(),
                    (field_arguments).capacity(),
                    std::mem::size_of::<CtSemanticActualRecipe>(),
                    depth + 1,
                )?;
                for item in field_arguments {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_hidden).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AnonymousSum(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtSemanticTypeRecipe>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::UnresolvedNamed(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::Unsupported(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticExactTypeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.type_reference(self.ty, depth + 1)?;
        (self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.classes).len(),
            (self.classes).capacity(),
            std::mem::size_of::<CtSemanticLifeRecipe>(),
            depth + 1,
        )?;
        for item in &self.classes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticCodeKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::FunctionPointer => {}
            Self::BorrowedFunctionPointer => {}
            Self::ForeignCallback => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticActualRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const {
                carrier: field_carrier,
                value: field_value,
            } => {
                (field_carrier).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Life(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticClassSequenceRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.parts).len(),
            (self.parts).capacity(),
            std::mem::size_of::<CtSemanticClassSequencePart>(),
            depth + 1,
        )?;
        for item in &self.parts {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticClassSequencePart {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::One(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AllClasses {
                ty: field_ty,
                path: field_path,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticStorageShapeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.domain).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.slots).len(),
            (self.slots).capacity(),
            std::mem::size_of::<CtSemanticSlotRecipe>(),
            depth + 1,
        )?;
        for item in &self.slots {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.retained).len(),
            (self.retained).capacity(),
            std::mem::size_of::<CtSemanticRetainedPart>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.nominal).len(),
            (self.nominal).capacity(),
            std::mem::size_of::<CtSemanticNominalPart>(),
            depth + 1,
        )?;
        for item in &self.nominal {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSlotRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<PlaceProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        let _ = &self.mode;
        visitor.node(depth + 1)?;
        (self.class).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticRetainedPart {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::One {
                path: field_path,
                child_class: field_child_class,
                parent: field_parent,
            } => {
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_child_class;
                visitor.node(depth + 1)?;
                (field_parent).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::All {
                path: field_path,
                parents: field_parents,
            } => {
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                (field_parents).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticNominalPart {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::One {
                path: field_path,
                declaration: field_declaration,
                argument: field_argument,
                class: field_class,
            } => {
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_argument;
                visitor.node(depth + 1)?;
                (field_class).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::FromType {
                path: field_path,
                ty: field_ty,
                classes: field_classes,
            } => {
                visitor.vector_storage(
                    (field_path).len(),
                    (field_path).capacity(),
                    std::mem::size_of::<LifeTypeProjection>(),
                    depth + 1,
                )?;
                for item in field_path {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                (field_classes).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticRetainedShapeRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.domain).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.ports).len(),
            (self.ports).capacity(),
            std::mem::size_of::<CtSemanticPortPart>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.retained).len(),
            (self.retained).capacity(),
            std::mem::size_of::<CtSemanticRetainedPart>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.nominal).len(),
            (self.nominal).capacity(),
            std::mem::size_of::<CtSemanticNominalPart>(),
            depth + 1,
        )?;
        for item in &self.nominal {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticPortPart {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::One {
                leaf: field_leaf,
                mode: field_mode,
                class: field_class,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_leaf).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
                (field_class).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::FromType {
                ty: field_ty,
                classes: field_classes,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_ty).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                (field_classes).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticConstRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Value(field_value_0) => {
                visitor.constant(field_value_0, depth + 1)?;
            }
            Self::IntegerLiteral(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::FloatLiteral(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::Formal(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Environment {
                scope: field_scope,
                ordinal: field_ordinal,
            } => {
                (field_scope).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_ordinal;
                visitor.node(depth + 1)?;
            }
            Self::Unary {
                op: field_op,
                operand_type: field_operand_type,
                value: field_value,
            } => {
                let _ = field_op;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_operand_type).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                ((field_value).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Binary {
                op: field_op,
                operand_type: field_operand_type,
                left: field_left,
                right: field_right,
            } => {
                let _ = field_op;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_operand_type).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                ((field_left).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                ((field_right).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Convert {
                source_type: field_source_type,
                target_type: field_target_type,
                value: field_value,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_source_type).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_target_type).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticConstRecipe>(), depth + 1)?;
                ((field_value).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AssociatedConst {
                receiver: field_receiver,
                application: field_application,
                member: field_member,
            } => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_receiver).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtSemanticTraitRecipe>(), depth + 1)?;
                ((field_application).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_member;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticTraitRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.arguments).len(),
            (self.arguments).capacity(),
            std::mem::size_of::<CtSemanticActualRecipe>(),
            depth + 1,
        )?;
        for item in &self.arguments {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticRequirementRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Structural {
                ty: field_ty,
                bound: field_bound,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_bound).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Trait {
                ty: field_ty,
                application: field_application,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_application).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Callable {
                ty: field_ty,
                kind: field_kind,
                signature: field_signature,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_kind).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.rows(
                    1,
                    std::mem::size_of::<CtSemanticSignatureRecipe>(),
                    depth + 1,
                )?;
                ((field_signature).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TypeEqual {
                left: field_left,
                right: field_right,
            } => {
                (field_left).visit_ct_dynamic(visitor, depth + 1)?;
                (field_right).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ConstEqual {
                carrier: field_carrier,
                left: field_left,
                right: field_right,
            } => {
                (field_carrier).visit_ct_dynamic(visitor, depth + 1)?;
                (field_left).visit_ct_dynamic(visitor, depth + 1)?;
                (field_right).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::LifeEqual {
                left: field_left,
                right: field_right,
            } => {
                (field_left).visit_ct_dynamic(visitor, depth + 1)?;
                (field_right).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Outlives {
                longer: field_longer,
                shorter: field_shorter,
            } => {
                (field_longer).visit_ct_dynamic(visitor, depth + 1)?;
                (field_shorter).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ForAll {
                lifetimes: field_lifetimes,
                requirements: field_requirements,
            } => {
                let _ = field_lifetimes;
                visitor.node(depth + 1)?;
                visitor.vector_storage(
                    (field_requirements).len(),
                    (field_requirements).capacity(),
                    std::mem::size_of::<CtSemanticRequirementRecipe>(),
                    depth + 1,
                )?;
                for item in field_requirements {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Builtin {
                ty: field_ty,
                trait_kind: field_trait_kind,
            } => {
                (field_ty).visit_ct_dynamic(visitor, depth + 1)?;
                (field_trait_kind).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticSignatureRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.bound_lifetimes;
        visitor.node(depth + 1)?;
        let _ = &self.invocation_lifetimes;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtSemanticParameterRecipe>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.result).visit_ct_dynamic(visitor, depth + 1)?;
        (self.result_mode).visit_ct_dynamic(visitor, depth + 1)?;
        (self.call_kind).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticParameterRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.ty).visit_ct_dynamic(visitor, depth + 1)?;
        (self.mode).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDynContextRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Closed => {}
            Self::Type(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtSemanticTypeRecipe>(), depth + 1)?;
                ((field_value_0).as_ref()).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticDynInterfaceRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        (self.context).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.associated).len(),
            (self.associated).capacity(),
            std::mem::size_of::<CtSemanticAssociatedBindingRecipe>(),
            depth + 1,
        )?;
        for item in &self.associated {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticAssociatedBindingRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.member;
        visitor.node(depth + 1)?;
        (self.value).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtSemanticAssociatedRecipe {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const {
                carrier: field_carrier,
                value: field_value,
            } => {
                (field_carrier).visit_ct_dynamic(visitor, depth + 1)?;
                (field_value).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtStorageSource {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Exact(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::PlaceAddress {
                place: field_place,
                mode: field_mode,
            } => {
                (field_place).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::OwnedView {
                head: field_head,
                leaf: field_leaf,
                mode: field_mode,
            } => {
                (field_head).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*field_leaf, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
            Self::ReborrowedLeaf {
                source: field_source,
                head: field_head,
            } => {
                (field_source).visit_ct_dynamic(visitor, depth + 1)?;
                (field_head).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Static {
                leaf: field_leaf,
                mode: field_mode,
            } => {
                visitor.type_reference(*field_leaf, depth + 1)?;
                let _ = field_mode;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtRetainedBorrowPort {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.type_reference(self.leaf, depth + 1)?;
        let _ = &self.mode;
        visitor.node(depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtRetainedLifeShape {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.binder_count;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.ports).len(),
            (self.ports).capacity(),
            std::mem::size_of::<CtRetainedBorrowPort>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.retained).len(),
            (self.retained).capacity(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.nominal_arguments).len(),
            (self.nominal_arguments).capacity(),
            std::mem::size_of::<NominalLifeArgument>(),
            depth + 1,
        )?;
        for item in &self.nominal_arguments {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtStorageSourceBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.position).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.sources).len(),
            (self.sources).capacity(),
            std::mem::size_of::<CtStorageSource>(),
            depth + 1,
        )?;
        for item in &self.sources {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifeNominalKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Record => {}
            Self::Package => {}
            Self::Resource => {}
            Self::Enum => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifeParameterKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type => {}
            Self::Const(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
            Self::Lifetime => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifeNominalDeclaration {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        (self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.parameters).len(),
            (self.parameters).capacity(),
            std::mem::size_of::<CtLifeParameterKind>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifeNominalActual {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                visitor.type_reference(*field_value_0, depth + 1)?;
            }
            Self::QualifiedType {
                ty: field_ty,
                life: field_life,
            } => {
                visitor.type_reference(*field_ty, depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(field_value_0) => {
                visitor.constant(field_value_0, depth + 1)?;
            }
            Self::Lifetime(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifeNominalInstance {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(self.ty, depth + 1)?;
        visitor.vector_storage(
            (self.arguments).len(),
            (self.arguments).capacity(),
            std::mem::size_of::<CtLifeNominalActual>(),
            depth + 1,
        )?;
        for item in &self.arguments {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynamicTypeNode {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Dyn {
                interface: field_interface,
            } => {
                (field_interface).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::OwnedDyn {
                interface: field_interface,
            } => {
                (field_interface).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::BorrowedStorage {
                storage: field_storage,
                life: field_life,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_storage).as_ref(), depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Unit => {}
            Self::Bool => {}
            Self::I32 => {}
            Self::I64 => {}
            Self::U64 => {}
            Self::Usize => {}
            Self::F32 => {}
            Self::F64 => {}
            Self::U8 => {}
            Self::U16 => {}
            Self::U32 => {}
            Self::I8 => {}
            Self::I16 => {}
            Self::Isize => {}
            Self::ByteBuffer => {}
            Self::OwnedStore {
                element: field_element,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_element).as_ref(), depth + 1)?;
            }
            Self::ForeignHandle => {}
            Self::SyncCell(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Mutex {
                payload: field_payload,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_payload).as_ref(), depth + 1)?;
            }
            Self::MutexGuard {
                payload: field_payload,
                retained_life: field_retained_life,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_payload).as_ref(), depth + 1)?;
                (field_retained_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Registration {
                environment: field_environment,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_environment).as_ref(), depth + 1)?;
            }
            Self::AsyncRegistration {
                environment: field_environment,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_environment).as_ref(), depth + 1)?;
            }
            Self::TaskI32 => {}
            Self::Task {
                result: field_result,
                retained_life: field_retained_life,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_result).as_ref(), depth + 1)?;
                (field_retained_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::CWideString => {}
            Self::ForeignCallback {
                parameters: field_parameters,
                result: field_result,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_result).as_ref(), depth + 1)?;
            }
            Self::Tuple(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    visitor.type_reference(*item, depth + 1)?;
                }
            }
            Self::Array {
                element: field_element,
                length: field_length,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_element).as_ref(), depth + 1)?;
                let _ = field_length;
                visitor.node(depth + 1)?;
            }
            Self::Slice {
                element: field_element,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_element).as_ref(), depth + 1)?;
            }
            Self::Named(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
            Self::Record {
                name: field_name,
                fields: field_fields,
            } => {
                visitor.string_storage(
                    (field_name).as_str(),
                    (field_name).capacity(),
                    depth + 1,
                )?;
                visitor.vector_storage(
                    (field_fields).len(),
                    (field_fields).capacity(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in field_fields {
                    visitor.string_storage(
                        ((item).0).as_str(),
                        ((item).0).capacity(),
                        depth + 1,
                    )?;
                    visitor.type_reference((item).1, depth + 1)?;
                }
            }
            Self::Package {
                name: field_name,
                fields: field_fields,
            } => {
                visitor.string_storage(
                    (field_name).as_str(),
                    (field_name).capacity(),
                    depth + 1,
                )?;
                visitor.vector_storage(
                    (field_fields).len(),
                    (field_fields).capacity(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in field_fields {
                    visitor.string_storage(
                        ((item).0).as_str(),
                        ((item).0).capacity(),
                        depth + 1,
                    )?;
                    visitor.type_reference((item).1, depth + 1)?;
                }
            }
            Self::Enum {
                name: field_name,
                variants: field_variants,
            } => {
                visitor.string_storage(
                    (field_name).as_str(),
                    (field_name).capacity(),
                    depth + 1,
                )?;
                visitor.vector_storage(
                    (field_variants).len(),
                    (field_variants).capacity(),
                    std::mem::size_of::<(String, Vec<CtDynamicTypeId>)>(),
                    depth + 1,
                )?;
                for item in field_variants {
                    visitor.string_storage(
                        ((item).0).as_str(),
                        ((item).0).capacity(),
                        depth + 1,
                    )?;
                    visitor.vector_storage(
                        ((item).1).len(),
                        ((item).1).capacity(),
                        std::mem::size_of::<CtDynamicTypeId>(),
                        depth + 1,
                    )?;
                    for item in &(item).1 {
                        visitor.type_reference(*item, depth + 1)?;
                    }
                }
            }
            Self::ErrorUnion {
                error: field_error,
                result: field_result,
            } => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_error).as_ref(), depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_result).as_ref(), depth + 1)?;
            }
            Self::Address {
                mutable: field_mutable,
                inner: field_inner,
            } => {
                let _ = field_mutable;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_inner).as_ref(), depth + 1)?;
            }
            Self::Managed(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_value_0).as_ref(), depth + 1)?;
            }
            Self::Desc(field_value_0) => {
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_value_0).as_ref(), depth + 1)?;
            }
            Self::FunctionPointer {
                parameters: field_parameters,
                result: field_result,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_result).as_ref(), depth + 1)?;
            }
            Self::BorrowedFunctionPointer {
                parameters: field_parameters,
                result: field_result,
                life: field_life,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_result).as_ref(), depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Closure(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Opaque {
                identity: field_identity,
                hidden: field_hidden,
            } => {
                let _ = field_identity;
                visitor.node(depth + 1)?;
                visitor.rows(1, std::mem::size_of::<CtDynamicTypeId>(), depth + 1)?;
                visitor.type_reference(*(field_hidden).as_ref(), depth + 1)?;
            }
            Self::Unsupported(field_value_0) => {
                visitor.string_storage(
                    (field_value_0).as_str(),
                    (field_value_0).capacity(),
                    depth + 1,
                )?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CallKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Direct => {}
            Self::Suspend => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for LifeDeclarationIdentity {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.string_storage(
            (self.package).as_str(),
            (self.package).capacity(),
            depth + 1,
        )?;
        visitor.vector_storage(
            (self.module_path).len(),
            (self.module_path).capacity(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.module_path {
            visitor.string_storage((item).as_str(), (item).capacity(), depth + 1)?;
        }
        visitor.vector_storage(
            (self.declaration_path).len(),
            (self.declaration_path).capacity(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.declaration_path {
            visitor.string_storage((item).as_str(), (item).capacity(), depth + 1)?;
        }
        visitor.vector_storage(
            (self.generation_path).len(),
            (self.generation_path).capacity(),
            std::mem::size_of::<u32>(),
            depth + 1,
        )?;
        for item in &self.generation_path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for PassingMode {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Copy => {}
            Self::Move => {}
            Self::Describe => {}
            Self::Create => {}
            Self::Give => {}
            Self::Borrow => {}
            Self::BorrowMut => {}
            Self::Infer => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CallableKind {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::FunctionPointer => {}
            Self::Fn => {}
            Self::FnMut => {}
            Self::FnOnce => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for StorageLifeSlot {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<PlaceProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        let _ = &self.mode;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for LifeDeclarationId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.declaration;
        visitor.node(depth + 1)?;
        let _ = &self.compiler;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for StorageRetainedBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<LifeTypeProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        let _ = &self.class;
        visitor.node(depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for NominalLifeArgument {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<LifeTypeProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.argument;
        visitor.node(depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for StorageLifeShape {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.binder_count;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.slots).len(),
            (self.slots).capacity(),
            std::mem::size_of::<StorageLifeSlot>(),
            depth + 1,
        )?;
        for item in &self.slots {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.retained).len(),
            (self.retained).capacity(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.nominal_arguments).len(),
            (self.nominal_arguments).capacity(),
            std::mem::size_of::<NominalLifeArgument>(),
            depth + 1,
        )?;
        for item in &self.nominal_arguments {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for StoragePosition {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.base;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<ContractProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for StorageLifeBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.position).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for SignatureLifeClass {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.base;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.path).len(),
            (self.path).capacity(),
            std::mem::size_of::<LifeTypeProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        let _ = &self.class;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for SignatureLifeClassBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.position).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for StorageLifeContract {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.fixed_binder_count;
        visitor.node(depth + 1)?;
        let _ = &self.invocation_binder_count;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.bindings).len(),
            (self.bindings).capacity(),
            std::mem::size_of::<StorageLifeBinding>(),
            depth + 1,
        )?;
        for item in &self.bindings {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.static_bindings).len(),
            (self.static_bindings).capacity(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.static_bindings {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.classes).len(),
            (self.classes).capacity(),
            std::mem::size_of::<SignatureLifeClassBinding>(),
            depth + 1,
        )?;
        for item in &self.classes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynamicCatalogue {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.semantic {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.stage {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.facet_requests).len(),
            (self.facet_requests).capacity(),
            std::mem::size_of::<CtStageFacetRequest>(),
            depth + 1,
        )?;
        for item in &self.facet_requests {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.types).len(),
            (self.types).capacity(),
            std::mem::size_of::<CtDynamicTypeNode>(),
            depth + 1,
        )?;
        for (index, node) in (self.types).iter().enumerate() {
            visitor.type_node(index, node, depth + 1)?;
        }
        visitor.vector_storage(
            (self.life_declarations).len(),
            (self.life_declarations).capacity(),
            std::mem::size_of::<CtLifeNominalDeclaration>(),
            depth + 1,
        )?;
        for item in &self.life_declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.life_instances).len(),
            (self.life_instances).capacity(),
            std::mem::size_of::<CtLifeNominalInstance>(),
            depth + 1,
        )?;
        for item in &self.life_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.declarations).len(),
            (self.declarations).capacity(),
            std::mem::size_of::<CtDynDeclaration>(),
            depth + 1,
        )?;
        for item in &self.declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.interfaces).len(),
            (self.interfaces).capacity(),
            std::mem::size_of::<CtDynInterface>(),
            depth + 1,
        )?;
        for item in &self.interfaces {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.vtables).len(),
            (self.vtables).capacity(),
            std::mem::size_of::<CtCoreDynVtable>(),
            depth + 1,
        )?;
        for item in &self.vtables {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.function_instances).len(),
            (self.function_instances).capacity(),
            std::mem::size_of::<CtDynFunctionInstanceRecord>(),
            depth + 1,
        )?;
        for item in &self.function_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.function_sources).len(),
            (self.function_sources).capacity(),
            std::mem::size_of::<CtFunctionSourceBindings>(),
            depth + 1,
        )?;
        for item in &self.function_sources {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtDynFunctionInstanceRecord {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.source;
        visitor.node(depth + 1)?;
        (self.instance).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtFunctionSourceBindings {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.source;
        visitor.node(depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.retained_return_sources {
            visitor.vector_storage(
                (item).len(),
                (item).capacity(),
                std::mem::size_of::<CtStorageSourceBinding>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtStageIdentity {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.source_fingerprint;
        visitor.node(depth + 1)?;
        let _ = &self.epoch;
        visitor.node(depth + 1)?;
        let _ = &self.committed_generation_fingerprint;
        visitor.node(depth + 1)?;
        let _ = &self.declaration_inventory_fingerprint;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtStageFacet {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Shape => {}
            Self::Contract => {}
            Self::Layout => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtStageSubject {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Declaration(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Type(field_value_0) => {
                visitor.type_handle(field_value_0, depth + 1)?;
            }
            Self::Instance {
                declaration: field_declaration,
                context: field_context,
                arguments: field_arguments,
            } => {
                (field_declaration).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_handle(field_context, depth + 1)?;
                visitor.vector_storage(
                    (field_arguments).len(),
                    (field_arguments).capacity(),
                    std::mem::size_of::<CtTypeArgument>(),
                    depth + 1,
                )?;
                for item in field_arguments {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtBodySourceRef {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.body_fingerprint;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtStageFacetRequest {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.subject).visit_ct_dynamic(visitor, depth + 1)?;
        (self.facet).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.content_fingerprint {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.body_source {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtTypeCatalogue {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        (self.stage).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.vector_storage(
            (self.types).len(),
            (self.types).capacity(),
            std::mem::size_of::<CtDynamicTypeNode>(),
            depth + 1,
        )?;
        for (index, node) in (self.types).iter().enumerate() {
            visitor.type_node(index, node, depth + 1)?;
        }
        visitor.vector_storage(
            (self.interfaces).len(),
            (self.interfaces).capacity(),
            std::mem::size_of::<CtDynInterface>(),
            depth + 1,
        )?;
        for item in &self.interfaces {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtTypeRef {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.scoped_catalogue {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.vector_storage(
            (self.nodes).len(),
            (self.nodes).capacity(),
            std::mem::size_of::<CtTypeNode>(),
            depth + 1,
        )?;
        for item in &self.nodes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        let _ = &self.root;
        visitor.node(depth + 1)?;
        visitor.vector_storage(
            (self.ambient_classes).len(),
            (self.ambient_classes).capacity(),
            std::mem::size_of::<CtAmbientClassBinding>(),
            depth + 1,
        )?;
        for item in &self.ambient_classes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtAmbientClassBinding {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.owner;
        visitor.node(depth + 1)?;
        let _ = &self.class;
        visitor.node(depth + 1)?;
        (self.source).visit_ct_dynamic(visitor, depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtAmbientLifeReference {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.scope;
        visitor.node(depth + 1)?;
        let _ = &self.parameter;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtTypeNode {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::ScopedDyn {
                descriptor: field_descriptor,
                context: field_context,
                classes: field_classes,
            } => {
                visitor.type_reference(*field_descriptor, depth + 1)?;
                visitor.node(depth + 1)?;
                if let Some(item) = field_context {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                visitor.vector_storage(
                    (field_classes).len(),
                    (field_classes).capacity(),
                    std::mem::size_of::<CtLifetimeArgument>(),
                    depth + 1,
                )?;
                for item in field_classes {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::ScopedTask {
                descriptor: field_descriptor,
                result: field_result,
                context: field_context,
                classes: field_classes,
            } => {
                visitor.type_reference(*field_descriptor, depth + 1)?;
                let _ = field_result;
                visitor.node(depth + 1)?;
                visitor.node(depth + 1)?;
                if let Some(item) = field_context {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                visitor.vector_storage(
                    (field_classes).len(),
                    (field_classes).capacity(),
                    std::mem::size_of::<CtLifetimeArgument>(),
                    depth + 1,
                )?;
                for item in field_classes {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::BorrowedStorage {
                storage: field_storage,
                life: field_life,
            } => {
                let _ = field_storage;
                visitor.node(depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Primitive(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Builtin(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Nominal {
                symbol: field_symbol,
                arguments: field_arguments,
            } => {
                (field_symbol).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.vector_storage(
                    (field_arguments).len(),
                    (field_arguments).capacity(),
                    std::mem::size_of::<CtTypeArgument>(),
                    depth + 1,
                )?;
                for item in field_arguments {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Tuple(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtTypeNodeId>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
            }
            Self::Array {
                element: field_element,
                length: field_length,
            } => {
                let _ = field_element;
                visitor.node(depth + 1)?;
                let _ = field_length;
                visitor.node(depth + 1)?;
            }
            Self::FunctionPointer {
                parameters: field_parameters,
                result: field_result,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtTypeNodeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_result;
                visitor.node(depth + 1)?;
            }
            Self::BorrowedFunctionPointer {
                parameters: field_parameters,
                result: field_result,
                life: field_life,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtTypeNodeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_result;
                visitor.node(depth + 1)?;
                (field_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Address {
                mutable: field_mutable,
                inner: field_inner,
            } => {
                let _ = field_mutable;
                visitor.node(depth + 1)?;
                let _ = field_inner;
                visitor.node(depth + 1)?;
            }
            Self::Slice(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Managed(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Desc(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::OwnedStore(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::ForeignCallback {
                parameters: field_parameters,
                result: field_result,
            } => {
                visitor.vector_storage(
                    (field_parameters).len(),
                    (field_parameters).capacity(),
                    std::mem::size_of::<CtTypeNodeId>(),
                    depth + 1,
                )?;
                for item in field_parameters {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
                let _ = field_result;
                visitor.node(depth + 1)?;
            }
            Self::Opaque {
                identity: field_identity,
            } => {
                (field_identity).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ErrorUnion {
                error: field_error,
                result: field_result,
            } => {
                let _ = field_error;
                visitor.node(depth + 1)?;
                let _ = field_result;
                visitor.node(depth + 1)?;
            }
            Self::AnonymousSum(field_value_0) => {
                visitor.vector_storage(
                    (field_value_0).len(),
                    (field_value_0).capacity(),
                    std::mem::size_of::<CtTypeNodeId>(),
                    depth + 1,
                )?;
                for item in field_value_0 {
                    let _ = item;
                    visitor.node(depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtSymbolId {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.module;
        visitor.node(depth + 1)?;
        let _ = &self.declaration;
        visitor.node(depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.generation_path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtTypeArgument {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Type(field_value_0) => {
                let _ = field_value_0;
                visitor.node(depth + 1)?;
            }
            Self::Scalar(field_value_0) => {
                visitor.constant(field_value_0, depth + 1)?;
            }
            Self::Lifetime(field_value_0) => {
                (field_value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtLifetimeArgument {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        let _ = &self.owner;
        visitor.node(depth + 1)?;
        let _ = &self.binder;
        visitor.node(depth + 1)?;
        Ok(())
    }
}
impl CtDynamicMetadata for CtBuiltinType {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::ByteBuffer => {}
            Self::ForeignHandle => {}
            Self::CWideString => {}
        }
        Ok(())
    }
}
impl CtDynamicMetadata for CtScalarTy {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::Unit => {}
            Self::Bool => {}
            Self::U8 => {}
            Self::U16 => {}
            Self::U32 => {}
            Self::I8 => {}
            Self::I16 => {}
            Self::Isize => {}
            Self::I32 => {}
            Self::I64 => {}
            Self::U64 => {}
            Self::Usize => {}
            Self::F32 => {}
            Self::F64 => {}
        }
        Ok(())
    }
}
