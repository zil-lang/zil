enum Left{V(i32)}
enum Right{V(i32)}
fn bad(t:Type,x:Left)->i32{match x{Right.V(v)=>{return v}}}
fn result()->i32{return #(bad(i32,Left.V(7)))}
