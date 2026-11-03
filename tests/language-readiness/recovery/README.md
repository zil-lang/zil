# 恢复后的真实语言探针

以下表格和`check.py`的硬编码基线均属于316在`7f039d16`的历史观察，不是当前
main结论。334保持25份源原字节复跑，现为8 Success、7 Unsupported、10
SafeReject；life参数返回及具体泛型trait已成功，三份未声明life源均明确拒绝。
当前复跑使用`../current334/check.py`，精简结果见
`docs/validation/language-gaps-334.json`；不覆盖本目录的历史收据。

本目录由316在现存主线重新建立；不继承无法恢复的301–312源码、公共表示或成绩。合同及后续阶段见[316说明](../../../changes/316-concurrency-lifetime-prerequisites.md)，逐源观察见[结果](../../../docs/validation/recovery-capabilities.json)。

```sh
source /workspace/.zil-env/env.sh
python3 tests/language-readiness/recovery/check.py \
  --zil /workspace/.zil-env/target-recovery-root/debug/zil \
  --output docs/validation/recovery-capabilities.json
```

`check.py`只调用真实生产CLI，不注册Cargo测试target，不执行非法程序、不做原生或性能测量。输出绑定源、二进制和规范SHA；`collection_status=Complete`只说明取证完成。新版编译器可用同一原源重跑，pending变化为Success应继续验证和晋升，不能以旧失败为永久测试预期。

| 来源组 | 本轮观察 | 含义 |
| --- | --- | --- |
| `controls` 6源 | Success，各1次Verify | 普通借用返回、两输入来源、静态trait、Copy函数指针及同线程scope保持。`ordinary-mutex-poll-names`只检查普通record与函数，不是Mutex/SABI。 |
| `rejects` 5源 | SafeReject | 局部逃逸、相交窗口、缺nominal impl、伪造Transfer、捕获closure转C入口仍拒绝。两个借用反例实际到独立Verify拒绝。 |
| `pending` 11源 | 9 Unsupported、2 SafeReject | life binder、间接借用签名、HRTB、四种trait组合、Suspend/spawn合同及Transfer/Share约束尚未贯通。后两项当前按未声明nominal trait误归SafeReject，不能据此宣称规范禁止。 |
| `probes` 3源 | Success，各1次Verify | 非泛型未声明life、错life拼写返回及Self接收者的life均被擦除；这是语义未完成观察，不是有效life正控或safe UB证明。 |

其中scoped正控只检查源到Verify，外部register/close承诺没有执行。runtime TypedIR/Wire并无旧121/122/123，普通名字、公共数字与源码解析都不产生线程、锁或挂起能力。当前结果不宣称完整1.4、ZUI、B0或Rust四目标产品验收。
