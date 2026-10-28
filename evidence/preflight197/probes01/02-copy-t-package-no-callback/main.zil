package Cell<T:Copy>{value:T}
fn make<T:Copy>(value:T)->Cell<T>{return Cell::<T>{value:value}}
fn read<T:Copy>(cell:Cell<T>)->T{return cell.value}
fn main()->i32{let a=read(make(41));let b=read(make(i64(42)));return a+i32(b)-83}
