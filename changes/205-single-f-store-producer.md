# 205：单 F 的真实 State 结果接入完整 T 值推导

五模块消费者 `784de34b` 在 clean `82361e8c`、冻结 CLI02（编译源 `eed44b70`）下实际止于 Expand：`StoreElement fields require finite Copy or explicit direct manages resources`。原 source01 保留五份源码、命令和 syntax；没有 Verify 或 native 成绩。producer01 只删主函数的后续操作，其余四模块逐字不变：`pack(...)` 对照成功，加一行 `identity(initial)` 就得到同一拒绝。

根因是 203 的完整值推导来源记录只接了两个声明 binder 的 producer。单 F 的完整 State 结果仍带有尚待 Types 绑定的符号类型，普通模板错误地进入 eager 特化。这里修复已有 bare 静态普通 `T:StoreElement` 从真实完整值推导的承诺，不增加 caller F、不制造 TypeApply，也不授予 Store 资格。

新单 F 记录只在已有预费 Engine 建表发现真实普通 StoreElement 模板的范围内启用。无该模板的旧 State 路径不建新事实、不增加收费。记录仍由实际 callable producer 的声明及生成 Package 完整实例决定；普通/phantom T、名义族和后续真实参数约束不变。混合 State+replacement 返回的 symbolic_result 早返也走对应记录。新树查询、名字复制和集合插入先计费；旧 Pair 入口保持原记录路径。Types 仍独立求全部实参的完整 T 和 callback 身份，实际 canonical/资源/借用由后续核检查。

producer02 实际 12/1：新增显式冲突反例确实被拒，但作者预期了错误的诊断字符串；已保留原输入和输出，只改为实际 `conflicting generic argument constraints`。producer03 的预执行 patch/三份源码 SHA 快照完整，实际 30/0（17 个既有 ordinary 模板测试、13 个 205 测试），包含 1/2 字段、pack→identity→replace→identity、main 无伪 binder、两个真正 bare Call、显式冲突和无约束 empty 近邻。

同轮实测 Expand 的旧 State alias N1938、普通 Store N2255 保持；205 header503、unused alias688 也保持。新的单 F 完整值组合 N3877，已实际测 N、N−1 ResourceLimit、fresh N。全部默认限额不变。该检查点尚不是五模块 Verify/native 或最终平台矩阵成绩，源原件等待完整集成编译器复验。
