# 062 完整 package 的独立责任与字段身份核验

2026-10-24（Asia/Shanghai）。本切片基线 draft11 `6acda08c72dbd3e387893b4260770f0834dcdbe0`；依赖 060 冻结公共表示 `6322b9c27b71f7f15019647e555268385a999d6a`。依据 `_zil_design.md` §§2、7、8 和主树 `.task/impl/062.md`。本提案先于本切片生产代码修改提交。

## 接受范围与责任表示

独立核验 `Type::Package { name, fields }` 和四个公共 package 指令。Package 至少含一个直接 `Managed<Record>` 字段；资源名义定义由原始模块表重检，其他字段须通过核内有限 Copy 检查。空名、重复字段名、零 manages、嵌套拥有载体、Desc/Address/Closure 字段、拥有错误联合、闭包直接捕获 package 及间接调用签名均不准入。普通参数和结果使用 `PassingMode::Infer`，没有 `give package` 的第二种移动语义。

本轮没有未完成 package 对象，也没有字段移出、replace 或用户整体 teardown。完整包的各拥有字段生命周期锁步，因此一个本地/SSA 责任槽足以独立证明 whole move 和整体释放；这不是抹掉字段身份。每个字段 Desc 保留 `(LocalId, field index)` 根，所有现有描述别名、存储、捕获及分支根联合都参与存活检查。whole Take/Drop/Store/StorageDead 必须拒绝仍被任何字段 Desc 使用的源外壳，不把旧 Desc 自动迁到目标包。

`MakePackage` 精确检查声明序字段类型并逐个消费 Managed SSA 输入一次；重复责任不能被集合归并隐藏。Copy 投影与 Desc 投影都要求源 package 在全部到达路径上完整活动。包参数入口持责，普通调用和返回消费 whole SSA，所有正常及错误返回仍检查遗漏的本地、未提交参数与未存储 SSA 责任。

## 清理、scope 与预算

所有 package 类型/操作触发现有严格 storage scope 核验。scope 证明实际 StorageLive 局部在退出前终止；拥有 SSA 继续由全函数责任核验守恒，不宣称 scope 证明 SSA 的词法归属。`DropPackage` 的核验要求合法 carrier、无活字段 Desc，并遵守跨局部的逆成功初始化序；其内部声明字段序 teardown 是冻结指令语义，由 060 的机器实现和 native 证据另行验证。

MakePackage 前已准备的字段仍是独立临时责任，后来求值失败清理这些临时，按逆成功准备序；完整 MakePackage 后才采用聚合声明字段序。不会把准备阶段收据写成一般部分成品证明。CFG 合流保持既有保守状态联合，消费/投影须恰为 LIVE；字段根、类型树、集合扫描与固定点工作继续实收费，不增加或放宽预算。

## 限定验证计划与边界

新增独立 raw/wire 正例及最近反例：完整构造/投影/调用/返回；重复输入、复制、漏清/重复清、晚参数错误、活字段 Desc 跨整体移动/重建、字段名义类型伪造、分支不一致、错误 scope 和未开放嵌套。测试不调用 Analyze/Types 的证明函数，序列化输入重走同一核验边界。只运行本 crate 的限定测试、格式及 clippy；精确命令、退出码、日志与提交身份在实际运行后记录于 `evidence/impl/062/`。

本切片不声称一般拥有聚合、逐字段部分移动、拥有错误载荷、逃逸/异步取消、ZUI 窗口或 GPU 已完成；不改产品验收门。四目标收益仅是有限拥有包安全核前置，性能/二进制/工程收益未测，后续由根集成的真实程序分别举证。

## 实际限定结果

生产实现提交 `f7ac2c990c203925d12fe74e102ffe9228ffebf1`；最终测试代码提交 `0a829453299bae3644476226dadeb0c02cafe271` 仅追加测试，生产字节未变。在干净 `0a829453` 上实际运行 `cargo test -p zil-verify`：135 个单元测试和 5 个 compile-fail 文档测试全部通过，无失败或忽略。新增 34 个 package 测试函数中，32 个同时重走 raw 与 wire，另外 2 个直接检查预算和候选身份；不是 34 个源码/native 案例。该 crate 的 clippy（所有 target、warnings 作为错误）、fmt check、diff check 均通过。

完整收据及精确日志哈希在 `evidence/impl/062/acceptance.json`；原始唯一 run 为 `out/runs/impl062/20261023T164437Z-candidate`。先前一次 test helper 调用 wire encoder 漏传 Limits/未解 Result 导致编译退出 101、零测试执行，其原始诊断和修正后的重跑分别保留，没有覆盖首轮失败。后续证据尾提交不变更生产或测试代码。
