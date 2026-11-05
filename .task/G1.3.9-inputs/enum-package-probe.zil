resource R{code:i32}
package P{manages owner:R;n:i32}
enum E{Left(P),Right(P)}
fn main()->i32{let value=E.Left(P{owner:R{code:65},n:42});match value{E.Left(p)=>{return p.n},E.Right(p)=>{return p.n}}}
