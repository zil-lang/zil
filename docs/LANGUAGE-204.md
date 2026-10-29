# ZIL 1.4 / Ante-R1：204 有限叶 Store 拥有捕获

2026-10-28。204 已完成有限范围验收并合入语言主线。最终干净候选 **`85f01d622d9c31bffe165e5720bbf09658af5084`** 的正式整仓结果为 **2469 通过、0 失败、0 忽略，252 个结果组**，同一候选的全 workspace/all-targets Clippy 通过。实际命令及原始输出摘要见[集成收据](../evidence/integration/204/final/RESULT.json)。127.21 秒是这次功能回归的执行时间，不是编译性能比较。

规范仍为 **ZIL-LANG-1.4 / Ante-R1**，本片合同是设计 §9.1.1；schema/Wire 为 **27**。设计 SHA256 为 `6975c100e3ae6905adc147828c7be0899d727adc5c6764462bc28ec5db8b55b0`，规范集合 SHA256 为 `92c5dd37cb319f7c81b5ccbdcc51830f872808b485458c3b3a7f35bae65c74b7`。实际输入注册表已据七个规范文件重新计算，未追改历史切片身份。

**完整 1.4 仍为 NotSatisfied，B0 为 Doing；产品、完整 draft/product 流水线和原生 Windows 为 NotRun。** 编译速度支线继续独立。

## 本片交付的能力

现在可以让自包含闭包真正拥有合格的 `Store<Item>`，或者拥有一个直接包含这类 Store 的完整 Package。支持现有 Fn、FnMut、FnOnce 模式；拥有性继续由实际源行为推导，包括显式 `move` 和正文中的 `give`，不只检查关键字拼写。

下面是原始固定输入之一，已经从源码通过独立核验并在 O0/O2 执行：

```zil
package Item { manages bytes: ByteBuffer; value: i32 }

fn make() -> impl FnMut() -> i32 {
    let items: Store<Item> = store_new()
    return move () => {
        if !store_reserve(items, store_len(items) + 1) { return -1 }
        store_push(items, Item { bytes: bytes_new(), value: 42 })
        return i32(store_len(items))
    }
}

fn main() -> i32 {
    var callback = make()
    if callback() != 1 { return 1 }
    return callback() - 2
}
```

完整输入位于 `crates/zil-driver/tests/impl204_fixtures/03_mutating_store_capture.zil`。同组另保留直接只读捕获、完整 Package 捕获及旧 Package 控制的原始字节。

FnOnce 可以在正文中取走捕获 Store，转交给内部局部；局部负责已取走部分的清理，闭包环境只清理仍然存活的捕获。Package 只取走一个 Store 字段时，兄弟字段继续保持各自的责任和清理顺序。相同 Item 或 ClosureId 的两个实际值不会合并成一份 Store、一个活跃标志或一次析构。

新环境可以进入当前 State 和外层 callback Store。五模块消费者实际组合了 Fn 子环境、FnMut 的完整 Store Package 捕获、FnOnce 取走 Store、外层增长、push/pop、普通完整 T helper、事件控制流及分配失败清理。

## 资格边界

本片内捕 Item 是有限、完整且非零布局的 Package，至少含一个直接 Managed ByteBuffer 或已有有限用户资源，其他部分遵守有限 Copy 约束。必须检查完整类型树和精确 canonical 资源定义，包含未使用的环境、仅参数/资源表出现的类型及第二个实际 Store。

内层 Item 的完整 Copy 聚合和资源头中也不能隐藏 Closure、FunctionPointer、ForeignCallback、Address、Desc、嵌套 Store、未开放的拥有数组、递归或未解析类型。此限制属于新内捕 Item；已有普通 Store 的 Copy callback 字段及捕获 Package 的合法 Copy 伴随字段保留原能力。

新 Store 拥有环境的结果限 Copy/Unit，该约束沿实际拥有的直接 Closure 子图传播。旧无 Store 的 lending 路径没有因此全局关闭。弱 Desc 也不会因 `move` 拼写升级成拥有权限。

Types、Analyze、Verify 分别处理实际类型及资格。Verify 继续从不可信 TypedIR、候选和 Wire 独立重建捕获、资源、布局、借用与清理事实；Lower 和 LLVM 只接收与同一 VerifiedModule 绑定的表示。没有新增公开 opcode 或隐式 RC/锁/堆盒。

## 真实发现并修复的窗口缺陷

验收前发现：FnOnce 正文先获取元素窗口，再取走捕获的 Store，最后继续读取旧窗口，原实现错误地通过 Verify。独立复现进一步在取走后对新局部执行真实 reserve；源码、fresh Analyze 候选配合 raw Verify、Wire Verify 当时均错误接受。

关键非法顺序如下；这不是可运行示例：

```zil
let view = store_at(items, 0)
let answer = view.value
let taken = give items
store_reserve(taken, 2)
return view.value
```

所有非法输入都没有执行 native。原始 `take-window01` 的 0/2 和作者最初反例完整保留，见[原失败](../evidence/reviews/204/by197/TAKE_WINDOW01_FAILURE.json)。

修复分别落在两套核中：

- Analyze 使用自己的强 Address 活性和精确 Capture/Field/Local 根，检查实际转交与结构修改。它不读取导出的粗化 Hold 候选作为安全事实，新工作仅由实际新捕获 profile 开启。
- Verify 分别补齐 TakeCapture 的父根失效检查和独占访问检查。它仍独立重建实际父容器关系，不调用 Analyze 的求证函数。

最终同一坏源码在 Analyze 以 `SafeReject / E-ANALYZE-IR` 拒绝。独审仍使用原控制生成的完整、不可信候选，分别把相同恶意指令顺序交给 raw/Wire Verify，两者必须独立返回 `E-BORROW-CONFLICT`；没有用前端提前拒绝替代 Verify 检查。

合法近邻保留：先读完窗口再转交、操作另一个同型捕获、借 left 字段时取走 right 字段、最后一次使用后转交整个 Package，均有实际控制。创建时移动的旧反例仍在 Verify 拒绝；正文取走、同字段取走及整个 Package 转交的三条原坏源则精确迁移到 Analyze，源字节不改。

## 实际验证，分别记账

| 范围 | 实际结果 | 身份与说明 |
|---|---:|---|
| 正式整仓 | 2469/0/0，252 组 | clean `85f01d62`，root 执行 |
| 正式 Clippy | Pass | 同一 `85f01d62`，workspace/all-targets，`-D warnings` |
| 前端作者 | 20 组源码 + 2 组预算，全部通过；32 次 native | clean `7453433e`；源码生产与 CLI05 相同 |
| Analyze 作者 | 7 组 raw + 7 组来源派生检查通过；4 次 callee native | 各次真实命令与 clean/dirty 身份分别保留 |
| 独立审核 | 21/0；46 次 O0/O2 native；33 对 raw/Wire | clean `694940ed`，15 对控制、18 对攻击 |
| 作者 Linux 消费者 | 148/0，其中 108 次真实分配失败 | 16 次普通执行、132 次 probe 执行 |
| 作者 Win64/Wine 消费者 | 148/0，其中 108 次真实分配失败 | Wine 兼容层，非原生 Windows |
| 独立 Linux 消费者 | 148/0，其中 108 次真实分配失败 | 非作者另行执行，保留其独立身份 |

这些范围存在重叠，不能相加得出新的测试总数。完整独审见[REVIEW](../evidence/reviews/204/by197/REVIEW.json)，作者收据分别见[前端](../evidence/impl/204/frontend/SUMMARY.json)、[Analyze](../evidence/impl/204/analyze/RESULT.json)与[消费者](../evidence/impl/204/consumer/RESULT.json)。

消费者各平台在 O0/O2 各构建一次，Verify 各执行一次。分配故障覆盖给出 Store 前后、callee 已准备但参数失败、增长与外层容器前缀等责任路径。所有最终存活分配为零；Wine probe 的错误操作计数也为零。观测最大请求中存活 payload 为 528 字节、同时存活分配最多四个，这不是进程 RSS，也不是对 Rust 的内存性能比较。

CLI05 实际来自 clean `7abbbac1f6479c52f4967653acfab74fc56c945d`，可执行文件 SHA256 为 `baa0b5757871b5f7bc31713f61c0a91042d1396087d9bddc65d6872c82a4312d`。178 个生产输入已与最终候选逐字核对；作者和独审的运行提交仍保持各自身份，没有改标成 root 的正式受测提交。

## 预算与原始失败

新增完整类型扫描、私有图、根集合、活性及比较工作均受限额约束，默认 limits 和 product-gates 没有修改。独审实际执行了 N、N−1 ResourceLimit、fresh N 恢复：

| 固定输入 | Expand | Types | Analyze | Verify |
|---|---:|---:|---:|---:|
| 旧 ByteBuffer 捕获 | 2 | 196 | 2230 | 2490 |
| 新双 Store 捕获 | 8 | 2692 | 21195 | 52430 |
| 新 Store 取走与窗口控制 | 8 | 1691 | 12981 | 27265 |

无 Store 操作、无 MakeClosure 的未用元数据控制为 Analyze 3252、raw/Wire Verify 各 5658。旧预算保持只指具名冻结控制，不推广成全部历史输入的统一结论。

首次正式整仓 `workspace01@057e7f67` 是 **2468/1/0**。唯一失败为旧 Analyze201 测试按旧文字匹配诊断。六个原 raw module 及实际 result 在迁移前后逐字相同，五个 Unsupported、一个 Success 保持；仅三个具名消息更新，未补造资源定义或将攻击改成正例。全量成功来自随后重新执行的 workspace02。首轮失败完整保留，见[迁移补审](../evidence/reviews/204/by197/MIGRATION02.json)。

更早的 Lower、LLVM 表示门失败，真实窗口误接受，夹具错误及拒绝阶段前移的旧断言失败均保留原身份。Wine01 的本地 IPC 启动被拒绝、成功匹配观察为零；同源/同 CLI/同 oracle 随后在允许的宿主上下文完成 Wine02，没有改源或预期来取得成功。

## 交付与剩余工作

源码通过标准完整 main bundle 和 203→204 增量 bundle 交付，保存原 Git 历史及精确审核提交。源码 Git 只增加代码、测试、工具、文档和紧凑收据，不加入执行文件、Cargo 缓存或大型执行证据。离线导入与编译支线合并命令随包提供；现有一键脚本未提供给本会话，因此没有声称测试过该脚本。

原始源码、raw/Wire、IR、二进制标准 I/O、命令和失败记录单独无损归档，执行产物仅保留哈希。完整文件列表、实际容器格式、大小和恢复方式以交付说明及 `evidence/integration/204/final/ARCHIVE.json` 为准。证据压缩和恢复检查不是一次新的语言测试。 主包实际22,216,240字节，独立补充工具包327,460字节，合计22.54MB；两包均经非作者逐文件/逐成员核验，外层只有普通文件。主包包含111,728份逻辑映射，解开唯一内容约1.50GB，完整逻辑恢复约5.43GB；普通源码同步不需要下载或展开证据。

下一205优先一个完整 `State<T,F>` 与独立 `replacement:F` 的更新组合，处理相同 F 下多个实际位置的责任、参数顺序、声明模式、名义身份和失败准备。现阶段只有只读入口分析，不能计作实现。

更一般的拥有组合、trait/关联项/life/HRTB/lending/dyn、卫生 Syntax/Type CT、长期外部回调、挂起/异常/取消/并发及产品量化条件继续保留。204 关闭了一个明确的 ZUI 语言前置，不代表全部 ZUI 前置或完整语言已经完成。

## Rust 四目标与突破判断

| 目标 | 本片实际证据 | 当前判断 |
|---|---|---|
| Rust 同类安全 | 独立两核、来源派生 raw/Wire、真实窗口缺陷与近邻控制、清理故障 | 当前有限片通过；完整语言安全未完成 |
| 运行与内存同级 | O0/O2、两平台失败清理和实际分配平衡 | 未测同语义 Rust 时间、RSS 或尾延迟，不能据此声称同级 |
| 表达与维护更简单 | 拥有动态 Store 的 callback 可进入当前 State/外层 Store | 组合能力增加；完整维护成本优势未证明 |
| 编译成本更低 | 新有限预算与旧具名控制保持，默认额度未改 | 本片没有编译性能比较；编译专项独立 |
| 至少一项实质突破 | 实际拥有组合与安全缺陷修复有完整生产路径证据 | 属有限工程推进，产品四目标合取和可重复净突破未建立 |

