trait Read<R> { fn read(self:&Self)->R; }
struct Meter { value:i32 }
impl Read<i32> for Meter { fn read()->i32 { return .value } }
fn read_value<T:Read<i32>>(value:&T)->i32 { return value.read() }
fn source<life a>(value:&'a i32)->&'a i32 { return value }
fn worker(value:i32)->i32 { return value+1 }
fn main()->i32 {
    let meter=Meter{value:41};
    let value=read_value(&meter);
    let borrowed=source(&value);
    let body=()->i32=>*borrowed;
    let result=thread_scope(worker,value,body);
    if !result.0 { return 1 }
    if result.1!=42 { return 2 }
    if result.2!=41 { return 3 }
    return 0
}
