struct Pair{x:i32;y:i32}fn main()->i32{var pair=Pair{x:20,y:22};let edit=(p)=>{p.x+=1};edit(&mut pair);return pair.x-21}
