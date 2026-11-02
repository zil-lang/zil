import bridge
pub resource Mark{pub code:i32}
impl Drop for Mark{fn drop(self:Self){bridge.dropped(.code)}}
pub enum Fault{NoMemory}
pub package Event{manages mark:Mark;manages bytes:ByteBuffer}
pub fn prepare(label:i32)->Fault!Event{
    let mark=Mark{code:label}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return Fault.NoMemory}
    bytes_push(bytes,86u8)
    return Event{mark:give mark,bytes:give bytes}
}
pub fn handler(event:Event)->impl FnMut(i32)->i32{
    bridge.factory(72)
    var count=0
    return move(value:i32)->i32=>{
        count+=1
        bridge.hit(event.mark.code)
        if bytes_len(event.bytes)!=1{return 901}
        if value==2{if bridge.reenter()!=1{return 902}}
        return value+count
    }
}
pub fn body(event:Event,base:i32,result:i32)->impl FnOnce()->i32{
    bridge.factory(75)
    return move()->i32=>{
        let current=event
        let started=bridge.body()
        if started<1{return 903}
        if bytes_len(current.bytes)!=1||current.mark.code<65{return 904}
        if bridge.pump(2)!=4+base{return 905}
        if bridge.pump(3)!=6+base{return 906}
        return result
    }
}
pub fn nested_body(event:Event)->impl Fn()->i32{
    bridge.factory(75)
    return move()->i32=>{
        bridge.body()
        if bytes_len(event.bytes)!=1||event.mark.code!=66{return 907}
        let inner_mark=Mark{code:67}
        let inner_bytes=bytes_new()
        if !bytes_reserve(inner_bytes,1){return -31}
        bytes_push(inner_bytes,86u8)
        var inner_count=0
        var inner=move(value:i32)->i32=>{
            inner_count+=1
            bridge.hit(inner_mark.code)
            if bytes_len(inner_bytes)!=1{return 917}
            if value==2{if bridge.reenter()!=1{return 918}}
            return value+inner_count
        }
        let body_mark=Mark{code:68}
        let body_bytes=bytes_new()
        if !bytes_reserve(body_bytes,1){return -32}
        bytes_push(body_bytes,86u8)
        let inner_body=move()->i32=>{
            bridge.body()
            if bytes_len(body_bytes)!=1||body_mark.code!=68{return 908}
            if bridge.roots()!=2{return 909}
            if bridge.pump(2)!=4{return 910}
            if bridge.outer(9)!=11{return 911}
            if bridge.pump(3)!=6{return 912}
            return 73
        }
        let reply=bridge.run(&mut inner,inner_body)
        if !reply.0||reply.1!=73{return 913}
        if bridge.roots()!=1{return 914}
        if inner(7)!=11{return 915}
        if bridge.pump(4)!=7{return 916}
        return 73
    }
}

pub fn trapping_body(event:Event)->impl FnOnce()->i32{
    return move()->i32=>{
        let current=event
        bridge.body()
        let value=current.mark.code+i32(bytes_len(current.bytes))
        return value/bridge.zero()
    }
}
