resource R{n:i32}
package Leaf{manages r:R}
package Middle{manages guard:R;leaf:Leaf}
package Root{child:Middle}
fn make()->Root{return Root{child:Middle{guard:R{n:9},leaf:Leaf{r:R{n:1}}}}}
package Pair{first:Middle;second:Middle}
fn main(){let root=make();let child=root.child;let duplicated=Pair{first:child,second:child}}
