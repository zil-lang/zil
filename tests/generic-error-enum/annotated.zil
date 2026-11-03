enum Fault{NoMemory}
fn fail()->Fault!i32{return Fault.NoMemory}
fn run<F:Fn()->i32>(f:F)->Fault!i32{
    if f()!=0{let error:Fault=Fault.NoMemory;return error}
    return 7
}
fn main()->i32{let f=()->i32=>1;run(f);return 0}
