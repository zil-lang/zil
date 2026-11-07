resource R{n:i32}package P{manages r:R}package Outer{manages tag:R;p:P}fn main(){let o=Outer{tag:R{n:0},p:P{r:R{n:1}}}}
