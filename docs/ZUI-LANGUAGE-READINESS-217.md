# 217：ZUI 开工语言闭合矩阵与真实同步消费者

2026-10-29。**普通同步框架基础所需的一组关键语言组合已通过本次真实消费者验证；“用户全部 ZUI 需求”“所有 ZUI 语言前置”“完整 1.4”仍不能宣布完成。** 本报告逐项区分已能表达的基础、现有库级替代、真正未实现能力及其适用条件。它不以自定的有限起步范围替代用户全部目标，也没有开发 ZUI 框架或放宽编译器语义。

## 本次实际交付

五个普通 ZIL 模块：[消费者目录](../examples/mainline/zui_start217/README.md)。F 是拥有完整嵌套 Product 的 FnMut；G 是另一种拥有 ByteBuffer 的 FnOnce。它们进入同一个 Node<F,G>，再进入外层 Store。运行时输入驱动多轮事件，实际中间节点删除立即关闭其资源，剩余节点继续使用各自原来的回调状态；另覆盖父 ID 重挂接、同类型新环境替换、未调用退出和容量满 push 返回完整拥有错误值的清理。

`collection.erase<T:StoreElement>` 用普通泛型操作准备临时后缀 Store、移动后缀、关闭目标、再移回。没有使用嵌套 Store 元素、拥有 enum、dyn、隐式盒子、RC、全局对象指针表或新的安全权限。它是实际可行的表示路线，复杂度为 O(n)，没有据此声称满足产品性能门。

| 执行范围 | 本轮实际结果 | 证据解释 |
|---|---:|---|
| 固定源码观察 | 29 个输入分类一致 | 11 个 Source→Verify 成功，16 个 Unsupported，2 个保留的早期无效语法输入 SafeReject；不能写成“29 个语言功能成功” |
| 五模块普通产物 | O0/O2 共 14 次 | 每个优化等级构建一次、5 个源模块、独立 Verify 各 1 次；运行编译器未修改的产物 |
| 真实分配探针 | O0/O2 共 102 次 | 包含 74 次实际申请失败，以及正常探针与最后申请之后的非失败控制；逐项输出、退出、alloc/free、live 和 peak_live 匹配预声明 oracle |
| 原索引表示控制 | O0/O2 共 2 次 | 两次均 Verify1、exit0、`B!AC`；与五模块消费者分开记账 |
| Windows / Wine、产品与 Rust 性能 | NotRun / NotMeasured | 本次没有运行，不借历史成绩升级 |

五模块共 **116 次**机器执行，另有两个索引控制。所有分配探针最终 live=0、allocated=frees；实际峰值同时存活分配最多 5 个。请求 payload 峰值最高为 681 字节，只是这些输入的实际分配请求，不是 RSS、所有场景上界或 Rust 对比。原始阶段文件、机器产物及日志有 494 项哈希定位，执行产物未提交到源码 Git。

**分配证据的具体层级：**14 次普通执行来自默认后端的原 CLI 产物；102 次故障执行使用同次生成的 `llvm.ll`，由 Clang 与既有 realloc/free linker wrapper 单独链接。它们验证同一已核源码/LLVM 的故障清理，不冒称使用原 LLVM API 对象重链接。非作者的原对象复核结果应独立登记。本任务没有新增对抗性 raw/Wire 篡改组；这组新消费者通过生产 driver 调用独立 Verify，214–216 的合同反例与独审仍单独归属原任务。

## 精确身份与重放

五模块源、探针、运行器与 oracle 在首次五模块原生执行**之前**冻结于：

- 受测干净源码：`06a80559e91f5fc743bbadf17ddb900e25bc85d8`。
- CLI 源提交：`68dd99af8b995275db8ba51df73e419899c10e3d`；生产代码与消费者检出逐项匹配。
- CLI SHA-256：`448ff19886570b775176bca237ef512a523bd924e61f465b13b20d8b3be88ca7`。
- 固定 oracle SHA-256：`05140f3423017be127f4610294f2b09decffcd71cca7d42a1b561b032e527362`。
- [SOURCE-MANIFEST.json](../tools/language/217/SOURCE-MANIFEST.json) 绑定全部源/工具输入；[ORACLE.json](../tools/language/217/ORACLE.json) 和 [oracle.py](../tools/language/217/oracle.py) 保留预声明操作模型。模型不读取编译器输出或机器观察，不从实际结果反推预期。
- [RESULT.json](../evidence/impl/217/RESULT.json)、[RUN.json](../evidence/impl/217/RUN.json)、[ARTIFACT-MANIFEST.json](../evidence/impl/217/ARTIFACT-MANIFEST.json) 分别保存结果、真实命令和完整运行产物身份。

使用匹配的干净提交、冻结 CLI 构建收据和全新的仓外输出目录：

```sh
python3 tools/language/217/run.py \
  --repo /absolute/clean/zil \
  --cli /absolute/zil \
  --compiler-build-receipt /absolute/compiler-build.json \
  --compiler-sha256 448ff19886570b775176bca237ef512a523bd924e61f465b13b20d8b3be88ca7 \
  --clang /absolute/clang \
  --out /absolute/new-run-directory
```

CLI 所需 LLVM 动态库路径和实际环境在构建收据与运行收据记录。运行器不会构建 Rust或运行正式全仓。本次作者使用的原始输出目录为 `/workspace/scratch/3cf12ce32e36/recovery/217-consumer01`，提交后的源/工具与固定 manifest 相同；后继说明/收据提交不冒充另一次执行。

## 判定“开工前”的规范依据

`_zil_design.md` §8.1 要求按责任形状逐片开放，明确不等于所有递归组合立即受支持；§8.1.1/§9.1.2 定义当前有限 Product 和拥有捕获。某一种形状不被准入，不自动证明对应产品操作无法使用已有库表达。

§18.2 将 `syntax use` 定义为显式选择的可替换表面层，明确普通未使用扩展的代码不启动扩展 VM。因此卫生 `ui!` 生成链是声明式入口的真实前置，不能用手写程序成功替代；它也不是所有普通同步代码开始构造库的统一硬门。`docs/NEXT-LANGUAGE-178.md` 明确已有 State、同质容器与同步回调可构造受限起步消费者；`docs/LANGUAGE-195.md` 优先剩余项第5项也明确外部保留/异步/并发不一概作为同步手写入口前置。这些施工范围说明不覆盖完整规范终态。

§12.4 区分同步 noescape/static callback 和 retained userdata。已有 Win32 消费者采用前者，不能据其成功声称后者完成。W2 的生成同核、dyn 和图演进，W3 的取消/外部回调以及 `validation/product-gates.json` 的 Rust 成本/演进条件仍保持各自责任。本报告不将 B0、181、W2、W3、完整1.4或产品门标为完成。

仓库没有一张已冻结、能将“所有 ZUI 开工前”直接等同“所有1.4功能”的精确清单。因此以下矩阵说明具体已核能力和剩余条件，既不添加一长串假想硬前置，也不以有限消费者替用户删去声明式、开放异构和外部生命周期目标。

## 闭合矩阵

表中 `probe/<name>` 指 [固定探针目录](../tools/language/217/probes/) 下相应 `main.zil`，完整源哈希在上述 manifest。历史专项只引用其固定收据，不计为本轮重新执行。

| 消费需求 | 真实能力与证据 | 判定及剩余事项 |
|---|---|---|
| 动态字节、事件队列、UTF-8/UTF-16 准备与文本编辑 | `examples/mainline/zui_boundary159/{main,queue,state}.zil`、`owned_text/edit.zil`、`text_decode/text.zil` 均是普通语言算法；既有124/139/159有实际消费者。217使用真实动态缓冲和输入事件。 | 同步基础可复用。完整 String 不变量、grapheme、IME、排版应按实际库/平台合同推进，不能全按编译器缺口列单。 |
| 普通静态控件协议与公共泛型函数 | `impl147_static_traits.rs` 有 Source→Verify→O0/O2 的512矩形消费者；162有定义期有限名义 bound。 | 不是“traits尚未实现”。关联项、泛型/default trait 方法和life/HRTB仍属更大抽象范围。 |
| 具体 State 保存多个真实回调、同型不同实例及声明模式 | 203五模块 F/G 源与独立核/机器/失败收据；本轮旧双callback探针Verify成功；217真实Node<F,G>保持三个运行时节点。 | 有限同步能力已闭合。不同节点同ClosureId不合并责任；不外推任意异构集合。 |
| 普通完整T容器库、状态替换 | 202普通StoreElement模板、203 `store_helpers.zil`、205更新源与正式收据；217 `collection.zil`跨模块处理完整Node并执行删除。 | 不重做202–205。本轮替换在已知具体Node局部完成；不声称新增泛型F/G加replacement接口。 |
| Store与多层状态留在回调环境 | 204已验收Store拥有捕获；206已验收普通Product；214–216实现Product捕获。217 factory的FnMut取出完整child、局部修改并在失败/成功都回填，进入Node/Store后实际反复调用。 | 本轮新组合的源、Verify、O0/O2与故障清理闭合；独审归214–216及217非作者。只读默认Product借入仍未实施，不计通过。 |
| 动态节点、关系、中间独立删除和兄弟继续使用 | 217删除ID2立即输出`Bb2`，ID1/3保留，父ID可改，原回调计数继续；原索引控制两次`B!AC`。 | 嵌套Store、拥有Sum不是这些操作不可替代前置。当前是有限ID关系与O(n)删除；通用图、陈旧ID公共协议、压力/性能未由本片全部验收。 |
| 当前调用借入、&/&mut和slice参数 | 184、189、193有既有source/raw/Wire/native；原地址参数、nested/loop helper源本轮Verify成功。 | 可显式传入调用期视图。直接slice捕获仍Types Unsupported；保存借入视图或更一般lending应另建完整合同。 |
| 同步平台事件/绘制入口 | `win32/platform.zil` 静态无环境WndProc、消息循环、资源包装和按地址C record已有真实Wine首窗证据；无C UI宿主。 | 有限平台起点已有。不是retained ZIL userdata、完整GUI或原生Windows验收；本轮没有重跑平台。 |
| Product进入Store、含Store的Product、Store元素含Store、Product错误载荷 | 本轮对应四个`product-*`/`nested-store-element`探针明确Types Unsupported；生产门在`packages/product.rs`、`dynamic_store.rs`、`aggregates.rs`。 | **真实语言缺口，尚未证明是所有同步起步不可规避阻塞。** 217给出State/closure/ID替代实证；需要这种自然公开数据形状时仍应逐片开放，不能说已等价全部完成。 |
| 可选状态与拥有Sum | 普通Copy `Option<T>` enum控制本轮Verify成功；`i32?`表面型与`enum Maybe{Empty,Some(Leaf)}`仍Unsupported。 | “Option全部缺失”不成立。真实拥有variant仍缺；Copy tag/ID/已有ErrorUnion能覆盖部分基础状态。任意拥有variant切换需要tag、布局、来源与清理完整实现。 |
| 开放式组件、任意用户callback统一异构集合 | 当前Store支持有限具体F/G状态；合法`dyn-valid-parameter`明确Types Unsupported。相同布局不能当相同callback身份。 | 对运行时任意类型统一注册并持有的API是真阻塞；同质家族/已知类型分表+ID不需要它。需要时选固定dyn/erased合同或真实拥有Sum，不能引入不安全全局指针表或隐式盒子绕过。 |
| 泛型F原样返回、拥有参数/结果的callback | `callback-return-generic`在Expand拒绝；`closure-package-param`在Types拒绝；State<T,F>封装、完整T identity和直接字段调用已有路线。 | 一般高阶组合仍缺。必须以实际public API最小源确定是否为当前开工硬门，不能把一个签名失败扩成全部保存/调用失败。 |
| 声明式ui!、Type/Syntax、生成类型/impl、卫生与映射 | 合法`syntax-valid-alias`在Expand拒绝，`ct-item-if`/`ct-statement-if`明确Unsupported；CT常量控制成功；关联项探针明确拒绝。 | **声明式表面层的真实未完成项。** 在声明式ZUI入口前须完成受预算生成、封闭阶段、卫生绑定、映射/预览/格式化与生成/手写同核；普通同步开工不因此统一阻塞。 |
| 外部长期userdata、Active handler重入、取消/挂起/并发 | §9.5、§11、§12完整合同未闭合；生产FFI仍是明确noescape/static无环境范围。 | 对相应系统是硬缺口，不能用本片同步回调冒充。稳定位置、注册失败、停止接收、注销排空/no-future和最终执行者清理必须单独闭环。 |

## 场景与失败闭环

| 输入 | 源操作 | 普通执行固定输出 | 真实申请次数（无故障） |
|---|---|---|---:|
| `0` | 无active事件；逐项pop并消费三个不同实际G环境 | `S>3Cc>2Bb>1AaZ` | 4 |
| `1` | 一轮事件；删除中间节点；兄弟继续调用；逐项关闭 | `S.Bb2D>3Cc>1AaZ` | 9 |
| `2` | 两轮事件；删除；替换C的active为新Product环境i；继续调用/关闭 | `S..Bb2DcR>3Ci>1AaZ` | 12 |
| `3` | 三轮事件；删除后提前退出；剩余G未调用也精确销毁 | `S...Bb2DEAa1Cc3` | 11 |
| `4` | 容量满push归还完整Node4并清理；原三节点不变 | `SDd4ZAa1Bb2Cc3` | 1 |
| `9`、EOF | 拒绝入口，不创建拥有者 | 空输出，exit9 | 0 |

每个无故障申请序列逐点失败，并另跑fail_at=0和最后一次之后的控制。失败覆盖外层Store准备、Product child取出时buffer增长、删除前的后缀准备、删除后两个独立环境、替换后新环境、FnOnce内部实际分配。失败时FnMut完整回填；删除准备失败不提前移动节点；已关闭节点不复活；FnOnce已消费字段不再重复清理；其余兄弟仍由真实拥有者负责。

没有更改源或oracle来匹配五模块机器观察；作者第一轮完整机器执行即与冻结预期一致。非作者重放、原API对象失败验证和更广产品场景另记，不能把重放次数相加作为新的覆盖家族。

## 原始审计、失败与文档迁移

正式217前的只读审计使用另一CLI：源`f581cd67d1f36b088111a4afc55d308a0a47fe59`，SHA-256 `efc1ee72a25722ec10262ade600242b1a1e27fb09aba5fe733d07ab30baaf037`，尚未含214–216。其25次原检查、4次补充与原索引机器结果分别保存于：

- [audit25-original-result.json](../evidence/impl/217/audit25-original-result.json)
- [audit-followup-original-result.json](../evidence/impl/217/audit-followup-original-result.json)
- [indexed-original-native.json](../evidence/impl/217/indexed-original-native.json)
- [audit-source-manifest.json](../evidence/impl/217/audit-source-manifest.json)

这些是原recovery目录的真实身份，不是217当前运行文件的相对入口；正式入口是本页上方的217 manifest/RESULT。原源码已追踪在217探针目录，合法修正与原失效输入分开。

原`dyn-parameter`缺trait方法分号，`syntax-extension`含当前parser拒绝的保留字路径，它们的Parse拒绝不用于证明功能缺失；对应判断使用后续合法输入的明确Unsupported。原索引控制的裸局部块改成普通泛型release后才通过。旧名`closure-owned-error-result`的实际源返回直接资源，并非ErrorUnion，不能按文件名推断所有拥有错误结果尚缺。

217首次写文件时磁盘耗尽，零字节源/报告与不可获得的CLI结果记InfrastructureFailure，不计成功。第二次源检查拒绝直接赋值给临时`store_at_mut(...).parent`投影；将既有可变窗口绑定到具名local后赋值，第三次通过Verify。前一份五模块源和诊断已保存，没有改编译器。详见[FAILURES.json](../evidence/impl/217/FAILURES.json)。

## 接续判定

当前可以据实开始普通同步库/框架基础的抽取与扩展：已有完整拥有状态、实际回调环境、动态数量Store、显式调用窗口、ID关系和关闭的共同运行证据。用户要求的全部ZUI目标不能据此一并勾选。

下一语言增量应绑定新的真实public API阻塞：声明式入口选择§16/§18.2，任意异构handler选择§9.5/§18.1，长期OS环境选择§12.4。Product容器、拥有Sum、通用F返回等真实缺口保留，但本轮结果不支持仅凭Unsupported探针就将它们全部设为统一开工硬门。任一新增范围仍须源、独立核、最近反例、机器清理和预算闭环，不以parser或更多测试数量替代。

## Rust四目标与突破判断

| 目标 | 本次实际证据 | 限制 |
|---|---|---|
| 同类安全 | 生产Verify、明确所有权转交、实际失败与精确清理；重复类型的不同环境保留独立状态 | 本片不提供完整语言安全证明；新对抗性raw/Wire归原语义任务及独审 |
| 运行与内存 | 正常/故障执行、真实分配计数、最终live0，无新增隐式分配机制 | 临时后缀Store是源码显式算法；没有同语义Rust时间/RSS/尾延迟比较 |
| 表达与维护 | 五模块公共StoreElement库真实组合Product环境、两callback、删除与替换 | 证明这条同步表示可行，不证明所有组件结构或维护成本优于Rust |
| 编译成本 | 未增语义检查、默认额度与产品阈值；冻结CLI一次构建/多输入执行 | 未测Rust相对编译速度，本片不触碰独立性能线 |
| 实质推进 | 从分散有限能力推进到有失败闭环的真实同步消费者，并排除若干虚假的统一硬前置 | 不是产品级四目标合取、FullZUI或完整1.4终态 |
