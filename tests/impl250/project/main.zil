import model
import dispatch
fn once(cancel:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let active=model.Mark{code:72}
    let spare=model.Mark{code:104}
    let handler=move(event:model.Event)->model.Reply=>{
        model.emit(67)
        let held=give active
        let seen=spare.code
        return model.reply(event,1)
    }
    let reply=dispatch.dispatch(handler,model.prepare(cancel,65,41)?)
    return model.render(reply,42)
}
fn later(cancel:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let active=model.Mark{code:72}
    let spare=model.Mark{code:104}
    let handler=move(first:model.Event,second:model.Event)->model.Reply=>{
        model.emit(67)
        let held=give active
        let seen=spare.code
        return model.reply(first,1)
    }
    let reply=handler(model.prepare(false,65,41)?,model.prepare(cancel,66,99)?)
    return model.render(reply,42)
}
fn repeated(mutate:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    if mutate{
        var count=0
        var handler=move(event:model.Event)->model.Reply=>{
            count+=1
            model.emit(48+count)
            return model.reply(event,count)
        }
        let first=handler(model.prepare(false,65,41)?)
        let a=model.render(first,42)
        let second=handler(model.prepare(false,66,40)?)
        return a+model.render(second,42)
    }
    let handler=(event:model.Event)->model.Reply=>{
        model.emit(67)
        return model.reply(event,1)
    }
    let first=handler(model.prepare(false,65,41)?)
    let a=model.render(first,42)
    let second=handler(model.prepare(false,66,41)?)
    return a+model.render(second,42)
}
fn sum(empty:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=(event:model.Packet)->model.Packet=>{model.emit(67);return event}
    let event=if empty{model.Packet.Empty}else{model.Packet.Data(model.prepare(false,65,41)?)}
    return model.packet(dispatch.packet(handler,event))
}
fn error_data()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=(event:model.Fault!model.Packet)->model.Fault!model.Packet=>{
        model.emit(67)
        let packet=event?
        return packet
    }
    let event:model.Fault!model.Packet=model.Packet.Data(model.prepare(false,65,41)?)
    let packet=dispatch.fallible(handler,event)?
    return model.packet(packet)
}
fn store_data()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=(event:model.Items)->model.Items=>{model.emit(67);return event}
    return model.render_items(dispatch.items(handler,model.items()?))
}
fn main()->model.Fault!i32{
    let mode=model.read()-48
    if mode==0{return once(false)}
    if mode==1{return once(true)}
    if mode==2{return later(true)}
    if mode==3{return repeated(false)}
    if mode==4{return repeated(true)}
    if mode==5{return sum(false)}
    if mode==6{return error_data()}
    if mode==7{return store_data()}
    if mode==8{return sum(true)}
    if mode==9{return later(false)}
    return 9
}
