import std.store
fn dispatch<F:FnMut(i64)->i32>(callback:&mut F,value:i64)->i32 {return callback(value)}
fn main()->i32 {
    let items:Store<i64>=store.new()
    if !store.reserve(items,1) {return 1}
    match store.push(items,10) {.err(_)=>{return 2},.ok(_)=>{}}
    var calls:i32=0
    var append=move(value:i64)->i32=>{
        calls+=1
        if calls!=1 || value!=20 {return 3}
        let length=store.len(items)
        let capacity=store.capacity(items)
        if length!=1 || capacity!=1 || *store_at(items,0)!=10 {return 4}
        match store.push_growing(items,value) {
            .err(returned)=>{
                if returned!=20 || store.len(items)!=length || store.capacity(items)!=capacity {return 5}
                if *store_at(items,0)!=10 {return 6}
                match store.push_growing(items,returned) {
                    .err(_)=>{return 7},
                    .ok(_)=>{}
                }
            },
            .ok(_)=>{}
        }
        if store.len(items)!=2 || store.capacity(items)!=2 {return 8}
        if *store_at(items,0)!=10 || *store_at(items,1)!=20 || calls!=1 {return 9}
        return 0
    }
    return dispatch(&mut append,20)
}
