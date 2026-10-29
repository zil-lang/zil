
fn leaf<F:Fn()->i32>(f:&F,flag:bool)->i32 {
    match flag {true=>{return f()},false=>{return 0}}
}
fn middle<F:Fn()->i32>(f:&F,flag:bool)->i32{let n=leaf(f,flag);return f()+n}
fn outer<F:Fn()->i32>(f:&F,flag:bool)->i32=>middle(f,flag)
fn main()->i32{let f=move()=>4;return outer(&f,true)-8}
