package Pair { manages left:Store<i64>; manages right:Store<i64> }
fn inspect(pair:&mut Pair)->i64 {
    let left=store_at(pair.left,0)
    let value=*left
    return value+store_len(pair.left)-1
}
fn main()->i32 {
    let left:Store<i64>=store_new()
    let right:Store<i64>=store_new()
    if !store_reserve(left,1) || !store_reserve(right,1) {return 1}
    store_push(left,11)
    store_push(right,12)
    var pair=Pair{left:give left,right:give right}
    let previous=store_at(pair.left,0)
    let next=store_at_mut(pair.right,0)
    *next=22
    let value=*previous
    if inspect(&mut pair)!=11 {return 2}
    return i32(value)-11
}
