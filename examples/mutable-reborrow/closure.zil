struct Stats { total: i32, count: i32 }
fn accumulate(stats: &mut Stats, amount: i32) {
    stats.total = stats.total + amount;
    stats.count = stats.count + 1;
}
fn answer() -> i32 {
    var stats = Stats { total: 0, count: 0 };
    var callback = (amount: i32) => { accumulate(&mut stats, amount); };
    callback(20);
    callback(22);
    return stats.total * 10 + stats.count;
}
