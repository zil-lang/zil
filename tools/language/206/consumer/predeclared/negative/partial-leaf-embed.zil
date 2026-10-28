resource R{n:i32}
package Leaf{manages r:R}
package Middle{manages guard:R;leaf:Leaf}
package Root{child:Middle}
fn make()->Root{return Root{child:Middle{guard:R{n:9},leaf:Leaf{r:R{n:1}}}}}
fn main(){var leaf=Leaf{r:R{n:1}};let detached=give leaf.r;let child=Middle{guard:R{n:9},leaf:leaf};let root=Root{child:child}}
