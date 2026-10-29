enum Option<T>{None,Some(T)}
fn choose(flag:bool)->Option<i32>{if flag{return Option.Some(7)};return Option.None()}
fn main()->i32{match choose(true){Option.Some(n)=>{return n-7},Option.None()=>{return 1}}}
