# LLVM API主路线（210）

## 目标与实际路径

当前Wire21已实现语言统一走：已核LIR → 类型化机器IR → 结构化代码生成单元 → LLVMBuilder → LLVM结构核验 → 优化 → LLVM结构核验 → 对象字节 → 链接与发布。
标准CLI默认llvm-api；`--backend clang`保留基于同一机器IR的文本参考后端。没有直接Builder失败后的文本解析回退。

209的文本解析API只由显式`zil-llvm-api/text-reference`测试feature开放。标准CLI没有`LLVMParseIRInContext`动态符号引用。完整`.ll`旁置文件仍为诊断和参考输出生成，API及分区不读取或扫描它；因此不能宣称整个请求零文本渲染成本。

当前有限整数、数组、记录、枚举、错误联合、函数值、闭包、资源、Package、ByteBuffer、Store、切片和有限C ABI共用这条路径。未实现的语言能力继续明确拒绝，不能因后端切换扩大接受集。这是现有语言后端路径的完成，不等于整门语言、增量编译或产品门全部完成。

## 为什么这样分层

- 唯一语言lowering：`zil-backend-llvm/src/emit.rs`及`emit/`。原`bind_lir`核对保留，不让API重新解释语言责任和借用。
- 纯Rust schema：`zil-llvm-api/src/model.rs`。类型、指令、值、基本块和引用是数据；文本渲染不反向决定结构。符号仍是机器层名字，不冒称稳定跨请求身份。
- 构图前核验：`zil-llvm-api/src/preflight.rs`。核对名称、类型、常量、对齐、聚合/GEP索引、调用ABI、CFG终结、phi边与SSA支配，并生成构图顺序。只核对机器表示，不产生ZIL验证授权。
- 结构化分区：`zil-backend-llvm/src/machine_cgu.rs`。来源文件分组，本地链接定义与引用者共置，只导入真实使用的声明。单元取得函数体所有权，跨单元声明只复制签名；不克隆函数体再丢弃。
- 原生边界：`zil-llvm-api/src/native.rs`及私有`native/builder.rs`。每个单元有独立Context/Module/Builder/TargetMachine，先声明类型与符号，再构造定义和phi回边。原生句柄不在线程间传递，成功返回拥有的对象字节。
- 调度与发布：`zil-backend-llvm/src/build.rs`及driver。有界工人、统一链接、阶段取消与失败清理。所有API单元进入原生Builder前都执行完整机器预检；不存在另一份重复整模块预检。

LLVM verifier只负责LLVM约束，不能替代每次请求的独立ZIL Verify。语言安全、预算、溢出、清理顺序及ABI规则未改，不新增默认noalias或dereferenceable。目标和布局必须明确，不使用宿主布局冒充交叉目标。

## 安装与构建

实际验证的开发宿主为Linux x86_64，Rust 1.99.1、LLVM/Clang 17.0.6。动态库目录需有`libLLVM-17.so`和真实动态库。头文件、llvm-sys、Inkwell均不是本版构建依赖；Cargo依赖仅workspace。源码包不携带工具链。

```sh
export ZIL_LLVM_LIB_DIR=/opt/llvm-17.0.6/lib
export LD_LIBRARY_PATH="$ZIL_LLVM_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ZIL_CLANG=/opt/llvm-17.0.6/bin/clang
cargo build --locked --offline --release -p zil-cli
```

启用API的CLI装载时需要LLVM动态库，即使选择Clang参考。无LLVM库时显式构建参考版：

```sh
cargo build --locked --offline --release -p zil-cli --no-default-features
```

参考版请求API明确报错，不静默回退。这里的`--offline`适用于已备妥工具链和项目依赖的环境。

## 实际程序

```sh
mkdir -p out
./target/release/zil build examples/mainline/zui_language177/main.zil \
  -o out/state --clang "$ZIL_CLANG" --opt O0 \
  --codegen-units 8 --codegen-jobs 4 --json > out/state-build.json
printf '3' | ./out/state
```

输入为ASCII字符`3`（0x33），不是0x03；预期输出`DEFHZMNOPK`、退出0。它真实使用拥有Store的State、元素只读窗口和保存回调。`verification_runs=1`，`native_codegen.object_reports[*].input_kind="builder"`且`parse_ns=0`记录实际路径；`commands`只记录真正的链接进程。

优化构建先保持单模块：

```sh
./target/release/zil build examples/mainline/zui_language177/main.zil \
  -o out/state-o2 --clang "$ZIL_CLANG" --opt O2 --codegen-units 1
```

206已观察到的O2多单元运行性能退化没有因为Builder接入而消失。默认并行参数和pipeline不在210改动范围。

## 回归和复现

```sh
cargo test --workspace --locked --offline
cargo test --workspace --locked --offline --no-default-features
cargo test --locked --offline -p zil-llvm-api --features text-reference
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
python3 tools/research/208/validate.py --repo . --cli target/release/zil \
  --clang "$ZIL_CLANG" --out out/api-consumer-new --source-commit <交付提交>
```

`--out`必须不存在；无Git源码快照用交付清单提供的提交身份，该身份不冒充现场Git查询。消费者探针包装的是API生成的对象，不把旁置`.ll`重新用Clang编译。

`tools/research/210/differential.py`对209/210的63个现有入口做O0/O2、单/多CGU原生差分。`no-parser.c`用于拦截旧解析API，有209触发失败作为正控制。`bench.py`冻结二进制和工作负载后测完整构建，首次负面结果和修正后的新窗口分别保留。

## 边界

- 实测Linux原生构建运行；Win64已验证COFF对象与PE交叉链接，没有Windows宿主安装或Windows本机运行验收。Store仍遵守原有Linux目标限制。
- 机器预检不使任意恶意IR成为可信输入，也不隔离LLVM本身的原生错误。原生unsafe仅在登记的适配边界；Verify继续禁止unsafe。
- 取消是阶段间协作检查，不能立即打断LLVM pass或链接器。失败不会返回旧成功产物作为本次结果；崩溃时暂存目录可能残留。
- LLVM前后结构核验、对象生成和链接均可失败，任何失败都停止发布。
- 对象缓存、稳定符号/匿名项身份、按项查询、ThinLTO属于后续专项，不是本轮已完成功能。
- 新性能结果仅适用于冻结场景、工具链和资源；没有完成产品级性能门、非作者独立审核或远端CI。
