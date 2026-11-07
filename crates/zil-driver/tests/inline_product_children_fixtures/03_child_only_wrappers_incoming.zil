resource R{n:i32}
package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}
package Only{p:P}
package Shell{only:Only}
fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let outer=Shell{only:Only{p:p}};let middle=outer.only;let child=middle.p;return give child.old}
fn main()->i32{let value=origin(R{n:17});return value.n-29}
