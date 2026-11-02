import model
fn ordinary(call:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:77}
    let buffer=bytes_new()
    if !bytes_reserve(buffer,1){return model.Fault.NoMemory}
    bytes_push(buffer,86u8)
    var count=0
    var handler=move(event:model.Event)->model.Reply=>{
        let view=bytes_view(buffer)
        let extra=i32(view[0])-86+mark.code-77
        count+=1
        model.emit(48+count)
        return model.reply(event,count+extra)
    }
    if !call{return 0}
    let first=handler(model.prepare(65,41,false)?)
    let a=model.render(first,42)
    let second=handler(model.prepare(66,40,false)?)
    return a+model.render(second,42)
}
fn main()->model.Fault!i32{
    let mode=model.read()-48
    if mode==0{return ordinary(true)}
    if mode==1{return ordinary(false)}
    return 9
}
