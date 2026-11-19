//! Tag facts follow actual construction and physical projections. A missing
//! subtree is unknown; it never proves an inactive payload or a fresh owner.
use super::*;

fn storage(mut ty: &Type) -> &Type {
    while let Type::BorrowedStorage { storage, .. } = ty { ty = storage; }
    ty
}
fn domain(ty: &Type) -> usize {
    match storage(ty) {
        Type::ErrorUnion { .. } => 2,
        Type::Enum { variants, .. } => variants.len(),
        _ => 0,
    }
}
fn children<'a>(ty: &'a Type, b: &mut Budget) -> Check<Vec<(PlaceProjection, &'a Type)>> {
    let mut result = Vec::new();
    match storage(ty) {
        Type::Tuple(fields) => {
            b.spend(fields.len())?;
            result.extend(fields.iter().enumerate().map(|(i,t)|(PlaceProjection::Field(i as u32),t)));
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            b.spend(fields.len())?;
            result.extend(fields.iter().enumerate().map(|(i,(_,t))|(PlaceProjection::Field(i as u32),t)));
        }
        Type::Enum { variants, .. } => {
            b.spend(variants.len())?;
            for (variant,(_,fields)) in variants.iter().enumerate() {
                b.spend(fields.len())?;
                result.extend(fields.iter().enumerate().map(|(index,t)|
                    (PlaceProjection::Variant(VariantProjection{variant:variant as u32,index:index as u32}),t)));
            }
        }
        Type::ErrorUnion { error, result: success } => {
            b.spend(2)?;
            result.push((PlaceProjection::ErrorArm(true),error));
            result.push((PlaceProjection::ErrorArm(false),success));
        }
        Type::Array { element, length } => {
            if *length as usize>b.limits.max_nodes{return Err(limit("complete Task tag array exceeds selected bound"));}
            b.spend(*length as usize)?;
            result.extend((0..*length).map(|i|(PlaceProjection::Field(i),element.as_ref())));
        }
        _ => {}
    }
    Ok(result)
}
fn selected(tags: &Variants, step: PlaceProjection) -> bool {
    match step {
        PlaceProjection::Variant(v) => tags.active(v.variant),
        PlaceProjection::ErrorArm(failed) => tags.active(u32::from(failed)),
        PlaceProjection::Field(_) => true,
    }
}
impl Variants {
    pub(in crate::stored_flow) fn runtime(ty: &Type, depth: usize, b: &mut Budget) -> Check<Self> {
        Self::tree(ty,None,Vec::new(),depth,b)
    }
    pub(in crate::stored_flow) fn full_parameter(index:u32,ty:&Type,b:&mut Budget)->Check<Self>{
        Self::tree(ty,Some(option_address::Input::Parameter(index)),Vec::new(),0,b)
    }
    pub(in crate::stored_flow) fn full_input(input:option_address::Input,ty:&Type,b:&mut Budget)->Check<Self>{Self::tree(ty,Some(input),Vec::new(),0,b)}
    fn tree(ty:&Type,input:Option<option_address::Input>,path:Path,depth:usize,b:&mut Budget)->Check<Self>{
        b.spend(path.len().saturating_add(1))?;
        if depth>b.limits.max_call_depth.min(128){return Err(limit("complete Task tag tree depth exceeded"));}
        let count=domain(ty);
        if count>b.limits.max_nodes{return Err(limit("complete Task tag domain exceeded"));}
        b.spend(count)?;
        let mut result=Self{domain:count as u32,..Self::default()};
        if count!=0 {
            if let Some(index)=input {result.inputs.push((index,path.clone()));}
            else {result.constructed.extend(0..count as u32);}
        }
        for(step,ty)in children(ty,b)?{
            b.spend(path.len().saturating_add(1))?;
            let mut child_path=path.clone();child_path.push(step);
            let child=Self::tree(ty,input,child_path,depth+1,b)?;
            if child.present(){result.nested.push((vec![step],child));}
        }
        Ok(result)
    }
    pub(in crate::stored_flow) fn full_grounded(&self,ty:&Type,depth:usize,b:&mut Budget)->Check<bool>{
        b.spend(1)?;
        if depth>b.limits.max_call_depth.min(128){return Err(limit("complete tag grounding depth exceeded"));}
        let count=domain(ty);
        if self.domain as usize!=count || count!=0&&(self.constructed.is_empty()&&self.inputs.is_empty()) {return Ok(false);}
        for(step,ty)in children(ty,b)?{
            if !selected(self,step){continue;}
            let expected=Self::runtime(ty,depth+1,b)?;
            if !expected.present(){continue;}
            b.spend(self.nested.len())?;
            let Some((_,child))=self.nested.iter().find(|(p,_)|p.as_slice()==[step])else{return Ok(false);};
            if !child.full_grounded(ty,depth+1,b)?{return Ok(false);}
        }
        Ok(self.present())
    }
    pub(in crate::stored_flow) fn path_active(&self,path:&[PlaceProjection])->bool{
        let Some((step,tail))=path.split_first()else{return true;};
        if !selected(self,*step){return false;}
        self.nested.iter().find(|(p,_)|p.as_slice()==[*step])
            .is_none_or(|(_,child)|child.path_active(tail))
    }
    pub(super) fn excludes_path(&self, path: &[PlaceProjection], b: &mut Budget) -> Check<bool> {
        let mut row = self;
        for step in path {
            b.spend(row.nested.len().saturating_add(row.constructed.len()).saturating_add(1))?;
            let tag = match step {
                PlaceProjection::ErrorArm(arm) => Some(u32::from(*arm)),
                PlaceProjection::Variant(field) => Some(field.variant),
                _ => None,
            };
            if let Some(tag) = tag && tag < row.domain && row.inputs.is_empty()
                && !row.constructed.is_empty() && row.constructed.binary_search(&tag).is_err() {
                return Ok(true);
            }
            let Some((_, child)) = row.nested.iter().find(|(p, _)| p.as_slice() == [*step]) else { break; };
            row = child;
        }
        Ok(false)
    }
    pub(in crate::stored_flow) fn project(&self,path:&[PlaceProjection],b:&mut Budget)->Check<Self>{
        let mut current=self;
        for step in path {
            b.spend(current.nested.len().saturating_add(1))?;
            if !selected(current,*step){return Ok(Self::default());}
            let Some((_,child))=current.nested.iter().find(|(p,_)|p.as_slice()==[*step])else{return Ok(Self::default());};
            current=child;
        }
        current.work(b)?;Ok(current.clone())
    }
    pub(in crate::stored_flow) fn join_child(&mut self,path:&[PlaceProjection],child:&Self,b:&mut Budget)->Check<()>{
        let Some((step,tail))=path.split_first()else{self.join(child,b)?;return Ok(());};
        b.spend(self.nested.len().saturating_add(1))?;
        if let Some((_,actual))=self.nested.iter_mut().find(|(key,_)|key.as_slice()==[*step]){actual.join_child(tail,child,b)?;}
        else if child.present(){let mut nested=Self::default();nested.join_child(tail,child,b)?;b.spend(2)?;self.nested.push((vec![*step],nested));}
        Ok(())
    }
    pub(in crate::stored_flow) fn replace_child(&mut self,path:&[PlaceProjection],child:&Self,b:&mut Budget)->Check<()> {
        let Some((step,tail))=path.split_first()else{child.work(b)?;*self=child.clone();return Ok(());};
        b.spend(self.nested.len().saturating_add(1))?;
        if let Some((_,row))=self.nested.iter_mut().find(|(key,_)|key.as_slice()==[*step]){row.replace_child(tail,child,b)?;}
        else if child.present(){let mut row=Self::default();row.replace_child(tail,child,b)?;b.spend(2)?;self.nested.push((vec![*step],row));}
        self.nested.retain(|(_,row)|row.present());
        Ok(())
    }
    fn attach(&mut self,step:PlaceProjection,child:&Self,b:&mut Budget)->Check<()> {
        if child.present(){child.work(b)?;self.nested.push((vec![step],child.clone()));}Ok(())
    }
}

/// Capture tag inputs before consuming instructions clear their local source.
pub(in crate::stored_flow) fn transfer(
    f:&TypedFunction,i:&Instruction,state:&Frame,values:&[Payload],b:&mut Budget,
)->Check<Option<Variants>>{
    if !b.stored_task_flow(){return Ok(None);}
    let get=|id:ValueId|values.get(id.0 as usize).map(|p|&p.enum_variants)
        .ok_or_else(||err("Task tag input is outside its actual SSA inventory"));
    let mut out=Variants::default();
    match &i.kind {
        InstructionKind::MakeAggregate{fields}|InstructionKind::MakePackage{fields}
        |InstructionKind::MakeOwnedTuple{fields}=>{
            for(index,value)in fields.iter().enumerate(){out.attach(PlaceProjection::Field(index as u32),get(*value)?,b)?;}
        }
        InstructionKind::MakeVariant{variant,fields}=>{
            let ty=i.result.and_then(|id|f.values.get(id.0 as usize)).ok_or_else(||err("Task sum tag producer result is absent"))?;
            out.domain=domain(ty) as u32;
            if *variant>=out.domain{return Err(err("Task sum tag producer selects an absent actual variant"));}
            out.constructed.push(*variant);
            for(index,value)in fields.iter().enumerate(){out.attach(PlaceProjection::Variant(VariantProjection{variant:*variant,index:index as u32}),get(*value)?,b)?;}
        }
        InstructionKind::MakeErrorUnion{value,is_error}=>{
            out.domain=2;out.constructed.push(u32::from(*is_error));
            out.attach(PlaceProjection::ErrorArm(*is_error),get(*value)?,b)?;
        }
        InstructionKind::ReadErrorUnion{value,is_error}|InstructionKind::TakeErrorUnion{value,is_error}=>{
            return get(*value)?.project(&[PlaceProjection::ErrorArm(*is_error)],b).map(Some);
        }
        InstructionKind::UpdateField{aggregate,index,value}=>{
            let source=get(*aggregate)?;source.work(b)?;out=source.clone();
            b.spend(out.nested.len())?;
            out.nested.retain(|(path,_)|path.as_slice()!=[PlaceProjection::Field(*index)]);
            out.attach(PlaceProjection::Field(*index),get(*value)?,b)?;
        }
        InstructionKind::ExtractField{aggregate,index}=>{
            return get(*aggregate)?.project(&[PlaceProjection::Field(*index)],b).map(Some);
        }
        InstructionKind::ReadVariant{value,variant,index}=>{
            return get(*value)?.project(&[PlaceProjection::Variant(VariantProjection{variant:*variant,index:*index})],b).map(Some);
        }
        InstructionKind::ReadVariantLocal{local,path}=>{
            let path=path.iter().copied().map(PlaceProjection::Variant).collect::<Vec<_>>();
            b.spend(path.len())?;
            return state.locals.get(local.0 as usize).ok_or_else(||err("Task tag read has no actual local"))?
                .enum_variants.project(&path,b).map(Some);
        }
        InstructionKind::TakeVariantField{local,path}=>{
            return state.locals.get(local.0 as usize).ok_or_else(||err("Task tag projection has no actual local"))?
                .enum_variants.project(path,b).map(Some);
        }
        InstructionKind::TakePackageField{local,index}|InstructionKind::ReadPackageField{local,index}=>{
            return state.locals.get(local.0 as usize).ok_or_else(||err("Task tag field projection has no actual local"))?
                .enum_variants.project(&[PlaceProjection::Field(*index)],b).map(Some);
        }
        _=>return Ok(None),
    }
    Ok(Some(out))
}
