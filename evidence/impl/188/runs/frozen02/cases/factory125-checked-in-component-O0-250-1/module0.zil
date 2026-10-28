import factory

fn value() -> i64 {
    var first = factory.make(10);
    var second = factory.make(20);
    let first_event = first(2);
    let second_event = second(3);
    return first_event + second_event + first(1);
}
