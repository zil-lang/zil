resource R{n:i32}
package Leaf{manages a:R;manages b:R}
package Root{child:Leaf}

fn unused(value:Store<Root>){}fn main(){}
