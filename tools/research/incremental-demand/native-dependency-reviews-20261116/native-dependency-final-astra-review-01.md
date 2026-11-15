# 最终限定静态审核与下一算法方向

## 审核范围与处置

**静态审核 SAFE；未发现以下三处的新阻断项。** 源码审核绑定候选 `f62965be7c568490251a2259f4bfd586f8db0a1f`。未运行构建、测试或修改仓库；backend/native验收以root收据为准。

root在本报告写入前补充：当前 `f19fc847c` 相对f629仅修改完整14项scalar-layout测试fixture及精确prune work counter `(1,1,1)`，backend 299已通过、driver尚在进行。这是root提供的状态，**本报告未独立复核f19差异或测试收据，不将f629静态审核扩大为f19完整验收。**

### 1. 重绑定增量：SAFE

`crates/zil-expand/src/incremental.rs`的placement与item_reuse两个OR门槛恢复完整headers不变、导入语义相同但位置移动的路径。Placement仍检查完整声明字节、顺序、parser context，并要求旧导入span重映射后精确等于当前导入；item Lower仍检查当前字节、位置、生成状态、输出owner和预算。没有仅凭函数输入身份直接接受旧输出。

主要消费者：`scalar_placement.rs`的between/imports/rebind，`item_lowering.rs:122–204`的replay_item。仅使旧计划可被考虑，不免除当前逐项绑定。

### 2. same_completed_output：SAFE，限定当前完整scalar范围

`crates/zil-backend-llvm/src/incremental.rs:351–357`的等长检查＋完整changed_indices＋source `.1` Arc比较补齐此前遗漏。source slots覆盖被折叠的Call、调用顺序及分区预算读取的块数和指令数；任务指令仍被准入规则排除。完整环境与最终机器输出比较继续保留。

此前遗漏必须保留：small_caller_components读取原始LIR调用槽位，不能仅凭相同context与相同MachineOutput证明分区输入相等。修复后的依据是完整环境、完整同arity source投影、实际完成MachineOutput三者共同相等。没有证明旧缺口已经产生误编译，也不能将当前受限结论用于未来扩大任务或非scalar准入。

### 3. 缓存生命周期脚本：SAFE，可作为独立诊断

审核文件：`/workspace/zil-recovery/cache_lifecycle_matrix-01.py`。

清理仅限新建且绑定inode的窗口，旧worker退出后执行；路径、符号链接及目录树检查和fd-safe删除均在。脚本须先安装到指定路径并冻结进tool_source，外部副本不能直接正式测量。原工具的pins、功能与运行质量门继续沿用；清理前后快照及启动时间另行记录。

**拒绝扩大上述结论：**该脚本测的是保留缓存与全清缓存，不是部分历史GC，也不能替代完整51-case窗口。清理和ZIL启动在编译计时外，Rust请求包含Cargo启动；Rust target清理包含可执行文件，ZIL保留program/IR。这些差异已披露，不能据此宣称对称清理或端到端总等待领先。磁盘统计是观测边界峰值，不是瞬时峰值。未增加磁盘Source→Machine授权。

## 下一步只建议一个方向

**沿现有函数产品建立真实消费依赖，把私改的失效传播截断到受影响class／导入闭包／CGU，在展开机器表示前决定会话内复用。**

实际Rust对应为固定提交 `58cf8f9c35b355199727627d07ae34a4e2846d67`：

- `compiler/rustc_middle/src/dep_graph/graph.rs:791–825`的alloc_and_color_node在重算结果指纹相等时停止传播；不是把“执行过”当作green。
- `compiler/rustc_codegen_ssa/src/base.rs:1238–1279`的determine_cgu_reuse要求旧work product存在且当前依赖green；`842–877`先分流复用，再在CguReuse::No路径构造LLVM。LTO路径区分PreLto/PostLto，不能只凭CGU本体不变复用跨unit优化后的最终对象。

优先依据是旧 `0276953/window05` 八次私改累计 **machine＋partition 4.777秒**，不是毫秒级noop；这不是新候选收益预测。先用新窗口确认剩余成本，再一次性完成上述链路。

当前独立Verify、原始调用槽位、全局代表变化及导入依赖必须完整绑定；无法证明局部性的部分保留全图计算，跨进程仍走现有完整生产。不引入通用source证明、不把Analyze绿色替代Verify，不放宽默认预算或完整O2。此方向借鉴Rust实际传播与CGU决策顺序，不照搬hash作为ZIL授权。

## 已审核文件绑定

以下SHA256取自f629审核时读取的内容；不表示另行审核了f19：

- crates/zil-expand/src/incremental.rs: `687d05f675575244d0cc2d0dcc4dc40f659d3d5926a1191dc8fd7846cb42c2d7`
- crates/zil-backend-llvm/src/incremental.rs: `417693ff64c18932435fed69306fc27aa70a259324789fd362dd9972bb320bb0`
- crates/zil-backend-llvm/src/emit/incremental.rs: `653ec1f68c48d3d7ad157a024a9ec002d894babed8fc2b9a828548579e3a8b99`
- /workspace/zil-recovery/cache_lifecycle_matrix-01.py: `e35c02e298ae94338e5206a805c40eddac9108105c6983fbb7d757bdddc2eccd`

READY。本次仅持久化这份外部报告；无build、Git或源码操作。写入后暂停。
