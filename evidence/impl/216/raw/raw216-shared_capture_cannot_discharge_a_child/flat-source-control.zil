resource R{n:i32}
package Leaf{manages a:R;n:i32}
fn local_capture(p:Leaf)->i32{let f=move()=>p.n;return f()}
fn main(){}
