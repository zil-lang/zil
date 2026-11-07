# 334 当前源码缺口复核

本节原52/18属于40fb历史；共同B的52源新投影与component4/CT8机器复核另存
`https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/language-gaps-334-final.json`。原JSON/hash及本历史collector不改。

以main `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`为基线，复用316的25份
源及已有语言/线程/状态消费者，只新增四个最小源：Copy tuple通配部分解构正控、
尚未支持的普通T/life组合和usize/u64链。不以目录名决定预期，不把合法目标的
Unsupported/SafeReject登记为永久禁止。

```sh
python3 tests/language-readiness/language-gap-replay/replay-frozen-baseline.py \
  --zil /absolute/path/to/frozen-zil \
  --output /tmp/language-gaps-334-observations.json
```

collector固定原release CLI的完整hash，并核其观察源码至基线的生产输入变化。
换编译器须先重新核来源，不能把旧CLI当新实现。当前52份check与9源×O0/O2的
18次LLVM API构建/执行单独记录；原生显式main harness预期打印`0\n`且exit0。
机器产物只放临时目录并清除，不归档raw evidence、不创建Cargo目标或target目录。

`validation/inputs/language-readiness/language-gaps-baseline.json`是压缩后的观察与十行裁决；collector输出
可用于重新比较，不能自动升级人工矩阵或其它工单。329–333的新源不进入此冻结
基线。一般大值ABI由333独立实测，Windows、Rust成本、弱内存和正式全量本片未跑。

后续共同候选的当前52源投影另用`compare-candidate.py --zil <新CLI> --compiler-source
<完整冻结SHA> --output /tmp/current52.json`。该工具核历史收据与每份输入hash，拒绝
89891e旧CLI，只做check，不重复18次历史native，不改历史JSON；编译器来源仍须
由实际构建收据绑定，不能仅凭传入SHA当作已认证。新观察放独立final报告。
