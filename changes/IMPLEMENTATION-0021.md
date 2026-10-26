# 021 独立标量 native 性质接入质量队列

依据主树 `.task/impl/021.md`，源基线 `437348d6cf4dd6838a3327d5c248b1f092d9924a`。本包不改变 ZIL 语义、公共编译 IR 或正式验收门。

保留 `roam/1`、`roam-worker/1`、session 默认 profile 和既有 claim/replay/verify/closed 契约。增加 `scalar-native` profile，采用已有 `zil-scalar-native/1` typed AST 输入；Rust 有界 i128 模型独立计算 i32/i64 预期，真实源码通过生产 compiler/link/native 路径观察全宽返回值、trap 或精确 CT 拒绝。模型不调用编译器求值，生成器不生成权限或安全证明。

新 profile 会启动 clang/链接器/程序，故监督器只为自己创建的 worker 建立独立 Linux 进程组，并在超时或协议失败时终止该组、回收直接子进程。其他平台的 native profile 返回 Unsupported。阶段运行、输出与输入有界，零执行和预算耗尽不算 Pass；保持先持久化 before 再 ACK、真实动作完成后 after 的计数。

原始缺陷仍只保存固定原件文件。case 固定 typed AST 与实际 clang 身份；result 内有界证据保存源码、独立预期、真实观察、诊断、LLVM/产物摘要及 argv。记录工具路径/摘要/版本/平台，历史重放拒绝已记录 clang 身份漂移；不宣称冻结 OS、CPU、动态库或任意外部程序。成功尝试清理临时二进制。

`scalar-sub-to-add` 是明确的受控 emitter 变异，oracle 保持不变；验收不将它描述为真实生产缺陷。先贯通生成/发现/入队/同性质重放/候选回归/关闭，再增加 `issue reduce`：须持有原 claim，步骤删除及同类型 AST 缩减必须保持同一 failure property；追加结果、不改原件、不自动关闭。缩减次数与总预算有界，infra/invalid/incomplete/不同性质不接受。

作者仅运行本包限定模型、native、协议/队列和真实进程回收检查；精确生产提交与小收据分别冻结，非作者审核后由 root 合入并执行统一门。

冻结前按同名协议对齐原 TS 的 256 AST 节点上限和字符串 `-0` 接受域（求值与源码发射归一为 0，原 case 不改）。clang 通过同一冻结 roam 的内部严格参数代理执行：调用前读取本动作绑定的绝对路径及摘要，拒绝路径/字节漂移，输出先写 owned 临时文件并实时检查限额，再有界读取；运行时同样处理，不先无界收集后拒绝。独立 `--version` 子进程组无论成功或失败都回收留存后代。
