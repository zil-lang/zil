fn a(x:&i32,y:&i32,n:i32)->&i32{if n==0{return x;}return b(y,x,n-1);}
fn b(x:&i32,y:&i32,n:i32)->&i32{return a(x,y,n);}
fn result()->i32{let x=3;let y=7;return *a(&x,&y,4);}
