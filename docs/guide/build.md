# 构建与 LLVM API 路径

## 编译路径

已核语言编译走：已核LIR → 类型化机器IR → 结构化代码生成单元 → LLVMBuilder → LLVM结构核验 → 优化 → LLVM结构核验 → 对象字节 → 链接与发布。
标准CLI默认llvm-api；`--backend clang`保留基于同一机器IR的文本参考后端。没有直接Builder失败后的文本解析回退。

文本解析参考 API只由显式`zil-llvm-api/text-reference`测试feature开放。标准CLI没有`LLVMParseIRInContext`动态符号引用。完整`.ll`旁置文件仍为诊断和参考输出生成，API及分区不读取或扫描它；因此不能宣称整个请求零文本渲染成本。

当前有限整数、数组、记录、枚举、错误联合、函数值、闭包、资源、Package、ByteBuffer、Store、切片和有限C ABI共用这条路径。未实现的语言能力继续明确拒绝，不能因后端切换扩大接受集。这是现有语言后端路径的完成，不等于整门语言、增量编译或产品门全部完成。

## 为什么这样分层

- 唯一语言lowering：`zil-backend-llvm/src/emit.rs`及`emit/`。原`bind_lir`核对保留，不让API重新解释语言责任和借用。
- 纯Rust schema：`zil-llvm-api/src/model.rs`。类型、指令、值、基本块和引用是数据；文本渲染不反向决定结构。符号仍是机器层名字，不冒称稳定跨请求身份。
- 构图前核验：`zil-llvm-api/src/preflight.rs`。核对名称、类型、常量、对齐、聚合/GEP索引、调用ABI、CFG终结、phi边与SSA支配，并生成构图顺序。只核对机器表示，不产生ZIL验证授权。
- 结构化分区：`zil-backend-llvm/src/machine_cgu.rs`。来源文件分组，本地链接定义与引用者共置，只导入真实使用的声明。单元取得函数体所有权，跨单元声明只复制签名；不克隆函数体再丢弃。
- 原生边界：`zil-llvm-api/src/native.rs`及私有`native/builder.rs`。每个单元有独立Context/Module/Builder/TargetMachine，先声明类型与符号，再构造定义和phi回边。原生句柄不在线程间传递，成功返回拥有的对象字节。
- 调度与发布：`zil-backend-llvm/src/build.rs`及driver。有界工人、统一链接、阶段取消与失败清理。所有API单元进入原生Builder前都执行完整机器预检；不存在另一份重复整模块预检。

普通已核Package达到4096字节时，后端使用私有完整值快照及既有`.lc1`生成函数约定：大参数先复制到callee自己的局部载体，大结果写入caller的独立结果槽。构造与字段读写直接消费目标布局内存，所有快照槽在入口分配；整值移动和字段take/refill仍按已核IR更新原有清理责任，字节复制不授予语言Copy。拥有快照不参加Copy槽复用。具名Closure环境、特殊helper及外部ABI仍须各自完整适配；此路径不授予新的语言组合资格，也不提高默认限额。

普通具体Closure的已核按值参数或结果若达到该阈值且物理布局完整，调用复用同一快照约定：真实环境参数在前，其后为大结果的隐藏out指针，再按源参数位置映射实参；callee仍先将大参数复制到自己的局部载体。Fn、FnMut、FnOnce保留原环境访问、消费与清理规则，小签名保留原调用约定。此路径不扩展环境布局、捕获权限或opaque工厂、泛型helper的签名准入；登记、外部回调、线程和同步单元的特殊桥接继续按各自完整ABI边界检查。

LLVM verifier只负责LLVM约束，不能替代每次请求的独立ZIL Verify。语言安全、预算、溢出、清理顺序及ABI规则未改，不新增默认noalias或dereferenceable。目标和布局必须明确，不使用宿主布局冒充交叉目标。

## 安装与构建

支持的构建环境为 Linux x86_64 宿主、Rust 1.99.1、LLVM/Clang/LLD 23.1.4。动态库目录需有`libLLVM-23.so`及其真实动态库，运行时拒绝错误补丁版本。头文件、llvm-sys、Inkwell均不是本版构建依赖；Cargo依赖仅workspace。源码包不携带工具链。

```sh
export ZIL_LLVM_LIB_DIR=/opt/llvm-23.1.4/lib
export LD_LIBRARY_PATH="$ZIL_LLVM_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ZIL_CLANG=/opt/llvm-23.1.4/bin/clang
export PATH=/opt/llvm-23.1.4/bin:$PATH
python3 tools/llvm/check_toolchain.py
cargo build --locked --offline --release -p zil-cli
```

启用API的CLI装载时需要LLVM动态库，即使选择Clang参考。无LLVM库时显式构建参考版：

```sh
cargo build --locked --offline --release -p zil-cli --no-default-features
```

参考版请求API明确报错，不静默回退。这里的`--offline`适用于已备妥工具链和项目依赖的环境。

官方固定包安装入口为`python3 tools/llvm/install.py --prefix /absolute/new/prefix`。
该入口校验完整发布包摘要，安装所需工具与Clang资源，并按上游llvm-shlib的方式
从官方PIC静态组件链接X86共享库；它是本地重链接产物，不是官方预编译DSO。
生成`env.sh`和记录静态组件、系统运行库及链接命令的安装收据。
需要Python 3.11+、tar、zstd、binutils和Ubuntu 24.04的C++/zlib/zstd/libxml2运行环境。
先按`rust-toolchain.toml`准备Rust，加载安装前缀的`env.sh`后执行checker。
历史LLVM17证据仍属于旧profile；Rust自身LLVM23.1.1不替代ZIL独立DSO。

## 实际程序

```sh
mkdir -p out
./target/release/zil build examples/mainline/zui-language-store-fields/main.zil \
  -o out/state --clang "$ZIL_CLANG" --opt O0 \
  --codegen-units 8 --codegen-jobs 4 --json > out/state-build.json
printf '3' | ./out/state
```

输入为ASCII字符`3`（0x33），不是0x03；预期输出`DEFHZMNOPK`、退出0。它真实使用拥有Store的State、元素只读窗口和保存回调。`verification_runs=1`，`native_codegen.object_reports[*].input_kind="builder"`且`parse_ns=0`记录实际路径；`commands`只记录真正的链接进程。

优化构建先保持单模块：

```sh
./target/release/zil build examples/mainline/zui-language-store-fields/main.zil \
  -o out/state-o2 --clang "$ZIL_CLANG" --opt O2 --codegen-units 1
```

历史多 CGU O2 性能观察不因 Builder 路径而自动改善；性能需按冻结源码和当前工具链重新测量。

## 增量 O2 分区策略

`--incremental-o2`是显式优化策略，不改变默认单元数、任务数或缓存开关。完整直接调用闭包导入可提高跨函数优化机会，却也会重复优化函数并扩大编辑失效范围；因此它与保持普通分区能力分开决定。

仅当完整闭包的总复制成本超限时，丢弃未发布的整个导入计划，移动原始机器模块，重新执行同一单元数下的普通分区规划；不截断闭包、不部分提交、不提高复制预算。成功的完整导入路径不变；重复符号、声明不合法、本地状态身份等其他拒绝仍沿原路径处理。普通分区仍共置本地链接定义及引用者、只保留必需声明，每个规范函数定义只有一个所有者；当前独立Verify和各单元机器预检不因回退或缓存命中而省略。

这是编译成本与跨单元优化机会之间的取舍，不保证任意程序运行更快。评估必须同时包含生成代码的运行质量，以及按预先声明的场景频率加权的完整请求等待时间和绝对节省时间；不能平均提速百分比，不能把少数合成语料或假设权重当作真实用户分布。无修改构建、函数体/接口编辑、冷构建及重负载退化须分别保留。

## 显式对象复用

在上面的build/run命令追加`--object-cache .zil/native-objects`。首次构建仍生成对象，并在整次
请求最终发布成功后原子保存；后续相同单元命中时运行当前Verify与机器预检，不再执行Builder、
LLVM优化及机器码生成，但仍执行Lower、诊断IR与链接。

`native_codegen.object_cache`分别报告命中、缺失、损坏重建与写入失败；命中的对象记录为
`input_kind="object-cache"`，明确`llvm_verification_this_request=false`，不伪装为新的LLVM核验。
该路径依赖可信本机缓存和执行期间不可变的工具安装，不面向不可信/共享网络缓存。
`--cache-dir`仅为既有未核源候选缓存，不隐式启用对象复用。细节见[冻结机制说明](https://github.com/zil-lang/zil/blob/28b3e37bb4d8a1fe8b146c72cf891fc99de32440/docs/research/native-object-cache/cache.md)。

## 回归和复现

```sh
cargo test --workspace --locked --offline
cargo test --workspace --locked --offline --no-default-features
cargo test --locked --offline -p zil-llvm-api --features text-reference
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
python3 tools/research/native-api-validation/validate.py --repo . --cli target/release/zil \
  --clang "$ZIL_CLANG" --out out/api-consumer-new --source-commit <交付提交>
```

`--out`必须不存在；无Git源码快照用交付清单提供的提交身份，该身份不冒充现场Git查询。消费者探针包装的是API生成的对象，不把旁置`.ll`重新用Clang编译。

`tools/research/compiler-differential/differential.py` 对登记的编译入口做 O0/O2、单/多 CGU 原生差分；`no-parser.c` 是旧文本解析路径的失败正控制。测完整构建时冻结二进制和工作负载，分别保留首次失败和修正后结果。

## 边界

- 实测Linux原生构建运行；Win64已验证COFF对象与PE交叉链接，没有Windows宿主安装或Windows本机运行验收。Store仍遵守原有Linux目标限制。
- 机器预检不使任意恶意IR成为可信输入，也不隔离LLVM本身的原生错误。原生unsafe仅在登记的适配边界；Verify继续禁止unsafe。
- 取消是阶段间协作检查，不能立即打断LLVM pass或链接器。失败不会返回旧成功产物作为本次结果；崩溃时暂存目录可能残留。
- LLVM前后结构核验、对象生成和链接均可失败，任何失败都停止发布。
- 显式可信本地对象缓存；稳定符号/匿名项身份、按项查询、ThinLTO仍属于后续专项。
- 新性能结果仅适用于冻结场景、工具链和资源；没有完成产品级性能门、非作者独立审核或远端CI。
