resource R{n:i32}
package Leaf{pad:i32;manages a:R;manages b:R}
package Twins{left:Leaf;right:Leaf}
package Root{twins:Twins}
fn origin(give x:R,give y:R)->R contract{result(create)}{let left=Leaf{pad:7,a:give x,b:R{n:29}};let right=Leaf{pad:9,a:R{n:31},b:give y};let root=Root{twins:Twins{left:left,right:right}};let first=root;let second=first;let twins=second.twins;let leaf=twins.right;return give leaf.b}
fn main()->i32{let value=origin(R{n:17},R{n:19});return value.n-31}
