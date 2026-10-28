extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn lower(n:i64)->i64{emit(76);return n}
fn upper(n:i64)->i64{emit(72);return n}
fn visit<T>(value:T)->T{
    let index:i64=7;var high:i64=9;
    for index in lower(index)..upper(high){emit(i32(index)+48);high=0}
    emit(i32(index)+48);
    return value
}
fn main()->i32{return visit(0)}
