# 0015：C1合入后的失败分类夹具迁移

## 原因与真实失败

统一运行 `20261023T024221Z-240099b2` 在 `feeea6eed718e307562545a4b643a87f7b76b30e` 上必要门 27/29，Fail 原样保留。`PLAN-B0-002`、`PLAN-B0-004` 与 `PLAN-T04-004` 的 Unsupported 输入仍是无捕获闭包；014 已实现这项能力，所以它们真实到达 Success。该失败是夹具未迁移，不是应拒绝程序被编译器错误放行。

## 变更

`examples/bootstrap/unsupported.zil` 与 diagnostic-parity 的对应内联源码改为默认借入捕获一个外部标量。默认借入不属于 C1，仍应在 types 返回 Unsupported / E-UNSUPPORTED。编译器、runner和验收门不变；失败分类、精确阶段/代码、无可执行产物、Unsupported不能算正常Pass及CLI/IDE诊断一致性断言全部保留。无捕获闭包的正例由014原生/独立审核/源码oracle保留，未通过删除负例或降低数量取得通过。

## 验证与交接

作者限定运行 failure-chain.py、runner-faults.py、diagnostic-parity.py，记录命令、实际结果与精确源码/编译器身份。生产字节相对C1统一受测提交无变化；复用同一个已构建编译器并明确记录来源，不冒称重新构建。独立审核绑定候选并复跑限定脚本。合入后由集成者执行完整门，首轮Fail不覆盖。
