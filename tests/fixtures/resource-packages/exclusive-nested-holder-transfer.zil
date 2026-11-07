resource Mark { code: i32 }
package Holder<life a> { manages mark: Mark; pointer: &'a mut i32 }
package Outer<life a> { child: Holder<'a> }

fn wrap<life a>(pointer: &'a mut i32) -> Outer<'a> {
    var holder = Holder::<'a> { mark: Mark { code: 1 }, pointer: pointer }
    return Outer::<'a> { child: holder }
}

fn main() -> i32 {
    var number = 0
    var outer = wrap(&mut number)
    *outer.child.pointer = 3
    if number == 3 { return 0 }
    return 1
}
