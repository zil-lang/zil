enum Fault{NoMemory}
fn fail()->Fault!i32{return Fault.NoMemory}
fn run()->Fault!i32{
    let f=()->i32=>1
    if f()!=0{return Fault.NoMemory}
    return 7
}
fn main()->i32{run();return 0}
