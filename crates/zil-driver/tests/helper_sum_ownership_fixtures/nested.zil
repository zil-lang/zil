resource R{n:i32}
impl Drop for R{fn drop(self:Self){}}
enum Inner{Nil,Pair(manages R,manages R)}
enum Outer{Empty,Pair(manages R,Inner),Copy(i32)}
fn make()->Outer{return Outer.Pair(R{n:65},Inner.Pair(R{n:67},R{n:68}))}
fn relay(value:Outer)->Outer{return value}
fn main()->i32{
 var value=make();value=relay(value);
 match value{
  Outer.Pair(a,Inner.Pair(c,d))=>{let aa=give a;let cc=give c;let dd=give d;}
  Outer.Pair(_,Inner.Nil())=>{}
  Outer.Empty()=>{}
  Outer.Copy(n)=>{return n}
 }
 return 0
}
