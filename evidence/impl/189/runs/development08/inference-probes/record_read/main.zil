struct Pair{x:i32;y:i32}fn main()->i32{let pair=Pair{x:20,y:22};let read=(p)=>p.x+p.y;return read(&pair)-42}
