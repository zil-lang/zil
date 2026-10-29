resource R{n:i32}pub package P{manages r:R}impl P{fn hidden(self:Self)->i32=>.r.n}pub fn make()->P=>P{r:R{n:42}}
