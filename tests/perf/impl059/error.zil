resource Token { value:i32 }
enum Fault { Stop(i32) }
fn check(flag:bool)->Fault!i32 {
    if flag { return Fault.Stop(2) }
    return 1
}
fn transaction(flag:bool)->Fault!i32 {
    let a=Token{value:1}
    let b=Token{value:2}
    let reader=move ()=>a.value+b.value
    let n=check(flag)?
    return reader()+n
}
fn main()->i32 { let result=transaction(false); return 0 }
