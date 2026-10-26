# 088：Source CT 在复制之前界定完整原始形状

2026-10-25。基线 0b5c1e485784f6b57c170f1568be3ee8664fea6b；父任务 B0。本文先于本单代码修复。087 非作者审查发现：新增 ForeignCallback 的递归类型与静态 UTF16 可经公开 Source CT API、后置函数提示、局部注解和可达 helper，在预算预检之前被复制。本片接管 087 的 Source CT 与可达 collector 写权；不修改 schema15、默认限额、CT 能力集合或 Verify。

## 修复合同

1. 先对整个模块的函数头、receiver、callable 合同、aggregate/enum 字段和相关外部元数据进行借用式预检，再建立会复制类型的提示表、遍历函数体或执行 CT 查询。声明顺序不能决定是否先复制一个尚未检查的后置声明。
2. 局部 binding、record/update、closure 参数与结果、pattern、显式 ascription/conversion 里的完整类型树在克隆前受同一节点/深度限额约束。文本元数据的复制工作受现有 analysis-work 限额约束。
3. 回调签名中所有原始子类型都需要结构预检，即使该签名不适合作为 CT 提示。预检不代表类型合法，也不授予任何 CT 能力。
4. 可达 CT collector 保持 0b5c1e48 对外部 Literal/c_callback 的提前拒绝，并在函数/query 或声明表复制前覆盖完整注解、类型与 metadata。拒绝必须先于 clone，不依赖 Expand/Types 已经验证。
5. UTF16 单元和新递归节点按实际数量计入既有额度。超限明确 ResourceLimit，不增加限额或静默跳过问题。

## 证据计划

先保留未修基线失败：公开 raw Core 函数头/aggregate/enum/局部类型、后置声明提示、可达 query/helper 顺序、深度及累计额度。再修复并运行限定 zil-ct 与 impl088 driver 测试、格式和 lint。测试分清源码解析后的 Core、人工 hostile Core 与真实完整源码驱动，不以手填 IR 冒称平台窗口或完整语言完成。独立审核由非作者另行绑定最终提交。

本片只修复可证明的边界；不宣称编译性能改善、Rust 同级完整安全或新算法突破。

## 已实施与作者验证

实现提交 `a40107bf73921c21a82508151bd2afbe21527715`。公开 Source CT 入口先检查全部声明元数据；普通作用域、调用上下文与前瞻 hint 每次复制递归类型前累计收取节点和文本工作。保留未知/非 CT 类型的提示资格边界，但递归检查其子树。闭包未注解参数本身也占节点，不能用空名字/空类型绕过表大小约束。

driver collector 对原始 query、所有可达 helper 的参数/receiver/callable 合同、局部 annotation、record/update、closure 和复制的声明/import 元数据做借用预检。单模块适配器另用同一 visitor 的 runtime-only 模式，在初始 `core.clone()` 之前界定全树；它不执行查询、不收集调用、不授予任何 CT 权限。真正 query 模式继续提前拒绝静态外部值、`c_callback`、foreign 和 unsafe。嵌套 `#` 去壳改为移动已检查的子节点，避免额外递归 clone。`materialize.rs` 经检查无本片必须改动：类型复制来自独立 Verify 的有限 CT 返回类型，外部静态值仍无 VM 构造通路。

未修基线的七项失败保留于 `evidence/impl/088/baseline/`，失败代码为 101。精确作者验证源码为 `3126855039353257eb5aee06453b98f4e961e4fa`：该提交仅消费 root 的旧 BuildArtifact 测试夹具 `commands` 字段迁移，原提交为 `73f1dbf9fba69bc39def86dc037bd2ad9d3a9c5a`，不计作 088 实现。验证命令、输出与退出码见 `evidence/impl/088/author-001-result.json`。独立审核尚须由非作者绑定最终提交。

## 独审退回后的必要往来：编号扫描的宽度分配

2026-10-25。非作者 `/root/research084` 对 `ab71ed24` 的公开自定义 `SourceCompiler` 入口审查发现：前置 CT 聚合查询可在普通 Walker 尚未遍历后置宽函数体时调用 `materialize::first_free_id`；该扫描器先把完整 statements/arguments/fields/arms 集合 push 到临时 Vec，再逐节点检查预算。因此先前对 `materialize.rs` 的判断只覆盖了受核验结果的类型复制，没有覆盖编号扫描的临时工作队列。原作者通过范围和旧收据按原身份保留，不能据此继续宣称此公开入口闭合。

root 裁定此项属于既定 088 写集和先界定后分配目标，暂停交付工具，优先修正。生产补丁之前由独审保存最小 allocator 观察反例；本片接入对应 raw API 回归。

补充实现合同：保留 aggregate→enum→function 头部 ID 的观察顺序，以及原来 LIFO 的真实节点遍历和诊断顺序。宽集合改为借用 slice 的惰性游标，每次只安排一个真实节点与剩余游标；游标不代表 Core 节点，不重复收费。Block/Statement/Expr 的计数、深度、出错 span 和最大 NodeId 溢出规则保持原行为。固定分支保持至多三个子节点，pending 分配只随实际遍历深度增长，不再随尚未检查的完整宽度增长。不提高默认/传入限额，不改变 CT 可执行能力，也不把源码后续诊断放宽为成功。

## 返修实现与精确受测提交

返修代码为 `c07113139cc8283760f4b41a2a95885b3084fcb9`。Functions、Statements、Expressions、Fields 和 Arms 使用借用 slice 的 `split_last` 游标；固定分支顺序保持。新增单位测试以独立列出的访问顺序逐一检查 0 至 N−1 节点限额、N 成功、深度 0 至 5、Match/Sequence 的子节点顺序，以及头部 ID 溢出先于 body 预算的顺序。

未修真实失败分别保存在 `queue-review-baseline/` 和 `queue-regression-baseline/`，其中前者是具名非作者观察的原始副本。作者隔离 allocator probe 覆盖 8 种宽列表各 64/256/1024 个元素及 2 个成功对照，共 26 个 case；探针只在自己的独立测试 crate 中转发 System allocator，不改编译器 allocator 或工作区 forbid-unsafe 规则。最初把探针放在工作区测试目录被 unsafe 禁令拒绝，该失败同样保留。

`author-002/result.json` 绑定上述精确代码提交。71 次限定 Rust 测试执行通过（其中 allocator probe 是 1 个测试函数、26 个 case），定向 clippy、五个受管 Rust 文件的格式和 whitespace 检查通过。26 个 case 的输入/输出指纹、状态、compiler 调用数、诊断与 usage 对比未修基线逐字相同；另与独审原始 5 个 block 对照核对。固定 max_nodes=64 时，8 类超限宽列表的分配请求直方图在三个宽度间相同，最大请求均为 940 字节；原最大请求随宽度为 3072/12288/49152 字节。此为特定 API 测量区间的 allocation-request 观察，不是峰值内存、计时、普遍 allocator 上界或产品性能门。

本次收据修正前段对 materialize 扫描的遗漏判断；`ab71ed24` 的独审 Fail、旧作者验证范围和全部失败继续保留。独审由 `/root/research084` 在新 target 另行出具，本作者不自审。
