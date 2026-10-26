struct Stats {
    total: i32,
    count: i32,
}
fn accumulate(stats: &mut Stats, amount: i32) {
    stats.total += amount;
    stats.count += 1;
}
fn answer() -> i32 {
    let mut stats = Stats { total: 0, count: 0 };
    let mut callback = |amount| accumulate(&mut stats, amount);
    callback(20);
    callback(22);
    stats.total * 10 + stats.count
}
fn main() {
    println!("{}", answer());
}
