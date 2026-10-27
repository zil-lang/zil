resource R{n:i32}package P{manages r:R;n:i32}fn make()->P=>P{r:R{n:1},n:41};fn first(p:[P;1])->P=>p[0];fn main()->i32{let p=first([make()]);return p.n-41}
