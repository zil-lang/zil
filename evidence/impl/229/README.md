# 229 作者交付：有限 where Copy

## 独审返修后的当前身份

独审发现普通类型 binder 与同名 nominal 的既有误接受。原 `6253c2e` 保留作者成绩与独审退回身份，不能作为本片验收候选。修复实现为 `d8c71880928c2e0497ffe078834382503960256b`，精确作者结果见 `repair/RESULT.json`：72/0 限定测试及 Clippy 通过；原六个误接受源、修前 2/1 失败、修后真实拒绝与合法同名 O0/O2 执行全部归档。

修复只在普通 type binder 实际撞上 nominal 时，先完成既有 scoped nominal 保护，再执行完整符号检查。该碰撞控制的实测费用 inline 从 281/131 变为 837/188，where 从 380/173 变为 936/230（work/nodes）；原无碰撞三组费用全部保持。独审和正式集成继续由非作者绑定最终提交完成；原有结果不回填为新结果。

## 原始作者实现与收据

实现冻结于 `6253c2eb468aa0284e9c201e0a1f0053dc20a6f4`。共享保留名提取是前置 `2e76a6dd09c2fc47ada43ec8192372c8bafa374b`，它等价于 228 作者的 `75f22dda0f80b49aeba16bc64b39249211d5e7ac`。本目录后续证据提交不改变生产字节。

普通自由函数的直接类型参数 `where T: Copy` 在别名正规化前转为原内联约束。仅全部参数为普通 type、inline 空或 Copy、where 主体是当前函数直接单段 binder 且 bound 为直接无实参 Copy 的形状进入此路径。重复 Copy 幂等；未知主体、组合/应用/投影主体、其它 bounds、lifetime/const 等明确拒绝。未调用正文和实际实参继续经过既有泛型符号检查与结构 Copy 资格检查。

新路径在复制完整 AST 前预费，随后把同一 Budget 经 where、alias 传给 Engine。旧无 where 的选择与费用保持。没有修改 schema、默认额度、独立 Verify 或后端权限。

## 精确受测结果

`RESULT.json` 给出完整命令、环境、实现文件哈希与归档哈希。作者只跑限定范围：driver 53/0，其中本片新套件 6/0，旧泛型、别名和 Copy 回归 47/0；expand 16/0；本片 driver 测试及其依赖 Clippy `-D warnings` 通过。所有测试均在精确实现提交的干净树开始，没有忽略或过滤项；重复预跑不叠加覆盖量。

新套件包括 9 个到独立 Verify 的源码正例、22 个 Expand 拒绝输入、三模块正例和未用非法正文的内联对照。Linux LLVM API 的五组程序各执行 O0/O2，共 10 次真实构建和执行，均 exit 0、空 stdout/stderr。同一普通程序的 inline/where/重复 where 三种写法生成的 LLVM IR 在仅移除 source verified-fingerprint 注释后逐字相同；原 LLVM、阶段 IR、接口、源码与完整报告均保存。

| 实际源与入口 | max_analysis_steps | max_nodes | 对照 |
| --- | ---: | ---: | --- |
| 普通 inline Copy，Core | 589 | 220 | 229 前后相同 |
| alias + inline Copy，Core | 952 | 233 | 229 前后相同 |
| alias + where Copy + specialization，Verify | 1651 | 488 | 同一累计预算 |

新源实际执行 fresh N、warm N、N−1、warm N。N−1 在 Expand 返回 ResourceLimit，Verify 执行数为 0，不产生 executable；恢复 N 后成功，成功请求独立 Verify 一次，每次 active query 为 0。旧源基线生产为共享名字提取提交，新测试当时尚未提交；该基线没有冒称干净测试树。三个 budget source 文件保持被测输入的实际字节（无末尾换行）。

## 原失败与限制

首轮保留 4/2 原结果，不重标为通过：应用 bound 的测试最初写作 `Copy<i32>=>`，词法相邻产生更早的 Parse 拒绝；增加空格后才测试预期的 Expand 拒绝。跨模块未用 `T:Copy` 算术原本就是 Unsupported，测试误要求 SafeReject；修正时新增独立 inline control，要求两种写法诊断相同。生产算法未因这两处测试修正而改动。

开工 handoff 在定位当前主树后返回历史记录完整性 Fail；完整原 JSON 保留，没有修写历史工单，也没有将该失败当成语言测试失败。最终正式集成和非作者审查由 root 接续；作者结果不能代替它们。

## Rust 四目标及突破

| 目标 | 本片实证 | 未建立 |
| --- | --- | --- |
| 同类安全 | 新表面约束复用原定义期/实例期检查与独立 Verify；未用越权正文、非 Copy 实参仍拒绝 | 全语言安全、一般 trait/lending 与生命周期 where |
| 运行和内存成本 | 有限三种约束写法产生相同实际 LLVM IR，O0/O2 行为一致 | 同语义 Rust 的计时、RSS、尾延迟比较 |
| 表达和维护 | 库函数可在独立 where 子句表达有限 Copy，三模块普通消费者可执行 | 完整库或框架维护成本比较 |
| 编译成本 | 两个旧源精确预算保持，新工作累计收费、缓存不绕限 | 编译速度提升或 Rust 成本胜出 |
| 至少一项突破 | 有限表面组合能力真实贯通 | 产品级可重复净突破和四目标合取 |

完整 1.4 仍 NotSatisfied，B0 Doing，产品及原生 Windows NotRun。本片不取消 StoreElement/callable/const/lifetime/method/impl where 等终态义务。
