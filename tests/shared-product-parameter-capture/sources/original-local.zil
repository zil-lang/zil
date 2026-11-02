resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn render(panel:&Panel)->i32{let body=()->i32=>panel.count;return body()}
fn main()->i32{let mark=Mark{n:3};let panel=Panel{mark:give mark,count:0};return render(&panel)}
