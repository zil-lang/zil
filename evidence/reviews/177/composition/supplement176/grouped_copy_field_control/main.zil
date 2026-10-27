resource R{n:i32}package P{manages r:R;n:i32}fn make()->P=>P{r:R{n:1},n:41};fn read(p:P)->i32{let n=(p).n;return n+p.n}fn main()->i32=>read(make())-82;
