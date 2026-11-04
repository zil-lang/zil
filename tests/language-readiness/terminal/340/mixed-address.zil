extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
enum E{Data((P,i32),i32),Other((P,i32),i32),None}
fn touch(value:&mut E){}
fn inspect(value:&mut E)->i32{
  touch(value);
  match *value{
    E.Data((ref p,ref mut n),ref mut m)=>{*n+=p.n;*m+=*n;return *m},
    E.Other(_,_)=>{return 0},
    E.None()=>{return 0}
  }
}
fn peek(value:&E)->i32{
  match *value{
    E.Data((ref p,n),m)=>{return p.n+n+m},
    E.Other(_,_)=>{return 0},
    E.None()=>{return 0}
  }
}
fn result()->i32{
  var value=E.Data((P{r:R{code:65},n:20},20),2);
  if peek(&value)!=42{return 1}
  return inspect(&mut value)
}
