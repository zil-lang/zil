    pub(super) fn declaration_id(&mut self) -> WireResult<LifeDeclarationId> {
        Ok(LifeDeclarationId{module:ModuleId(self.u32()?),declaration:NodeId(self.u32()?),compiler:self.boolean()?})
    }
    pub(super) fn declaration_identity(&mut self) -> WireResult<LifeDeclarationIdentity> {
        let package=self.text()?;
        let mut paths=Vec::with_capacity(2);
        for _ in 0..2 { let n=self.count(self.limits.max_nodes,4)?;let mut path=Vec::with_capacity(n);for _ in 0..n {path.push(self.text()?);}paths.push(path); }
        let n=self.count(self.limits.max_call_depth,4)?;let mut generation_path=Vec::with_capacity(n);for _ in 0..n{generation_path.push(self.u32()?);}
        let declaration_path=paths.pop().expect("two declaration paths");let module_path=paths.pop().expect("two declaration paths");
        Ok(LifeDeclarationIdentity{package,module_path,declaration_path,generation_path})
    }
    pub(super) fn parameter_kind(&mut self, depth: usize) -> WireResult<LifeParameterKind> {
        Ok(match self.u8()? {0=>LifeParameterKind::Type,1=>LifeParameterKind::Const(self.ty(depth+1)?),2=>LifeParameterKind::Lifetime,_=>return Err(malformed("E-WIRE-TAG","unknown nominal parameter kind"))})
    }
    pub(super) fn storage_binder(&mut self) -> WireResult<StorageBinder> {
        Ok(match self.u8()?{0=>StorageBinder::Fixed(self.u32()?),1=>StorageBinder::Invocation(self.u32()?),_=>return Err(malformed("E-WIRE-TAG","unknown storage life binder domain"))})
    }
    pub(super) fn storage_position(&mut self) -> WireResult<StoragePosition> {
        let base=match self.u8()?{0=>StorageBase::Parameter(self.u32()?),1=>StorageBase::Capture(self.u32()?),2=>StorageBase::Result,_=>return Err(malformed("E-WIRE-TAG","unknown storage contract base"))};
        let n=self.count(self.limits.max_call_depth,3)?;let mut path=Vec::with_capacity(n);
        for _ in 0..n{path.push(match self.u8()?{0=>ContractProjection::Place(self.storage_step()?),1=>ContractProjection::RetainedPort(self.u32()?),2=>ContractProjection::RetainedClass(self.u32()?),_=>return Err(malformed("E-WIRE-TAG","unknown contract projection"))});}
        Ok(StoragePosition{base,path})
    }
    fn stored_borrow_mode(&mut self)->WireResult<StoredBorrowMode>{Ok(match self.u8()?{0=>StoredBorrowMode::SharedAddress,1=>StoredBorrowMode::ExclusiveAddress,2=>StoredBorrowMode::ReadOnlyDescription,3=>StoredBorrowMode::WritableDescription,_=>return Err(malformed("E-WIRE-TAG","unknown stored borrow mode"))})}
    fn life_maps(&mut self)->WireResult<(Vec<StorageRetainedBinding>,Vec<NominalLifeArgument>)>{
        let n=self.count(self.limits.max_nodes,12)?;let mut retained=Vec::with_capacity(n);
        for _ in 0..n{retained.push(StorageRetainedBinding{path:self.life_type_path()?,class:self.u32()?,binder:self.u32()?});}
        let n=self.count(self.limits.max_nodes,21)?;let mut nominal_arguments=Vec::with_capacity(n);
        for _ in 0..n{nominal_arguments.push(NominalLifeArgument{path:self.life_type_path()?,declaration:self.declaration_id()?,argument:self.u32()?,binder:self.u32()?});}
        Ok((retained,nominal_arguments))
    }
    pub(super) fn retained_shape(&mut self, depth:usize)->WireResult<RetainedLifeShape>{
        if depth>self.limits.max_call_depth{return Err(quota("retained life shape nesting exceeds wire limit"));}
        let binder_count=self.count(self.limits.max_nodes,0)? as u32;
        let n=self.count(self.limits.max_nodes,6)?;let mut ports=Vec::with_capacity(n);
        for _ in 0..n{ports.push(RetainedBorrowPort{leaf:self.ty(depth+1)?,mode:self.stored_borrow_mode()?,binder:self.u32()?});}
        let (retained,nominal_arguments)=self.life_maps()?;
        Ok(RetainedLifeShape{binder_count,ports,retained,nominal_arguments})
    }
    fn storage_shape(&mut self)->WireResult<StorageLifeShape>{
        let binder_count=self.count(self.limits.max_nodes,0)? as u32;
        let n=self.count(self.limits.max_nodes,9)?;let mut slots=Vec::with_capacity(n);
        for _ in 0..n{slots.push(StorageLifeSlot{path:self.storage_path()?,binder:self.u32()?,mode:self.stored_borrow_mode()?});}
        let (retained,nominal_arguments)=self.life_maps()?;Ok(StorageLifeShape{binder_count,slots,retained,nominal_arguments})
    }
    pub(super) fn required_storage_contract(&mut self)->WireResult<StorageLifeContract>{
        let fixed_binder_count=self.count(self.limits.max_nodes,0)? as u32;
        let invocation_binder_count=self.count(self.limits.max_nodes,0)? as u32;
        let n=self.count(self.limits.max_nodes,10)?;let mut bindings=Vec::with_capacity(n);
        for _ in 0..n{bindings.push(StorageLifeBinding{position:self.storage_position()?,binder:self.storage_binder()?});}
        Ok(StorageLifeContract{fixed_binder_count,invocation_binder_count,bindings})
    }
    fn storage_contract(&mut self)->WireResult<Option<StorageLifeContract>>{
        if self.boolean()?{self.required_storage_contract().map(Some)}else{Ok(None)}
    }
    pub(super) fn function_life(&mut self)->WireResult<FunctionLifeContract>{
        let binder_count=self.count(self.limits.max_locals,0)? as u32;
        let n=self.count(self.limits.max_locals,1)?;let mut parameter_lives=Vec::with_capacity(n);
        for _ in 0..n{parameter_lives.push(if self.boolean()?{Some(self.u32()?)}else{None});}
        let result_life=if self.boolean()?{Some(self.u32()?)}else{None};
        Ok(FunctionLifeContract{binder_count,parameter_lives,result_life})
    }
    fn life_inventory(&mut self)->WireResult<(Vec<LifeNominalDeclaration>,Vec<LifeNominalInstance>)>{
        let n=self.count(self.limits.max_nodes,30)?;let mut declarations=Vec::with_capacity(n);
        for _ in 0..n{
            let id=self.declaration_id()?;let identity=self.declaration_identity()?;
            let kind=match self.u8()?{0=>LifeNominalKind::Record,1=>LifeNominalKind::Package,2=>LifeNominalKind::Resource,3=>LifeNominalKind::Enum,_=>return Err(malformed("E-WIRE-TAG","unknown nominal declaration kind"))};
            let n=self.count(self.limits.max_nodes,1)?;let mut parameters=Vec::with_capacity(n);for _ in 0..n{parameters.push(self.parameter_kind(0)?);}
            declarations.push(LifeNominalDeclaration{id,identity,kind,parameters});
        }
        let n=self.count(self.limits.max_nodes,14)?;let mut instances=Vec::with_capacity(n);
        for _ in 0..n{
            let declaration=self.declaration_id()?;let ty=self.ty(0)?;
            let n=self.count(self.limits.max_nodes,1)?;let mut arguments=Vec::with_capacity(n);
            for _ in 0..n{arguments.push(match self.u8()?{0=>LifeNominalActual::Type(self.ty(0)?),1=>LifeNominalActual::Const(self.constant()?),2=>LifeNominalActual::Lifetime(self.u32()?),_=>return Err(malformed("E-WIRE-TAG","unknown nominal actual argument kind"))});}
            instances.push(LifeNominalInstance{declaration,ty,arguments});
        }
        Ok((declarations,instances))
    }
