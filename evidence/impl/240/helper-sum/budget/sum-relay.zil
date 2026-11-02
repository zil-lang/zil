resource R{n:i32}
package P{manages r:R}
enum E{V(manages R)}
fn consume<F:Fn()->i32>(value:E,callback:F)->i32{
 match value{
  E.V(r)=>{let saved=give r;let p=P{r:give saved};return callback()+p.r.n}
 }
}
fn main()->i32{return consume(E.V(R{n:2}),()=>3)-5}
