resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn render(panel:&Panel)->i32{let body=()->i32=>panel.count;return body()}
fn main()->i32{var panel=Panel{mark:Mark{n:3},count:7};let count=render(&panel);panel.count=9;return count+panel.count-16}
