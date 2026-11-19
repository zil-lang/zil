//! Initial field facts are tied to actual initialized input slots. This walk
//! does not create a stored pointer from a missing directory or grant Static.
use super::*;
pub(in crate::stored_flow) fn seed_parameter(module:&TypedModule,ty:&Type,index:u32,payload:&mut Payload,b:&mut Budget)->Check<()>{
    seed(module,ty,option_address::Input::Parameter(index),payload,b)
}
pub(in crate::stored_flow) fn seed_capture(module:&TypedModule,ty:&Type,index:u32,payload:&mut Payload,b:&mut Budget)->Check<()>{
    seed(module,ty,option_address::Input::Capture(index),payload,b)
}
fn seed(module:&TypedModule,ty:&Type,input:option_address::Input,payload:&mut Payload,b:&mut Budget)->Check<()>{
    let mut pending=vec![(ty,Path::new())];
    while let Some((ty,path))=pending.pop(){
        b.spend(path.len().saturating_add(1))?;
        if path.len()>b.limits.max_call_depth.min(128){return Err(limit("actual field input directory depth exceeded"));}
        {
            let mut root=FieldFacts::default();
            if option_address::signature(ty){root.option=option_address::Variants::input_at(input,&path,b)?;}
            if matches!(ty,Type::Desc(_)){
                if let Some(input)=leaf(payload,&path,b)?{permission_work(input,b)?;root.description=input.clone();}
            }
            if matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Dyn{..})){
                if let Some(input)=leaf(payload,&path,b)?{
                    permission_work(input,b)?;
                    // This is the real initialized input pointer, not the
                    // address of its carrier slot or a fabricated parent.
                    root.views.push(dyn_ports::View{table:None,head:input.clone()});
                }
            }
            if path.is_empty(){
                if root.option.mask()!=0{payload.option_variants=root.option;}
                // Root descriptions/views were independently initialized by
                // their namespace seed; do not replace them from type.
            }else{put(payload,&path,&root,b)?;}
        }
        let mut storage=ty;while let Type::BorrowedStorage{storage:inner,..}=storage{storage=inner;}
        let mut fields=Vec::new();
        match storage{
            Type::Tuple(children)=>{b.spend(children.len())?;fields.extend(children.iter().enumerate().map(|(i,t)|(PlaceProjection::Field(i as u32),t)));},
            Type::Record{fields:children,..}|Type::Package{fields:children,..}=>{b.spend(children.len())?;fields.extend(children.iter().enumerate().map(|(i,(_,t))|(PlaceProjection::Field(i as u32),t)));},
            Type::Enum{variants,..}=>{for(v,(_,children))in variants.iter().enumerate(){b.spend(children.len())?;fields.extend(children.iter().enumerate().map(|(i,t)|(PlaceProjection::Variant(VariantProjection{variant:v as u32,index:i as u32}),t)));}},
            Type::ErrorUnion{error,result}=>{b.spend(2)?;fields.extend([(PlaceProjection::ErrorArm(true),error.as_ref()),(PlaceProjection::ErrorArm(false),result.as_ref())]);},
            Type::Closure(id)=>{let definition=module.closures.get(id.0 as usize).filter(|d|d.id==*id).ok_or_else(||err("field input closure identity is absent"))?;b.spend(definition.captures.len())?;fields.extend(definition.captures.iter().enumerate().map(|(i,t)|(PlaceProjection::Field(i as u32),t)));},
            Type::Array { .. } if scalar_array(storage, path.len(), b)? => {}
            Type::Array{element,length}=>{b.spend(*length as usize)?;fields.extend((0..*length).map(|i|(PlaceProjection::Field(i),element.as_ref())));},
            _=>{},
        }
        b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
        for(step,child)in fields{let mut key=path.clone();key.push(step);pending.push((child,key));}
    }
    Ok(())
}
