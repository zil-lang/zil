//! Raw frame domains retain each complete child domain before composing the
//! header's outside images. No row in this module is a root or a WF receipt.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-TASK", None, message)
}

pub(crate) struct Frame {
    pub(crate) shape: RetainedLifeShape,
    pub(crate) sources: Vec<RetainedPortBinding>,
    pub(crate) parameters: Vec<StorageLifeShape>,
    pub(crate) result: StorageLifeShape,
    pub(crate) maps: Vec<Vec<u32>>,
}
/// The raw joint directory is an untrusted declaration. Each child's own
/// domain remains intact even when the declared parent images coincide.
pub(crate) fn frame(
    module: &TypedModule,
    f: &TypedFunction,
    declaration: &Type,
    b: &mut Budget,
) -> Check<Frame> {
    let Type::Task {
        result,
        retained_life,
    } = declaration
    else {
        return Err(bad("raw Task frame selects another declared constructor"));
    };
    dynamic_metadata::ty(declaration, b)?;
    if **result != f.return_type {
        return Err(bad("raw Task frame changes its complete worker result"));
    }
    if retained_life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("raw Task joint frame domain exceeded"));
    }
    b.spend(retained_life.binder_count as usize)?;
    let mut parent = vec![None; retained_life.binder_count as usize];
    let mut parameters = Vec::new();
    let mut maps = Vec::new();
    let mut rebuilt = empty_context();
    let mut sources = Vec::new();
    for (ordinal, parameter) in f.parameters.iter().enumerate() {
        let ty = local_type(f, *parameter, Some(f.span))?;
        let child = life(module, ty, f, StorageBase::Parameter(ordinal as u32), b, 0)?;
        b.spend(child.binder_count as usize)?;
        let mut map = Vec::new();
        let mut injective = BTreeSet::new();
        for class in 0..child.binder_count {
            b.spend(retained_life.retained.len())?;
            let path = vec![LifeTypeProjection::Parameter(ordinal as u32)];
            let mut rows = retained_life
                .retained
                .iter()
                .filter(|row| row.path == path && row.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("raw Task joint declaration omits a whole input class"))?;
            if rows.next().is_some() || !injective.insert(row.binder) {
                return Err(bad(
                    "raw Task joint declaration duplicates or coalesces an intrinsic child class",
                ));
            }
            let image = image(f, StorageBase::Parameter(ordinal as u32), &child, class, b)?;
            let slot = parent
                .get_mut(row.binder as usize)
                .ok_or_else(|| bad("raw Task child map escapes its declared full context"))?;
            if slot.is_some_and(|known| known != image) {
                return Err(bad(
                    "raw Task joint embedding disagrees with its complete worker header",
                ));
            }
            *slot = Some(image);
            map.push(row.binder);
            rebuilt.retained.push(row.clone());
        }
        for row in &child.nominal_arguments {
            b.spend(row.path.len().saturating_add(1))?;
            let mut row = row.clone();
            row.path
                .insert(0, LifeTypeProjection::Parameter(ordinal as u32));
            row.binder = *map
                .get(row.binder as usize)
                .ok_or_else(|| bad("raw Task phantom occurrence loses its complete child map"))?;
            rebuilt.nominal_arguments.push(row);
        }
        for (leaf, mode, class, path) in
            crate::semantic::task_domains::raw_input_ports(module, ty, &child, b)?
        {
            let binder = *map
                .get(class as usize)
                .ok_or_else(|| bad("raw Task port loses its complete child class"))?;
            let port = rebuilt.ports.len() as u32;
            rebuilt
                .ports
                .push(RetainedBorrowPort { leaf, mode, binder });
            sources.push(RetainedPortBinding {
                port,
                source: StoragePosition {
                    base: StorageBase::Parameter(ordinal as u32),
                    path,
                },
            });
        }
        parameters.push(child);
        maps.push(map);
    }
    if parent.iter().any(Option::is_none) {
        return Err(bad(
            "raw Task joint domain contains an ungrounded or redundant parent class",
        ));
    }
    let result = life(module, &f.return_type, f, StorageBase::Result, b, 0)?;
    let mut injective = BTreeSet::new();
    for class in 0..result.binder_count {
        b.spend(retained_life.retained.len())?;
        let mut rows = retained_life
            .retained
            .iter()
            .filter(|row| row.path == [LifeTypeProjection::Result] && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("raw Task joint declaration omits a complete result class"))?;
        if rows.next().is_some() || !injective.insert(row.binder) {
            return Err(bad(
                "raw Task result map coalesces independent intrinsic classes",
            ));
        }
        let image = image(f, StorageBase::Result, &result, class, b)?;
        if parent.get(row.binder as usize) != Some(&Some(image)) {
            return Err(bad(
                "raw Task result embedding disagrees with its full worker header",
            ));
        }
        rebuilt.retained.push(row.clone());
    }
    rebuilt.binder_count = retained_life.binder_count;
    rebuilt.retained.sort();
    rebuilt.nominal_arguments.sort();
    dynamic_metadata::retained_shape(&rebuilt, b)?;
    if rebuilt != *retained_life {
        return Err(bad(
            "raw Task declaration changes its full input/result/port/phantom occurrence directory",
        ));
    }
    // Actual return sources, every worker PC and every constructor's operands
    // remain mandatory in the ordinary stored/resource/Task passes.
    Ok(Frame {
        shape: rebuilt,
        sources,
        parameters,
        result,
        maps,
    })
}
pub(crate) fn image(
    f: &TypedFunction,
    base: StorageBase,
    life: &StorageLifeShape,
    class: u32,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let header = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("raw Task nonzero child has no complete worker header"))?;
    signature_life::charge(header, b)?;
    let mut found = None;
    let mut bind = |image: StorageBinder| -> Check<()> {
        if found.is_some_and(|known| known != image) {
            return Err(bad(
                "raw Task child occurrences disagree within the worker header",
            ));
        }
        found = Some(image);
        Ok(())
    };
    for row in &header.classes {
        if row.position.base == base && row.position.path.is_empty() && row.position.class == class
        {
            bind(row.binder)?;
        }
    }
    for slot in &life.slots {
        if slot.binder != class {
            continue;
        }
        b.spend(slot.path.len().saturating_add(header.bindings.len()))?;
        let path = slot
            .path
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        for row in &header.bindings {
            if row.position.base == base && row.position.path == path {
                bind(row.binder)?;
            }
        }
    }
    for child in &life.retained {
        if child.binder != class {
            continue;
        }
        b.spend(header.classes.len().saturating_add(child.path.len()))?;
        for row in &header.classes {
            if row.position.base == base
                && row.position.path == child.path
                && row.position.class == child.class
            {
                bind(row.binder)?;
            }
        }
    }
    found.ok_or_else(|| bad("raw Task whole class has no exact complete worker-header occurrence"))
}

pub(crate) fn life(
    module: &TypedModule,
    ty: &Type,
    f: &TypedFunction,
    base: StorageBase,
    b: &mut Budget,
    depth: usize,
) -> Check<StorageLifeShape> {
    if depth==0&&let Some(header)=&f.storage_life{
        return header_life(module,ty,header,base,&[],b);
    }
    let static_outer = depth == 0
        && f.storage_life.as_ref().is_some_and(|header| {
            header
                .static_bindings
                .contains(&StoragePosition { base, path: vec![] })
        });
    type_life(module, ty, static_outer, Some(f.span), b, depth)
}

/// Complete type domain at a declared Task result boundary. No missing
/// Result mapping is interpreted as a static source declaration.
pub(crate) fn declared_life(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<StorageLifeShape> {
    type_life(module, ty, false, None, b, 0)
}

/// A real signature's immutable static positions are a directory context,
/// not a literal/source receipt. Raw Type-only queries deliberately lack it.
pub(crate) fn header_life(module:&TypedModule,ty:&Type,header:&StorageLifeContract,base:StorageBase,prefix:&[PlaceProjection],b:&mut Budget)->Check<StorageLifeShape>{
    dynamic_metadata::bounds_type(ty,b)?;
    signature_life::charge(header,b)?;
    let mut positions=Vec::new();
    for row in &header.static_bindings{
        b.spend(prefix.len().saturating_add(row.path.len()))?;
        if row.base!=base{continue;}
        let Some(path)=row.path.iter().map(|step|match step{ContractProjection::Place(step)=>Some(*step),_=>None}).collect::<Option<Vec<_>>>()else{continue;};
        if let Some(tail)=path.strip_prefix(prefix){b.spend(tail.len().saturating_add(1))?;positions.push(tail.to_vec());}
    }
    if positions.is_empty(){return declared_life(module,ty,b);}
    header_node(module,ty,&positions,&mut vec![],b,0)
}
fn header_node(module:&TypedModule,ty:&Type,statics:&[Vec<PlaceProjection>],path:&mut Vec<PlaceProjection>,b:&mut Budget,depth:usize)->Check<StorageLifeShape>{
    b.spend(1)?;
    if depth>b.limits.max_call_depth.min(128){return Err(limit("contextual header child domain depth exceeded"));}
    let children:Vec<(PlaceProjection,&Type)>=match ty{
        Type::Tuple(fields)=>{b.spend(fields.len())?;fields.iter().enumerate().map(|(i,t)|(PlaceProjection::Field(i as u32),t)).collect()},
        Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;fields.iter().enumerate().map(|(i,(_,t))|(PlaceProjection::Field(i as u32),t)).collect()},
        Type::ErrorUnion{error,result}=>{b.spend(2)?;vec![(PlaceProjection::ErrorArm(true),error),(PlaceProjection::ErrorArm(false),result)]},
        _=>vec![],
    };
    if !children.is_empty()||matches!(ty,Type::Tuple(_)|Type::Record{..}|Type::Package{..}|Type::ErrorUnion{..}){
        let mut result=StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]};
        for(step,child_type)in children{
            b.spend(path.len().saturating_add(1))?;path.push(step);
            let child=header_node(module,child_type,statics,path,b,depth+1)?;path.pop();
            let offset=result.binder_count;
            result.binder_count=offset.checked_add(child.binder_count).ok_or_else(||limit("header complete aggregate domain overflow"))?;
            if result.binder_count as usize>b.limits.max_nodes{return Err(limit("header complete aggregate domain exceeded"));}
            let opaque=matches!(child_type,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..});
            if opaque||matches!(child_type,Type::Managed(_)){
                b.spend(child.binder_count as usize)?;
                for class in 0..child.binder_count{result.retained.push(StorageRetainedBinding{path:vec![LifeTypeProjection::Place(step)],class,binder:offset+class});}
            }
            for row in &child.slots{b.spend(row.path.len().saturating_add(1))?;let mut full=vec![step];full.extend_from_slice(&row.path);result.slots.push(StorageLifeSlot{path:full,mode:row.mode,binder:row.binder+offset});}
            if !opaque{for row in &child.retained{b.spend(row.path.len().saturating_add(1))?;let mut full=vec![LifeTypeProjection::Place(step)];full.extend_from_slice(&row.path);result.retained.push(StorageRetainedBinding{path:full,class:row.class,binder:row.binder+offset});}}
            for row in &child.nominal_arguments{b.spend(row.path.len().saturating_add(1))?;let mut full=vec![LifeTypeProjection::Place(step)];full.extend_from_slice(&row.path);result.nominal_arguments.push(NominalLifeArgument{path:full,declaration:row.declaration,argument:row.argument,binder:row.binder+offset});}
        }
        result.retained.sort();result.nominal_arguments.sort();return Ok(result);
    }
    if matches!(ty,Type::Address{mutable:false,..}|Type::Desc(_)){
        b.spend(statics.len().saturating_mul(path.len().saturating_add(1)))?;
        // Only the exact physical readonly head is static. Pointee κ are
        // rebuilt by type_life independently and are never subtracted.
        return type_life(module,ty,statics.iter().any(|candidate|candidate==path),None,b,depth);
    }
    // Mutable heads are not promoted by a missing variable row. A description
    // only uses the explicit header Static context above, never slot absence.
    // Opaque
    // children keep their own complete declared scope and phantom classes.
    declared_life(module,ty,b)
}

fn type_life(
    module: &TypedModule,
    ty: &Type,
    static_outer: bool,
    span: Option<Span>,
    b: &mut Budget,
    depth: usize,
) -> Check<StorageLifeShape> {
    dynamic_metadata::ty(ty, b)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("raw Task complete child domain depth exceeded"));
    }
    let mut shape = match ty {
        Type::BorrowedStorage { life, .. } => life.clone(),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            StorageLifeShape {
                binder_count: retained_life.binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            }
        }
        Type::Closure(id) => {
            b.spend(module.closures.len())?;
            let mut rows = module.closures.iter().filter(|row| row.id == *id);
            let closure = rows
                .next()
                .ok_or_else(|| bad("raw Task closure child is absent"))?;
            if rows.next().is_some() {
                return Err(bad("raw Task closure child identity is duplicated"));
            }
            let header = closure
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("raw Task closure full domain is Unknown"))?;
            signature_life::charge(header, b)?;
            StorageLifeShape {
                binder_count: header.fixed_binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            }
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            b.spend(module.dyn_interfaces.len())?;
            let mut rows = module
                .dyn_interfaces
                .iter()
                .filter(|row| row.id == *interface);
            let interface = rows
                .next()
                .ok_or_else(|| bad("raw Task opaque child is absent"))?;
            if rows.next().is_some() {
                return Err(bad("raw Task opaque child identity is duplicated"));
            }
            dynamic_metadata::retained_shape(&interface.retained_life,b)?;
            StorageLifeShape {
                binder_count: interface.retained_life.binder_count,
                slots: vec![],
                retained: interface.retained_life.retained.clone(),
                nominal_arguments: interface.retained_life.nominal_arguments.clone(),
            }
        }
        Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))=>{
            let child=type_life(module,element,false,span,b,depth+1)?;
            let mut parent=StorageLifeShape{binder_count:child.binder_count,slots:vec![],retained:vec![],nominal_arguments:vec![]};
            b.spend(child.binder_count as usize)?;
            for class in 0..child.binder_count{parent.retained.push(StorageRetainedBinding{path:vec![LifeTypeProjection::Element],class,binder:class});}
            parent
        },
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => {
            let child = type_life(module, inner, false, span, b, depth + 1)?;
            let outer = matches!(ty, Type::Address { .. } | Type::Desc(_));
            let offset = u32::from(outer && !static_outer);
            let mut parent = StorageLifeShape {
                binder_count: child
                    .binder_count
                    .checked_add(offset)
                    .ok_or_else(|| limit("raw Task child domain overflow"))?,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            };
            if offset != 0 {
                parent.slots.push(StorageLifeSlot {
                    path: vec![],
                    binder: 0,
                    mode: match ty {
                        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                        Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                        _ => StoredBorrowMode::ReadOnlyDescription,
                    },
                });
            }
            b.spend(child.binder_count as usize)?;
            for class in 0..child.binder_count {
                parent.retained.push(StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Pointee],
                    class,
                    binder: class + offset,
                });
            }
            for row in &child.retained {
                b.spend(row.path.len().saturating_add(1))?;
                let mut row = row.clone();
                row.path.insert(0, LifeTypeProjection::Pointee);
                row.binder = row.binder.checked_add(offset)
                    .ok_or_else(|| limit("raw Task descendant class domain overflow"))?;
                parent.retained.push(row);
            }
            for row in &child.nominal_arguments {
                b.spend(row.path.len().saturating_add(1))?;
                let mut row = row.clone();
                row.path.insert(0, LifeTypeProjection::Pointee);
                row.binder += offset;
                parent.nominal_arguments.push(row);
            }
            parent
        }
        // Reconstruct from the actual module/header here. The preliminary
        // input-scope cache can be absent while semantic child flow runs;
        // absence is Unknown and cannot establish a closed child domain.
        _ => match dynamic_witness::intrinsic_domain(module,ty,b)?.0 {
            0 => StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            _ => {
                return Err(unsupported(
                    "raw Task child requires its complete qualified domain",
                    span,
                ));
            }
        },
    };
    if shape.binder_count as usize > b.limits.max_nodes {
        return Err(limit("raw Task child full domain exceeded"));
    }
    b.spend(shape.binder_count as usize)?;
    shape.retained.sort();
    shape.nominal_arguments.sort();
    Ok(shape)
}

#[cfg(test)]
mod static_capture_context_tests {
    use super::*;
    fn header(path: Vec<ContractProjection>) -> StorageLifeContract {
        StorageLifeContract { fixed_binder_count: 0, invocation_binder_count: 0,
            bindings: vec![], classes: vec![], static_bindings: vec![StoragePosition{base:StorageBase::Capture(0),path}] }
    }
    #[test]
    fn static_tuple_capture_uses_exact_header_position_without_erasing_pointee_classes_raw_wire(){
        let (mut module,_)=crate::tests::baseline();
        let pointer=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};
        let tuple=Type::Tuple(vec![pointer,Type::I32]);
        module.functions[0].locals[0].ty=tuple.clone();
        let limits=Limits::default();
        let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();
        let (wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        for module in [&module,&wire]{
            let actual=&module.functions[0].locals[0].ty;
            let query=|header:&StorageLifeContract|header_life(module,actual,header,StorageBase::Capture(0),&[],&mut Budget::new(&limits));
            let exact=header(vec![ContractProjection::Place(PlaceProjection::Field(0))]);
            let life=query(&exact).unwrap();assert_eq!(life.binder_count,0);assert!(life.slots.is_empty());
            assert!(crate::dynamic_witness::intrinsic_domain(module,actual,&mut Budget::new(&limits)).is_err(),"bare Type cannot invent the header's Static context");
            let mut missing=exact.clone();missing.static_bindings.clear();assert!(query(&missing).is_err());
            let wrong=header(vec![ContractProjection::Place(PlaceProjection::Field(1))]);
            assert_eq!(query(&wrong).unwrap().binder_count,1,"another field does not certify this pointer");
            let Type::Tuple(fields)=actual else{unreachable!()};
            let mut mutable=fields.clone();let Type::Address{mutable:mode,..}=&mut mutable[0]else{unreachable!()};*mode=true;
            assert_eq!(header_life(module,&Type::Tuple(mutable),&exact,StorageBase::Capture(0),&[],&mut Budget::new(&limits)).unwrap().binder_count,1);
            // The opaque pointee's entire two-class domain survives a Static
            // outer. This metadata check grants neither target nor phase.
            let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:2,ports:vec![],retained:vec![],nominal_arguments:vec![]}};
            let qualified=Type::Tuple(vec![Type::Address{mutable:false,inner:Box::new(task)}]);
            let life=header_life(module,&qualified,&exact,StorageBase::Capture(0),&[],&mut Budget::new(&limits)).unwrap();
            assert_eq!(life.binder_count,2);assert!(life.slots.is_empty());
            assert_eq!(life.retained,vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0)),LifeTypeProjection::Pointee],class:0,binder:0},StorageRetainedBinding{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0)),LifeTypeProjection::Pointee],class:1,binder:1}]);
        }
    }
}
