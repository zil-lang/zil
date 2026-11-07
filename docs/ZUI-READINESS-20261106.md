# ZUI：按 GPUI 方向复核语言缺口

基线为 main `adbead51fc8916ac1cb1019702d906a19ebd3e98`。本次三路 sol high 审计复用 PR46 的 release 编译器；生产输入与该 main 相同。原始执行身份、可重放命令、阶段与诊断见[精简收据](validation/G9.6.1-zui-readiness-audit.json)，小型探针现保存在[语言缺口输入](../validation/inputs/zui-readiness)，其旧路径、固定提交与逐文件字节摘要见[资产定位表](../validation/asset-provenance.json)。收据内的旧命令按当时基线保留；使用当前路径重放时应按定位表替换输入，并生成新的执行记录。这是缺口核对与下一批合同，不是新增语言能力或完整 ZUI 验收；当前排期只读 [goal](https://github.com/zil-lang/zil/blob/goal/goal.md)。

## 架构约束

用户明确要求 **ZUI 对标 GPUI，不使用 GDI 渲染**。因此 ZUI 的主路径是组件/状态产生场景数据，GPU 绘制并通过真实交换链呈现；字体塑形、字形图集、输入/IME、焦点和任务更新逐步接入同一应用。现有 Win32 示例只提供窗口、事件与资源管理的历史参考。GDI 绘制和 `std.ui_raster` 的 CPU 填充均不能替代 ZUI 的 GPU 验收，也不作为过渡交付路线。

为避免凭印象选型，本轮核读了 [GPUI 固定版本](https://github.com/zed-industries/zed/tree/4a6121bb0f10c6e8037b741b7dc0a4cd639ae289/crates/gpui_windows/src)：Windows 后端有 `directx_renderer.rs`、`directx_devices.rs`、`directx_atlas.rs`、`direct_write.rs`，使用 Direct3D11、DXGI 和 DirectWrite，含可选 DirectComposition 路径；通用层含 `scene.rs`、`element.rs`、`app.rs` 和 `text_system.rs`。这些是架构与具体平台接口参考，不是 ZIL 已实现能力，也不要求照搬 Rust 的 Rc/RefCell、生命周期编码或所有后端。

## 已有基础，避免重新派工

- 普通记录属性、默认值/spread、Enum/match、捕获 FnMut 和泛型 dispatch 的组合已通过当前 Verify；闭包/拥有替换与 scoped CT 的 PR46 范围保持已交付。
- shared stored View、保存共享 slice、直接可变借用返回、静态 trait、inherent 借入 getter 和省略 life 的 trait 借入 getter都有当前成功正例。活借用后修改目标的反例在 Analyze 被拒绝。
- UTF8、拥有 String、事务编辑、显式容量 Store 和 Copy 片回调迭代已有普通库。当前 `ui_layout`、`ui_raster`、`ui_input` 三个现有消费者均在 Linux LLVM API O0 原生运行成功；它们证明各自内核，不证明窗口、GPU 或 OS IME。
- 原 Win32 首窗源码当前 `--target win64` 可通过 Verify；历史 Wine/X11 窗口实验存在。本轮没有 Wine/Xvfb/显示环境，未重放 PE，也未执行原生 Windows。不能把“统一 ZUI 平台尚缺”写成“从未有窗口”。
- Task 的有限挂起/取消、拥有帧与同线程 AsyncRegistration 的晚通知拒入、真实 drain 已实现；不等于跨线程 executor 或一般 Mutex/Guard。

## 当前可复现的语言缺口

| 真实 API 形状 | 首个可观察边界 | 对 ZUI 的影响与最近正例 |
| --- | --- | --- |
| `string.from_utf8("按钮",64)` | parse/core 成功，Types 不支持普通 runtime 字符串字面量消费者 | 常规 label 源写法缺桥；显式 UTF8 字节片已通过。需先定义静态只读表示、编码/借入期限及显式拥有分配，不靠隐式堆分配补语法。 |
| 仓库 `342/slice_iter.zil`，`next()->Option<&'s T>` | parse 成功；Expand 在 `Option::<&'s T>.None()` 报未声明 life | 借入迭代链未闭合；当前 `iter.for_each`/`Option<T>` 不能替代它。修首个诊断后仍要检查返回来源、失效与后续消费者。 |
| `struct View<life a>{p:&'a mut i64}` | Core 成功，Types 拒绝该聚合字段 | 保存 affine 可变 Context 尚缺；直接 `&mut` 输入/返回可用，首帧可采用调用期间短借入。 |
| 同时有 `model:&'a T` 与 `manages bytes:ByteBuffer` 的 Package | parse 成功，Expand 拒绝 stored nominal ownership shape | 借入与拥有混合组件未接；共享 View、无借入拥有 Package 各自有成功正例。 |
| trait 方法显式 `<life a>` 并返回 `&'a` | parse 成功，Expand 限制为 direct/inherent 方法 | 不能泛称 trait lending 全缺：相同 inherent 签名、life elision trait getter 已通过。 |
| `fn peek(x:&dyn Read)` | Core 成功，Types 明确拒绝尚未实现的 dynamic inventory consumer；具体构造调用还会更早失败 | 开放用户组件/监听器的统一派发是实质缺口；静态 trait 已通过。声明/vtable 元数据不等于机器派发。 |

以上主要是已有语法的语义消费者缺口。`for x in &xs[..]` 已能解析，但一般 iterable lowering 未接；整数 range、while 和 Copy 回调迭代可用。match guard、`#[ui]`/专用 DSL 不属于当前必需前置。f32/f64 已可用；DPI/亚像素几何属于布局 API，Unicode scalar 编辑不等于 grapheme 分段、字体 shaping 或完整文本编辑器。

## GPUI 方向新增的硬前置

**GPU 调用与借入边界。** 当前 `c_handle` 不允许解引用、整数/Address 转换，外部函数指针不能直接当普通间接调用。因此不能用现有句柄原样读取 COM vtable、调用方法或取得可解引用的映射地址。本轮三个封闭 ABI 形状（scalar/out-handle、多 `&mut c_handle` 输出槽、固定 POD 上传）已经通过 Win64 Verify，尚未链接/执行 GPU。`&mut c_handle` 可以承载接口输出槽，不能笼统称 double pointer 都不支持；`#[align(16)]` 仍在 Expand 拒绝，固定 POD 正例也不授予任意过对齐布局；运行时方法入口、mapped memory、嵌套 Address 与普通同步借入是不同问题。实现前需提出 GPU 平台适配的 changes 合同：明确采用受审的显式平台接口还是扩展有限外部调用 profile；可优先验证显式、窄 C ABI 的受审 native adapter，GPU SDK/COM 调用留在可信边界，场景、布局和应用逻辑留在 ZIL；不得把整个 renderer 搬到宿主后称纯 ZIL。若选择直接纯 ZIL COM，则另补受审的外部方法/指针能力。不能隐式加入对象表、Box、RC，不能把平台胶水当作独立安全核证明。因此当前没有证据表明封闭首帧路线必须先增加新语法或全面扩展通用 ABI；缺的是适配实现、真实完成合同与 GPU 执行。

**GPU 提交不是 CPU 调用返回。** 需要独立拥有的 device/context/swapchain/buffer/texture，以及上传、提交、完成、resize/关闭与失败释放的责任。`noescape` 只适用于真正同步完成的 CPU 借入，不证明 GPU/DMA 已停止访问。首片可以明确等待完成以缩小范围，但不能用 Drop 或 CPU acquire/release 代替设备完成证明。

**场景容器与框架身份。** 首帧可用 Copy 记录、浮点及固定数组；现有 `StoreElement` 并非通用 `Vec<Vertex>`，动态 POD 显示列表要另补库/载体消费者。App/Entity/View 建议以显式 App 拥有根、短借入更新和结构变化队列组织；ID/世代不能自行制造存活或借用权限。开放组件和异构订阅需要 dyn 派发及显式拥有存储/删除清理，不能因固定场景可运行就从终态目标删去。

**文本与异步。** GPU 文本还需字体选择/塑形、字形图集、DPI/clip/alpha 和输入映射；345 Composition 不是 OS IME。后台字体/图像加载还需要一般 Transfer/Share、拥有结果回投、删除后的晚通知与跨线程排空，现有有限 Task/Registration 不能直接推成完整执行器。

## 下一批可独立派工的交付合同

以下是依赖与验收定义，状态/优先级不在本文维护。

| 任务 | 最小交付 | 关键验收与依赖 |
| --- | --- | --- |
| **G10.3.1 · GPU 平台 ABI 与完成合同** | 确定并实现最小真实 GPU 调用入口，覆盖句柄/out 参数、明确布局的上传输入、方法调用边界及资源完成；不一次开放任意裸指针 | 先 changes、独审；实际 ABI/失败清理/最近伪造反例。平台完成义务明确，GPU 真实运行另计。此项是 GPU 首帧的接口/验证前置；只有选定 ABI 超出现有能力时才扩展语言。 |
| **G9.5.1 · GPU 场景到首帧** | 复用 Win32 窗口/消息层；显式 App root 与固定 POD scene，经 GPU 绘制矩形/裁切并呈现，输入导致下一帧变化 | 与 G10.3.1 并行开发、在接口处合流；必须真实 GPU draw/present、resize、设备/创建失败及关闭清理。模拟结果、CPU raster、GDI、仅编译 PE 均不能标完成。 |
| **G5.2.1 · 两个 provider 的共享 dyn 派发** | 一个固定标量方法槽、两个不同具体 provider、实际接收者/入口身份和共享借入派发 | Source→两核/raw/Wire→原生结果；不先承诺拥有擦除存储、所有关联项/泛型方法。可与 GPU 独立推进，静态首帧不等待它。 |
| **G9.3.1 · 普通 UTF8 文本入口** | 普通文本字面量连接已有 String/只读文本 API，明确表示与分配边界 | Unicode/空文本/编码边界、生命周期、最近非法写/逃逸和预算；这是源语言补片，不等于完成字形绘制。 |
| G3.4.1 · 借入迭代器与返回组合 | 原 `slice_iter.zil` 正例贯通，保留合法两个结果及失效反例 | 后续接 Context/借入容器；不以修复 Expand 名字错误宣称完整 lending 已验收。 |
| G9.6.2 · GPUI 式最小应用合流 | 将前述窗口/GPU、状态/事件和场景接成可观察应用；后继接真实文字、开放组件与输入法 | 明确阶段目标；第一帧不是完整框架，完整任务仍受 G9.4/G9.5/G10.4/G10.5 等约束。 |

首批按前四项并行，root 负责公共 ABI、源输入和集成；常规实现与独审使用 sol high，具体难点再交 astra。只建实际开工的任务分支，复用既有测试目标；不为计划批量制造空分支。借入迭代、动态 POD 容器、拥有 dyn 存储、字体/IME 与后台回投按后续真实消费者接续。原生 GPU 验证环境需与接口开发同步准备，不能等实现全部完成才发现没有执行条件。

## 实证边界与 Rust 四目标

本轮补充了当前 Source 可用/拒绝边界和三个既有 UI 内核的机器复跑；没有生产修复、GPU 执行或产品性能测量。安全目标仍要求独立来源/权限/责任与设备完成核验；运行/内存、编译与维护成本仍需同语义对照。已接受的 PR46 不重做，尚未实现的 GPUI 式框架能力不因审计报告或更短语法被宣布完成；`validation/product-gates.json` 保持不变。
