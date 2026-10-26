fn id(a:&i32)->&i32{return a;}
fn sum(a:&i32,b:&i32)->i32{let f=()=>*a+*b;return f();}
fn result()->i32{var n=40;var f=()=>{n+=1;return n;};f();f();let v=2;let p=id(&v);return sum(&n,p);}
