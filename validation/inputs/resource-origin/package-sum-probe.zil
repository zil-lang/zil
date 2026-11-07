resource R{code:i32}
package P{manages owner:R;n:i32}
enum E{Left(P),Right(P)}
package Wrapper{value:E;n:i32}
fn main()->i32{let value=Wrapper{value:E.Left(P{owner:R{code:65},n:42}),n:0};return value.n}
