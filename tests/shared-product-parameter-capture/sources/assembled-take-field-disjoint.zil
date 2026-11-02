resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
package DB{manages left:Store<Panel>;manages right:Store<Panel>}
fn render(panel:&Panel,give items:Store<Panel>)->i32{let body=()->i32=>panel.count;return body()}
fn main()->i32{let left:Store<Panel>=store_new();store_reserve(left,1);store_push(left,Panel{mark:Mark{n:3},count:7});let right:Store<Panel>=store_new();let third:Store<Panel>=store_new();store_reserve(third,1);store_push(third,Panel{mark:Mark{n:3},count:7});let p=store_at(third,0);let db=DB{left:give left,right:give right};return render(p,give db.left)-7}
