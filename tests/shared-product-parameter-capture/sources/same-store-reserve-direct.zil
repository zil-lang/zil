resource Mark{n:i32}
package Panel{manages mark:Mark;count:i32}
fn render(panel:&Panel,items:Store<Panel>)->i32{store_reserve(items,2);return panel.count}
fn main()->i32{let items:Store<Panel>=store_new();store_reserve(items,1);store_push(items,Panel{mark:Mark{n:3},count:7});return render(store_at(items,0),items)}
