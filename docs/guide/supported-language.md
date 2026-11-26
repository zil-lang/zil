# ZIL 语言初稿开发预览

状态：**有限 profile，实验性开发预览**。受支持环境为 Rust 1.100.0、LLVM 23.1.4、Linux x86_64。历史阶段联合验收快照固定于 `08da36ff0:validation/native-language-composition.json`，标准库阶段快照固定于 `08da36ff0:validation/library-foundations.json`（当前活合同见 [validation/README.md](../../validation/README.md)）。本页列出有限能力与边界；完整语言、B0、ZUI 和 Rust 产品目标分别验收。

当前有限拥有组合包括普通 POD、十三种标量和完整 Copy Tuple/Array 的 Store、普通拥有父形状与自包含捕获。借入 dyn 支持固定标量/Unit 槽和共享/独占接收者。具体范围与限制如下。

普通拥有 Package 可组合已支持的资源与多个外部共享/独占借入叶，支持嵌套 qualified 子 Package、槽替换及 `move_out` 后完整补回。各槽保留真实来源、父 loan、初始化状态与独立拥有清理；通用捕获及更广异步运输仍有边界。规则分别见[可变存储载体](../../spec/mutable-stored-holder.md)、[trait 合同](../../spec/trait-method-lending.md)和[affine Mutex](../../spec/affine-mutex.md)。[基础语言发布门](../../validation/language-release-gates.json)定义完整功能验收，不代表当前预览已达到1.0。

包含真实共享借入叶的 qualified Record 可作为有限拥有 Package 的字段，局部窗口和字段投影保留实际来源。普通多输入共享/独占借入返回（如 `select_shared<'a>`、`select_mut<'a>`、多生命周期选择 `pick_first<'a, 'b>`、`Pair<'a, 'b>` 交换、借入 `Option<&'a T>` / `Enum<'a>` / `Err!&'a [u8]`）以及泛型 trait/拥有 `Store<T>` lending（含 `impl<life a>` 上经 `&self -> &[u8]` 的借入返回与调用方 loan 保全）均已通过 `zil-semantics` 共享语义、独立 `zil-check` 及双后端原生测试。已准入窗口及 Direct helper 支持 ByteBuffer 长度/容量查询，查询按实际来源与当前委派检查共享读取；见[查询合同](../../spec/borrowed-byte-metadata-queries.md)和[联合普通程序](../../tests/fixtures/resource-packages/package-array-byte-query.zil)。查询不延长拥有对象的生命，旧描述也不能豁免后来创建的独占窗口。

普通拥有 Item 的 lending 窗口可与独立拥有结果的纯 Copy 字段观察及共享 Copy 错误臂共存；实际父对象、字段、tag、初始化和活动 loan 仍逐项检查。无输入标量入口与循环中的 Byte 查询必须使用实际 callee 效应和当前委派证明，见[普通拥有观察合同](../../spec/stored-owned-observations.md)及其原四模块程序。 同一存储借入程序还可经 trait 与泛型 helper 调用借入输入、拥有输出的同步闭包，并同步读取普通本地 ByteBuffer 字段视图；完整程序及边界见[拥有回调组合合同](../../spec/stored-owned-callback-pipeline.md)。该拥有结果可继续经泛型 helper 收集到可增长的 Store，扩容失败返还传入 owner；完整程序和逐分配点清理见[拥有结果收集合同](../../spec/ordinary-collected-pipeline.md)。

拥有 Package 现在可直接保存固定共享地址数组，并通过实际字段、常量或动态下标和普通 helper 取得借入元素；拥有责任与借入来源分别验证。见[普通程序](../../tests/fixtures/resource-packages/owned-package-shared-address-array.zil)和[限定合同](../../spec/qualified-package-shared-array.md)。原[512槽拥有程序](../../tests/fixtures/resource-packages/owned-package-wide-shared-address-array.zil)和[完整数组字段 snapshot](../../tests/fixtures/resource-packages/owned-package-shared-address-array-snapshot.zil)保留全部实际来源，覆盖普通参数/结果及拥有容器释放后的末项读取；字段快照不复制 Package 的 Managed 责任。 整个共享地址数组的读取按每个实际来源逐槽核初始化，不能由另一完整来源掩盖未补回槽（历史初始化验收快照固定于 `08da36ff0:validation/shared-array-snapshot-initialization.json`）。

普通整数包括 `u8/u16/u32/u64/usize/i8/i16/i32/i64/isize`；同型运算、checked 转换、显式 wrapping、目标位宽 CT、泛型 const 和聚合保留精确类型；完整整数和f32/f64进入已核 C参数/结果、地址Record与静态回调ABI。`isize` 与 `i64`、`usize` 与 `u64` 在 x64 上各同宽而类型不同。真实输入见[整数消费者](../../tests/integers/complete-domains.zil)及[C ABI消费者](../../tests/integers/ffi.zil)。

普通拥有迭代支持 `std.iter` 的惰性 `map`/`filter`、可增长 `collect`、拥有 fold 和每次调用借入的 callback 消费者；source/callback 的 `Value` 与同步 `SyncDrop` 义务独立于真正 StoreElement，见[普通值与同步迭代合同](../../spec/ordinary-values-and-cleanup.md)。

## 支持矩阵

| 能力簇 | 预览支持范围 | 验证范围 | 边界 |
| --- | --- | --- | --- |
| 拥有组合与动态派发 | 拥有父形状、多输入共享/独占/多生命周期借入返回、借入 `Option`/`Enum`/`ErrorUnion`、泛型 trait 与 `Store<T>` lending、捕获与递归 Package 模式；借入 `dyn Trait<'a>`（含 trait 生命周期参数方法返回 region 替换与 `DynImpl` 独立区域合同核验）、共享/独占值槽、Panic 合同及共享字段地址结果；闭合及借入 provider `OwnedDyn<dyn Trait, Context<'a>>`（含共同借入 `Store<OwnedDyn<..., Context<'a>>>`、`&self`/`&mut self`/`self: Self` 槽与 entry-block 循环分配）的 Heap 路线与 moved Fn/FnMut/FnOnce 捕获、实际拥有地址 helper 与重复调用 | 真实双 provider、跨模块调用、字段访问、分配失败/一次 cleanup、共同借入 Store push/at/pop、raw/Wire 有效证书 `zil-check` 与双后端 O0/O2 | 高阶生命周期 `for<'a>`、`dyn` 常量泛型实参、一般泛型方法槽与非接收者未知来源仍未开放，未知合同仍拒绝 |
| 普通集合、文本、系统与网络 I/O | 完整 POD Tuple/Array/Record/标量 Store；[可增长 POD HashMap 和 Copy Set](../../spec/pod-hash-collections.md)；[普通 owning K/V 哈希 Map 与 owning Set](../../spec/owning-hash-collections.md)、拥有 Entry 的整库转交；UTF8 `std.string`（含 `starts_with`/`ends_with`/`contains`/`trim*`/`split`/`lines`/`replace_all`）、排序、文件/目录/元数据/重命名 `std.fs` 与 `std.path`、CLI 参数 `std.env`（`arg_count`/`arg`/`args`）、POSIX TCP Socket `std.net`（`TcpListener`/`TcpStream`） | 真实普通库、私有跨模块 Key、增长/rehash 准备失败保留、拥有键值返还、活动 get 窗口、逐分配点清理、环回 TCP 与 fd 回收压力、Source/raw/Wire 与双后端 O0/O2 | owning 哈希容器要求实际 StoreElement 与稳定 KeyEq/KeyHash；完整 Unicode 排版、TLS/UDP/异步非阻塞网络属于 Post-Alpha |
| 数值文本与缓冲 I/O | [十种整数十进制、浮点解析格式化与固定容量 Reader/Writer](../../spec/numeric-text-and-buffered-io.md) | binary32/64 位模式 roundtrip、边界与舍入、locale 失败清理；短读写、EOF、失败后精确 suffix 重试与关闭一次；两核默认和 N/N−1 预算 | 浮点文本当前为 Linux/glibc 普通固定签名适配；roundtrip 使用 round-to-nearest，其他外部舍入模式仍生效；Writer 需显式 flush/finish 观察输出错误 |
| 拥有资源图 | [runtime-sized 循环／共享边图、逻辑删除、回收与有界 ticket 复用](../../spec/checked-resource-graph.md) | 真实 ByteBuffer payload 与分配失败返还、循环遍历、重复删除／collect／take／clear、旧 ticket 失效、两图同号边界与图依赖分析器应用 | NodeKey 是接收 Graph 的相对 namespace 值；运行时检查实际节点，不提供跨图 generative 来源证明或稳定地址；历史 metadata 容量固定 |
| trait 与泛型库 | Type/const/life、关联项与字段/别名投影、默认/泛型方法、静态关联函数；普通标量/Enum 与拥有 Package 的共享／独占 trait provider；`impl<life a>` 借入/拥有 trait lending 与 `Store<Item>` 泛型 lending；多 binder Entry/Map | 未用定义的义务、真实 nominal/provider 身份、完整实参、独立两核与原生；零资源 Package 仍执行 affine 消费检查 | 高阶生命周期 `for<'a>`、非可调用或多约束 `impl Trait` 返回类型与开放泛型方法槽属于 Post-Alpha；实例不能补上定义期缺失前提 |
| 普通程序与日常应用组合 | 真实跨模块程序中的集合更新、错误传播与明确资源责任；8 类日常多模块应用（[`examples/everyday-apps/`](../../examples/everyday-apps)）：文本统计与文件搜索、CSV 处理（RFC 4180）、JSON 解析序列化（RFC 8259 含 UTF-16 代理对与控制字符转义）、CLI Todo、HTTP/1.1 小服务与客户端、文件批处理、图依赖分析、异步任务调度器 | 固定 Source、CLI/文件/目录/TCP/异步取消全路径执行、OOM 注入清理、配对安全反例、独立 raw/Wire 与双后端 O0/O2 | 上述 8 类应用及 12 类能力族在冻结 `alpha-acceptance.yml` 中逐项留证；通用网络/TLS/异步生态属于 Post-Alpha |
| 固定共享地址数组 | 非零 `[&'a T;N]`、qualified Record／拥有 Package 的直接字段、常量及十种整数动态索引、普通 helper 参数/结果及共享视图复制 | 每 ordinal 的真实来源、两 life 非恒等映射、父 loan、逐根初始化、不同 upper 的局部读、Source/raw/Wire、四机器配置与精确预算 | `Field(i)` 经实际 Array/length/element 核对才是物理 ordinal；`Element` 仅类型元数据。动态读取先快照且保全候选；空数组与独占复制受安全规则限制；见 [固定合同](../../spec/fixed-shared-address-array.md)和[动态合同](../../spec/dynamic-shared-address-array.md) |
| 可变借入存储 | 有限 Record/拥有 Package 的多个地址叶；嵌套 qualified 子 Package、槽替换与实际取走/补回 | 真实 Source、独立 raw/Wire、CFG hole/init、SSA 消费、构造 child loan、四机器配置与实际资源释放 | 多路径缺口不能靠复制或别名重载清除（安全规则限制） |
| 借入 Task | 多个共享/独占输入、有限标量/Record/Tuple/Array/Slice referent、普通闭合结果；多个共享输入中声明结果类的实际共享地址结果；无显式 Task/Poll 类型头的创建、轮询和跨模块 Direct worker；直接临时表达式匹配保留真实 owner；显式 Task<Result,Context> helper 转交；嵌套可挂起调用确定性取消展开 | PC0、挂起、取消/关闭、逐端口实际父贷款、未再使用 SSA 的持有、嵌套可挂起取消清理、raw/Wire 与双后端 O0/O2 | Ready 保留真实外部父 loan，Task Destroy 不撤销已导出的结果；frame-local 结果、线程迁移与外部 ABI 仍未开放；pub Suspend 跨模块导出仍受标量 SABI1 限制 |
| 线程与同步 | 结构化线程、实际 pthread Mutex<T>/affine Guard、拥有 Package/Tuple/Enum/Array payload、明确来源的多输入 helper；i64/u64/usize 原子及强 CAS | 实际共享线程、逐输入来源、顺序合法矩阵、双后端 O0/O2、malloc/init 失败与逐元素一次清理 | Guard 不自动获得线程迁移或跨挂起资格（安全规则限制）；资源结构须满足实际同步清理与 Transfer/Share 证明 |
| 协议与登记状态合同 | `AsyncRegistration<F>` 逐结果三态合同、Pending 重试与 checked finish；用户自定义有限状态协议（`enum` 或含 `enum` 类型 `state` 字段的句柄，最多 64 态），支持 `requires(state/apart)`、`provides(state/on_ok/on_err)`、导出投影失效上界 `effects(invalidates(p.f))` / `effects(invalidates())`、历史撤销 `H`（`effects(rollback(p))`）、跨 `match` 分支精化、未标注调用方前置核验；以及基于共享 `zil-semantics` 事实的联合语义：`Replace` 历史与责任隔离、`Store` Push/Pop 成败分支与非修改借用、`Task`/`Registration` 条件创建与 `Valid` 只读语义及 `TaskStart` 完整定义、取消边保守保留副作用与所有权转移 | 真正 pre-trampoline raw holder、原子准入/发布、FnMut 串行执行、用户多角色状态机、Replace/Store/Task/Registration/Cancel 正负例、Source/Raw/Wire 有效证书 `zil-check` 及双后端 O0/O2 | 跨线程 profile 只接已证明的 closed 环境与同步 cleanup；超 4 层嵌套字段路径或超 64 态协议域仍为 Post-Alpha |
| 项目使用 | 项目清单、路径依赖与锁、check/build/run/test、真实测试发现和失败状态 | 可复现依赖解析、每 importer 同名 helper、CT/缓存、Unit/Bool/I32 测试与最近失败反例 | 无集中注册表（Post-Alpha）；空选择、失败编译、超时和失败测试不报 Pass |
| 交付与质量 | Source→Types→Analyze→独立 Verify→机器产物；CLI 区分 Success、SafeReject、Unsupported、ResourceLimit、InfraError；提供 Linux x86_64 可复现归档包（含 `bin/zil`、`lib/libLLVM-23.so`、`RELEASE-MANIFEST.json`、`SHA256SUMS.txt`）与空环境验证工作流 `alpha-release.yml` | 源码与可执行验证绑定；冻结候选由 `alpha-acceptance.yml` 逐项验证 12/12 能力族 + 8/8 日常应用 + Native API(16/128/56) + Fixed Projects(21/21, 152/152 required / 196 historical, 44 archival) + Roam(42/42, 24/24) 并由 `alpha-release.yml` 完成无源码/无 Rust 空环境安装运行验收 | 零命中、Unsupported、超时、工具失败与旧提交收据均非本次 Pass；正式 B0 与 `validation/product-gates.json` 性能/产品门维持独立验收 |

## `language-release-gates.json` 十二项能力逐条三分类核对

| 能力 ID | 满足（Alpha 已闭环） | 受安全规则限制（设计要求，非缺口） | 尚未实现（Post-Alpha） |
| --- | --- | --- | --- |
| `values-and-patterns` | 13 种标量、checked 转换、数组/切片、Record/Enum/Option/ErrorUnion、具名/位置解构与部分 drop | 禁止对独占/拥有叶静默 Copy | 浮点字面量模式匹配（`float literal patterns`） |
| `generic-libraries` | Type/Const/Life 泛型、trait/supertrait、关联类型/常量投影、默认/泛型方法、未用定义义务核验、Copy 与 owning 迭代器/容器 | 实例不得掩盖定义期缺失义务；重叠/歧义/循环 trait 必须拒绝 | 高阶生命周期 `for<'a>`、非可调用或多约束 `impl Trait` 返回类型、开放泛型 `dyn` 方法槽 |
| `ownership-composition` | 拥有 Package 含外部借入、多输入共享/独占/多生命周期借入返回、借入 `Option`/`Enum`/`ErrorUnion`、泛型 trait 与 `Store<T>` lending、槽替换/取走/补回与 G6.3 状态协议联合语义 | 多路径缺口不可靠复制清除；借入视图与拥有槽冲突必须 `SafeReject` | 任意深度（>4 层）嵌套字段路径状态协议、>64 态协议域 |
| `dynamic-abstraction` | 借入 `dyn Trait<'a>`（共享/独占接收者与 trait 生命周期返回替换）与 `OwnedDyn<dyn Trait, Context<'a>>`（移动/析构/OOM 清理/共同借入 `Store`） | 未知/未证明动态合同与悬空接收者返回必须拒绝 | `dyn` 常量泛型实参、一般泛型方法槽 |
| `resource-graphs` | 运行时尺寸循环/共享边资源图（`std.graph`）、逻辑删除、回收、有界 ticket 复用与图依赖分析器应用 | 逻辑删除与槽复用不可复活旧句柄；静态限制与显式检查分界明确 | 跨图 generative 静态来源证明与无界历史元数据 |
| `async-ownership` | 多输入借入 `Task`、PC0/Pending/Ready/Cancel/Close、helper 参数与返回、嵌套挂起取消展开、G6.3 Task/Cancel 协议 | frame-local 借入结果不可逃逸；未关闭/活跃任务不可静默丢弃 | 跨线程任务迁移、非标量 `pub Suspend` 外部 ABI |
| `threads-and-synchronization` | 结构化 `thread_scope`、`Mutex<T>`/affine `Guard`、`AtomicI32/I64/U64/Usize` 与有序操作/强 CAS | `Guard` 不自动获得跨线程或跨挂起迁移资格；须具实际 `Transfer`/`Share`/`SyncDrop` 证明 | 无锁通用工作窃取调度运行时 |
| `foreign-lifecycle` | C ABI（完整整数/浮点/地址 Record/静态回调）与 `AsyncRegistration<F>` pre-trampoline/原子准入/重入关闭/排空清理 | 跨线程回调仅接已证明 closed 环境与同步 cleanup；无证明的裸指针逃逸必须拒绝 | 通用动态库/跨语言 ABI 生成器 |
| `standard-library` | `std.utf8`、`std.string`、排序、POD/owning `HashMap`/`HashSet`、`std.iter`、`std.io`/`std.fs`/`std.path`/`std.env`/`std.net`（Linux POSIX TCP 与 Windows Win32/UCRT Unicode `*W`/Winsock2） | 所有分配错误显式返回，失败与早退确定性清理拥有句柄与 Winsock 引用计数 | 完整 Unicode 排版/locale、TLS/UDP/异步非阻塞网络 |
| `project-workflow` | `Zil.toml` 清单、路径依赖与 `zil lock`、`check`/`build`/`run`/`test`、独立测试发现与确定性失败码 | 空选择、编译失败、超时与测试失败一律不得报 Pass | 集中式包注册表发布与下载 |
| `native-delivery` | Linux x86_64 与 Windows x64 Clang + LLVM 23.1.4 C API 双后端 O0/O2、确定性归档发布包与无源码/无 Rust 空环境安装运行验收（`alpha-release.yml` / `windows-alpha-release.yml`） | 必须运行独立 `zil-check` 验证器与真实二进制执行；发行包不含系统 C 链接器 | macOS 与多架构交叉发布 |
| `release-acceptance` | 同一冻结提交执行 12/12 能力族 + 8/8 日常应用 + Native API(16/128/56) + Fixed Projects(21/21, 152/152 required / 196 historical, 44 archival) + Roam(42/42, 24/24) 的 fail-closed 收据门 | `Unsupported`/零执行/跳过测试/旧提交收据一律失败；不以测试数冒充产品门 | `validation/product-gates.json` 性能/B0/同语义 Rust 净突破正式批准、ZUI/GPU/IME、Managed GC/RC 模式 |

## 快速开始

### 方式一：使用预构建 Alpha 归档包（无需 Rust 或源码树）

- **Linux x86_64**（需系统 `clang` 链接器与 `libzstd1`/`libxml2`/`zlib1g`）：解压 `.tar.gz` 归档包并校验 `SHA256SUMS.txt` 后直接运行：

```sh
sha256sum -c SHA256SUMS.txt
tar -xzf zil-v1.0.0-alpha.2-linux-x86_64.tar.gz
./zil-v1.0.0-alpha.2-linux-x86_64/bin/zil version
./zil-v1.0.0-alpha.2-linux-x86_64/bin/zil build zil-v1.0.0-alpha.2-linux-x86_64/examples/everyday-apps/json-processor -o ./json-processor --opt O2
```

- **Windows x64**（归档内置 `zil.exe` 与 `LLVM-C.dll`，**无需 Rust/Cargo**；最终原生 PE 链接需系统已安装 `clang.exe` 与 Windows SDK/MSVC UCRT 导入库并在 `PATH` 可见）：解压 `.zip` 归档包并校验 `SHA256SUMS.txt` 后直接运行：

```powershell
.\zil-v1.0.0-alpha.2-windows-x86_64\bin\zil.exe version
.\zil-v1.0.0-alpha.2-windows-x86_64\bin\zil.exe build .\zil-v1.0.0-alpha.2-windows-x86_64\examples\everyday-apps\json-processor -o .\json-processor.exe --opt O2
```

### 方式二：从源码构建

在具备 `rust-toolchain.toml` 指定 Rust、相应 LLVM/clang 与系统链接器的已支持环境中，从仓库根目录运行。LLVM 安装、环境变量和版本检查见 [LLVM 工具链](build.md#安装与构建)；先 `cargo fetch --locked` 备妥依赖，再使用离线构建：

```sh
cargo build --offline --locked -p zil-cli
./target/debug/zil version
./target/debug/zil check examples/mainline/stream/main.zil --json
mkdir -p out
./target/debug/zil build examples/mainline/stream/main.zil -o out/stream --emit-dir out/stream-stages
printf '10 20 -3\n' | ./out/stream
```

该已存在的普通多模块样例预期输出为 `3 0 45`（含换行），退出 0。更多组合样例位于 `examples/mainline/draft_foundations/` 与 `examples/everyday-apps/`（涵盖文本统计与文件搜索、CSV、JSON、CLI Todo、HTTP 小服务/客户端、文件批处理、图依赖分析、异步任务调度器 8 类完整多模块应用）：

```sh
./target/debug/zil run examples/mainline/draft_foundations/main.zil --entry main
# 完整返回值应为 42
```

## 未支持及环境限制

- 明确属于 Post-Alpha 的编译器/语言项：`for<'a>` 高阶生命周期、非可调用或多约束 `impl Trait` 返回类型、浮点字面量模式匹配、`dyn` 常量泛型参数与开放泛型方法槽、运行期 `Type` 值、Managed GC/RC 模式、超 4 层/64 态状态协议、完整 Unicode 排版/locale、TLS/UDP/异步非阻塞网络与集中式包注册表。
- GPU 首帧、字体、OS IME、完整 ZUI 与 macOS/ARM64 目标不包含在当前 Alpha 范围。Windows 发行包不内置 `clang.exe` 与 Windows SDK，需宿主机器提供系统 C 链接工具链；目标/后端只按本次收据中真实执行的组合列出。
- `validation/product-gates.json` 中零 safe UB、同语义 Rust 运行/内存与编译/维护成本条件、至少一项可重复净突破及发布前提原样有效。目前不得宣称这些产品条件达成。B0、完整语言与 ZUI 终态验收继续独立进行。

本说明依据 `_zil_design.md` §20、`validation/language-alpha-profile.json`、`validation/language-release-gates.json`、`validation/product-gates.json`、`goal:workflow.md` 与实时 `goal:goal.md`。源码、各门结果与发布包校验绑定同一冻结提交收据；`alpha-release.yml` 与 `windows-alpha-release.yml` 的干净环境验证作业在不检出源码、不安装 Rust、不使用 Cargo 缓存的空机器上独立验证归档包安装与运行。
