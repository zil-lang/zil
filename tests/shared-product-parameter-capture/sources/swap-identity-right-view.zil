resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
package DB{manages left:Store<Panel>;manages right:Store<Panel>}
fn render(panel:&Panel,give items:Store<Panel>)->i32{let body=()->i32=>panel.count;return body()}
fn main()->i32{let left:Store<Panel>=store_new();store_reserve(left,1);store_push(left,Panel{mark:Mark{n:3},count:7});let right:Store<Panel>=store_new();store_reserve(right,1);store_push(right,Panel{mark:Mark{n:5},count:11});var db=DB{left:give left,right:give right};let moved:Store<Panel>=give db.left;db.left=give db.right;db.right=give moved;let p=store_at(db.right,0);return render(p,give db.left)-7}
