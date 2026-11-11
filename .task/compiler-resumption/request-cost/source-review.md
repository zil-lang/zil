# Worker 拥有并释放机器单元：独立源码审阅

结论：**Source Accepted；Testing Pending；Performance Pending；Acceptance Pending。** 非作者只读审阅未发现阻断项。本结论只接受精确候选的源码行为，不代表测试、性能或合入验收已通过。

## 精确候选与范围

- 基线：`99035931d358add0eaed6d4535035c38413701dd`。
- 冻结候选：`9e15ab226b5c0e95f18b1e1a7d1d2517dc646850`。
- 候选 tree：`ba2a08e59c67339b1420e47480d2055ef81d1021`。
- 唯一生产修改：`crates/zil-backend-llvm/src/build.rs`，29 行增加、27 行删除。
- 已审文件 Git blob：`751ce5b65d0e0187b15e60a6bbaff2b51263d0f2`。
- 已审文件 SHA256：`528d9185dcede20406d47f01c05036c316170bdae6c3b0de550a0c9ba8c351ac`。
- 冻结后重新读取 commit 的 tree 与文件 blob，均与以上身份一致；工作树干净。

审阅者未修改生产源码、未添加测试、未启动构建或性能运行。本文件是独立源码审阅记录。

## 修改目的与实际行为

真实 shared4096 profile 中，warm 请求的对象全部命中，LLVM Builder、优化和对象生成时间均为零，但主线程仍在对象 workers 结束与 link 开始之间耗时约 69.976ms。原实现提取 `UnitInput.object` 时在主线程串行销毁各 `UnitInput.module`；此处定位来自 root 的原始 span 复核，不是本审阅重新测量所得。

新实现让 `compile_units` 消费 `Vec<UnitInput>`，保留原 cost 降序、id 升序调度，将排序后的拥有迭代器放入一个 Mutex。每个 worker 只在领取任务时持锁，随后独占编译该 unit，保留 object 路径和编译结果，并在本轮 worker 作用域内释放机器模块。全部 worker join 后仍按原 unit id 排序结果。

## 独立源码检查

### 队列所有权与锁生命周期

`build.rs:635–652` 先保存 jobs，再把每个原始 unit 与其原 id 绑定；排序标准与原实现一致，没有复制或重新编号机器模块。`Mutex<IntoIter<(id, UnitInput)>>` 对每个任务只有一个拥有者；`next()` 移出唯一任务，不会重复领取。

`build.rs:662` 采用独立的 `let job = ...next();` 语句，其临时 MutexGuard 在该语句结束释放。匹配 job、取消回调、Clang 子进程、LLVM API 编译、结果分配和模块销毁均在锁外。poison 恢复只恢复领取队列的访问，不会吞掉编译错误；临界区不执行用户回调或编译逻辑。

空输入时 jobs 为零、结果为空，与旧实现相同。正式入口仍先验证 jobs 与 units 为 1..=64。

### 模块释放与线程边界

`build.rs:700` 只移动 `unit.object` 到 worker 的结果向量；剩余 input 路径与 `module` 在该轮 worker 作用域结束时释放。返回值不携带机器 model 引用，object 路径在释放 model 后仍由结果拥有并可用于链接。

LLVM 原始 handle 仍在既有 `compile_api_unit` 调用线程内创建、验证和销毁；此次只移动普通 Rust 机器 model，不让 LLVM handle 跨线程，也不改变 LLVM 管线。

### 普通错误、取消、panic 与未领取任务

每个 unit 的普通 `Err` 仍先记入结果，worker 随后继续领取其他任务，没有新增 fail-fast。取消检查位置保留在每个任务开始和原有内部边界；只改变任务完成时间可能影响并发观察时刻，不改变取消合同。

`build.rs:708–719` 显式 join 全部 workers，任一 worker panic 仍统一返回 `E-CODEGEN-WORKER`，不在其他 worker 或其子进程尚未结束时提前返回并清理 staging。panic 时已领取的 unit 随 worker 展开释放；尚未领取的 unit 仍由队列迭代器拥有，队列销毁时释放。成功 worker 的结果在整体错误路径上丢弃。

`build.rs:721–725` 仍在所有 worker join 后按 unit id 排序，再 collect Result。多个普通失败时返回最小 id 的错误；命令、receipt 和 object 路径顺序保持 id 顺序，不改为完成顺序。

### 核验、缓存与发布

`compile_api_unit` 及其命中、miss、patch 路径未修改：当前完整机器预检、Verify、LLVM 优化前后核验均保持；预算、目标/ABI、对象缓存 key、布局缓存与函数地址身份策略均保持。

外层 `build.rs:544–556` 只有在 `compile_units` 整体成功后才收集 pending 对象、receipt 与 cache 计数。普通错误、取消或 panic 都不会将已成功 worker 的 pending 对象提前提交。后续 link、取消检查、产物发布和 `PendingObjectCache::commit` 规则未变。

### Clang 参考后端

各 unit 的 LLVM 文本仍在启动 workers 前全部生成并写入；Clang 编译命令、object 存在性检查、诊断码与最终链接对象顺序保持。所有权调整适用于参考后端，但不修改其输入字节或优化策略。

## 尚未判定的验证与性能

本记录没有运行测试。root 安排独立 release 构建、已有 `native_object_cache` 整个 target，以及真实产物字节等价、取消、链接失败检查；是否通过以该独立运行的精确日志和后续收据为准。本候选不再添加 fake Clang 单测，不启动完整性能矩阵。

源码检查不能保证原约 70ms 都转化为端到端收益。模块析构现在占用 worker，可能影响下一任务启动、冷构建关键路径和分配器争用；这些是性能限制，不是已观察到的退化。限定性能 screen 若没有净收益，则停止此候选，不扩大施工，不降低原运行质量、核验或预算门。

最终状态仍为：**Source Accepted；Testing Pending；Performance Pending；Acceptance Pending。**
