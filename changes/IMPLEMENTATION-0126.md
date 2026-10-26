# 126：有限事件批处理 helper

## 行为和实现

移除有限无环 Fn/FnMut helper 的两次调用实现上限，保留真实调用次数和 Return/Trap 出口。3、4、16 次源码回调、超过六条不同长度的退出轨迹可通过；FnOnce 仍至多一次，循环和转发仍按原边界拒绝。`examples/mainline/event_batch` 是四模块普通消费者，成功执行四次累加，第三次失败则不执行第四次，资源唯一清理。

私有轨迹以单一已核调用种类的精确次数表示，公共 IR 仍是完整 `CallableTrace.calls`。Types 逐路径检查；Analyze 独立来源重建和拓扑传播；Verify 从自己核过的图和实际指令重建。各阶段在承认次数表示前逐实际调用核对 kind；同次数的 Return 与 Trap 分开。Verify 不使用前两阶段的求解结果作为证明。

次数集合树访问按层数、传播按真实元素计费，公开向量按完整长度先收费再物化。Verify 原始合同在指纹之前受既有 max_nodes 与 work 限制；wire 还有自己的编码节点总量限制，两入口的精确低预算接受集合并非完全相同。默认额度、schema17、ABI、资源权限和产品门不变。Types 仍可枚举重复语法路径，工作额度限制其失败；不声称消除了所有路径爆炸。

## 实际验证

受测最终源码 `da4ad9d40b253e1d966cf4e34503f806d9c511a5`。126 个限定回归组在 `7ce2a69b` 全过：56 Driver、32 Verify helper、38 Analyze。最终源码相对该提交只有测试源码生成的固定 vec 改成数组，生产字节相同；新增 5 组在最终提交重新通过，fmt 和 clippy `-D warnings` 通过。

新增 10 次 O0/O2 原生执行：3、4、16 次 FnMut 累加共六次，四模块正常及错误清理共四次。成功输出 `EEEEST`、exit 0；第三次错误输出 `EEET`、exit 1。计数是最终轮次的实际执行，不把早期重复运行累加成独立覆盖。

Verify 手填原始候选与 wire：3/4/16/65 次真实完整合同通过；缩短末次、末种类伪造、出口伪造和重复轨迹精确拒绝。原有 FnOnce 重复消费、环境提前移动/销毁、资源漏清、循环、借入权限反例继续通过。不能把这些有限反例称作完整安全证明。

| 阶段 | 最小成功 work N | N−1 | 重试 N |
|---|---:|---|---|
| Types | 507 | ResourceLimit | Pass |
| Analyze | 2811 | ResourceLimit | Pass |
| Verify | 22542 | ResourceLimit | Pass |

预算绑定16次真实源码；不是所有输入的全局阈值。

## 保留的失败与边界

初次5组中4组通过，唯一失败是测试把循环诊断预期在Types；实际更早在Expand明确Unsupported，保留原输出并修正期待，没有放行循环。首次限定回归126组全过，但 clippy 发现新测试的固定九元素 vec 可直接用数组，按建议修复，未抑制 lint。全部原件和源快照见 `evidence/impl/126/acceptance.json` 与归档清单。

泛型 helper 内创建工厂返回环境后再直接调用该局部环境仍未开放，不把这项125独审发现的问题算入126。一般循环不变式、多个 callable 参数、递归/转发、一般借入逃逸仍有后续工作。当前没有新的运行/内存/Rust编译性能证据，也没有完整语言或产品验收结论。

本单由root施工，独立审核交给 `/root/zui124`，精确交付审核通过后才合入。

## 集成复核补充：资源额度分类

root发现同一合法长轨迹仅降低max_nodes便被标成SafeReject，独审确认这是分类遗漏。2ac5b65a将本函数三类可配置节点额度门改为ResourceLimit，保留实际合同造假为SafeReject。33个Verify helper组、5个driver126组（重放10次native）、fmt与clippy通过；降低/恢复额度和旧指纹超额metadata均直接验证。原126组及首次独审不重标、不覆盖；补充收据与原件单列quota-correction。最终独审须同时引用resource-limit-correction更正记录，不能只引用最初Pass。
