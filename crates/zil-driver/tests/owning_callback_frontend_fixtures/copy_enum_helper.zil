enum E{V(i32),Nil}fn apply<T:Copy,F:Fn(E)->i32>(f:&F,value:T)->T{f(E.V(1));return value}fn main()->i32{let f=(event:E)->i32=>{match event{E.V(n)=>{return n}
E.Nil()=>{return 0}}};return apply(&f,2)-2}
