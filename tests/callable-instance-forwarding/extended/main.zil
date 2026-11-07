import model
import relay
fn store_data()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=model.marker(65)
    let current=relay.store_first(model.prepare(68,false)?,handler)
    return current(1)+current(2)-133
}
fn address_data(first:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:65}
    var count=0
    var handler=move(value:&mut i32)->i32=>{
        let label=mark.code
        count+=1
        model.emit(48+count)
        *value+=count
        return *value
    }
    var value=40
    var early=0
    if first{early=handler(&mut value)}
    var current=relay.address(model.prepare(68,false)?,handler)
    let a=if first{early}else{current(&mut value)}
    let b=current(&mut value)
    return a+b-84+value-43
}
fn slice_data()->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:65}
    let handler=move(values:&[u8])->i32=>{
        let label=mark.code
        model.emit(67)
        return i32(values[0])+i32(values[1])
    }
    let current=relay.slice(7,handler,model.prepare(68,false)?)
    let values=[7u8,9u8]
    let view=&values[0..2]
    return current(view)+current(view)-32
}
fn union_data(error:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let handler=model.marker(65)
    let data:model.FaultData!model.Packet=if error{
        model.FaultData{packet:model.Packet.Stored(model.prepare(68,false)?)}
    }else{model.Packet.Stored(model.prepare(68,false)?)}
    let current=relay.union_last(7,data,handler,error)
    return current(1)-66
}
fn loop_forward(count:i32)->i32{
    model.emit(83)
    defer{model.emit(81)}
    let current=relay.spin(count,model.marker(65))
    return current(1)+current(2)-133
}
fn loop_mutating(count:i32)->i32{
    model.emit(83)
    defer{model.emit(81)}
    let mark=model.Mark{code:65}
    var total=0
    var handler=move(delta:i32)->i32=>{
        let label=mark.code
        total+=delta
        model.emit(48+total)
        return total
    }
    var current=relay.tick(count,handler)
    return current(1)-count-1
}
fn later(cancel:bool)->model.Fault!i32{
    model.emit(83)
    defer{model.emit(81)}
    let active=model.Mark{code:72}
    let spare=model.Mark{code:104}
    let handler=move()->i32=>{
        model.emit(67)
        let held=give active
        let label=spare.code
        return 0
    }
    let current=relay.later(model.prepare(68,false)?,handler,model.prepare(76,cancel)?)
    return current()
}
fn main()->model.Fault!i32{
    let mode=model.read()-48
    if mode==0{return store_data()}
    if mode==1{return address_data(false)}
    if mode==2{return address_data(true)}
    if mode==3{return slice_data()}
    if mode==4{return union_data(true)}
    if mode==5{return union_data(false)}
    if mode==6{return loop_forward(0)}
    if mode==7{return loop_forward(3)}
    if mode==8{return loop_mutating(0)}
    if mode==9{return loop_mutating(3)}
    if mode==10{return later(true)}
    if mode==11{return later(false)}
    return 9
}
