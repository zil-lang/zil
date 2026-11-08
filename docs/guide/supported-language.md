# ZIL 语言初稿开发预览

状态：**有限 profile，实验性开发预览**。受支持环境为 Rust 1.99.1、LLVM 23.1.4、Linux x86_64。精确受测源码、独审、实际执行计数及恢复范围见[联合验收记录](../../validation/native-language-composition.json)。本页列出有限能力与边界；完整语言、B0、ZUI 和 Rust 产品目标分别验收。

当前有限拥有组合包括普通 POD、九种标量和完整 Copy Tuple/Array 的 Store、普通拥有父形状与自包含捕获。借入 dyn 支持固定标量/Unit 槽和共享/独占接收者。具体范围与限制如下。

普通拥有 Package 可组合已支持的资源与多个外部共享/独占借入叶，支持嵌套 qualified 子 Package、槽替换及 `move_out` 后完整补回。各槽保留真实来源、父 loan、初始化状态与独立拥有清理；通用捕获及更广异步运输仍有边界。规则分别见[可变存储载体](../../spec/mutable-stored-holder.md)、[trait 合同](../../spec/trait-method-lending.md)和[affine Mutex](../../spec/affine-mutex.md)。[基础语言发布门](../../validation/language-release-gates.json)定义完整功能验收，不代表当前预览已达到1.0。

包含真实共享借入叶的 qualified Record 可作为有限拥有 Package 的字段，局部窗口和字段投影保留实际来源。这类父 Package 的同型整对象共享地址可经普通 helper 从唯一实际输入返回；来自多个输入的返回来源、投影实参和经窗口访问拥有 ByteBuffer 的长度/容量仍有边界。

## 支持矩阵

| 能力簇 | 预览支持范围 | 验证范围 | 边界 |
| --- | --- | --- | --- |
| 拥有组合与动态派发 | 有限拥有父形状、捕获和递归 Package 模式；借入 dyn 的共享/独占标量及 Unit 槽；自含 ByteBuffer Package 的显式 `OwnedDyn` Heap 路线 | 真实双 provider、跨模块调用、字段访问、分配失败/一次 cleanup、raw/Wire、缓存与四机器配置 | owned dyn 尚不接受 user Drop、Store/借入 provider 或闭包捕获；借入结果及一般关联/泛型槽未开放，未知合同仍拒绝 |
| 普通集合、文本与 I/O | 完整 POD Tuple/Array/Record/标量 Store；有限完整 owning K/V 的 Entry 与按实际 KeyEq 查找的线性 Map；UTF8 String、排序、文件/流 I/O | 真实普通库、私有跨模块 Key、old Entry 转交、容量/分配失败返还责任、短写/EOF、四机器配置 | owning Map 不是 HashMap 性能承诺；已准入 owning Package／字段的 Option 借入 get 保留真实 manager；自动增长泛型 HashMap 及完整 Unicode 排版仍有缺口 |
| trait 与泛型库 | Type/const/life、关联项与字段/别名投影、默认/泛型方法、静态关联函数；普通标量/Enum 与有限拥有 Package 的共享／独占 trait provider；`Store<Item>` manager 的普通拥有 Item lending；多 binder Entry/Map | 未用定义的义务、真实 nominal/provider 身份、完整实参、独立两核与原生；零资源 Package 仍执行 affine 消费检查 | 一般 blanket/约束方程、更广 owning lending 和开放泛型方法槽仍需完整证明，实例不能补上定义期缺失前提 |
| 普通程序组合 | 真实跨模块程序中的集合更新、错误传播与明确资源责任；同一普通函数可组合借入结果Task、拥有Map的Option借出、结束借出后的插入与移动，支持直接匹配临时Task构造结果 | 固定Source、真实两来源分支、默认预算、完整stdout/退出值、calloc/realloc失败清理、独立raw/Wire与最近反例；各范围按精确源码验收 | 上述Task共享结果可来自多共享输入中声明结果类的真实外部来源；更广holder、捕获环境、借入结果及跨线程运输仍须逐一证明，有限同函数组合不关闭父能力 |
| 固定共享地址数组 | 非零 `[&'a T;N]`、qualified Record／拥有 Package 的直接字段、常量索引、普通 helper 参数/结果及共享视图复制 | 每 ordinal 的真实来源、两 life 非恒等映射、父 loan、Source/raw/Wire、四机器配置与精确预算 | `Field(i)` 仅在实际 Array/length/element 核对后表示物理 ordinal；`Element` 仅类型元数据；动态下标、空数组和独占数组复制尚未开放；大型拥有Package snapshot/ABI尚未验收；见 [语义合同](../../spec/fixed-shared-address-array.md) |
| 可变借入存储 | 有限 Record/拥有 Package 的多个地址叶；嵌套 qualified 子 Package、槽替换与实际取走/补回 | 真实 Source、独立 raw/Wire、CFG hole/init、SSA 消费、构造 child loan、四机器配置与实际资源释放 | 多路径缺口不能靠复制或别名重载清除；可变捕获、任意嵌套载体及跨异步运输仍需各自证明 |
| 借入 Task | 多个共享/独占输入、有限标量/Record/Tuple/Array/Slice referent、普通闭合结果；多个共享输入中声明结果类的实际共享地址结果；无显式 Task/Poll 类型头的创建、轮询和跨模块 Direct worker；直接临时表达式匹配保留真实owner；显式 Task<Result,Context> helper 转交 | PC0、挂起、取消/关闭、逐端口实际父贷款、未再使用 SSA 的持有、raw/Wire 与四机器配置 | Ready 保留真实外部父 loan，Task Destroy 不撤销已导出的结果；frame-local 结果、嵌套 owner/capture 运输、线程迁移与外部 ABI 仍未开放；pub Suspend 跨模块导出仍受标量 SABI1 限制 |
| 线程与同步 | 结构化线程、实际 pthread Mutex<T>/affine Guard、拥有 Package/Tuple/Enum/Array payload、明确来源的多输入 helper；i64/u64/usize 原子及强 CAS | 实际共享线程、逐输入来源、顺序合法矩阵、双后端 O0/O2、malloc/init 失败与逐元素一次清理 | Guard 不自动获得线程迁移或跨挂起资格；资源结构须满足实际同步清理与 Transfer/Share 证明 |
| 登记对象状态合同 | `AsyncRegistration<F>` helper 的逐结果三态合同、Pending 重试与 checked finish；具体闭合 F 的真实跨线程调用和完整关闭排空 | 真正 pre-trampoline raw holder、原子准入/发布、FnMut 串行执行、driver、失败清理及双后端 O0/O2 | 跨线程 profile 只接已证明的 closed 环境、同步 cleanup 与直接 entry 图；owner 不获 Transfer/Share，未取得 provider no-future/drain 时不得释放 |
| 项目使用 | 项目清单、路径依赖与锁、check/build/run/test、真实测试发现和失败状态 | 可复现依赖解析、每 importer 同名 helper、CT/缓存、Unit/Bool/I32 测试与最近失败反例 | 无集中注册表；空选择、失败编译、超时和失败测试不报 Pass |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物的有限 profile；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError | 源码和摘要绑定绑定源码的共同收据；默认整树、参考配置相关目标、两后端原生、失败清理及冷恢复分别记录，结果与冷恢复边界按精确源码记录 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 和产品门维持原状态 |

## 快速开始

在具备 `rust-toolchain.toml` 指定 Rust、相应 LLVM/clang 与系统链接器的已支持环境中，从仓库根目录运行。LLVM 安装、环境变量和版本检查见 [LLVM 工具链](build.md#安装与构建)；先 `cargo fetch --locked` 备妥依赖，再使用离线构建：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
```

该已存在的普通多模块样例预期输出为 `3 0 45`（含换行），退出 0；冻结验收已在远端重新拉取的同一源码与所标环境执行。组合样例在 `examples/mainline/draft_foundations/`，结合实际 std.store、trait 静态工厂、可变 helper 与显式失败处理。开发运行命令如下（本次冷恢复使用从源码新构建的 dev CLI）：

```sh
./target/debug/zil run examples/mainline/draft_foundations/main.zil --entry main
# 完整返回值应为 42
```

## 未支持及环境限制

- 完整借入枚举/结果、长期借用与资源宿主 lending、开放泛型容器、一般 owned dyn provider、异步借入结果及跨线程登记的更广组合仍有未闭合范围。父能力不能因某个有限子项通过而视为完成。
- GPU 首帧、字体、OS IME、完整 ZUI 与原生 Windows 执行不包含在本有限语言预览。目标/后端只按本次收据中真实执行的组合列出；Win64 交叉构建或 Wine 不等于原生 Windows 验证。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。B0、完整语言与 ZUI 终态验收继续独立进行。

本预览依据 `_zil_design.md` §20、`validation/product-gates.json`、`goal:workflow.md` 与实时 `goal:goal.md`。源码、各门结果及恢复范围绑定共同收据；远端恢复复用已准备的固定工具链和 Cargo 缓存，不代表空机器安装或旧历史重新验收。
