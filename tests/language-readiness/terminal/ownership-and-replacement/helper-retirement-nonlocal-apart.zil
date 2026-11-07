resource R{code:i32}
package P{manages owner:R;n:i32}
fn reset(p:&mut P){p.owner=R{code:66};}
fn relay(p:&mut P,old:R)->i32{reset(p);return old.code;}
fn main()->i32{
    var p=P{owner:R{code:65},n:0};
    var q=P{owner:R{code:67},n:1};
    let old=q.owner;
    return relay(&mut p,old);
}
