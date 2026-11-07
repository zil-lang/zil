#for (n in [1, 2]) {
    type Payload = [i32; n]
    struct Frame { data: Payload, tag: i32 = n }
    fn score(value: Frame) -> i32 => value.data[0] + value.tag
    #if (n == 1) {
        pub fn first() -> i32 => score(Frame { data: [10] })
    } else {
        pub fn second() -> i32 => score(Frame { data: [20, 30] })
    }
}
fn main() -> i32 => first() + second() - 33
