# draft07 公共接口与交付集成

状态：集成施工；语言规范保持 ZIL-LANG-1.3/A，语法及工具命名候选不在本轮定案。

025–041 已分别记录各自接受性、表示、预算和反例。本记录收束这些切片的公共版本，不追加语言规则，不把部分实现宣布为完整 schema.v1 交接。

## 公共版本

compiler schema、候选 wire 及 TypedModule 指纹域由 3 升至 4。此次已经加入聚合/枚举操作、可变闭包调用、地址投影、函数引用与间接调用、跨函数共享来源合同；旧字节不再具有同一解释。只接受当前版本，增加显式拒绝 wire 3 的回归。所有 raw 输入仍须独立核验；序列化和指纹不授予 Verified 权限。CT 的受限 CFG 与聚合 VM 也记录于当前部分接口，输出仍为未核 Core。

## 索引与历史

`spec/diagnostics.json` 是显式诊断登记表，conformance 仅提供其中一部分用例映射。重建索引必须保留已登记的实现诊断，再合并用例对应的阶段与规则，不能把尚无 conformance 条目的借用/编译期诊断删除。更新规范输入摘要及尚未合入工单；已合入切片继续绑定各自精确实现提交的原规范摘要。原审核收据不改写，必要的机器字段归一化另写引用原收据哈希的 binding。

## 最终验收与交付

先完成精确候选独审及主线合入，再在干净已提交主线运行统一门。多模块应用须从 ZIL 源码经过各阶段到真实 native，并与等价 Rust 在 O0/O2 比较完整返回值及自检退出状态。质量工具、历史反例与既有必过门继续运行。完整语言未实现项、产品门未运行状态保持可见。

源码包排除构建缓存、可执行产物和批量 raw；紧凑观察/反例保留。Git bundle 与明确选择的原始验收目录独立提供。打包后从解压源码重新构建/执行，从 bundle 恢复并检查交接，避免只验证工作树。

## 旧范围循环 CT 夹具迁移

统一门 `20261023T082949Z-b1db2659` 中，`rng12_runtime_iteration_variable_cannot_enter_closed_ct_even_when_empty` 仍把运行期循环变量进入 CT 查询预期为 `Unsupported / E-UNSUPPORTED`。027 的已核函数 CT 路径现已给出精确的 `SafeReject / Ct / E-CT-RUNTIME`，因此仅更新该测试的分类和错误码断言。空范围与非空范围的原源码都保留，均必须拒绝读取运行期 `n`；不接受任选分类、不改变生产代码。原失败摘要、日志哈希及限定 `ranges` 执行记录见 `evidence/integration/range-ct-migration.json`。

## 旧共享接收者夹具迁移

统一门 `20261023T083314Z-8daeecc1` 启用 `--no-fail-fast` 后，workspace 剩余失败来自005的 `r005_03`：原夹具仍将 `struct Point` 与 `impl Point(self: &Self)` 中的 `.x` 读取预期为 Unsupported。当前已支持该共享接收者，故保留原源码和 in-memory/file 两个类型入口，精确改为 Success；追加两个入口的真实源码→verify→LLVM/native 调用，均必须输出42。原 trait/trait impl 源码继续在两个入口精确拒绝为 Types/Unsupported/E-UNSUPPORTED，不删除反例，不允许任选分类，也不修改生产实现。仅调整这组测试与过时范围注释；原失败日志保持不变，有限4项回归和摘要见 `evidence/integration/receiver-fixture-migration.json`。
