resource R{n:i32}
package Leaf{manages r:R}
package Middle{manages guard:R;leaf:Leaf}
package Root{child:Middle}
fn make()->Root{return Root{child:Middle{guard:R{n:9},leaf:Leaf{r:R{n:1}}}}}
package OtherLeaf{manages r:R}
package OtherMiddle{manages guard:R;leaf:OtherLeaf}
fn main(){let other=OtherMiddle{guard:R{n:9},leaf:OtherLeaf{r:R{n:1}}};let root=Root{child:other}}
