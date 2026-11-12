//! Structural CT metadata traversal; callbacks carry the caller's own budget.
use super::*;
use crate::*;
pub trait CtDynamicVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize)
    -> Result<(), Self::Error>;
    fn type_node(
        &mut self,
        index: usize,
        node: &CtDynamicTypeNode,
        depth: usize,
    ) -> Result<(), Self::Error>;
    fn type_reference(&mut self, id: CtDynamicTypeId, depth: usize) -> Result<(), Self::Error>;
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
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynImplicitLifeDeclaration>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.direct_supertraits {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynSelfPattern>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.requirements {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynObligation>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.associated_definitions {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynAssociatedDefinition>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.method_signatures {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynMethodRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.method_definitions {
            visitor.rows(
                (item).len(),
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
        visitor.rows(
            (self.parameters).len(),
            std::mem::size_of::<CtLifeParameterKind>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.members).len(),
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
        visitor.rows(
            (self.arguments).len(),
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
                visitor.text(field_value_0, depth + 1)?;
            }
            Self::FloatLiteral(field_value_0) => {
                visitor.text(field_value_0, depth + 1)?;
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
                visitor.rows(
                    (field_value_0).len(),
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
        visitor.text(&self.name, depth + 1)?;
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
                visitor.rows(
                    (field_value_0).len(),
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
        visitor.rows(
            (self.actuals).len(),
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
        visitor.type_reference(*&self.ty, depth + 1)?;
        (self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.classes).len(),
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
        visitor.type_reference(*&self.ty, depth + 1)?;
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
        visitor.rows(
            (self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.parameters).len(),
            std::mem::size_of::<CtDynamicTypeId>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            visitor.type_reference(*item, depth + 1)?;
        }
        visitor.rows(
            (self.parameter_modes).len(),
            std::mem::size_of::<PassingMode>(),
            depth + 1,
        )?;
        for item in &self.parameter_modes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.type_reference(*&self.result, depth + 1)?;
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
        visitor.rows(
            (self.inherited).len(),
            std::mem::size_of::<CtDynTraitApplication>(),
            depth + 1,
        )?;
        for item in &self.inherited {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.associated_types).len(),
            std::mem::size_of::<CtDynAssociatedType>(),
            depth + 1,
        )?;
        for item in &self.associated_types {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.associated_constants).len(),
            std::mem::size_of::<CtDynAssociatedConstant>(),
            depth + 1,
        )?;
        for item in &self.associated_constants {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.retained_life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.slots).len(),
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
        visitor.rows(
            (self.sources).len(),
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
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynImplementationWitness>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.rows(
            (self.self_classes).len(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.self_type, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.ports).len(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.entries).len(),
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
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynImplementationWitness>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.rows(
            (self.self_classes).len(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.self_type, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.ports).len(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.entries).len(),
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
        visitor.rows(
            (self.reads).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.reads {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.writes).len(),
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
                visitor.rows(
                    (field_sources).len(),
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
        visitor.rows(
            (self.positions).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.effects).len(),
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
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynRequirement>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.guarantees {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynGuarantee>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.hold {
            visitor.rows(
                (item).len(),
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
            visitor.rows((item).len(), std::mem::size_of::<CtDynEffect>(), depth + 1)?;
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
        visitor.type_reference(*&self.ty, depth + 1)?;
        (self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.classes).len(),
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
        visitor.rows((self.path).len(), std::mem::size_of::<u32>(), depth + 1)?;
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
        visitor.rows(
            (self.occurrences).len(),
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
        visitor.rows(
            (self.parameters).len(),
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
        visitor.rows(
            (self.positions).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.effects).len(),
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
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynRequirementRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.guarantees {
            visitor.rows(
                (item).len(),
                std::mem::size_of::<CtDynGuaranteeRecipe>(),
                depth + 1,
            )?;
            for item in item {
                (item).visit_ct_dynamic(visitor, depth + 1)?;
            }
        }
        visitor.node(depth + 1)?;
        if let Some(item) = &self.hold {
            visitor.rows(
                (item).len(),
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
            visitor.rows(
                (item).len(),
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
        visitor.rows(
            (self.requirements).len(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.instances).len(),
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
        visitor.rows(
            (self.requirements).len(),
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
        visitor.rows(
            (self.header_embedding).len(),
            std::mem::size_of::<CtDynLifeEmbedding>(),
            depth + 1,
        )?;
        for item in &self.header_embedding {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.definition).visit_ct_dynamic(visitor, depth + 1)?;
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.declaration_actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.method_actuals).len(),
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
        visitor.rows(
            (self.requirements).len(),
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
        visitor.rows(
            (self.to_context).len(),
            std::mem::size_of::<CtDynLifeEmbedding>(),
            depth + 1,
        )?;
        for item in &self.to_context {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        (self.application).visit_ct_dynamic(visitor, depth + 1)?;
        (self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.associated).len(),
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
        visitor.rows(
            (self.declaration_actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.method_actuals).len(),
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
        visitor.type_reference(*&self.leaf, depth + 1)?;
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
        visitor.rows(
            (self.ports).len(),
            std::mem::size_of::<CtRetainedBorrowPort>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.retained).len(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.nominal_arguments).len(),
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
        visitor.rows(
            (self.sources).len(),
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
        visitor.rows(
            (self.parameters).len(),
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
        visitor.type_reference(*&self.ty, depth + 1)?;
        visitor.rows(
            (self.arguments).len(),
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
                visitor.rows(
                    (field_parameters).len(),
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
                visitor.rows(
                    (field_value_0).len(),
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
                visitor.text(field_value_0, depth + 1)?;
            }
            Self::Record {
                name: field_name,
                fields: field_fields,
            } => {
                visitor.text(field_name, depth + 1)?;
                visitor.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in field_fields {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.type_reference(*&(item).1, depth + 1)?;
                }
            }
            Self::Package {
                name: field_name,
                fields: field_fields,
            } => {
                visitor.text(field_name, depth + 1)?;
                visitor.rows(
                    (field_fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in field_fields {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.type_reference(*&(item).1, depth + 1)?;
                }
            }
            Self::Enum {
                name: field_name,
                variants: field_variants,
            } => {
                visitor.text(field_name, depth + 1)?;
                visitor.rows(
                    (field_variants).len(),
                    std::mem::size_of::<(String, Vec<CtDynamicTypeId>)>(),
                    depth + 1,
                )?;
                for item in field_variants {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.rows(
                        ((item).1).len(),
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
                visitor.rows(
                    (field_parameters).len(),
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
                visitor.rows(
                    (field_parameters).len(),
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
                visitor.text(field_value_0, depth + 1)?;
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
        visitor.text(&self.package, depth + 1)?;
        visitor.rows(
            (self.module_path).len(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.module_path {
            visitor.text(item, depth + 1)?;
        }
        visitor.rows(
            (self.declaration_path).len(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.declaration_path {
            visitor.text(item, depth + 1)?;
        }
        visitor.rows(
            (self.generation_path).len(),
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
        visitor.rows(
            (self.path).len(),
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
        visitor.rows(
            (self.path).len(),
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
        visitor.rows(
            (self.path).len(),
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
        visitor.rows(
            (self.slots).len(),
            std::mem::size_of::<StorageLifeSlot>(),
            depth + 1,
        )?;
        for item in &self.slots {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.retained).len(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.nominal_arguments).len(),
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
        visitor.rows(
            (self.path).len(),
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
        visitor.rows(
            (self.path).len(),
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
        visitor.rows(
            (self.bindings).len(),
            std::mem::size_of::<StorageLifeBinding>(),
            depth + 1,
        )?;
        for item in &self.bindings {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.static_bindings).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.static_bindings {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.classes).len(),
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
        visitor.rows(
            (self.types).len(),
            std::mem::size_of::<CtDynamicTypeNode>(),
            depth + 1,
        )?;
        for (index, node) in (self.types).iter().enumerate() {
            visitor.type_node(index, node, depth + 1)?;
        }
        visitor.rows(
            (self.life_declarations).len(),
            std::mem::size_of::<CtLifeNominalDeclaration>(),
            depth + 1,
        )?;
        for item in &self.life_declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.life_instances).len(),
            std::mem::size_of::<CtLifeNominalInstance>(),
            depth + 1,
        )?;
        for item in &self.life_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.declarations).len(),
            std::mem::size_of::<CtDynDeclaration>(),
            depth + 1,
        )?;
        for item in &self.declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.interfaces).len(),
            std::mem::size_of::<CtDynInterface>(),
            depth + 1,
        )?;
        for item in &self.interfaces {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.vtables).len(),
            std::mem::size_of::<CtCoreDynVtable>(),
            depth + 1,
        )?;
        for item in &self.vtables {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.function_instances).len(),
            std::mem::size_of::<CtDynFunctionInstanceRecord>(),
            depth + 1,
        )?;
        for item in &self.function_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (self.function_sources).len(),
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
            visitor.rows(
                (item).len(),
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
