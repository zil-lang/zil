resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
package DB{manages left:Store<Panel>;manages right:Store<Panel>}
fn render(panel:&Panel,db:DB)->i32{let body=()->i32=>panel.count;return body()}
fn main()->i32{let left:Store<Panel>=store_new();store_reserve(left,1);store_push(left,Panel{mark:Mark{n:3},count:7});let right:Store<Panel>=store_new();let db=DB{left:give left,right:give right};let other_left:Store<Panel>=store_new();let other_right:Store<Panel>=store_new();let other=DB{left:give other_left,right:give other_right};return render(store_at(db.left,0),other)-7}