// Ordinary single-thread byte-event FIFO. No runtime handle/generation table.
// Commands: 0..255 post; -1 receive; -2 close; -3 count; -4 allocation capacity;
// -5 cancel/discard; -6 closed flag. Close drains, cancel releases immediately.
// Post failures: -1 closed, -2 maximum reached, -3 invalid/failed initialization,
// -5 invalid command, -6 growth allocation failed. Receive empty is -4.
package Queue {manages data:ByteBuffer; head:i64; count:i64; closed:bool; ready:bool}

fn reverse(data:&mut[u8],start:i64,end:i64) {
    var left=start
    var right=end
    while left<right {
        right-=1
        let byte=data[left]
        data[left]=data[right]
        data[right]=byte
        left+=1
    }
}
fn rotate(data:&mut[u8],head:i64) {
    if head==0 {return}
    let length=data.len()
    reverse(data,0,head)
    reverse(data,head,length)
    reverse(data,0,length)
}
fn set(data:ByteBuffer,index:i64,value:u8) {
    let view=bytes_view_mut(data)
    view[index]=value
}
// No content/length mutation until reserve has succeeded. Existing bytes are
// rotated in place; clipped final growth does not require a second allocation.
fn grow(data:ByteBuffer,head:i64,next:i64)->bool {
    let old=bytes_len(data)
    if head<0 || head>old || next<=old {return false}
    if !bytes_reserve(data,next) {return false}
    rotate(bytes_view_mut(data),head)
    var at=old
    while at<next {bytes_push(data,0u8);at+=1}
    return true
}
pub fn make(initial:i64,maximum:i64)->impl FnMut(i32)->i64 {
    let data=bytes_new()
    let valid=initial>=0 && maximum>0 && initial<=maximum
    let ready=valid && bytes_reserve(data,initial)
    if ready {
        var at:i64=0
        while at<initial {bytes_push(data,0u8);at+=1}
    }
    var state=Queue{data:give data,head:0,count:0,closed:false,ready:ready}
    return move(command:i32)->i64=>{
        if !state.ready {return -3}
        if command == -3 {return state.count}
        if command == -4 {return bytes_capacity(state.data)}
        if command == -6 {if state.closed {return 1};return 0}
        if command == -5 {
            let discarded=state.count
            state.closed=true
            state.data=bytes_new()
            state.count=0
            state.head=0
            return discarded
        }
        if command == -2 {
            state.closed=true
            if state.count==0 {state.data=bytes_new();state.head=0}
            return state.count
        }
        if command == -1 {
            if state.count==0 {return -4}
            let value=bytes_view(state.data)[state.head]
            let capacity=bytes_len(state.data)
            state.head=(state.head+1)%capacity
            state.count-=1
            if state.count==0 {
                state.head=0
                if state.closed {state.data=bytes_new()}
            }
            return i64(value)
        }
        if command<0 || command>255 {return -5}
        if state.closed {return -1}
        if state.count>=maximum {return -2}
        var capacity=bytes_len(state.data)
        if state.count==capacity {
            var next:i64=1
            if capacity>0 {
                if capacity<=maximum/2 {next=capacity*2}else{next=maximum}
            }
            if !grow(state.data,state.head,next) {return -6}
            state.head=0
            capacity=next
        }
        let tail=capacity-state.head
        var index=state.count-tail
        if state.count<tail {index=state.head+state.count}
        set(state.data,index,u8(command))
        state.count+=1
        return state.count
    }
}
