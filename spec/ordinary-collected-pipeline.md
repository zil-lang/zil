# 借入迭代输入与拥有结果收集

本合同覆盖一个保留完整源码的跨模块组合：`LendingIterator + Tag` 的泛型 helper 返回 `Option<&Item>`，借入的 `Item` 通过普通 trait 方法及 `FnMut(&Item)->Error!Reply` 回调产生独立拥有结果，再由 `StoreElement` helper 收集到可增长的 `Store<Reply>`。它组合既有[借入输入与拥有回调结果](borrowed-input-owned-callbacks.md)、[普通 Store 责任](ordinary-store-ownership.md)及共享字节视图规则，不授予额外类型、捕获或借入返回资格。

## 固定输入与责任路径

项目由以下四个模块组成，共7547字节；模块顺序为 `main, protocol, library, model`。完整项目指纹为 `4011cf8d1255cbf085e02cce4cd9d9fc54c28797bebf9e61cd06bbfbc855dd7f`，逐文件身份及正式门见[验收记录](../validation/ordinary-collected-pipeline.json)。

| 原始输入 | 字节数 | 作用 |
|---|---:|---|
| [main.zil](../tests/language-readiness/owning-trait-hosts/collected_pipeline/main.zil) | 51 | 调用完整消费者 |
| [protocol.zil](../tests/language-readiness/owning-trait-hosts/collected_pipeline/protocol.zil) | 308 | lending、Tag、拥有复制方法合同 |
| [library.zil](../tests/language-readiness/owning-trait-hosts/collected_pipeline/library.zil) | 586 | 泛型借入迭代与拥有结果收集 |
| [model.zil](../tests/language-readiness/owning-trait-hosts/collected_pipeline/model.zil) | 6602 | 两个输入元素、回调、错误分支及完整清理 |

`Cursor` 拥有 `Store<Item>`，每个 `Item` 管理自己的 `ByteBuffer`。取得的共享元素地址只借入输入；`duplicate`、`try_duplicate` 及捕获计数器的回调分别创建新责任，不能转移或释放原输入。返回的 `Reply` 管理自己的缓冲区，局部字段的 `bytes_view` 保留实际来源和父 loan。成功收集后，消费者继续读取原 `Item`，最后逐项消费所有 `Reply`，正常路径的 tag 总和为45。

`collect<T:StoreElement>(items:Store<T>,item:T)->T!void` 先检查长度和实际扩容，再执行入库。扩容或入库失败必须返还本次传入的同一个拥有值，保留已有元素；第一、第二次收集的失败分支分别核对返还值的字节和 tag，再清理已有结果。`Requested` 错误、缓冲区分配失败及每个退出只清理实际已经取得的责任，不把尚未成功转移的输入记为已消费。

## Loan 与独立核验

产生新拥有结果不会解除输入迭代器的借用。原最近反例在 `original` 仍需后用时再次执行 `next_tagged(&mut cursor,9)`；它必须在 Analyze 以 `E-ANALYZE-IR` 拒绝，具体原因是实际 stored target／parent loan 上存在同时活跃的重叠访问，不能用任意 Unsupported 或资源限额代替此负控。来源、字段代际、初始化、父 loan、责任转移及错误清理由实际 CFG 独立重建；拥有结果同型、成功收集或类型标签均不能提供额外权限。

正式门保留原四模块和[三个消费者测试](../tests/language-readiness/owning-trait-hosts/collected_pipeline/consumers.rs)，覆盖默认 Source 独立验证、上述 loan 反例，以及 Linux 下 Clang／LLVM API各 O0／O2 的真实机器执行。另一个[完整 raw／Wire 控制](../tests/language-readiness/owning-trait-hosts/owned_results/stored_composition.rs)以 `collected_pipeline_raw_and_wire_reconstruct_the_complete_project` 重建全部四模块，分别执行 raw Verify 和 Wire 解码后的独立 Verify。启用 LLVM API 后，每配置运行正常路径和第1至13次分配尝试失败，共56次；每次均核实际退出码、stdout及空 stderr，具体断言保持原测试表。

分配观察值是 `attempts:new_allocations:releases`。正常路径为 `13:12:12`、退出0：第二次收集使用 `realloc` 扩容已有 Store 分配，13次尝试只建立12个分配身份，最终全部释放。失败门既核清理计数，也核返还 owner 的内容和已有集合；不能把尝试次数直接当作应释放的分配数。

## 有界算法与计费不变量

Verify 普通来源检查与 loan 检查的两套 liveness 使用逻辑宽度仍为 `SSA + locals` 的私有 word 行。SSA／local坐标、完整 CFG fixed point、result kill → local kill/gen → operands → retained 的逆向顺序及每条指令的完整 after 均保持；即时调用参数、保留 owner、当前 result ignore 和 replacement pins 不受未来活跃位筛除。矩阵、word 初始化／复制／OR／比较、快照占位记录及外层容器都先付实际费用，零宽表也不免费；after 的实际 operand 遍历另行预付，升序置位提取先付费再执行。四态生命周期表、仿射 ownership 的 bool 表及通用预算预付规则不变。

Head-access 仅将 `locals/initial/outputs` 改为保逻辑宽度的 canonical `LocalRows`：local id 严格升序且唯一，只有完整空 `Rows` 才可缺席，非空路径即使 constraints 为空仍须保留。SSA rows、路径及约束发现顺序、前驱合流、每条 transfer、两层求解顺序和最终 ReadOnly 拒绝均保持；每次 body 重建局部表，原 Contract 求解及其更新不变，不引入跨请求缓存。实际查找、移位、容量增长、旧行销毁、完整 payload 复制及深比较逐项预付。独立冻结的 dense oracle 对比每轮所有 block outputs、progressed、完整 Contract及错误；新旧准确预算边界另用 N／N−1／fresh 控制，不通过提高默认工作额度或栈使程序通过。

## 有限范围

这是已有有限语言路径的组合预览，不构成完整语言或通用集合产品门验收。该消费者使用可增长的顺序 `Store<Reply>`；本合同未验收完整 Map、HashMap 或 Set 组合，也不把它们的历史测试计入本项目。其他集合、泛型、借入、并发及 ABI 范围仍受各自独立合同和真实门约束；完整结果以本合同对应验收记录的实际源码、阶段和执行范围为准。
