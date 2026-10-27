// Four frames through an owned queue, imported methods and replaceable payload.
import state
import queue
fn render()->impl FnMut(i32)->i64 {
    var data=bytes_new()
    var cursor=state.make()
    return move(event:i32)->i64=>{
        if event == -1 {data=bytes_new();return bytes_capacity(data)}
        if event == -2 {
            let view=bytes_view(data)
            for i in 0..view.len() {if view[i]!=u8(i%256) {return -8}}
            return view.len()
        }
        if event == -3 {return cursor.position()}
        if event<0 || event>255 {return -7}
        if !bytes_reserve(data,8192) {return -6}
        if !bytes_push(data,u8(event)) {return -5}
        cursor.advance(1)
        return cursor.position()
    }
}
fn main()->i32 {
    var inbox=queue.make(3,8192)
    var draw=render()
    for frame in 0..4 {
        for i in 0..4096 {if inbox(i%256)!=i64(i)+1 {return 1}}
        for i in 0..4096 {
            let event=inbox(-1)
            if event!=i64(i%256) {return 2}
            if draw(i32(event))!=i64(frame)*4096+i64(i)+1 {return 3}
        }
        if draw(-2)!=4096 {return 4}
        if draw(-1)!=0 {return 5}
    }
    if draw(-3)!=16384 {return 6}
    if inbox(-2)!=0 || inbox(-4)!=0 || inbox(0)!= -1 {return 7}
    return 0
}
