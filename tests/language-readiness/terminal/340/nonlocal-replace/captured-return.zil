resource R{code:i32}
package P{manages owner:R;n:i32}
fn make()->fn()->i32{
var packet=P{owner:R{code:65},n:0};
let target=&mut packet;
return ()->i32=>{*target=P{owner:R{code:66},n:42};return target.n}
}
fn main()->i32{let update=make();return update()}
