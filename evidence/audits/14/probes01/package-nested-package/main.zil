resource R{n:i32}package P{manages r:R}package Outer{p:P}fn main(){let o=Outer{p:P{r:R{n:1}}}}
