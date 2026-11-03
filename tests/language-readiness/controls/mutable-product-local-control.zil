resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn main()->i32{var panel=Panel{mark:Mark{n:3},count:0};panel.count+=1;return panel.count-1}
