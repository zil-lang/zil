
resource R { n:i32 }
fn leaf<F:FnOnce()->i32>(f:F)->i32=>0
fn outer<F:FnOnce()->i32>(f:F)->i32{let n=leaf(f);return n+1}
fn main()->i32{let r=R{n:4};let f=move()=>r.n;return outer(f)-1}
