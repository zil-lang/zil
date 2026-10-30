# ac5d7ca runtime 非作者复核

审核者 `/root/benchmark_audit`，仅精确 Git 对象的工具控制，不运行性能采样、不改作者源码。18 项原控制重放通过；8 项独立负例均在重新计算 raw-log / receipt 摘要后拒绝，另核旧 O0/1 unit/1 job 默认。

将 `ac5d7ca1d8cd3b1ef8949f18c354f9651ec81d86` 的 `tools/research/compile-performance`、`tools/research/212`、`tools/research/210`、`tools/product/109` 用 git archive 解到一个新的临时目录，再把本目录 `independent_controls.py` 复制到临时目录根。执行：

```sh
python3 -m unittest discover -s /tmp/REVIEW/tools/research/compile-performance -p test_runtime_bench.py -v
python3 /tmp/REVIEW/independent_controls.py
```

REVIEW.json 为范围内 Pass：实际 argv 的 profile/units/jobs/额外覆盖参数与冻结配置一致，requested closed 模式必须明确返回布尔 true；忽略、缺字段、字符串或整数不可接受。此判定不代表 CP13 编译器候选已通过或有运行质量收益。
