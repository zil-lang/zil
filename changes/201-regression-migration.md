# 201 原始200 Store探针晋升

精确clean `b14b9d5f313e93d744b81e50f7bd5a64a6e1d895` 限定执行 `cargo test --offline --locked -j2 -p zil-driver --test impl200_state_source original_` 得到0通过/2失败/10 filtered。两失败都因为原02实际返回Success，而历史200断言要求Expand/SafeReject；不是201源程序编译失败。原完整命令和stdout/stderr位于外置201-root/legacy-boundary01。

迁移只将这两个历史测试中的原3份输入全部要求Types成功、Verify运行一次并O0/O2实际执行；不删除原输入或跳过安全核，不改生产。原02当前与受测200 b9c006b0逐字节一致，626字节，SHA256 `23faee6a4575e6c522fa8ffed71d929dfd8463241197c7760cbe670f11f65fa8`。原始200报告仍绑定原冻结提交，不能倒标成当时已通过Store。

新结果须由限定复跑、非作者复核和最终全仓分别记录。本迁移不开放未列profile，不变更默认预算或产品门。普通T:StoreElement helper对callback元素的后续类型绑定失败另行保留，不用本晋升掩盖。
