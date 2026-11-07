resource R{n:i32}package Leaf{manages r:R}package Outer{child:Leaf}fn main(){let outer=Outer{child:Leaf{r:R{n:1}}}}
