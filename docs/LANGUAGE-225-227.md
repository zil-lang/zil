# 施工8续段：225–227

本轮旧基线为 `42ce8081a2aa5722a14fea53f23f819f46685a60`，公共合同/派工提交为 `5be89dfb36b1f927524008526ae647f3c4a03e59`。主树工单仍是实际状态权威；本报告不把未合入分支当作main能力。

## 旧分支清理

222、223、224、integration/222-224及stdin-repair五个精确分支头均经Git祖先证明已进入旧main，随后带expected SHA lease原子删除。sync/construction8-history只有已核验的历史运输差异，先固定归档标签archive/construction8-transport-20261031再删除。原提交未改写；详见 evidence/planning/225-227/BRANCH-AUDIT.json。

编译性能分支及cp21/22/23/26/27存在未合入提交，保留。另一个施工线的228–229不属于本报告，不清理、不接管、不重复计算其成果。

## 实际交付

| 切片 | 交付与实现 | 结果与边界 |
| --- | --- | --- |
| 225 | PR10；交付8ea37c2de56cf65f1448e4fa647671f4459ad148；源代码5f47ffa58f5e8cb7bb03ac94675e9bc6bed3b9a0 | 完整有限Product Store拥有捕获；独立Types/Analyze/Verify资格、递归清理、Fn/FnMut/FnOnce及外层State/Store组合。仍未合入main。 |
| 226 | PR12；修后交付c8dd71a4b75ed7731fca8372a3a055f017991eb3；测试修复b4eef0b533d088829abfc7785bc716c48c89d889；生产代码35b97d2bece94a884bfc6b6edd474081f6105e3a | 仅实际普通局部Product根的显式共享地址捕获；Analyze/Verify各自在来源固定点完成后核验，动态Store/参数/外层capture来源仍关闭。不是完整默认Product捕获合同，未合main。 |
| 227 | changes/227-defer-lowering-plan.md及evidence/planning/227/BLOCKERS.json | 冻结前的清理动作候选及两个真实Types拒绝。普通defer仍Unsupported，不计实现。 |

225三层资格实现彼此独立，排除隐藏回调、借入及未开放嵌套Store；复用原来的内联布局和资源清理。226沿已核验来源借用现有指针表示，原loan、初始化、责任与NLL检查保留。没有增加隐式Box、RC、锁、操作码、ABI或放宽产品门。

## 验证，不能混加

225已提交源身份的限定测试107/0/0，限定Clippy通过；6构建34执行的分配故障矩阵中22次实际注入失败，所有路径最终live0。另有两次真实LLVM API O0/O2的外层泛型State/Store组合，输出ZAB、退出0，见 evidence/impl/225/OUTER-CLI.json。两轮不能叠加成独立Rust测试覆盖。

225完整GitHub CI41154544248实际checkout为c0a72749624aa4e3596f53cce2b70e0cc820afb6：默认workspace2873/0/0、reference2837/0/0；既有原生消费者16构建128执行，其中88次带分配观察、56次真实失败注入，最终live全0；格式、all-target Clippy、text-reference36及迁移runner9均通过。原始产物下载摘要与复算见 evidence/integration/225-226/CI225.json。它不是新main合入后的CI。

226原交付的64项限定测试通过，但完整CI41154731186为Fail：默认2871/1/0、reference2836/1/0。唯一旧175捕获反例继续被正确拒绝，只是原断言要求Types、实际拒绝已改为Analyze真实来源检查。保留原ZIL字节、Unsupported/E-UNSUPPORTED及所有其他反例，只精确迁移该phase/message。修后b4eef0提交身份API84/0/0、reference28/0/0及限定Clippy通过；这84包含之前64，不能相加。原Fail永久保留；c8dd触发的完整重跑结果需另查，不能拿限定通过替代。详见PR分支 evidence/impl/226/CI-DIAGNOSTIC-REPAIR.json。

旧平坦捕获预算在原42主线及两个候选均为Types835、Analyze7129、Verify12161。新225为3889/36339/56097，新226为1787/15791/30761；逐层N成功、N-1仅ResourceLimit、fresh N成功。这里是逻辑计费边界，不是编译时间或Rust性能比较。

## 并行与审查身份

本环境无直接子代理调度工具。实际并行包括GitHub工具链恢复作业、两项PR全量CI和已接单的GitHub Codex审查；不声称有未调用的代理。Codex对225最终8ea37及226首交付1748完成代码审查、无主要发现。226诊断修正已请求最终复核。机器人没有提交独立执行命令或测试回执，不能将其摘要、作者测试或CI称作独立可执行验收；主树review暂不伪造Pass。

## Rust四目标

| 目标 | 本轮实证 | 尚不能宣称 |
| --- | --- | --- |
| 内存安全 | 来源分离、raw/Wire伪造拒绝、借用冲突及真实故障清理 | 全语言无UB、所有生命周期/并发路径完整安全 |
| 运行性能 | 无新增隐式分配/RC/锁，沿原布局；原生O0/O2语义通过 | 与最佳Rust等布局等任务的性能达标 |
| 语法与维护 | 两个原先Unsupported组合可表达，且有原字节晋升 | 全部默认捕获、任意拥有容器组合或完整1.4 |
| 编译成本 | 原无新功能路径逻辑预算保留；新路径可复现边界 | 冷编译3–5秒、比Rust更快或整体净突破 |

B0/181、完整1.4、完整ZUI和Rust产品门不因本轮新增测试数量晋升；原生Windows仍未测。本轮没有使用编译性能研究支线的数字。

## 接续与日志

先读取真实main和主树225/226工单，再核PR10/12最新头、全部CI步骤和审核证据；不要用已经发生的代码审查推导不存在的独立执行。225–226仍待验。227的候选必须先与权威设计的责任/作用域规则核验、冻结公共Core合同，再派正式实现；不能直接把AST复制到函数尾。

原zil-log页面本轮无可用Pages写入口。施工心跳仅写 docs/log/zil施工8.md，标明北京时间及真实工作，不冒充原Page已更新，不声称响应结束后仍有本会话后台施工。原生阶段产物、迭代失败及精确增量bundle另有可下载恢复备份；GitHub分支才是在线交付，不要求用户回灌已推送内容。
