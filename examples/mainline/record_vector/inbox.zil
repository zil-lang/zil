// Single-thread bounded byte-event inbox with FIFO, close, drain and release.
// Not an async scheduler and not a queue of resource-owning callbacks.
import records
package Inbox {
    manages data:ByteBuffer
    head:i64
    count:i64
    closed:bool
    ready:bool
}
fn read(data:ByteBuffer,index:i64)->u8 {return records.at(data,1,index)[0]}
// Commands: 0..255 post; -1 receive; -2 close; -3 pending count; -4 retained capacity.
// Results: post => count, receive => byte; -1 closed, -2 full,
// -3 initialization failed, -4 empty, -5 invalid command.
pub fn make(capacity:i64)->impl FnMut(i32)->i64 {
    let data=bytes_new()
    let ready=capacity>0 && bytes_reserve(data,capacity)
    if ready {
        var i:i64=0
        while i<capacity {bytes_push(data,0u8);i+=1}
    }
    var state=Inbox{data:give data,head:0,count:0,closed:false,ready:ready}
    return move(command:i32)->i64=>{
        if !state.ready {return -3}
        if command == -3 {return state.count}
        if command == -4 {return bytes_capacity(state.data)}
        if command == -2 {
            state.closed=true
            if state.count==0 {state.data=bytes_new()}
            return state.count
        }
        if command == -1 {
            if state.count==0 {return -4}
            let value=read(state.data,state.head)
            state.head=(state.head+1)%capacity
            state.count-=1
            if state.closed && state.count==0 {state.data=bytes_new()}
            return i64(value)
        }
        if command<0 || command>255 {return -5}
        if state.closed {return -1}
        if state.count==capacity {return -2}
        let remaining=capacity-state.head
        var index=state.count-remaining
        if state.count<remaining {index=state.head+state.count}
        let item=[u8(command)]
        if !records.set(state.data,1,index,&item[..]) {return -5}
        state.count+=1
        return state.count
    }
}
