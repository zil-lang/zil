fn put(p:&mut[i32]){p[0]=42}fn read<T>(p:&mut[i32])->i32{put(p);return p[0]}fn main()->i32{var a=[0];return read::<i32>(&mut a[..])-42}
