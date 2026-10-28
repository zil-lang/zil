# 201 原始200 Store探针晋升

精确clean `b14b9d5f313e93d744b81e50f7bd5a64a6e1d895` 限定执行 `cargo test --offline --locked -j2 -p zil-driver --test impl200_state_source original_` 得到0通过/2失败/10 filtered。两失败都因为原02实际返回Success，而历史200断言要求Expand/SafeReject；不是201源程序编译失败。原完整命令和stdout/stderr位于外置201-root/legacy-boundary01。

迁移只将这两个历史测试中的原3份输入全部要求Types成功、Verify运行一次并O0/O2实际执行；不删除原输入或跳过安全核，不改生产。原02当前与受测200 b9c006b0逐字节一致，626字节，SHA256 `23faee6a4575e6c522fa8ffed71d929dfd8463241197c7760cbe670f11f65fa8`。原始200报告仍绑定原冻结提交，不能倒标成当时已通过Store。

新结果须由限定复跑、非作者复核和最终全仓分别记录。本迁移不开放未列profile，不变更默认预算或产品门。普通T:StoreElement helper对callback元素的后续类型绑定失败另行保留，不用本晋升掩盖。

## 175 旧拥有字段拒绝文字

独审只读发现旧`element_shape_mutable_owned_field_return_and_capture_guards_are_explicit`仍要求旧Copy-only投影文案。root在clean`f1fcd5fe59c0511746ef3e3a3c4d47b5abaf4dd0`限定执行确认0/1（19 filtered）：原`view.r.code`仍Types/Unsupported/E-UNSUPPORTED，Verify0，实际消息为`Store element windows only project finite Copy or concrete callback fields`。原命令、JSON诊断和失败输出保留于外置201-root/legacy-17501。

本次只替换该旧测试的一处消息子串为实际新文字，源程序、类别、阶段、诊断码及后续正反例保持。此项与前述原02的能力晋升不同，不把仍拒绝的Managed字段读取改成成功，也不动生产。独立只读审核和限定复跑另记。
