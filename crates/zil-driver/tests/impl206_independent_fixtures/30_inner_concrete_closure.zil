resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}
package C<F>{manages r:R;callback:F}package Forbidden<F>{child:C<F>}
fn pack<F:Fn()->i32>(f:F)->Forbidden<F>{return Forbidden::<F>{child:C::<F>{r:R{n:1},callback:f}}}fn main(){let value=pack(move()=>7)}
