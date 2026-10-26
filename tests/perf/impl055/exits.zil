resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}
enum Result {Ok(i32), Err(i32)}
fn transaction(flag:bool)->Result {let t=Token{value:7};let f=()=>t.value;if flag {return Result.Err(f())}return Result.Ok(f())}
fn main()->i32 {return 0}
