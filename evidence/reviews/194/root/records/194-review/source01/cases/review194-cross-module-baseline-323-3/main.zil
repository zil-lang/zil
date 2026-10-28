import first
import second
import api
type Alias=first.Item;fn main()->i32{let f=move()=>1;let a=api.keep(&f,first.Item{n:3});let b=api.keep(&f,second.Item{n:4});let again:Alias=api.keep(&f,first.Item{n:5});return i32(a.n+b.n+again.n)-12}