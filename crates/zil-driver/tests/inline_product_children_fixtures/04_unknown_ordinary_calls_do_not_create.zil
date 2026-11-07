resource R{n:i32}
package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}
package Outer{manages tag:R;p:P}
fn relay(value:Outer)->Outer{return value}
fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let outer=Outer{tag:R{n:5},p:p};let first=relay(outer);let second=relay(first);let child=second.p;return give child.fresh}
fn main()->i32{let value=origin(R{n:17});return value.n-29}
