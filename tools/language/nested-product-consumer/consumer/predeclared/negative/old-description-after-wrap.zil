resource R{n:i32}
package Leaf{manages r:R}
package Middle{manages guard:R;leaf:Leaf}
package Root{child:Middle}
fn make()->Root{return Root{child:Middle{guard:R{n:9},leaf:Leaf{r:R{n:1}}}}}
fn main()->i32{let leaf=Leaf{r:R{n:1}};let old=leaf.r;let middle=Middle{guard:R{n:9},leaf:leaf};let root=Root{child:middle};return old.n}
