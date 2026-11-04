    pub(super) fn declaration_id(&mut self, id: &LifeDeclarationId) -> WireResult<()> {
        self.u32(id.module.0)?; self.u32(id.declaration.0)?; self.boolean(id.compiler)
    }
    pub(super) fn declaration_identity(&mut self, id: &LifeDeclarationIdentity) -> WireResult<()> {
        self.text(&id.package)?;
        for path in [&id.module_path, &id.declaration_path] {
            self.count(path.len(), self.limits.max_nodes)?;
            for part in path { self.text(part)?; }
        }
        self.count(id.generation_path.len(), self.limits.max_call_depth)?;
        for part in &id.generation_path { self.u32(*part)?; }
        Ok(())
    }
    pub(super) fn parameter_kind(&mut self, kind: &LifeParameterKind, depth: usize) -> WireResult<()> {
        match kind {
            LifeParameterKind::Type => self.u8(0),
            LifeParameterKind::Const(ty) => { self.u8(1)?; self.ty(ty, depth + 1) },
            LifeParameterKind::Lifetime => self.u8(2),
        }
    }
    pub(super) fn storage_binder(&mut self, binder: &StorageBinder) -> WireResult<()> {
        match binder {
            StorageBinder::Fixed(i) => { self.u8(0)?; self.u32(*i) },
            StorageBinder::Invocation(i) => { self.u8(1)?; self.u32(*i) },
        }
    }
    pub(super) fn storage_position(&mut self, position: &StoragePosition) -> WireResult<()> {
        match position.base {
            StorageBase::Parameter(i) => { self.u8(0)?; self.u32(i)?; },
            StorageBase::Capture(i) => { self.u8(1)?; self.u32(i)?; },
            StorageBase::Result => self.u8(2)?,
        }
        self.count(position.path.len(), self.limits.max_call_depth)?;
        for step in &position.path {
            match step {
                ContractProjection::Place(p) => { self.u8(0)?; self.storage_step(*p)?; },
                ContractProjection::RetainedPort(i) => { self.u8(1)?; self.u32(*i)?; },
                ContractProjection::RetainedClass(i) => { self.u8(2)?; self.u32(*i)?; },
            }
        }
        Ok(())
    }
    fn life_maps(&mut self, retained: &[StorageRetainedBinding], nominal_arguments: &[NominalLifeArgument]) -> WireResult<()> {
        self.count(retained.len(), self.limits.max_nodes)?;
        for binding in retained { self.life_type_path(&binding.path)?; self.u32(binding.class)?; self.u32(binding.binder)?; }
        self.count(nominal_arguments.len(), self.limits.max_nodes)?;
        for argument in nominal_arguments {
            self.life_type_path(&argument.path)?; self.declaration_id(&argument.declaration)?;
            self.u32(argument.argument)?; self.u32(argument.binder)?;
        }
        Ok(())
    }
    pub(super) fn retained_shape(&mut self, shape: &RetainedLifeShape, depth: usize) -> WireResult<()> {
        if depth > self.limits.max_call_depth { return Err(quota("retained life shape nesting exceeds wire limit")); }
        self.count(shape.binder_count as usize, self.limits.max_nodes)?;
        self.count(shape.ports.len(), self.limits.max_nodes)?;
        for port in &shape.ports { self.ty(&port.leaf, depth + 1)?; self.u8(port.mode as u8)?; self.u32(port.binder)?; }
        self.life_maps(&shape.retained, &shape.nominal_arguments)
    }
    fn storage_shape(&mut self, shape: &StorageLifeShape) -> WireResult<()> {
        self.count(shape.binder_count as usize, self.limits.max_nodes)?;
        self.count(shape.slots.len(), self.limits.max_nodes)?;
        for slot in &shape.slots { self.storage_path(&slot.path)?; self.u32(slot.binder)?; self.u8(slot.mode as u8)?; }
        self.life_maps(&shape.retained, &shape.nominal_arguments)
    }
    pub(super) fn required_storage_contract(&mut self, contract: &StorageLifeContract) -> WireResult<()> {
        self.count(contract.fixed_binder_count as usize, self.limits.max_nodes)?;
        self.count(contract.invocation_binder_count as usize, self.limits.max_nodes)?;
        self.count(contract.bindings.len(), self.limits.max_nodes)?;
        for binding in &contract.bindings { self.storage_position(&binding.position)?; self.storage_binder(&binding.binder)?; }
        Ok(())
    }
    fn storage_contract(&mut self, contract: &Option<StorageLifeContract>) -> WireResult<()> {
        self.boolean(contract.is_some())?;
        if let Some(contract) = contract { self.required_storage_contract(contract)?; }
        Ok(())
    }
    pub(super) fn function_life(&mut self, life: &FunctionLifeContract) -> WireResult<()> {
        self.count(life.binder_count as usize, self.limits.max_locals)?;
        self.count(life.parameter_lives.len(), self.limits.max_locals)?;
        for tag in &life.parameter_lives { self.boolean(tag.is_some())?; if let Some(tag)=tag { self.u32(*tag)?; } }
        self.boolean(life.result_life.is_some())?;
        if let Some(tag)=life.result_life { self.u32(tag)?; }
        Ok(())
    }
    fn life_inventory(&mut self, declarations: &[LifeNominalDeclaration], instances: &[LifeNominalInstance]) -> WireResult<()> {
        self.count(declarations.len(), self.limits.max_nodes)?;
        for declaration in declarations {
            self.declaration_id(&declaration.id)?; self.declaration_identity(&declaration.identity)?;
            self.u8(match declaration.kind { LifeNominalKind::Record=>0, LifeNominalKind::Package=>1, LifeNominalKind::Resource=>2, LifeNominalKind::Enum=>3 })?;
            self.count(declaration.parameters.len(), self.limits.max_nodes)?;
            for parameter in &declaration.parameters { self.parameter_kind(parameter, 0)?; }
        }
        self.count(instances.len(), self.limits.max_nodes)?;
        for instance in instances {
            self.declaration_id(&instance.declaration)?; self.ty(&instance.ty, 0)?;
            self.count(instance.arguments.len(), self.limits.max_nodes)?;
            for argument in &instance.arguments {
                match argument {
                    LifeNominalActual::Type(ty)=>{self.u8(0)?;self.ty(ty,0)?;},
                    LifeNominalActual::Const(value)=>{self.u8(1)?;self.constant(value)?;},
                    LifeNominalActual::Lifetime(i)=>{self.u8(2)?;self.u32(*i)?;},
                }
            }
        }
        Ok(())
    }
