struct Z{value:i32}
struct A{value:i32}
fn choose(t:Type,first:bool)->A|Z{if first{return Z{value:20}}return A{value:22}}
fn score(value:Z|A)->i32{match value{Z(v)=>{return v.value},A(v)=>{return 0}}}
fn result()->i32{let first=#(choose(i32,true));let second=#(choose(i32,false));return score(first)+score(second)+22}
