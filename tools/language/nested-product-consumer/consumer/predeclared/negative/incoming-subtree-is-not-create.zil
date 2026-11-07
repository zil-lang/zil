resource R{n:i32}
package Leaf{manages r:R}
package Middle{manages guard:R;leaf:Leaf}
package Root{child:Middle}
fn make()->Root{return Root{child:Middle{guard:R{n:9},leaf:Leaf{r:R{n:1}}}}}
fn invalid(root:Root)->R contract{result(create)}{let middle=root.child;let leaf=middle.leaf;return give leaf.r}
fn main(){}
