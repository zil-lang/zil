struct V{n:i32}fn id(p:&V)->&V=>p;fn read<T>(p:&V)->i32{return (*id(p)).n+(*id(p)).n}fn main()->i32{let v=V{n:21};return read::<i32>(&v)-42}
