resource R{n:i32}package P{manages r:R;n:i32}fn make()->P=>P{r:R{n:1},n:41};fn relay(p:P)->P=>p;fn read()->i32{var p=make();p=relay(p);return p.n}fn main()->i32=>read()-41;
