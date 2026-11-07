fn next<life s>(slice:&'s [i64])->Option<&'s i64>{return Option::<&'s i64>.Some(&slice[0]);}
fn result()->i64{let values:[i64;1]=[42];let item=next(&values[..]);match item{Option.Some(value)=>{return *value},Option.None()=>{return 0}}}
