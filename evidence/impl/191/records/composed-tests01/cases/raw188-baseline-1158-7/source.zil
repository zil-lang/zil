
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn middle<F:Fn()->i32>(f:&F)->i32=>leaf(f)
fn outer<F:Fn()->i32>(f:&F)->i32{let n=middle(f);return leaf(f)+n}
fn main()->i32{let f=move()=>4;return outer(&f)-8}
