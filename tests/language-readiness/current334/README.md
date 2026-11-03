# 334 当前源码缺口复核

以main `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`为基线，复用316的25份
源及已有语言/线程/状态消费者，只新增四个最小源：Copy tuple通配部分解构正控、
尚未支持的普通T/life组合和usize/u64链。不以目录名决定预期，不把合法目标的
Unsupported/SafeReject登记为永久禁止。

```sh
source /workspace/.zil-env/env.sh
python3 tests/language-readiness/current334/check.py \
  --zil /workspace/.zil-env/target-mainline-release/release/zil \
  --output /tmp/language-gaps-334-observations.json
```

collector固定原release CLI的完整hash，并核其观察源码至基线的生产输入变化。
换编译器须先重新核来源，不能把旧CLI当新实现。当前52份check与9源×O0/O2的
18次LLVM API构建/执行单独记录；原生显式main harness预期打印`0\n`且exit0。
机器产物只放临时目录并清除，不归档raw evidence、不创建Cargo目标或target目录。

`docs/validation/language-gaps-334.json`是压缩后的观察与十行裁决；collector输出
可用于重新比较，不能自动升级人工矩阵或其它工单。329–333的新源不进入此冻结
基线。一般大值ABI由333独立实测，Windows、Rust成本、弱内存和正式全量本片未跑。
