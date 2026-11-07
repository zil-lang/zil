fn worker(value:i32)->i32{return value-3} fn main()->i32{let(c,w,b)=thread_scope(worker,-19,()->i32=>-27);if !c{return 2}if w!=-21||b!=-27{return 3}return 0}
