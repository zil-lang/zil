resource R{n:i32}package P{manages r:R;n:i32}fn make()->P=>P{r:R{n:1},n:41};fn read<T>(p:[P;1])->i32{return p[0].n+p[0].n}fn main()->i32=>read::<i32>([make()])-82;
