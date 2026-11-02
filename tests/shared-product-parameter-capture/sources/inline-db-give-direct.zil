resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
package DB{manages left:Store<Panel>;manages right:Store<Panel>}
fn render(panel:&Panel,db:DB)->i32{return panel.count}
fn main()->i32{let left:Store<Panel>=store_new();store_reserve(left,1);store_push(left,Panel{mark:Mark{n:3},count:7});let right:Store<Panel>=store_new();return render(store_at(left,0),DB{left:give left,right:give right})-7}
