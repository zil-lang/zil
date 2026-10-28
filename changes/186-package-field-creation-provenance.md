# 186 可变拥有槽的实际创建来源

## 依据与审核发现

本提案先于生产修改。1.4 §2.2、§3.3、§8.1、§8.3 要求实际构造与移动决定责任来源；局部可变槽的编译器表示不能把新建资源降为外部给入，也不能把旧资源升级为新建。root 已在主线 `.task/impl/186.md` 授权两个私有 `resources.rs` 子块，默认预算和公共 IR 不变。

独立审核源码 `972bf63d767cc9c3df3d76f5941f98cfcfef8c8c` 的 `an_error_returning_old_branch_does_not_poison_the_surviving_fresh_branch` 对 `var slot:Store<T>=empty(); if flag {slot=give old; return E.Fail}; return give slot` 得到 Analyze 误拒。原始失败位于 `run-logs/186-review/probes01`，不更改审核源码或预期。

检查实际 Typed IR 后确认，错误不是已 Return 分支进入 CFG 合流。Types 将可变 Managed 槽表示为单字段 Package。现有资源分析在 `MakePackage`、整个 Package 的 `Store`、`StorePackageField` 和 `TakePackageField` 处丢弃字段出生来源，固定记为 Incoming。成功分支因此无法证明原本的新建来源；无分支可变新建槽也有相同根因。

## 私有实现

- Analyze 为已有完整 Package SSA 值的直接 Managed 字段预分配命名出生集合，并复用局部 PackageField 生命周期槽。实际 `MakePackage` 从字段操作数取得集合；整个 `Store` 与 `TakeLocal` 在完整 SSA 字段集合和局部字段槽之间传递；字段替换及取出传递实际值的集合。非此精度的拥有字段保留现有保守规则。
- Verify 独立以有界的 Package SSA 字段字节矩阵记录 FRESH/INPUT，实际指令重建同一语义；不读取 Analyze 的表或生产证书。输入 Package、捕获 Package 和未知工厂产物的字段仍是 INPUT。只有已证明的实际新建来源能通过 create。
- CFG 仍只沿真实后继传播；来自多条可达路径的创建来源取 may-union。新旧混合不会被压成 FRESH，已终止分支不会污染其他后继。保留现有一次移动、活跃字段、责任守恒、Desc 存活、替换和清理检查。
- 所有新增形状扫描、映射/矩阵分配、查找、clone、join、字段传递均按实际工作计费，先收费再分配。仅追加实际工作收费，不改变任何默认上限，不新增公开字段或 wire opcode。

## 写集与验证

生产仅 `crates/zil-analyze/src/resources.rs` 与 `crates/zil-verify/src/resources.rs` 的状态及 Package 构造/Store/Take 子块；避开 184 的闭包调用和借址逻辑。测试/提案/精简证据沿 186 已授权目录。

限定验证先覆盖最小可变新建返回、原 root 错误分支反例、fresh/fresh 分支、真实 fresh/old 混合拒绝、直接输入 Package 字段及旧资源重包装拒绝。使用原生 O0/O2 验证实际清理，再从合法候选独立篡改 raw/Wire，确保输入来源、字段替换、重复移动及伪造 create 均不能经包装获得权限。随后复跑 183/186 与相邻 Package/Store 既有限定组，并保留第一次失败。

此修复不宣称递归拥有形状、任意 Package 工厂字段新鲜度合同、捕获环境借出或编译速度专项完成。
