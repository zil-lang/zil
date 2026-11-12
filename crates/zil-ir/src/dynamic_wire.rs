//! Bounded raw dynamic table transport; shared life codecs live in wire.rs.
use super::*;

impl Writer<'_> {
    pub(super) fn dyn_interface_id(&mut self, value: &DynInterfaceId) -> WireResult<()> {
        self.u32(value.module.0)?;
        self.u32(value.index)?;
        Ok(())
    }
    pub(super) fn dyn_vtable_id(&mut self, value: &DynVtableId) -> WireResult<()> {
        self.u32(value.module.0)?;
        self.u32(value.index)?;
        Ok(())
    }
    pub(super) fn dyn_declaration(&mut self, value: &DynDeclaration) -> WireResult<()> {
        self.declaration_id(&value.id)?;
        self.declaration_identity(&value.identity)?;
        self.dyn_declaration_kind(&value.kind)?;
        self.boolean(value.self_pattern.is_some())?;
        if let Some(pattern) = &value.self_pattern {
            self.dyn_self_pattern(pattern, 0)?;
        }
        self.count(value.parameters.len(), self.limits.max_nodes)?;
        for value in &value.parameters {
            self.parameter_kind(value, 0)?;
        }
        self.count(value.members.len(), self.limits.max_nodes)?;
        for value in &value.members {
            self.dyn_member_declaration(value)?;
        }
        Ok(())
    }

    fn dyn_pattern_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dyn Self recipe nesting quota exceeded"));
        }
        self.charge(1)
    }
    fn dyn_self_pattern(&mut self, value: &DynSelfPattern, depth: usize) -> WireResult<()> {
        self.dyn_pattern_node(depth)?;
        self.declaration_id(&value.declaration)?;
        self.count(value.arguments.len(), self.limits.max_nodes)?;
        for actual in &value.arguments {
            self.dyn_pattern_actual(actual, depth + 1)?;
        }
        Ok(())
    }
    fn dyn_pattern_actual(&mut self, value: &DynPatternActual, depth: usize) -> WireResult<()> {
        self.dyn_pattern_node(depth)?;
        match value {
            DynPatternActual::Type(ty) => {
                self.u8(0)?;
                self.dyn_type_pattern(ty, depth + 1)
            }
            DynPatternActual::Const(value) => {
                self.u8(1)?;
                self.dyn_const_pattern(value, depth + 1)
            }
            DynPatternActual::LifeParameter(ordinal) => {
                self.u8(2)?;
                self.u32(*ordinal)
            }
        }
    }
    fn dyn_const_pattern(&mut self, value: &DynConstPattern, depth: usize) -> WireResult<()> {
        self.dyn_pattern_node(depth)?;
        match value {
            DynConstPattern::Value(value) => {
                self.u8(0)?;
                self.constant(value)
            }
            DynConstPattern::Parameter(ordinal) => {
                self.u8(1)?;
                self.u32(*ordinal)
            }
        }
    }
    fn dyn_type_pattern(&mut self, value: &DynTypePattern, depth: usize) -> WireResult<()> {
        self.dyn_pattern_node(depth)?;
        match value {
            DynTypePattern::Concrete(ty) => {
                self.u8(0)?;
                self.ty(ty, depth + 1)
            }
            DynTypePattern::Parameter(ordinal) => {
                self.u8(1)?;
                self.u32(*ordinal)
            }
            DynTypePattern::Nominal(value) => {
                self.u8(2)?;
                self.dyn_self_pattern(value, depth + 1)
            }
            DynTypePattern::Address {
                mutable,
                lifetime,
                inner,
            } => {
                self.u8(3)?;
                self.boolean(*mutable)?;
                self.boolean(lifetime.is_some())?;
                if let Some(ordinal) = lifetime {
                    self.u32(*ordinal)?;
                }
                self.dyn_type_pattern(inner, depth + 1)
            }
            DynTypePattern::Tuple(values) => {
                self.u8(4)?;
                self.count(values.len(), self.limits.max_nodes)?;
                for value in values {
                    self.dyn_type_pattern(value, depth + 1)?;
                }
                Ok(())
            }
            DynTypePattern::Array { element, length } => {
                self.u8(5)?;
                self.dyn_const_pattern(length, depth + 1)?;
                self.dyn_type_pattern(element, depth + 1)
            }
            DynTypePattern::Slice(inner) => {
                self.u8(6)?;
                self.dyn_type_pattern(inner, depth + 1)
            }
            DynTypePattern::Managed(inner) => {
                self.u8(7)?;
                self.dyn_type_pattern(inner, depth + 1)
            }
            DynTypePattern::Desc(inner) => {
                self.u8(8)?;
                self.dyn_type_pattern(inner, depth + 1)
            }
            DynTypePattern::OwnedStore(inner) => {
                self.u8(9)?;
                self.dyn_type_pattern(inner, depth + 1)
            }
            DynTypePattern::ErrorUnion { error, result } => {
                self.u8(10)?;
                self.dyn_type_pattern(error, depth + 1)?;
                self.dyn_type_pattern(result, depth + 1)
            }
        }
    }
    pub(super) fn dyn_member_declaration(
        &mut self,
        value: &DynMemberDeclaration,
    ) -> WireResult<()> {
        self.text(&value.name)?;
        self.dyn_member_kind(&value.kind)?;
        Ok(())
    }
    pub(super) fn dyn_trait_application(&mut self, value: &DynTraitApplication) -> WireResult<()> {
        self.declaration_id(&value.declaration)?;
        self.count(value.actuals.len(), self.limits.max_nodes)?;
        for value in &value.actuals {
            self.dyn_generic_actual(value)?;
        }
        Ok(())
    }
    pub(super) fn dyn_member(&mut self, value: &DynMember) -> WireResult<()> {
        self.dyn_trait_application(&value.application)?;
        self.u32(value.ordinal)?;
        Ok(())
    }
    pub(super) fn dyn_associated_type(&mut self, value: &DynAssociatedType) -> WireResult<()> {
        self.dyn_member(&value.member)?;
        self.ty(&value.ty, 0)?;
        Ok(())
    }
    pub(super) fn dyn_associated_constant(
        &mut self,
        value: &DynAssociatedConstant,
    ) -> WireResult<()> {
        self.dyn_member(&value.member)?;
        self.ty(&value.ty, 0)?;
        self.constant(&value.value)?;
        Ok(())
    }
    pub(super) fn dyn_receiver(&mut self, value: &DynReceiver) -> WireResult<()> {
        self.boolean(value.mutable)?;
        self.dyn_passing_mode(&value.mode)?;
        Ok(())
    }
    pub(super) fn dyn_slot(&mut self, value: &DynSlot) -> WireResult<()> {
        self.dyn_member(&value.member)?;
        self.count(value.actuals.len(), self.limits.max_nodes)?;
        for value in &value.actuals {
            self.dyn_generic_actual(value)?;
        }
        self.dyn_receiver(&value.receiver)?;
        self.count(value.parameters.len(), self.limits.max_nodes)?;
        for value in &value.parameters {
            self.ty(value, 0)?;
        }
        self.count(value.parameter_modes.len(), self.limits.max_nodes)?;
        for value in &value.parameter_modes {
            self.dyn_passing_mode(value)?;
        }
        self.ty(&value.result, 0)?;
        self.dyn_passing_mode(&value.result_mode)?;
        self.dyn_call_kind(&value.call_kind)?;
        self.boolean(value.storage_life.is_some())?;
        if let Some(value) = &value.storage_life {
            self.required_storage_contract(value)?;
        }
        self.dyn_contract(&value.contract)?;
        Ok(())
    }
    pub(super) fn dyn_interface(&mut self, value: &DynInterface) -> WireResult<()> {
        self.boolean(value.context.is_some())?;
        if let Some(context) = &value.context {
            self.ty(context, 0)?;
        }
        self.boolean(value.owning_cleanup.is_some())?;
        if let Some(cleanup) = &value.owning_cleanup {
            self.dyn_cleanup_contract(cleanup)?;
        }

        self.dyn_interface_id(&value.id)?;
        self.dyn_trait_application(&value.application)?;
        self.count(value.inherited.len(), self.limits.max_nodes)?;
        for value in &value.inherited {
            self.dyn_trait_application(value)?;
        }
        self.count(value.associated_types.len(), self.limits.max_nodes)?;
        for value in &value.associated_types {
            self.dyn_associated_type(value)?;
        }
        self.count(value.associated_constants.len(), self.limits.max_nodes)?;
        for value in &value.associated_constants {
            self.dyn_associated_constant(value)?;
        }
        self.retained_shape(&value.retained_life, 0)?;
        self.count(value.slots.len(), self.limits.max_nodes)?;
        for value in &value.slots {
            self.dyn_slot(value)?;
        }
        Ok(())
    }
    pub(super) fn dyn_entry(&mut self, value: &DynEntry) -> WireResult<()> {
        self.u32(value.slot)?;
        self.u32(value.function.0)?;
        Ok(())
    }
    pub(super) fn dyn_vtable(&mut self, value: &DynVtable) -> WireResult<()> {
        self.dyn_vtable_id(&value.id)?;
        self.dyn_interface_id(&value.interface)?;
        self.ty(&value.self_type, 0)?;
        self.count(value.self_classes.len(), self.limits.max_nodes)?;
        for binder in &value.self_classes {
            self.storage_binder(binder)?;
        }
        self.declaration_id(&value.implementation)?;
        self.count(value.actuals.len(), self.limits.max_nodes)?;
        for value in &value.actuals {
            self.dyn_generic_actual(value)?;
        }
        self.count(value.ports.len(), self.limits.max_nodes)?;
        for value in &value.ports {
            self.u32(value.port)?;
            self.count(value.sources.len(), self.limits.max_nodes)?;
            for source in &value.sources {
                self.storage_position(source)?;
            }
        }
        self.count(value.entries.len(), self.limits.max_nodes)?;
        for value in &value.entries {
            self.dyn_entry(value)?;
        }
        self.boolean(value.cleanup.is_some())?;
        if let Some(value) = &value.cleanup {
            self.dyn_entry(value)?;
        }
        self.u8(match value.owned_storage {
            None => 0,
            Some(DynOwnedStorage::Heap) => 1,
        })?;
        Ok(())
    }
    pub(super) fn dyn_call_access(&mut self, value: &DynCallAccess) -> WireResult<()> {
        self.count(value.reads.len(), self.limits.max_nodes)?;
        for value in &value.reads {
            self.storage_position(value)?;
        }
        self.count(value.writes.len(), self.limits.max_nodes)?;
        for value in &value.writes {
            self.storage_position(value)?;
        }
        Ok(())
    }
    pub(super) fn dyn_cleanup_contract(&mut self, value: &DynCleanupContract) -> WireResult<()> {
        self.boolean(value.environment)?;
        self.count(value.positions.len(), self.limits.max_nodes)?;
        for value in &value.positions {
            self.storage_position(value)?;
        }
        self.dyn_call_access(&value.access)?;
        self.count(value.effects.len(), self.limits.max_nodes)?;
        for value in &value.effects {
            self.dyn_effect(value)?;
        }
        Ok(())
    }
    pub(super) fn dyn_thread_requirements(
        &mut self,
        value: &DynThreadRequirements,
    ) -> WireResult<()> {
        self.boolean(value.transfer)?;
        self.boolean(value.share)?;
        Ok(())
    }
    pub(super) fn dyn_contract(&mut self, value: &DynContract) -> WireResult<()> {
        self.boolean(value.requires.is_some())?;
        if let Some(value) = &value.requires {
            self.count((value).len(), self.limits.max_nodes)?;
            for value in value {
                self.dyn_requirement(value)?;
            }
        }
        self.boolean(value.guarantees.is_some())?;
        if let Some(value) = &value.guarantees {
            self.count((value).len(), self.limits.max_nodes)?;
            for value in value {
                self.dyn_guarantee(value)?;
            }
        }
        self.boolean(value.hold.is_some())?;
        if let Some(value) = &value.hold {
            self.count((value).len(), self.limits.max_nodes)?;
            for value in value {
                self.storage_position(value)?;
            }
        }
        self.boolean(value.call.is_some())?;
        if let Some(value) = &value.call {
            self.dyn_call_access(value)?;
        }
        self.boolean(value.result.is_some())?;
        if let Some(value) = &value.result {
            self.dyn_result_source(value)?;
        }
        self.boolean(value.cleanup.is_some())?;
        if let Some(value) = &value.cleanup {
            self.dyn_cleanup_contract(value)?;
        }
        self.boolean(value.effects.is_some())?;
        if let Some(value) = &value.effects {
            self.count((value).len(), self.limits.max_nodes)?;
            for value in value {
                self.dyn_effect(value)?;
            }
        }
        self.boolean(value.resume.is_some())?;
        if let Some(value) = &value.resume {
            self.dyn_resume_bound(value)?;
        }
        self.boolean(value.threads.is_some())?;
        if let Some(value) = &value.threads {
            self.dyn_thread_requirements(value)?;
        }
        Ok(())
    }
    pub(super) fn dyn_declaration_kind(&mut self, value: &DynDeclarationKind) -> WireResult<()> {
        match value {
            DynDeclarationKind::Trait => {
                self.u8(0)?;
            }
            DynDeclarationKind::Impl => {
                self.u8(1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_member_kind(&mut self, value: &DynMemberKind) -> WireResult<()> {
        match value {
            DynMemberKind::Type => {
                self.u8(0)?;
            }
            DynMemberKind::Constant(value_0) => {
                self.u8(1)?;
                self.ty(value_0, 0)?;
            }
            DynMemberKind::Method(value_0) => {
                self.u8(2)?;
                self.count(value_0.len(), self.limits.max_nodes)?;
                for value in value_0 {
                    self.parameter_kind(value, 0)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn dyn_generic_actual(&mut self, value: &DynGenericActual) -> WireResult<()> {
        match value {
            DynGenericActual::Type(value_0) => {
                self.u8(0)?;
                self.ty(value_0, 0)?;
            }
            DynGenericActual::Const(value_0) => {
                self.u8(1)?;
                self.constant(value_0)?;
            }
            DynGenericActual::Life(value_0) => {
                self.u8(2)?;
                self.storage_binder(value_0)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_requirement(&mut self, value: &DynRequirement) -> WireResult<()> {
        match value {
            DynRequirement::Read(value_0) => {
                self.u8(0)?;
                self.storage_position(value_0)?;
            }
            DynRequirement::Write(value_0) => {
                self.u8(1)?;
                self.storage_position(value_0)?;
            }
            DynRequirement::Manage(value_0) => {
                self.u8(2)?;
                self.storage_position(value_0)?;
            }
            DynRequirement::Apart(value_0, value_1) => {
                self.u8(3)?;
                self.storage_position(value_0)?;
                self.storage_position(value_1)?;
            }
            DynRequirement::State {
                position,
                ty,
                variant,
            } => {
                self.u8(4)?;
                self.storage_position(position)?;
                self.ty(ty, 0)?;
                self.u32(*variant)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_guarantee(&mut self, value: &DynGuarantee) -> WireResult<()> {
        match value {
            DynGuarantee::ManageResult => {
                self.u8(0)?;
            }
            DynGuarantee::State {
                position,
                ty,
                variant,
            } => {
                self.u8(1)?;
                self.storage_position(position)?;
                self.ty(ty, 0)?;
                self.u32(*variant)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_effect(&mut self, value: &DynEffect) -> WireResult<()> {
        match value {
            DynEffect::Alloc => {
                self.u8(0)?;
            }
            DynEffect::Io => {
                self.u8(1)?;
            }
            DynEffect::Block => {
                self.u8(2)?;
            }
            DynEffect::Suspend => {
                self.u8(3)?;
            }
            DynEffect::Spawn => {
                self.u8(4)?;
            }
            DynEffect::Ffi => {
                self.u8(5)?;
            }
            DynEffect::Reenter => {
                self.u8(6)?;
            }
            DynEffect::Panic => {
                self.u8(7)?;
            }
            DynEffect::Call(value_0) => {
                self.u8(8)?;
                self.storage_position(value_0)?;
            }
            DynEffect::Cleanup(value_0) => {
                self.u8(9)?;
                self.ty(value_0, 0)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_result_source(&mut self, value: &DynResultSource) -> WireResult<()> {
        match value {
            DynResultSource::Value => {
                self.u8(0)?;
            }
            DynResultSource::Create => {
                self.u8(1)?;
            }
            DynResultSource::Give => {
                self.u8(2)?;
            }
            DynResultSource::Borrow { sources, mutable } => {
                self.u8(3)?;
                self.count(sources.len(), self.limits.max_nodes)?;
                for value in sources {
                    self.storage_position(value)?;
                }
                self.boolean(*mutable)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_resume_bound(&mut self, value: &DynResumeBound) -> WireResult<()> {
        match value {
            DynResumeBound::Local => {
                self.u8(0)?;
            }
            DynResumeBound::Transfer => {
                self.u8(1)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_passing_mode(&mut self, value: &PassingMode) -> WireResult<()> {
        match value {
            PassingMode::Copy => {
                self.u8(0)?;
            }
            PassingMode::Move => {
                self.u8(1)?;
            }
            PassingMode::Describe => {
                self.u8(2)?;
            }
            PassingMode::Create => {
                self.u8(3)?;
            }
            PassingMode::Give => {
                self.u8(4)?;
            }
            PassingMode::Borrow => {
                self.u8(5)?;
            }
            PassingMode::BorrowMut => {
                self.u8(6)?;
            }
            PassingMode::Infer => {
                self.u8(7)?;
            }
        }
        Ok(())
    }
    pub(super) fn dyn_call_kind(&mut self, value: &CallKind) -> WireResult<()> {
        match value {
            CallKind::Direct => {
                self.u8(0)?;
            }
            CallKind::Suspend => {
                self.u8(1)?;
            }
        }
        Ok(())
    }
}

impl Reader<'_> {
    pub(super) fn dyn_interface_id(&mut self) -> WireResult<DynInterfaceId> {
        Ok(DynInterfaceId {
            module: ModuleId(self.u32()?),
            index: self.u32()?,
        })
    }
    pub(super) fn dyn_vtable_id(&mut self) -> WireResult<DynVtableId> {
        Ok(DynVtableId {
            module: ModuleId(self.u32()?),
            index: self.u32()?,
        })
    }
    pub(super) fn dyn_declaration(&mut self) -> WireResult<DynDeclaration> {
        Ok(DynDeclaration {
            id: self.declaration_id()?,
            identity: self.declaration_identity()?,
            kind: self.dyn_declaration_kind()?,
            self_pattern: if self.boolean()? {
                Some(self.dyn_self_pattern(0)?)
            } else {
                None
            },
            parameters: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_1 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_1.push(self.parameter_kind(0)?);
                }
                values_1
            },
            members: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_2 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_2.push(self.dyn_member_declaration()?);
                }
                values_2
            },
        })
    }

    fn dyn_pattern_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dyn Self recipe nesting quota exceeded"));
        }
        self.charge(1)
    }
    fn dyn_self_pattern(&mut self, depth: usize) -> WireResult<DynSelfPattern> {
        self.dyn_pattern_node(depth)?;
        let declaration = self.declaration_id()?;
        let count = self.count(self.limits.max_nodes, 2)?;
        let mut arguments = Vec::with_capacity(count);
        for _ in 0..count {
            arguments.push(self.dyn_pattern_actual(depth + 1)?);
        }
        Ok(DynSelfPattern {
            declaration,
            arguments,
        })
    }
    fn dyn_pattern_actual(&mut self, depth: usize) -> WireResult<DynPatternActual> {
        self.dyn_pattern_node(depth)?;
        Ok(match self.u8()? {
            0 => DynPatternActual::Type(self.dyn_type_pattern(depth + 1)?),
            1 => DynPatternActual::Const(self.dyn_const_pattern(depth + 1)?),
            2 => DynPatternActual::LifeParameter(self.u32()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dyn Self actual recipe")),
        })
    }
    fn dyn_const_pattern(&mut self, depth: usize) -> WireResult<DynConstPattern> {
        self.dyn_pattern_node(depth)?;
        Ok(match self.u8()? {
            0 => DynConstPattern::Value(self.constant()?),
            1 => DynConstPattern::Parameter(self.u32()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown dyn Self const recipe")),
        })
    }
    fn dyn_type_pattern(&mut self, depth: usize) -> WireResult<DynTypePattern> {
        self.dyn_pattern_node(depth)?;
        Ok(match self.u8()? {
            0 => DynTypePattern::Concrete(self.ty(depth + 1)?),
            1 => DynTypePattern::Parameter(self.u32()?),
            2 => DynTypePattern::Nominal(Box::new(self.dyn_self_pattern(depth + 1)?)),
            3 => {
                let mutable = self.boolean()?;
                let lifetime = if self.boolean()? {
                    Some(self.u32()?)
                } else {
                    None
                };
                let inner = Box::new(self.dyn_type_pattern(depth + 1)?);
                DynTypePattern::Address {
                    mutable,
                    lifetime,
                    inner,
                }
            }
            4 => {
                let count = self.count(self.limits.max_nodes, 2)?;
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(self.dyn_type_pattern(depth + 1)?);
                }
                DynTypePattern::Tuple(values)
            }
            5 => {
                let length = self.dyn_const_pattern(depth + 1)?;
                let element = Box::new(self.dyn_type_pattern(depth + 1)?);
                DynTypePattern::Array { element, length }
            }
            6 => DynTypePattern::Slice(Box::new(self.dyn_type_pattern(depth + 1)?)),
            7 => DynTypePattern::Managed(Box::new(self.dyn_type_pattern(depth + 1)?)),
            8 => DynTypePattern::Desc(Box::new(self.dyn_type_pattern(depth + 1)?)),
            9 => DynTypePattern::OwnedStore(Box::new(self.dyn_type_pattern(depth + 1)?)),
            10 => {
                let error = Box::new(self.dyn_type_pattern(depth + 1)?);
                let result = Box::new(self.dyn_type_pattern(depth + 1)?);
                DynTypePattern::ErrorUnion { error, result }
            }
            _ => return Err(malformed("E-WIRE-TAG", "unknown dyn Self type recipe")),
        })
    }
    pub(super) fn dyn_member_declaration(&mut self) -> WireResult<DynMemberDeclaration> {
        Ok(DynMemberDeclaration {
            name: self.text()?,
            kind: self.dyn_member_kind()?,
        })
    }
    pub(super) fn dyn_trait_application(&mut self) -> WireResult<DynTraitApplication> {
        Ok(DynTraitApplication {
            declaration: self.declaration_id()?,
            actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_3 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_3.push(self.dyn_generic_actual()?);
                }
                values_3
            },
        })
    }
    pub(super) fn dyn_member(&mut self) -> WireResult<DynMember> {
        Ok(DynMember {
            application: self.dyn_trait_application()?,
            ordinal: self.u32()?,
        })
    }
    pub(super) fn dyn_associated_type(&mut self) -> WireResult<DynAssociatedType> {
        Ok(DynAssociatedType {
            member: self.dyn_member()?,
            ty: self.ty(0)?,
        })
    }
    pub(super) fn dyn_associated_constant(&mut self) -> WireResult<DynAssociatedConstant> {
        Ok(DynAssociatedConstant {
            member: self.dyn_member()?,
            ty: self.ty(0)?,
            value: self.constant()?,
        })
    }
    pub(super) fn dyn_receiver(&mut self) -> WireResult<DynReceiver> {
        Ok(DynReceiver {
            mutable: self.boolean()?,
            mode: self.dyn_passing_mode()?,
        })
    }
    pub(super) fn dyn_slot(&mut self) -> WireResult<DynSlot> {
        Ok(DynSlot {
            member: self.dyn_member()?,
            actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_4 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_4.push(self.dyn_generic_actual()?);
                }
                values_4
            },
            receiver: self.dyn_receiver()?,
            parameters: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_5 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_5.push(self.ty(0)?);
                }
                values_5
            },
            parameter_modes: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_6 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_6.push(self.dyn_passing_mode()?);
                }
                values_6
            },
            result: self.ty(0)?,
            result_mode: self.dyn_passing_mode()?,
            call_kind: self.dyn_call_kind()?,
            storage_life: if self.boolean()? {
                Some(self.required_storage_contract()?)
            } else {
                None
            },
            contract: self.dyn_contract()?,
        })
    }
    pub(super) fn dyn_interface(&mut self) -> WireResult<DynInterface> {
        Ok(DynInterface {
            context: if self.boolean()? {
                Some(self.ty(0)?)
            } else {
                None
            },
            owning_cleanup: if self.boolean()? {
                Some(self.dyn_cleanup_contract()?)
            } else {
                None
            },
            id: self.dyn_interface_id()?,
            application: self.dyn_trait_application()?,
            inherited: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_7 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_7.push(self.dyn_trait_application()?);
                }
                values_7
            },
            associated_types: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_8 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_8.push(self.dyn_associated_type()?);
                }
                values_8
            },
            associated_constants: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_9 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_9.push(self.dyn_associated_constant()?);
                }
                values_9
            },
            retained_life: self.retained_shape(0)?,
            slots: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_10 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_10.push(self.dyn_slot()?);
                }
                values_10
            },
        })
    }
    pub(super) fn dyn_entry(&mut self) -> WireResult<DynEntry> {
        Ok(DynEntry {
            slot: self.u32()?,
            function: FunctionId(self.u32()?),
        })
    }
    pub(super) fn dyn_vtable(&mut self) -> WireResult<DynVtable> {
        Ok(DynVtable {
            id: self.dyn_vtable_id()?,
            interface: self.dyn_interface_id()?,
            self_type: self.ty(0)?,
            self_classes: {
                let count = self.count(self.limits.max_nodes, 5)?;
                let mut binders = Vec::with_capacity(count);
                for _ in 0..count {
                    binders.push(self.storage_binder()?);
                }
                binders
            },
            implementation: self.declaration_id()?,
            actuals: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_11 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_11.push(self.dyn_generic_actual()?);
                }
                values_11
            },
            ports: {
                let count = self.count(self.limits.max_nodes, 8)?;
                let mut values_12 = Vec::with_capacity(count);
                for _ in 0..count {
                    let port = self.u32()?;
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut sources = Vec::with_capacity(count);
                    for _ in 0..count {
                        sources.push(self.storage_position()?);
                    }
                    values_12.push(DynRetainedPortBinding { port, sources });
                }
                values_12
            },
            entries: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_13 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_13.push(self.dyn_entry()?);
                }
                values_13
            },
            cleanup: if self.boolean()? {
                Some(self.dyn_entry()?)
            } else {
                None
            },
            owned_storage: match self.u8()? {
                0 => None,
                1 => Some(DynOwnedStorage::Heap),
                _ => return Err(malformed("E-WIRE-TAG", "unknown dyn owned storage route")),
            },
        })
    }
    pub(super) fn dyn_call_access(&mut self) -> WireResult<DynCallAccess> {
        Ok(DynCallAccess {
            reads: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_14 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_14.push(self.storage_position()?);
                }
                values_14
            },
            writes: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_15 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_15.push(self.storage_position()?);
                }
                values_15
            },
        })
    }
    pub(super) fn dyn_cleanup_contract(&mut self) -> WireResult<DynCleanupContract> {
        Ok(DynCleanupContract {
            environment: self.boolean()?,
            positions: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_16 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_16.push(self.storage_position()?);
                }
                values_16
            },
            access: self.dyn_call_access()?,
            effects: {
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_17 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_17.push(self.dyn_effect()?);
                }
                values_17
            },
        })
    }
    pub(super) fn dyn_thread_requirements(&mut self) -> WireResult<DynThreadRequirements> {
        Ok(DynThreadRequirements {
            transfer: self.boolean()?,
            share: self.boolean()?,
        })
    }
    pub(super) fn dyn_contract(&mut self) -> WireResult<DynContract> {
        Ok(DynContract {
            requires: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut values_18 = Vec::with_capacity(count);
                    for _ in 0..count {
                        values_18.push(self.dyn_requirement()?);
                    }
                    values_18
                })
            } else {
                None
            },
            guarantees: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut values_19 = Vec::with_capacity(count);
                    for _ in 0..count {
                        values_19.push(self.dyn_guarantee()?);
                    }
                    values_19
                })
            } else {
                None
            },
            hold: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut values_20 = Vec::with_capacity(count);
                    for _ in 0..count {
                        values_20.push(self.storage_position()?);
                    }
                    values_20
                })
            } else {
                None
            },
            call: if self.boolean()? {
                Some(self.dyn_call_access()?)
            } else {
                None
            },
            result: if self.boolean()? {
                Some(self.dyn_result_source()?)
            } else {
                None
            },
            cleanup: if self.boolean()? {
                Some(self.dyn_cleanup_contract()?)
            } else {
                None
            },
            effects: if self.boolean()? {
                Some({
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut values_21 = Vec::with_capacity(count);
                    for _ in 0..count {
                        values_21.push(self.dyn_effect()?);
                    }
                    values_21
                })
            } else {
                None
            },
            resume: if self.boolean()? {
                Some(self.dyn_resume_bound()?)
            } else {
                None
            },
            threads: if self.boolean()? {
                Some(self.dyn_thread_requirements()?)
            } else {
                None
            },
        })
    }
    pub(super) fn dyn_declaration_kind(&mut self) -> WireResult<DynDeclarationKind> {
        Ok(match self.u8()? {
            0 => DynDeclarationKind::Trait,
            1 => DynDeclarationKind::Impl,
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynDeclarationKind tag")),
        })
    }
    pub(super) fn dyn_member_kind(&mut self) -> WireResult<DynMemberKind> {
        Ok(match self.u8()? {
            0 => DynMemberKind::Type,
            1 => DynMemberKind::Constant(self.ty(0)?),
            2 => DynMemberKind::Method({
                let count = self.count(self.limits.max_nodes, 1)?;
                let mut values_22 = Vec::with_capacity(count);
                for _ in 0..count {
                    values_22.push(self.parameter_kind(0)?);
                }
                values_22
            }),
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynMemberKind tag")),
        })
    }
    pub(super) fn dyn_generic_actual(&mut self) -> WireResult<DynGenericActual> {
        Ok(match self.u8()? {
            0 => DynGenericActual::Type(self.ty(0)?),
            1 => DynGenericActual::Const(self.constant()?),
            2 => DynGenericActual::Life(self.storage_binder()?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynGenericActual tag")),
        })
    }
    pub(super) fn dyn_requirement(&mut self) -> WireResult<DynRequirement> {
        Ok(match self.u8()? {
            0 => DynRequirement::Read(self.storage_position()?),
            1 => DynRequirement::Write(self.storage_position()?),
            2 => DynRequirement::Manage(self.storage_position()?),
            3 => DynRequirement::Apart(self.storage_position()?, self.storage_position()?),
            4 => DynRequirement::State {
                position: self.storage_position()?,
                ty: self.ty(0)?,
                variant: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynRequirement tag")),
        })
    }
    pub(super) fn dyn_guarantee(&mut self) -> WireResult<DynGuarantee> {
        Ok(match self.u8()? {
            0 => DynGuarantee::ManageResult,
            1 => DynGuarantee::State {
                position: self.storage_position()?,
                ty: self.ty(0)?,
                variant: self.u32()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynGuarantee tag")),
        })
    }
    pub(super) fn dyn_effect(&mut self) -> WireResult<DynEffect> {
        Ok(match self.u8()? {
            0 => DynEffect::Alloc,
            1 => DynEffect::Io,
            2 => DynEffect::Block,
            3 => DynEffect::Suspend,
            4 => DynEffect::Spawn,
            5 => DynEffect::Ffi,
            6 => DynEffect::Reenter,
            7 => DynEffect::Panic,
            8 => DynEffect::Call(self.storage_position()?),
            9 => DynEffect::Cleanup(self.ty(0)?),
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynEffect tag")),
        })
    }
    pub(super) fn dyn_result_source(&mut self) -> WireResult<DynResultSource> {
        Ok(match self.u8()? {
            0 => DynResultSource::Value,
            1 => DynResultSource::Create,
            2 => DynResultSource::Give,
            3 => DynResultSource::Borrow {
                sources: {
                    let count = self.count(self.limits.max_nodes, 1)?;
                    let mut values_23 = Vec::with_capacity(count);
                    for _ in 0..count {
                        values_23.push(self.storage_position()?);
                    }
                    values_23
                },
                mutable: self.boolean()?,
            },
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynResultSource tag")),
        })
    }
    pub(super) fn dyn_resume_bound(&mut self) -> WireResult<DynResumeBound> {
        Ok(match self.u8()? {
            0 => DynResumeBound::Local,
            1 => DynResumeBound::Transfer,
            _ => return Err(malformed("E-WIRE-TAG", "unknown DynResumeBound tag")),
        })
    }
    pub(super) fn dyn_passing_mode(&mut self) -> WireResult<PassingMode> {
        Ok(match self.u8()? {
            0 => PassingMode::Copy,
            1 => PassingMode::Move,
            2 => PassingMode::Describe,
            3 => PassingMode::Create,
            4 => PassingMode::Give,
            5 => PassingMode::Borrow,
            6 => PassingMode::BorrowMut,
            7 => PassingMode::Infer,
            _ => return Err(malformed("E-WIRE-TAG", "unknown PassingMode tag")),
        })
    }
    pub(super) fn dyn_call_kind(&mut self) -> WireResult<CallKind> {
        Ok(match self.u8()? {
            0 => CallKind::Direct,
            1 => CallKind::Suspend,
            _ => return Err(malformed("E-WIRE-TAG", "unknown CallKind tag")),
        })
    }
}
