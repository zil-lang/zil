struct Cell {pub key:i64;pub value:i64}
package Map {prefix:i64;manages slots:Store<Cell>;suffix:i64}
fn bump(value:&mut Map,index:i64)->i64 {
    let item=store_at_mut(value.slots,index)
    item.value+=1
    return item.value
}
fn main()->i32 {
    var value=Map{prefix:7,slots:store_new(),suffix:5}
    if !store_reserve(value.slots,2) {return 1}
    store_push(value.slots,Cell{key:1,value:40})
    store_push(value.slots,Cell{key:2,value:80})
    if bump(&mut value,0)!=41 || bump(&mut value,1)!=81 {return 2}
    if value.prefix!=7 || value.suffix!=5 {return 3}
    if store_at(value.slots,0).value!=41 || store_at(value.slots,1).value!=81 {return 4}
    return 0
}
