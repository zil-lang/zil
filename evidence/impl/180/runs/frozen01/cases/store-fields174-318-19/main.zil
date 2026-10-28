
            resource R{code:i32}
            package Item{manages item:R;manages data:ByteBuffer;n:i32}
            package State{manages items:Store<Item>}
            fn sink(value:State!i32)->i32=>0
            fn main()->i32=>0
        