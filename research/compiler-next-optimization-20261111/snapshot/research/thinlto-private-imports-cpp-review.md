# 私有导入插件：冻结 C++ 与合法地址控制静审

## 结论及精确身份

对下列**实际磁盘字节重新完整读取、前后核 SHA256**后，未发现阻断当前固定 pure 两 case 研究的源码问题；可以继续 Root 统一安排的构建及两项微控制。此结论不代报构建、插件加载、LLVM verifier、native、质量门或编译收益通过，也不授权扩大输入范围。

| 材料 | 精确身份 |
| --- | --- |
| 正式源码 | `research/thinlto-private-imports/PrivateImports.cpp`，SHA256 `a22e5004b5f99150ae197ffa88f406eafb2136c796d5d3cc651a8fc97cc99623` |
| 发布提交 | `1cc866a08bac100012e41726067711c94caf1899`，tree `910d281b288b9266f67c92ae9891b3b9290d16d2`；Root 绑定作者提交 `01e4d97` 为同 tree，不声称同 commit |
| 独立控制源码 | `research/thinlto-private-imports-controls/PrivateImportsControls.cpp`，SHA256 `83e11d0106de5e5418f407358fb66e5f4c028028ce24f4a4e39844b64c689be0` |
| LLVM API 基准 | 官方 `llvm/llvm-project` commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`，LLVM 23.1.4 |

审者不是该 C++ 作者；本次只执行文本读取、hash 和报告写入，没有编译/加载/执行工具。先前将冻结前 `json::Value(Object)` 旧读取结果误套到 a22 的 API 阻断已撤回：当前冻结实际为 `json::Object Copy = Object; json::Value(std::move(Copy))`，不需要修正式字节；这不是一次真实构建首败。

## 接入及名单边界

`llvmGetPassPluginInfo` 对结构零初始化，只注册 `PipelineEarlySimplificationEPCallback`，精确筛 `ThinLTOPostLink` 并要求 O2，没有 PreCodeGen 替代或自定义优化管线。官方插入点位于实际导入之后、IPSCCP 和标准 inliner 之前；并非所有 postlink 转换的绝对第一步。准确官方链接见同目录上级的 `thinlto-private-imports-design-review.md`。

插件每次读取绑定的 manifest、真实 Rust preflight/原完整 planner proof、真实 prelink witness，并核各文件 SHA。8 unit 的 index、当前 preflight 非零、canonical owner 唯一、Allowed/Required 对应、原 cost 一致性、加法溢出及总 copy budget 均检查。改写许可是实际 AE 集合属于原 Allowed；Required 另要求完整存在，不能把 Required 的覆盖检查误称为自动允许新增导入。它不创建 body、不重新计算放宽预算。

模块通过实际 `source_filename` 与 manifest 的八个绝对路径唯一匹配；wrapper 从实际 prelink IR提取并拒重复，选中 unit 的输入 bitcode 另核 SHA。普通 LLVM 模块的 source filename 本无唯一保证，本轮只依赖已冻结 wrapper 的明确唯一约束，不能泛化为产品模块身份方案。C++ 不从 LLVM Module 重建全部原语言语义，原 Verify/机器 preflight、Rust proof、wrapper 对实际输入/导入的独立核验仍是必要前置；单独伪造一组 JSON 不构成本审授权的执行路径。

## 变更及身份

`inspect` 完成所有函数的 gather/check，之后才调用 `rewrite`。拒 global/alias/ifunc/module asm、未知 declaration、非本 owner 的普通 definition，以及 COMDAT/personality/prefix/prologue/GC；AE 还必须属于跨 owner Allowed、无 metadata use、地址空间为零且私有名未占用。目标名字由 unit 和原 canonical 名唯一确定；变更后再核 LLVM 没有自动加后缀。

最关键的 only-direct 检查确实逐 **Use**：User 为 CallInst、`isCallee(&U)`、直接 called Function 和 FunctionType 相同、无 operand bundle。它不会把同一个 CallInst 的参数 Use 当成 callee，遇到 bitcast/constant/地址/未知用途即拒。递归及互递归的直接边仍指向同一 Function 对象；其他 unit 的规范 owner 不变。当前所有本 unit uses 已改为私有实现，不需要创建零 use canonical declaration。

实际变更只有 `setName`、`setLinkage(Internal)`。源码核 attrs、calling convention、block/instruction/operand 指针序列未变；不克隆、不 RAUW、不加 alias/alwaysinline、不调阈值。只对 receipt 的 Change 列表排序，没有重排 Module function list。`Internal` setter 隐含的 default visibility/DLL storage 与 dso_local 变化均记录；Python whole-module 逆名字/header 归一化只允许 linkage/dso_local，非默认 visibility/DLL 若实际被改变会使全文核验拒绝。本次不是对特殊 visibility、section、CFI、PGO、地址身份输入的普遍合法性证明。

pass 前后各调用 `verifyModule`，失败 fatal；有变更返回 analyses none，无变更 all。每 unit 独立四文件、exclusive create，不能覆盖旧成功收据；失败留下 started/before 等不完整证据但不产生成功 final。没有跨 backend 共享 LLVM 句柄或可变全局 owner 游标。wrapper 必须继续要求 exact8/exact32文件及前后全文变化边界；单份 receipt 不足以证明完整构建执行了插件。

官方 LTO cache key 不绑定插件 SO/manifest 环境，仍须本轮 fresh empty cache；命中旧 cache 跳过 hook 会被八收据门拒绝。SO、配置、prelink、Rust proof 和 protocol 身份必须随正式结果保留。

## 两项控制的真实覆盖范围

控制源是单独 test-only opt 插件，通过 `NO_ENTRY` 包含正式实现；未给正式插件增添测试入口。它使用 LLVM parser/verifyModule，而非伪造 Function/User：

1. `define available_externally i32 @callee(ptr %p)` 与 `call i32 @callee(ptr null)`：预期 inspect 准入且一个 use，原地 rename 后仍是同 Function 对象、internal、调用目标准确、attrs 不变并再次 verify。
2. 同样合法签名下 `call i32 @callee(ptr @callee)`：同一个 CallInst 含 callee 与参数两个 Use，预期 inspect 以明确 non-direct-callee 原因拒绝，Module 全文不变并再次 verify。

第二项是合法 typed IR 的最近地址反例，弥补此前 strict Rust proof 中 malformed nested-address 控制不能代表合法地址输入的边界。两控制复用生产 inspect/rewrite，期望判断独立手写；**它们不覆盖 manifest/env/真实 lld hook/8 backend/预算输入绑定路径**，控制输出也明确 `formal_manifest_receipt_path_exercised=false`。本审只读公式，尚未执行，不能声称已经通过。

## 研究门仍待实证

恢复 local sole-call bonus 有本轮真实 inline remarks 依据，但不能据此保证所有预链接 phase-ordering 差异消失。新机制与旧 CompleteInternal 的共同点是 private IPO 权限；新差别是导入已预优化 body 来复用 prelink 工作。当前未证明 native B/A≤1.10、R/B≥.90 或完整请求编译净收益；须保持已冻结两 case、8 unit、原预算和完整 O2，不因插件通过静审就扩大矩阵或声称采用。
