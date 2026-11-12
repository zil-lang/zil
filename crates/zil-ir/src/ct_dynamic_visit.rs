//! Structural CT metadata traversal; callbacks carry the caller's own budget.
use super::*;
use crate::*;
pub trait CtDynamicVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(), Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize)
    -> Result<(), Self::Error>;
    fn type_reference(&mut self, id: CtDynamicTypeId, depth: usize) -> Result<(), Self::Error>;
    fn type_graph(&mut self, graph: &CtTypeRef, depth: usize) -> Result<(), Self::Error>;
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
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.parameters).len(),
            std::mem::size_of::<CtLifeParameterKind>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.members).len(),
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
        (&self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.arguments).len(),
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
            Self::Type(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Life(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
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
            Self::IntegerLiteral(value_0) => {
                visitor.text(value_0, depth + 1)?;
            }
            Self::FloatLiteral(value_0) => {
                visitor.text(value_0, depth + 1)?;
            }
            Self::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::Unary { op, value, ty } => {
                let _ = op;
                visitor.node(depth + 1)?;
                (value).visit_ct_dynamic(visitor, depth + 1)?;
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Binary {
                op,
                left,
                right,
                ty,
            } => {
                let _ = op;
                visitor.node(depth + 1)?;
                (left).visit_ct_dynamic(visitor, depth + 1)?;
                (right).visit_ct_dynamic(visitor, depth + 1)?;
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Convert { value, ty } => {
                (value).visit_ct_dynamic(visitor, depth + 1)?;
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                (receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (application).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = member;
                visitor.node(depth + 1)?;
            }
            Self::Value(value_0) => {
                visitor.constant(value_0, depth + 1)?;
            }
            Self::Parameter(value_0) => {
                let _ = value_0;
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
            Self::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::AssociatedProjection {
                receiver,
                application,
                member,
            } => {
                (receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (application).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = member;
                visitor.node(depth + 1)?;
            }
            Self::Concrete(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
            }
            Self::Parameter(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::Nominal(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Address {
                mutable,
                lifetime,
                inner,
            } => {
                let _ = mutable;
                visitor.node(depth + 1)?;
                visitor.node(depth + 1)?;
                if let Some(item) = lifetime {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                (inner).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Tuple(value_0) => {
                visitor.rows(
                    (value_0).len(),
                    std::mem::size_of::<CtDynTypePattern>(),
                    depth + 1,
                )?;
                for item in value_0 {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
            }
            Self::Array { element, length } => {
                (element).visit_ct_dynamic(visitor, depth + 1)?;
                (length).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Slice(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Managed(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Desc(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::OwnedStore(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ErrorUnion { error, result } => {
                (error).visit_ct_dynamic(visitor, depth + 1)?;
                (result).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.kind).visit_ct_dynamic(visitor, depth + 1)?;
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
            Self::Method(value_0) => {
                visitor.rows(
                    (value_0).len(),
                    std::mem::size_of::<CtLifeParameterKind>(),
                    depth + 1,
                )?;
                for item in value_0 {
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
            Self::Type(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
            }
            Self::QualifiedType(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(value_0) => {
                visitor.constant(value_0, depth + 1)?;
            }
            Self::Life(value_0) => {
                let _ = value_0;
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
        (&self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.actuals).len(),
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
        (&self.application).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.member).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.ty, depth + 1)?;
        (&self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.classes).len(),
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
        (&self.member).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.mode).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.member).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.parameters).len(),
            std::mem::size_of::<CtDynamicTypeId>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            visitor.type_reference(*item, depth + 1)?;
        }
        visitor.rows(
            (&self.parameter_modes).len(),
            std::mem::size_of::<PassingMode>(),
            depth + 1,
        )?;
        for item in &self.parameter_modes {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.type_reference(*&self.result, depth + 1)?;
        (&self.result_mode).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.call_kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.node(depth + 1)?;
        if let Some(item) = &self.storage_life {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.contract).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.application).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.inherited).len(),
            std::mem::size_of::<CtDynTraitApplication>(),
            depth + 1,
        )?;
        for item in &self.inherited {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.associated_types).len(),
            std::mem::size_of::<CtDynAssociatedType>(),
            depth + 1,
        )?;
        for item in &self.associated_types {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.associated_constants).len(),
            std::mem::size_of::<CtDynAssociatedConstant>(),
            depth + 1,
        )?;
        for item in &self.associated_constants {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.retained_life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.slots).len(),
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
            (&self.sources).len(),
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
            (&self.self_classes).len(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.self_type, depth + 1)?;
        (&self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.ports).len(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.entries).len(),
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
            (&self.self_classes).len(),
            std::mem::size_of::<StorageBinder>(),
            depth + 1,
        )?;
        for item in &self.self_classes {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.interface).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.self_type, depth + 1)?;
        (&self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.ports).len(),
            std::mem::size_of::<CtDynRetainedPortBinding>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.entries).len(),
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
            Self::Read(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Write(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Manage(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Apart(value_0, value_1) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
                (value_1).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::State {
                position,
                ty,
                variant,
            } => {
                (position).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*ty, depth + 1)?;
                let _ = variant;
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
            Self::StorageSources(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ManageResult => {}
            Self::State {
                position,
                ty,
                variant,
            } => {
                (position).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*ty, depth + 1)?;
                let _ = variant;
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
            Self::Call(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Cleanup(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
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
            (&self.reads).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.reads {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.writes).len(),
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
            Self::Borrow { sources, mutable } => {
                visitor.rows(
                    (sources).len(),
                    std::mem::size_of::<CtStorageSource>(),
                    depth + 1,
                )?;
                for item in sources {
                    (item).visit_ct_dynamic(visitor, depth + 1)?;
                }
                let _ = mutable;
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
            (&self.positions).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.effects).len(),
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
        (&self.life).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.classes).len(),
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
impl CtDynamicMetadata for CtDynLifePattern {
    fn visit_ct_dynamic<V: CtDynamicVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        match self {
            Self::DeclarationParameter(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::MethodParameter(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::Bound { depth, index } => {
                let _ = depth;
                visitor.node(depth + 1)?;
                let _ = index;
                visitor.node(depth + 1)?;
            }
            Self::Invocation(value_0) => {
                let _ = value_0;
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
            Self::Structural { receiver, bound } => {
                (receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (bound).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Trait {
                receiver,
                application,
            } => {
                (receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (application).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature,
            } => {
                (receiver).visit_ct_dynamic(visitor, depth + 1)?;
                (kind).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = quantified_lifetimes;
                visitor.node(depth + 1)?;
                (signature).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::TypeEqual { left, right } => {
                (left).visit_ct_dynamic(visitor, depth + 1)?;
                (right).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ConstEqual { ty, left, right } => {
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
                (left).visit_ct_dynamic(visitor, depth + 1)?;
                (right).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.ty).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.mode).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.parameters).len(),
            std::mem::size_of::<CtDynParameterRecipe>(),
            depth + 1,
        )?;
        for item in &self.parameters {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.result).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.result_mode).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.call_kind).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.invocation_binder_count;
        visitor.node(depth + 1)?;
        (&self.contract).visit_ct_dynamic(visitor, depth + 1)?;
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
            Self::Read(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Write(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Manage(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Apart(value_0, value_1) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
                (value_1).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::State {
                position,
                ty,
                variant,
            } => {
                (position).visit_ct_dynamic(visitor, depth + 1)?;
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = variant;
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
            Self::StorageSources(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::ManageResult => {}
            Self::State {
                position,
                ty,
                variant,
            } => {
                (position).visit_ct_dynamic(visitor, depth + 1)?;
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = variant;
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
            Self::Call(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Cleanup(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.positions).len(),
            std::mem::size_of::<StoragePosition>(),
            depth + 1,
        )?;
        for item in &self.positions {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.access).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.effects).len(),
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
            (&self.requirements).len(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.instances).len(),
            std::mem::size_of::<CtDynDefinitionSpecialization>(),
            depth + 1,
        )?;
        for item in &self.instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        let _ = &self.member;
        visitor.node(depth + 1)?;
        (&self.scope).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.signature).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.requirements).len(),
            std::mem::size_of::<CtDynObligation>(),
            depth + 1,
        )?;
        for item in &self.requirements {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        let _ = &self.member;
        visitor.node(depth + 1)?;
        (&self.signature).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.definition).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.declaration_actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.method_actuals).len(),
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
            Self::Type(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Constant { ty, value } => {
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
                (value).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.requirements).len(),
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
                declaration,
                member,
            } => {
                (declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = member;
                visitor.node(depth + 1)?;
            }
            Self::ImplOverride {
                declaration,
                member,
            } => {
                (declaration).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = member;
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
            Self::Type(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Constant { ty, value } => {
                (ty).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.constant(value, depth + 1)?;
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
        (&self.member).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.source).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.value).visit_ct_dynamic(visitor, depth + 1)?;
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
        (&self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.application).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.implementation).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.associated).len(),
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
            Self::Core { module, source } => {
                let _ = module;
                visitor.node(depth + 1)?;
                let _ = source;
                visitor.node(depth + 1)?;
            }
            Self::Typed(value_0) => {
                let _ = value_0;
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
        (&self.function).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.receiver).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.declaration_actuals).len(),
            std::mem::size_of::<CtDynGenericActual>(),
            depth + 1,
        )?;
        for item in &self.declaration_actuals {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.method_actuals).len(),
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
            Self::Exact(value_0) => {
                (value_0).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::PlaceAddress { place, mode } => {
                (place).visit_ct_dynamic(visitor, depth + 1)?;
                let _ = mode;
                visitor.node(depth + 1)?;
            }
            Self::OwnedView { head, leaf, mode } => {
                (head).visit_ct_dynamic(visitor, depth + 1)?;
                visitor.type_reference(*leaf, depth + 1)?;
                let _ = mode;
                visitor.node(depth + 1)?;
            }
            Self::ReborrowedLeaf { source, head } => {
                (source).visit_ct_dynamic(visitor, depth + 1)?;
                (head).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Static { leaf, mode } => {
                visitor.type_reference(*leaf, depth + 1)?;
                let _ = mode;
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
            (&self.ports).len(),
            std::mem::size_of::<CtRetainedBorrowPort>(),
            depth + 1,
        )?;
        for item in &self.ports {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.retained).len(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.nominal_arguments).len(),
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
        (&self.position).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.sources).len(),
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
            Self::Const(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
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
        (&self.id).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.identity).visit_ct_dynamic(visitor, depth + 1)?;
        (&self.kind).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.rows(
            (&self.parameters).len(),
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
            Self::Type(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
            }
            Self::QualifiedType { ty, life } => {
                visitor.type_reference(*ty, depth + 1)?;
                (life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Const(value_0) => {
                visitor.constant(value_0, depth + 1)?;
            }
            Self::Lifetime(value_0) => {
                let _ = value_0;
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
        (&self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
        visitor.type_reference(*&self.ty, depth + 1)?;
        visitor.rows(
            (&self.arguments).len(),
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
            Self::Dyn { interface } => {
                (interface).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::OwnedDyn { interface } => {
                (interface).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::BorrowedStorage { storage, life } => {
                visitor.type_reference(*storage, depth + 1)?;
                (life).visit_ct_dynamic(visitor, depth + 1)?;
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
            Self::OwnedStore { element } => {
                visitor.type_reference(*element, depth + 1)?;
            }
            Self::ForeignHandle => {}
            Self::SyncCell(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::Mutex { payload } => {
                visitor.type_reference(*payload, depth + 1)?;
            }
            Self::MutexGuard {
                payload,
                retained_life,
            } => {
                visitor.type_reference(*payload, depth + 1)?;
                (retained_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Registration { environment } => {
                visitor.type_reference(*environment, depth + 1)?;
            }
            Self::AsyncRegistration { environment } => {
                visitor.type_reference(*environment, depth + 1)?;
            }
            Self::TaskI32 => {}
            Self::Task {
                result,
                retained_life,
            } => {
                visitor.type_reference(*result, depth + 1)?;
                (retained_life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::CWideString => {}
            Self::ForeignCallback { parameters, result } => {
                visitor.rows(
                    (parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.type_reference(*result, depth + 1)?;
            }
            Self::Tuple(value_0) => {
                visitor.rows(
                    (value_0).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in value_0 {
                    visitor.type_reference(*item, depth + 1)?;
                }
            }
            Self::Array { element, length } => {
                visitor.type_reference(*element, depth + 1)?;
                let _ = length;
                visitor.node(depth + 1)?;
            }
            Self::Slice { element } => {
                visitor.type_reference(*element, depth + 1)?;
            }
            Self::Named(value_0) => {
                visitor.text(value_0, depth + 1)?;
            }
            Self::Record { name, fields } => {
                visitor.text(name, depth + 1)?;
                visitor.rows(
                    (fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in fields {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.type_reference(*&(item).1, depth + 1)?;
                }
            }
            Self::Package { name, fields } => {
                visitor.text(name, depth + 1)?;
                visitor.rows(
                    (fields).len(),
                    std::mem::size_of::<(String, CtDynamicTypeId)>(),
                    depth + 1,
                )?;
                for item in fields {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.type_reference(*&(item).1, depth + 1)?;
                }
            }
            Self::Enum { name, variants } => {
                visitor.text(name, depth + 1)?;
                visitor.rows(
                    (variants).len(),
                    std::mem::size_of::<(String, Vec<CtDynamicTypeId>)>(),
                    depth + 1,
                )?;
                for item in variants {
                    visitor.text(&(item).0, depth + 1)?;
                    visitor.rows(
                        (&(item).1).len(),
                        std::mem::size_of::<CtDynamicTypeId>(),
                        depth + 1,
                    )?;
                    for item in &(item).1 {
                        visitor.type_reference(*item, depth + 1)?;
                    }
                }
            }
            Self::ErrorUnion { error, result } => {
                visitor.type_reference(*error, depth + 1)?;
                visitor.type_reference(*result, depth + 1)?;
            }
            Self::Address { mutable, inner } => {
                let _ = mutable;
                visitor.node(depth + 1)?;
                visitor.type_reference(*inner, depth + 1)?;
            }
            Self::Managed(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
            }
            Self::Desc(value_0) => {
                visitor.type_reference(*value_0, depth + 1)?;
            }
            Self::FunctionPointer { parameters, result } => {
                visitor.rows(
                    (parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.type_reference(*result, depth + 1)?;
            }
            Self::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                visitor.rows(
                    (parameters).len(),
                    std::mem::size_of::<CtDynamicTypeId>(),
                    depth + 1,
                )?;
                for item in parameters {
                    visitor.type_reference(*item, depth + 1)?;
                }
                visitor.type_reference(*result, depth + 1)?;
                (life).visit_ct_dynamic(visitor, depth + 1)?;
            }
            Self::Closure(value_0) => {
                let _ = value_0;
                visitor.node(depth + 1)?;
            }
            Self::Opaque { identity, hidden } => {
                let _ = identity;
                visitor.node(depth + 1)?;
                visitor.type_reference(*hidden, depth + 1)?;
            }
            Self::Unsupported(value_0) => {
                visitor.text(value_0, depth + 1)?;
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
            (&self.module_path).len(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.module_path {
            visitor.text(item, depth + 1)?;
        }
        visitor.rows(
            (&self.declaration_path).len(),
            std::mem::size_of::<String>(),
            depth + 1,
        )?;
        for item in &self.declaration_path {
            visitor.text(item, depth + 1)?;
        }
        visitor.rows(
            (&self.generation_path).len(),
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
            (&self.path).len(),
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
            (&self.path).len(),
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
            (&self.path).len(),
            std::mem::size_of::<LifeTypeProjection>(),
            depth + 1,
        )?;
        for item in &self.path {
            let _ = item;
            visitor.node(depth + 1)?;
        }
        (&self.declaration).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.slots).len(),
            std::mem::size_of::<StorageLifeSlot>(),
            depth + 1,
        )?;
        for item in &self.slots {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.retained).len(),
            std::mem::size_of::<StorageRetainedBinding>(),
            depth + 1,
        )?;
        for item in &self.retained {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.nominal_arguments).len(),
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
            (&self.path).len(),
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
        (&self.position).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.path).len(),
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
        (&self.position).visit_ct_dynamic(visitor, depth + 1)?;
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
            (&self.bindings).len(),
            std::mem::size_of::<StorageLifeBinding>(),
            depth + 1,
        )?;
        for item in &self.bindings {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.classes).len(),
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
            (&self.types).len(),
            std::mem::size_of::<CtDynamicTypeNode>(),
            depth + 1,
        )?;
        for item in &self.types {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.life_declarations).len(),
            std::mem::size_of::<CtLifeNominalDeclaration>(),
            depth + 1,
        )?;
        for item in &self.life_declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.life_instances).len(),
            std::mem::size_of::<CtLifeNominalInstance>(),
            depth + 1,
        )?;
        for item in &self.life_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.declarations).len(),
            std::mem::size_of::<CtDynDeclaration>(),
            depth + 1,
        )?;
        for item in &self.declarations {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.interfaces).len(),
            std::mem::size_of::<CtDynInterface>(),
            depth + 1,
        )?;
        for item in &self.interfaces {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.vtables).len(),
            std::mem::size_of::<CtCoreDynVtable>(),
            depth + 1,
        )?;
        for item in &self.vtables {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.function_instances).len(),
            std::mem::size_of::<CtDynFunctionInstanceRecord>(),
            depth + 1,
        )?;
        for item in &self.function_instances {
            (item).visit_ct_dynamic(visitor, depth + 1)?;
        }
        visitor.rows(
            (&self.function_sources).len(),
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
        (&self.instance).visit_ct_dynamic(visitor, depth + 1)?;
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
        visitor.node(depth + 1)?;
        if let Some(item) = &self.returned_capture_sources {
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
