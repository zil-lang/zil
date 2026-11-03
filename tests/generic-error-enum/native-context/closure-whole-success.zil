enum Fault { Bad }
fn main()->Fault!i32 {
    let value:Fault!i32=7;
    let f=(arg:Fault!i32)->Fault!i32=>arg;
    return f(value)?-7;
}
