struct ReadHold<life a> { at:&'a i32 }
fn hold<life a>(at:&'a i32)->ReadHold<'a> { return ReadHold::<'a>{at:at} }
package Reply { manages data:ByteBuffer; tag:i64 }
fn main()->i32 {
    let number=7
    let held=hold(&number)
    let data=bytes_new()
    if !bytes_reserve(data,1) {return 1}
    if !bytes_push(data,66u8) {return 2}
    var reply=Reply{data:give data,tag:20}
    let view=bytes_view(reply.data)
    let observed=view[0]
    bytes_reserve(reply.data,2)
    return i32(observed)-66+*held.at-7
}
