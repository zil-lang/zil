# 冻结语言能力探针

本目录保留 25 份原始 ZIL Source，用于比较普通借用、trait、life、线程和作用域边界。`controls` 是到 Verify 的正控；`rejects` 是相交窗口、局部逃逸、缺 nominal impl、伪造 Transfer 和捕获闭包转 C 入口的拒绝控制；`pending` 是当时尚未实现的合法目标，不是永久拒绝条件；`probes` 显示旧实现对未声明 life 的擦除。普通名字、解析成功或公共数字不产生线程、锁或挂起能力。

固定基线提交 `7f039d16` 的逐源观察在 [冻结收据](../../../validation/inputs/language-readiness/recovery-capabilities.json)。该收据记录 6 个 controls 成功、5 个 rejects 为 SafeReject、11 个 pending 中 9 个 Unsupported/2 个 SafeReject、3 个 probes 到 Verify；其中 pending 的 SafeReject 和 probes 的成功不建立规范许可。另一次在更晚候选上的相同字节复跑记录 8 Success、7 Unsupported、10 SafeReject，见 [语言缺口基线](../../../validation/inputs/language-readiness/language-gaps-baseline.json)。两次观察绑定不同编译器，不能合并为当前主线结论。

使用当前编译器重新观察时，把输出写到独立临时文件，保留原收据：

```sh
python3 tests/language-readiness/recovery/check.py \
  --zil /absolute/path/to/zil \
  --output /tmp/zil-recovery-observation.json
```

`check.py` 调用真实 CLI，仅检查源码阶段和诊断；它不执行非法程序、原生程序或性能测量。报告绑定输入、二进制和规范摘要。`collection_status=Complete` 只表示取证完成。合法目标从 Unsupported 变为 Success 时须继续验证机器行为及近邻反例；scoped 正控的外部 register/close 也未在本收据执行。完整语言和产品门另行验收。
