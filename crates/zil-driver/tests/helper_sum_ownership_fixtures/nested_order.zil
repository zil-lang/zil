extern "C" fn putchar(code:i32)->i32;
extern "C" fn getchar()->i32;
fn read()->i32{unsafe{return getchar()}}
fn emit(code:i32){unsafe{putchar(code)}}
resource R{code:i32}
impl Drop for R{fn drop(self:Self){emit(.code)}}
enum Inner{Nil,Pair(manages R,manages R)}
enum Outer{Empty,Pair(manages R,Inner),Copy(i32)}
fn make(input:i32)->Outer{
 if input==49{return Outer.Empty}
 if input==50{return Outer.Pair(R{code:65},Inner.Nil)}
 if input==51{return Outer.Copy(3)}
 return Outer.Pair(R{code:65},Inner.Pair(R{code:67},R{code:68}))
}
fn relay(value:Outer)->Outer{return value}
fn main()->i32{
 let early=R{code:69}
 var value=make(read())
 defer{emit(81)}
 let later=R{code:76}
 value=relay(value)
 match value{
  Outer.Pair(a,Inner.Pair(c,d))=>{}
  Outer.Pair(_,Inner.Nil())=>{}
  Outer.Empty()=>{}
  Outer.Copy(_)=>{}
 }
 return 0
}
