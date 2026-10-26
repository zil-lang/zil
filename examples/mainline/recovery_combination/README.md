# 三项语言能力的组合消费者

此例把120独立ByteBuffer字段、121跨模块泛型名义callable、122纯CT与运行时Drop组合在同一个三模块程序。`main.zil`的`value`是入口。`p.b`的视图活跃时，程序取走并补回实际独立拥有的`p.c`；名义记录/枚举经公开Fn helper调用；CT构造记录，退出时Token的Drop真实输出T。

```sh
cargo build --offline --locked -p zil-cli
mkdir -p out
./target/debug/zil build examples/mainline/recovery_combination/main.zil --entry value --opt O0 -o out/recovery-combination
./out/recovery-combination
```

预期标准输出为`T42`加换行、进程退出0。把`let old=give p.c;`改成取`p.b`，应在Verify以E-BORROW-CONFLICT拒绝。O0/O2组合验证和原始反例见evidence/recovery-20261026/integration.json及其integration-raw附件。

本例显式在bytes_push前调用bytes_reserve；不要把初始空缓冲区当作已有容量。它证明限定能力能组合执行，不代表任意字段、外部借入闭包或完整语言已支持。
