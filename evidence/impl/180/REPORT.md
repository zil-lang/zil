# 180 独占 Store 元素窗口：作者交付

源码冻结为 `74254d8da12004519ca342b1c0602a5a90831c44`，已吸收 main 的 1.4 规则身份修正。公共 schema/Wire 22 与 opcode 88 由集成者发布，本片实现其全部实际语义消费者。作者限定验收通过，独立审查及主树合入由集成者完成。

## 为什么与结果

ZUI 子项更新需要保持资源责任不动，同时原地修改位置、尺寸、标志等普通数据。现在 `store_at_mut(items, index)` 在已支持的完整 Store 元素上建立真实独占窗口，Copy 字段及其中的数组、record、借片投影可以写入；跨模块 `&mut Item` helper 使用显式再借关系。

四模块 `examples/mainline/store_mut180` 构造同时拥有用户资源和 ByteBuffer 的元素，按运行时索引更新布局元数据，再经共享窗口观察变化。输入 `3` 实际输出 `abcABC`：更新后的元数据产生 `abc`，未转移的原始资源在退出时按序析构产生 `ABC`。最后一次窗口使用后可以扩容。

## 实现

Types 解析已保留的 `store_at_mut`，依次求值接收者和 i64 索引，生成 `OwnedStoreBorrowMut`。精确的受支持 Package 地址类型允许作为同步参数；可变 Package 窗口同样进入原有返回及直接捕获拒绝条件。地址投影仍只开放有限 Copy 字段，未改变一般 pointee 或资源头权限。

Analyze 为新指令重建 Store 来源并发布写效果。Verify 独立核对 Store 定义、结果可变权限、索引类型、来源、真实初始化与活跃贷款，将该指令登记为独占贷款，再通过现有仿射转交、重借、NLL、真实字段来源和跨函数参数分离规则核验。Verify 没有调用 Types/Analyze 的证明函数。

Lower 和 LLVM 复用真实 Store `len` 检查以及现有地址写入机器路径。无符号比较同时排除负数，只有检查通过才计算元素地址；没有新增分配、运行时借用计数、世代、保活表或 noalias 假设。

## 实际执行

`runs/frozen02/summary.json` 绑定上述冻结提交与实际 Rust 1.99.1/Cargo 1.99.1/Clang 17.0.6 工具链。

| 项目 | 实际结果 |
|---|---|
| 新 180 用例组 | 19 通过，0 失败，0 忽略 |
| 相邻 174/175/175 独审原用例组 | 37 通过，0 失败，0 忽略 |
| 本轮限定总计 | 56 通过，0 失败，0 忽略 |
| 新 180 实际原生构建 | 28 次，覆盖 O0/O2 |
| 新 180 原生执行 | 36 次，其中 26 次普通/错误退出，10 次索引边界执行 |
| 运行时范围检查 | O0/O2 各执行两个合法索引、三个非法索引；6 次非法执行均实际 SIGILL checked trap |
| 源码最近反例 | 31 个：21 个 Verify、1 个 Analyze、9 个 Types 拒绝；其中 24 SafeReject、7 明确 Unsupported |
| 独立 raw/Wire | 10 组相同变异分别经两条入口拒绝，共 20 次独立 Verify 拒绝 |
| 格式 | `cargo fmt --all -- --check` 通过 |
| Clippy | 修改的五个生产 crate 的 lib 与新 driver 测试在 `-D warnings` 下通过 |

原生测试还核对接收者、索引、RHS 各求值一次及 `R → I → V → Drop` 顺序，索引计算中的合法扩容、索引计算中消费接收者的拒绝、错误 RHS 提前退出清理、子字段地址经局部 FnMut 捕获保持来源，以及 Copy 数组子片的独占保护。

源码反例覆盖共享窗口不能写或升级，两个未知索引不默认分离，活跃独占窗口与同 Store 的 reserve/push/pop/len/capacity/give、所属字段替换及父状态移动冲突。raw/Wire 变异覆盖错误 carrier/index/pointee、伪造 mutable 结果、共享指令升级、抹除 Hold、改指向兄弟 Store、投影 Managed 字段、提前 Drop 与一致降为共享后仍保留 WriteAddress。

## 保留的限制

- 当前片沿原有 Linux x64 Store 准入；Win64 实现与证据由 181 负责。
- 冲突范围保守覆盖整个 Store，没有证明不同元素索引或容器元数据与元素地址分离。
- 不允许完整 Package 复制、Managed 字段读写/转移、孔洞、直接捕获 Package 窗口、窗口或其派生地址返回与逃逸。
- 借片及标量子地址沿已有有限合同，局部闭包不能延长它们的实际存活期。
- 本片未进行分配失败注入或性能比较；错误 RHS 的真实析构、正常资源清理与索引 trap 已执行，不把它们等同于所有失败场景或产品预算达标。
- 完整 workspace、完整语言与产品门不由本片验收：分别为 NotRun、NotSatisfied、NotRun；B0 状态不变。

## 失败记录与交付收录

首次格式差异、初始测试断言错误、FnMut 夹具漏写 `var`、旧 175 blanket `&mut P` Unsupported 期待均保留原始结果。旧 175 原源码保留并迁移为 Success，同时增加共享 `store_at` 不能传给 `&mut P` 的拒绝用例；其它旧拒绝和预算未改。

`runs/` 保存命令、完整测试输出、逐 case 输入/输出/退出与 raw/Wire 收据；不收录原生程序和缓存。`phase-sample/` 保存一个真实四模块运行的 TypedIR、候选及接口；`phase-artifacts.json` 记录完整新 180 阶段产物的哈希，`machine-observations.json` 从真实 LLVM 输出抽取检查先于元素 GEP 的指令片段。`FILES.json` 为证据文件内容索引。

复验入口：先配置 Rust/Cargo/Clang，再运行 `python3 tools/language/180/check.py --repo . --out /absolute/new-output --target /absolute/independent-target --adjacent`。必须使用干净已提交源码，runner 创建新输出目录并保存真实失败，未执行的组不能计为通过。
