# 333：一般大 Copy 内部按值 ABI

## 原源实测（实现前）

基线40fbcca6，自建target333、显式llvm-api、jobs1，原
`tests/language-readiness/codegen-pending/large-array-a18-by-value.zil`未改，
SHA256 `3acb74e1ffc85123469f402d2a3c02178d267543b7928692ad9e01547d9a7791`。
每组CPU45s/VM2GiB/wall55s、core0、原生等待5s。LLVM23.1.4/Linux x64：

| 请求 | 实际结果 |
| --- | --- |
| Clang O0 | 23.190s，Verify1后link InfraError/E-LLVM-COMPILE；cc1明确Segmentation fault及SelectionDAG回溯，外层工具SIGABRT；非OOM |
| LLVM API O0 | 25.223s，编译器SIGSEGV(-11)，无最终JSON |
| Clang O2 | 51.541s，工具报告Killed；CPU受限运行，不宣称无约束优化逻辑崩溃 |
| LLVM API O2 | 47.293s，编译器SIGKILL(-9)，无JSON、未触发55s父wall超时；资源受限未完成，不宣称无约束逻辑崩溃 |

本地结果/tmp/zil-333-probe/results.json；不归档大回溯，不缩源或改优化级别。

## 待 root 冻结的合同

333复用324已建立的>=4096B有限Copy Array/Tuple/Record独立快照与私有copy helper。
不把Source Value改成Address，不制造NoAlias或隐式heap。不只修改Call端：统一
计划定义/参数入口、返回、直接调用、FunctionRef和CallIndirect的完整机器签名。

建议内部大参数用指向已准备独立值快照的指针，在callee入口复制到真实参数local；
大结果由caller固定结果槽的显式outptr承接，callee在真实Return位置写完整结果后
返回void。保留原源求值顺序，所有普通实参与outptr位置一致；不能把可变local的
定位代替调用前值快照，多个同型实际参数仍是独立值。闭包的隐藏env角色保留，
若允许大显式参数/结果进入闭包入口，则所有closure调用模式也须消费同一ABI计划。
ABI按真实完整类型、目标和定义角色决定，不按名字或相同布局推断源类型相等。

FunctionPointer从其完整参数/结果类型重建相同ZIL机器签名；FunctionRef与间接调用
必须一起迁移，不能将foreign callback指针默认为ZIL内部指针。foreign声明及C
callback保持旧受支持C ABI，不以内部indirect规则扩大外部按值聚合准入。

CGU声明、跨单元引用、完整call-closure副本和函数地址身份须保留统一签名；完整
machine项/ABI版本进入对象、layout和patch缓存键，旧候选不能复用错误签名对象。
新增计划、类型快照与发射工作用当前Verified limits模块累计预付，N/N−1/fresh
和冷热对象控制须实测，默认额度不提高。

### 导出边界的实际阻塞

当前TypedFunction无pub/export字段，所有ZIL机器函数均先发为External。
默认生成符号导出合同在效，只有显式closed-executable放弃该合同。因此不能用
“Source私有函数”猜机器入口可任意改签名。保留旧LLVM大SSA wrapper本身仍可能
再次触发该故障，不能用一个未证明可编译的adapter冒称ABI兼容。

root需明确选择：证明并实现与既有目标机器ABI兼容的独立adapter编码；或者为受
影响大型ZIL生成入口明确发布版本化ABI/符号及迁移规则，而非暗改既有导出。
若仅closed模式允许迁移，要明确原默认构建仍未完成，不以该模式替代原探针。
所需public machine typed attrs（如byval/sret）由root持写；实际位置在
`crates/zil-llvm-api/src/model.rs`，文本/preflight/API须同批完整消费。

本片写集先限共享backend与已有aggregates/机器测试；未冻结导出边界前不实施
破坏性签名变化。普通大Enum/ErrorUnion及大型环境内部值构造的旧SSA桥接也须
明确范围，不把只修普通数组调用宣称所有大值机器路径完成。

## 冻结与实现

root在e28e08d8825085e9a3c6bae57d2a85d3a2418481冻结§19.4：选版本化迁移，
受影响生成入口后缀`.lc1`、仍External；参数copy-in指针，大结果首hidden outptr，
retvoid；小值/foreign C符号与ABI保持。无旧大型SSA兼容wrapper。
共享Emitter同时消费definition/call/ref/indirect/return；ABI计划与324 snapshot
计划共用同一剩余Codegen额度，额外header/函数指针完整类型布局检查确实收费。
CGU按实际版本化符号分组、导入和声明；完整machine签名/名称继续进入原对象键。
未贯通的closure entry/foreign/scoped-thread大签名提前Unsupported，未消费大snapshot
不再临时Load回巨型SSA：发射记录Unsupported并在返回机器产物之前终止。
