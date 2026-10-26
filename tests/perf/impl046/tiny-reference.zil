fn id(a:&i32)->&i32{return a;}
fn result()->i32{let x=42;return *id(&x);}
