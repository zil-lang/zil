# Checked-add Cache 交叉审查

结论：**无已知阻塞**，限以下精确冻结源码和审查范围；不是测试通过或性能采纳结论。

- Commit：`7b08cc846edbcd73b5e7f2ddb08dd3594c764fd9`
- Tree：`da85c53e7f11e98e8be5beb71127002b357f3c5b`，已从 commit 对象核实。
- 方法：只读该 commit 的 Git blobs；未采用活动工作树替代冻结内容。未运行编译、测试、fmt 或性能窗口，未修改仓库源码。
- Performance：**pending / not claimed**；没有本候选的新测量结论。

以下路径均相对于该 commit 的仓库根。

| 核查项 | 源码依据与判断 |
| --- | --- |
| 当前 original preflight | `crates/zil-backend-llvm/src/machine_checked_add_chain.rs:683–736`：可保留路径在查旧函数、全值 Eq、规划或替换之前，对完整当前原始 module 执行 `validate_module`。无关非法 declaration/global 不能被函数 hit 隐藏。缺 intrinsic/trap 直接不折叠；超保留界限回原 fold，原 fold 的非空计划仍先校验原 module。 |
| 完整输入资格 | 同文件 `29–48, 709–736`：context 比较实际 triple 与完整 declaration Function 向量，含签名、属性、linkage 和 trap 声明；按名字找到行后仍要求完整当前 Function Eq，含局部 trap block、CFG、指令、参数和 header。名字和 module fingerprint 不授予复用资格。全局项不是 planner 的跨函数读取输入，但仍接受完整当前 preflight。 |
| 预算与有界保留 | 同文件 `8–73, 88–126, 692–702, 742–769`：每个 Cache 只有一个 epoch，不链接前驱；保留准入上限为 16,384 items 及 1,000,000 参数/块/指令计数，**不是字节/RSS 上限**。超界只关闭保留，原 planner 执行一次；每函数 use scan 仍为原 128 深度、1,000,000 values，不截断后宣称已完成。 |
| 失败与事务 | 同文件 `704–707, 757–769`：preflight 失败不改当前 model、不替换旧成功 epoch；全部替换先暂存后应用。`crates/zil-driver/src/session.rs:1419–1444, 1468–1485` 在克隆 backend 上施工，只有 native 成功、取消检查和 publication commit 后才安装。超界/缺声明导致的临时 Cache 清退也不会从失败请求泄漏到原 session。 |
| 两个 fold 入口 | `crates/zil-backend-llvm/src/build.rs:453–545, 684–733`：正常 opt-in O2 路径和 stable-class import fallback 均调用 cached fold，并累计本次实际 checked/reused 工作；fallback 从 `original_machine` 重建，复用同请求 Cache。两个 TransformProduct 构造均保存 `checked_add_cache`；whole-transform hit 恢复对应缓存，未冒认本次 planner 工作。普通 O2 的启用条件不变。 |
| TF 四槽与后继产物 | `crates/zil-backend-llvm/src/incremental.rs:273–338`、`crates/zil-backend-llvm/src/emit/incremental/whole.rs:120–192, 221–266, 302–329`：Cache 跟随真实 TransformProduct，后者随既有最多四个 MachineProduct 保留，无独立无界 history。lower 命中同步恢复 LowerEpoch 和当前 identity；附着要求 lower 与 transform identity 一致。`build.rs:461–478` 同时检查完整 TransformPolicy 和真实 identity；`599–637` 仍核 PartitionInput，并恢复对应 partition/object-key 产品和当前 diagnostic fingerprint；`1273, 1680` 在两条成功 build 返回路径附着完成链。 |
| 原 emitter 账本 | `crates/zil-backend-llvm/src/emit.rs:214–247` 先执行当前 metadata reservation 和 `bind_lir`，再查询 whole-machine history；`emit/incremental/whole.rs:223–247` 要求完整 limits、原 before 及输入相等，并从同一 before 扣除真实 spent。Cache 没有更新默认限额、伪造 Verify 或重新发放 emitter 预算。 |

已只读检查同 SHA 的最近反例：`machine_checked_add_chain_tests.rs:569–811` 覆盖 cold/warm/单函数修改、空计划、trap body、声明/triple/header 变化、无关非法当前项后恢复、重排/删除、checkpoint 隔离及保留超界回原 fold。`whole_machine_tests.rs:666–976` 保留四槽、open/closed 的真实 native 历史恢复、策略切换和取消隔离断言。这里只记录断言存在与源码对应关系，**未执行这些测试**；统一 batch（含 IR lib 门）仍由 root 验收。
