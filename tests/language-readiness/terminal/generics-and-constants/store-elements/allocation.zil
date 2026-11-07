fn main()->i32{
    let pairs:Store<(i32,bool)>=store_new()
    let copies:Store<[i32;2]>=store_new()
    if !store_reserve(pairs,1){return 10}
    if !store_reserve(copies,1){return 20}
    match store_push(pairs,(7,true)){.err(_)=>{return 30},.ok(_)=>{}}
    match store_push(copies,[8,9]){.err(_)=>{return 40},.ok(_)=>{}}
    return (*store_at(pairs,0)).0+(*store_at(copies,0))[0]-15
}
