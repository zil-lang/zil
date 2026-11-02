import bridge
import model
fn pure()->i32{
    var f=(value:i32)->i32=>{bridge.hit(70);return value+1}
    let body=()->i32=>{
        bridge.body()
        if bridge.pump(2)!=3{return 101}
        if bridge.pump(3)!=4{return 102}
        return -17
    }
    let result=bridge.run(&mut f,body)
    if !result.0||result.1!= -17||bridge.roots()!=0{return 103}
    if f(4)!=5{return 104}
    return 0
}
fn owning(mode:i32)->model.Fault!i32{
    var f=model.handler(model.prepare(65)?)
    let result=bridge.run(&mut f,model.body(model.prepare(66)?,0,i32(-2147483648)))
    if bridge.roots()!=0{return 105}
    if mode==2{
        if result.0||result.1!=0||f(4)!=5{return 106}
    }else if mode==3{
        if result.0||result.1!=0||f(4)!=6{return 107}
    }else{
        if !result.0||result.1!=i32(-2147483648)||f(4)!=8{return 108}
    }
    return 0
}
fn pair()->model.Fault!i32{
    var first=model.handler(model.prepare(65)?)
    var second=model.handler(model.prepare(67)?)
    let a=bridge.run(&mut first,model.body(model.prepare(66)?,0,17))
    if !a.0||a.1!=17||bridge.roots()!=0{return 109}
    let b=bridge.run(&mut second,model.body(model.prepare(68)?,0,19))
    if !b.0||b.1!=19||bridge.roots()!=0{return 110}
    if first(4)!=8||second(4)!=8{return 111}
    return 0
}
fn nested()->model.Fault!i32{
    var outer=model.handler(model.prepare(65)?)
    let reply=bridge.run(&mut outer,model.nested_body(model.prepare(66)?))
    if !reply.0||bridge.roots()!=0{return 112}
    if reply.1<0{return 1}
    if reply.1!=73||outer(5)!=9{return 113}
    return 0
}
fn boundaries()->i32{
    var f=(value:i32)->i32=>{bridge.hit(70);return value}
    let body=()->i32=>{
        bridge.body()
        if bridge.pump(i32(-2147483648))!=i32(-2147483648){return 114}
        if bridge.pump(-1)!= -1{return 115}
        if bridge.pump(0)!=0{return 116}
        if bridge.pump(2147483647)!=2147483647{return 117}
        return 2147483647
    }
    let reply=bridge.run(&mut f,body)
    if !reply.0||reply.1!=2147483647||bridge.roots()!=0{return 118}
    return 0
}
fn repeat()->model.Fault!i32{
    var f=model.handler(model.prepare(65)?)
    for i in 0..2{
        let reply=bridge.run(&mut f,model.body(model.prepare(66+i)?,i*3,17+i))
        if !reply.0||reply.1!=17+i||bridge.roots()!=0{return 119}
    }
    if f(4)!=11{return 120}
    return 0
}
fn callback_trap()->i32{
    let mark=model.Mark{code:84}
    var f=move(value:i32)->i32=>{bridge.hit(mark.code);return 1/value}
    let body=()->i32=>{bridge.body();return 0}
    let reply=bridge.run(&mut f,body)
    return reply.1
}
fn body_trap()->model.Fault!i32{
    var f=model.handler(model.prepare(65)?)
    let body=model.trapping_body(model.prepare(66)?)
    let reply=bridge.run(&mut f,body)
    return reply.1
}
fn main()->model.Fault!i32{
    let mode=bridge.read()-48
    bridge.configure(mode)
    if mode==0{return pure()}
    if mode==1||mode==2||mode==3{return owning(mode)}
    if mode==4{return pair()}
    if mode==5{return nested()}
    if mode==6{return boundaries()}
    if mode==7{return repeat()}
    if mode==8{return callback_trap()}
    if mode==9{return body_trap()}
    return 121
}
