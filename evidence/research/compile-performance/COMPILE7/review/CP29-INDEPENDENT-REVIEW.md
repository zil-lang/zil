# CP29 有限 Types memo 独立审核

结论：**Pass，限本提交的有限正确性边界；性能采用待独立完整成本结果**。作者 frontend_incremental，审核 cp25_evidence。精确提交 `a0f38d2bca8f01fc19ff1f2fa0a42c3983c223b8`；审核前后 worktree 干净。9 文件写集与派工一致，其中 driver/lib.rs 的单次调用构造器一行外延已由 root 明确授权。

## 实际复用单位

保留的是最近一次成功项目 epoch 中，**整个未改变模块的普通 scalar 函数 Builder 结果**。改动模块内的全部函数仍重查；这不是单函数依赖图，也不是跨进程或多版本增量缓存。A→B→A 只复用相邻 epoch 仍未改的模块，不把旧 raw query 的多版本命中算作本功能。

原完整 shape/name/declaration/signature/resource-effect/visibility/interface 工作继续执行。缓存项是私有 raw TypedFunction，无插入、反序列化或验证凭证 API。非原语签名、聚合、枚举、资源、闭包、泛型、地址及不在白名单内的正文旁路本优化；typed 输出再经过一次白名单。含 scalar foreign/unsafe 的模块始终调用原 Builder，不保留模块条目，但可与其它被复用的纯 scalar 模块共存。这不宣称完整 FFI、异步或生命周期增量化。

## 身份与配额

环境核对完整 Signature 表、有序模块/声明身份、imports/alias、root、整个 TargetProfile、全部 11 个 Limits、参数/结果传递模式以及完整 foreign ABI/symbol/noescape/static_callbacks 合同。driver 再带入现有 Types rule/build-input fingerprint。模块使用完整 CoreModule 逐字段相等，涵盖正文、NodeId、FileId、Span、schema；不能只靠 source_fingerprint。无效声明、可见性变化及负名称查找仍由实时检查处理。

在白名单内，不创建 closure/helper/resource 身份或改变相应共享表；普通调用绑定完整签名和固定 FunctionId，callee 正文另外实时检查并交给后续当前项目处理。剩余共享 Builder 成本记录六维收据：blocks/locals/values/instructions/work/nodes。命中前先检查全部余额，足够才一次回放；不足则不改任何余额，走原 Builder 复现首错及位置。每函数本地配额因同 Core 与同 Limits 仍与旧成功执行一致。

独立消费者为四个增长维度另建了真正的后模块失败：改动前模块单独通过，旧完整项目通过，新完整项目第一条 ResourceLimit 必须落在未改的 FileId(1)。冷/热错误 category/code/message/span/notes 全等，随后旧 epoch 仍能完整复用。原作者控制也强制了该后模块首错条件；不能用前模块先失败冒充回放不足覆盖。

## 生命周期与运行边界

driver 只在全部请求阶段和产物成功后替换 epoch；失败或合作式取消保留旧成功 epoch。Types-only 成功可以保留 raw Types，这不证明 Analyze/Verify。普通更改项目仍经过新 Analyze，所有请求到 Verify 的路径保留独立 Verify。自动 raw-query 容量回收只清原 query 表；显式 clear 清两个缓存。单次 compile_path/compile_project 禁用 memo，持久 CompilationSession 默认启用并提供显式禁用控制。

单 epoch 最多 16,384 模块位置、64 MiB 结构计量。计量包括 target 字符串、源码/类型数组、完整合同及分配余量，但**不是 RSS**。请求中旧 epoch、新候选、当前 IR/raw query 同时存在，只有未改模块的 Arc 存储共享；整个进程峰值内存和旧/新完整 wall/CPU 由根计量决定。没有跑容量极限压力或完整语言验收。

## 实际验证与证据

作者先前 12 项控制之后又补了两处小改动，因此审核没有把旧 rlib 当作精确提交证据。根正式计量窗口结束后，审核在冻结 checkout 真实重编 zil-types/zil-driver 并执行：

`CARGO_TARGET_DIR=/workspace/scratch/c3607737969d/target-cp29 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p zil-driver --test cp29_types_memo --offline`

环境来自 recovery/env.sh：Rust 1.99.1、CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2，原 LLVM23 动态库。**12 通过，0 失败/忽略/过滤，退出 0**。源码编辑/恢复、完整身份、11 limits、伪旧 hash 等长正文、foreign 四字段/unsafe 反例、保守旁路、fresh Analyze/Verify、取消/后期失败、clear/disable、raw 容量回收均实际执行。完整输出 `CP29-INDEPENDENT-AUTHOR-CONTROLS.log`。

随后审核用上述重编的依赖 rlib，独立 rustc --test 链接 `cp29_independent_consumer.rs`，真实解析/展开源码并执行 **3 通过，0 失败/忽略/过滤，退出 0**：12 个混合实时 FFI/纯 scalar 相邻 epoch 与冷 TypedProgram 全等；四维后模块配额首错；三种语义拒绝不能污染旧 epoch。完整命令、输入/rlib/二进制 SHA256 在 `CP29-INDEPENDENT-CONSUMER-EXECUTION.json`，输出在对应 TEST.log。

这是 12 项作者控制的非作者重编复跑，加 3 项独立消费者；不与作者旧跑次数相加。没有执行本功能原生程序行为窗口、正式全量或性能测量，不能用此 Pass 宣称全语言/产品门通过或已确立性能突破。历史稀疏恢复 handoff Fail 保持原结论。
