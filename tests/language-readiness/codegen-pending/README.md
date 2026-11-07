# 大数组与按值 ABI 输入

`large-array-a18.zil` 保留 262144 字节嵌套数组原字节，预期退出 0。历史 O0 崩溃及 O2 观察见 [冻结复现](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/recovery-large-array.json)。后续修复在 Clang/LLVM API × O0/O2 的限定机器检查通过，保留输入、优化等级和无回退条件；原失败不因修复而重标。

`large-array-a18-by-value.zil` 及返回/函数指针控制覆盖有限大型 Array/Tuple/Record 的内部调用和返回。完整 signature/fnptr 载体用 `.lc1` 版本隔离；原版本传播阻断及小栈失败仍需独立解释。一般大型 Enum/ErrorUnion 按值、大型闭包接口/环境和特殊 helper 不由这些样本证明。精确候选、机器结果及 N/N−1/fresh 条件见 [冻结独审](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review333-by332.json)；旧施工文字可从 `28b3e37bb4d8a1fe8b146c72cf891fc99de32440:changes/324-large-aggregate-lowering.md` 与同提交 `changes/333-large-value-abi.md` 读取。
