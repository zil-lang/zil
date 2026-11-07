import model
import relay
fn concrete_fn(right:bool)->i32{
    model.emit(83)
    defer{model.emit(81)}
    let first=model.marker(65)
    let second=model.marker(66)
    let moved=first
    let current=relay.nested(moved,right)
    return current(1)+current(2)-133
}
fn concrete_copy()->i32{
    model.emit(83)
    defer{model.emit(81)}
    let offset=3
    let handler=move(value:i32)->i32=>{model.emit(67);return value+offset}
    let current=relay.nested(handler,false)
    return current(4)+current(5)+handler(6)-24
}
fn mutating()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:77}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return model.Fault.NoMemory}
    bytes_push(bytes,7u8)
    var count=0
    var handler=move(delta:i32)->i32=>{
        let label=mark.code
        if bytes_len(bytes)!=1{return 99}
        count+=delta
        model.emit(48+count)
        return count
    }
    var current=relay.step(handler,1)
    let second=current(1)
    let third=current(1)
    return second+third-5
}
fn once_owned(call:bool,cancel:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let active=model.Mark{code:72}
    let spare=model.Mark{code:104}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return model.Fault.NoMemory}
    bytes_push(bytes,7u8)
    let handler=move(event:model.Event)->model.Reply=>{
        model.emit(67)
        let held=give active
        let label=spare.code
        let length=bytes_len(bytes)
        return model.reply(event,1)
    }
    let current=relay.owned_nested(handler,model.Packet.Data(model.prepare(68,0,cancel)?),call)
    if !call{return 0}
    let reply=current(model.prepare(65,41,false)?)
    return model.render(reply,42)
}
fn union_data(error:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=model.marker(65)
    let data:model.Meta!model.Packet=if error{
        model.Meta{packet:model.Packet.Data(model.prepare(68,0,false)?)}
    }else{model.Packet.Data(model.prepare(68,0,false)?)}
    let current=relay.with_union(handler,data)
    return current(1)-66
}
fn owned_fn(mutate:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:78}
    if mutate{
        var count=0
        var handler=move(event:model.Event)->model.Reply=>{
            let label=mark.code
            count+=1
            model.emit(48+count)
            return model.reply(event,count)
        }
        var current=relay.owned_identity(handler)
        let first=current(model.prepare(65,41,false)?)
        let a=model.render(first,42)
        let second=current(model.prepare(66,40,false)?)
        return a+model.render(second,42)
    }
    let handler=move(event:model.Event)->model.Reply=>{
        let label=mark.code
        model.emit(67)
        return model.reply(event,1)
    }
    let current=relay.owned_identity(handler)
    let first=current(model.prepare(65,41,false)?)
    let a=model.render(first,42)
    let second=current(model.prepare(66,41,false)?)
    return a+model.render(second,42)
}
fn generic_fn()->i32{
    model.emit(83)
    defer{model.emit(81)}
    return relay.fn_twice(model.marker(65))-133
}
fn product_data()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=model.marker(65)
    let current=relay.with_product(handler,model.prepare(68,0,false)?)
    return current(1)-66
}
fn main()->model.Fault!i32{
    let mode=model.read()-48
    if mode==0{return concrete_fn(false)}
    if mode==1{return concrete_fn(true)}
    if mode==2{return concrete_copy()}
    if mode==3{return mutating()}
    if mode==4{return once_owned(false,false)}
    if mode==5{return once_owned(true,false)}
    if mode==6{return once_owned(true,true)}
    if mode==7{return union_data(true)}
    if mode==8{return union_data(false)}
    if mode==9{return owned_fn(false)}
    if mode==10{return owned_fn(true)}
    if mode==11{return generic_fn()}
    if mode==12{return product_data()}
    return 9
}
