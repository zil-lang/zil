struct Cell {value:i64}
package Map {prefix:i64;manages slots:Store<Cell>;suffix:i64}
fn conflict(value:&mut Map)->i64 {
    let held=store_at(value.slots,0)
    let edit=store_at_mut(value.slots,1)
    edit.value+=1
    return held.value+edit.value
}
fn main()->i32 {var value=Map{prefix:1,slots:store_new(),suffix:2};store_reserve(value.slots,2);store_push(value.slots,Cell{value:1});store_push(value.slots,Cell{value:2});return i32(conflict(&mut value))}
