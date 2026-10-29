resource R{n:i32}
fn only_second<F:Fn()->i32,G:Fn()->i32>(first:&F,second:&G)->i32{return second()}
fn make()->impl FnMut()->i32{let owner=R{n:2};return move()=>{let edit=owner;let first=move()=>1;let second=()=>{edit.n+=1;return edit.n};return only_second(&first,&second)}}
fn main()->i32{var f=make();return f()-3}
