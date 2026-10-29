struct V{n:i32}type Row=[V;2];fn copy<T>(a:Row)->i32{let b=a;return a[0].n+b[1].n}fn main()->i32=>copy::<i32>([V{n:20},V{n:22}])-42;
