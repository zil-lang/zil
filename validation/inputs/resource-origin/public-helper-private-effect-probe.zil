// Static review probe for fcb8d81d7e62134c61d25c7032f767c87f80fc99.
// UNEXECUTED. The ordinary &mut P helper companion may reject in Types;
// an early qualification refusal is safe and does not demonstrate the concern.
// If it reaches replacement checking, old.code must not survive run's reset.
resource R { code:i32 }
package P { manages owner:R; n:i32 }
fn reset(target:&mut P) { *target=P{owner:R{code:66},n:42} }
fn run<F:FnMut()->void>(f:&mut F,target:&mut P) { reset(target); f() }
fn main()->i32 {
    var packet=P{owner:R{code:65},n:0};
    let old=packet.owner;
    var callback=()->void=>{};
    run(&mut callback,&mut packet);
    return old.code
}
