resource R{n:i32}package P{manages r:R}fn main(){let f=(p:P)=>p;let result=f(P{r:R{n:7}})}
