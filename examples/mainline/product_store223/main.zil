import model
import storage
extern "C" fn getchar()->i32;

fn main()->void!i32 {
    var mode:i32=0
    unsafe { mode=getchar()-48 }
    if mode<0 || mode>3 { return 9 }
    let items=storage.empty()
    if !store_reserve(items,2) { return () }
    let first=model.make(65,2)?
    store_push(items,first)
    if mode==0 { return 0 }
    let second=model.make(67,4)?
    store_push(items,second)
    if mode==2 {
        let popped=storage.pop(items)?
        return popped.tail-69
    }
    if mode==3 {
        if !store_reserve(items,4) { return () }
    }
    return 0
}
