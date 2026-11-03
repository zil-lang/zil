resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn bump(panel:&mut Panel){panel.count+=1}
fn main()->i32{var panel=Panel{mark:Mark{n:3},count:0};bump(&mut panel);return panel.count-1}
