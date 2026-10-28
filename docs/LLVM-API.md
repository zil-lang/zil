# LLVM API主路线（210）
## 当前路径
已核LIR → 类型化机器IR → 结构化CGU → LLVMBuilder → LLVM结构核验 → 优化 → LLVM结构核验 → 对象 → 链接。
标准CLI默认llvm-api；`--backend clang`保留共用机器IR的文本参考后端。
209的文本解析API仅在`zil-llvm-api/text-reference`测试feature启用；标准构建不引用LLVMParseIRInContext。
`.ll`旁置文件仅为诊断/对照输出，API和分区不读取或扫描它；当前仍支付这份诊断渲染成本。
### 实现位置
- `zil-backend-llvm/src/emit*.rs`：唯一语言lowering，原bind_lir核对不变；产出拥有的类型/指令/控制流和符号引用。
- `zil-llvm-api/src/model.rs`：纯Rust机器schema及文本诊断渲染，无原生指针，无程序集解析。
- `zil-llvm-api/src/preflight.rs`：类型/名称/ABI/CFG/SSA结构检查与构图调度；不产生ZIL安全授权。
- `zil-backend-llvm/src/machine_cgu.rs`：按来源分组、局部链接引用合并、直接声明裁剪；旧文本扫描器只在cfg(test)保留。
- `zil-llvm-api/src/native/builder.rs`：唯一逐操作LLVMBuilder适配，Context内缓存类型，先声明后定义、预建phi后填回边。
- `zil-llvm-api/src/native.rs`：目标/布局、上下文生命周期、优化、对象字节及失败处理。
### 支持和限制
Linux x64开发宿主，LLVM17.0.6；x64 Linux/Win64目标原有profile。Win64交叉生成不代表Windows本机验收。
当前有限整数、数组/记录/枚举/错误联合、闭包/资源/Package/ByteBuffer/Store、切片和有限C ABI都使用同一个结构化入口。未实现语言继续明确拒绝，不因后端切换扩大接受集。
没有直接Builder的文本回退；构图输入不合法返回错误。LLVM verifier不替代独立ZIL Verify。没有新增noalias/dereferenceable。
O0/O2仍使用既定优化pipeline；O2多CGU已有的运行性能限制不因Builder自动消失。
取消为阶段间协作检查，不能打断LLVM pass。原生LLVM bug不受本适配层沙箱隔离。
对象缓存、稳定符号、增量查询和ThinLTO属于下一专项，不是本轮已完成能力。
## 构建
沿用209的LLVM17.0.6、Rust1.99.1环境，标准CLI自动启用API。`cargo build --release --locked -p zil-cli`。
无LLVM库的参考构建：`cargo build --release --locked -p zil-cli --no-default-features`。
显式文本API差分测试：`cargo test -p zil-llvm-api --features text-reference`，不建议在发布CLI开启。
## 原安装与平台说明（209沿用，阶段说明以本页上段为准）
# LLVM API：主线安装、使用与边界（209）

## 当前主线策略

语言174–177/Wire21与编译专项204/206/207的实际集成版本。标准CLI默认在 **ZIL编译器进程内部** 调用LLVM17 C API生成对象，然后由Clang驱动Linux链接（Win64用显式lld-link）。它不是Python探针，不把ZIL转为C，也不把单元交给Clang编译。

本版仍走 **已核LIR → 共用发射器产生LLVM文本 → 内存解析API → 对象**，不是直接LLVMBuilder；文本生成/解析成本仍存在。Clang参考路径及默认单模块策略保留。新设计的来源投影精化、稳定符号、结构化CGU、对象缓存和ThinLTO不在本版完成范围。

## 安装

实测构建宿主为Linux x86_64；Rust 1.99.1、libLLVM **17.0.6**、Clang **17.0.6**。动态库目录需要同时有供链接使用的`libLLVM-17.so`及其真实动态库。头文件、llvm-sys、Inkwell均不是本版构建依赖。Cargo依赖仅本workspace；LLVM及系统链接器是外部可信工具链，源码包不重新携带它们。

```sh
# 改为本机安装位置；必须是LLVM 17.0.6，而不是Rust自己附带的LLVM。
export ZIL_LLVM_LIB_DIR=/opt/llvm-17.0.6/lib
export LD_LIBRARY_PATH="$ZIL_LLVM_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export ZIL_CLANG=/opt/llvm-17.0.6/bin/clang

cargo build --locked --offline --release -p zil-cli
```

LLVM动态库参与本版CLI的装载；启用feature构建的CLI即使选择Clang也需能加载该库。不需要API时，显式执行`cargo build --locked --offline --release -p zil-cli --no-default-features`，该参考构建没有libLLVM链接依赖；在它上面请求`--backend llvm-api`会得到`E-LLVM-API-UNAVAILABLE`，不会静默回退。

## 第一个实际程序

```sh
mkdir -p out
./target/release/zil build examples/mainline/zui_language177/main.zil   -o out/state-api --backend llvm-api --clang "$ZIL_CLANG" --opt O0   --codegen-units 8 --codegen-jobs 4 --json > out/state-api-build.json
printf '3' | ./out/state-api
```

输入是**ASCII字符`3`（字节0x33）**；0x03不是合法输入，会以9退出。209核对真实源码后修正了208说明中的输入错误。预期输出`DEFHZMNOPK`，退出码0。程序真实使用拥有Store的State、实际元素共享读取及可保存回调。JSON里的`verification_runs=1`、`native_codegen.backend="llvm-api"`与对象receipt是实际链路记录；`commands`只有真正的链接命令，不伪造LLVM API为shell命令。

优化构建建议先保留单模块：

```sh
./target/release/zil build examples/mainline/zui_language177/main.zil   -o out/state-api-o2 --backend llvm-api --clang "$ZIL_CLANG" --opt O2   --codegen-units 1 --codegen-jobs 1
```

O2分块可显式选择，但206已观察到的跨单元运行性能退化尚未修复，不能因为API可用就改成默认多单元。`--backend clang`使用同一已核LIR和现有Clang参考实现；本版没有宣称两个LLVM pipeline逐pass等价。

## 回归与真实分配失败注入

```sh
cargo test --workspace --locked --offline --features zil-cli/llvm-api -- --test-threads=4
cargo test --workspace --locked --offline --no-default-features -- --test-threads=4
python3 tools/research/208/validate.py --repo . --cli target/release/zil   --clang "$ZIL_CLANG" --out out/api-consumer-new   --source-commit <本包SOURCE-208.json中的source_commit>
```

`--out`必须不存在；Git工作树中可省略`--source-commit`。源码ZIP不含Git数据库时明确给出交付身份，脚本记录其为用户提供身份，而不是伪装成现场Git查询。测试oracle来自177已存在的业务场景。探针只给 **API已生成的对象** 加入分配/释放包装并链接，不把发布`.ll`重新交给Clang编译冒充API验证。

## 支持和失败边界

- 当前生产验证仍按177的语言profile执行；Store仍只支持checked Linux x64 LLVM。API不会开启新语义或默认`noalias`。
- Win64已实测COFF对象和带显式`kernel32`导入库的PE链接；**没有Windows原生运行验证**。不宣称Windows宿主上的编译器安装已验收。
- 每CGU独立Context/Module/TargetMachine，输出为拥有对象字节；统一初始化一次。新`unsafe`仅在登记的`zil-llvm-api/src/native.rs`，Verify和原有安全crate保持禁止unsafe。
- 解析、LLVM结构核验、版本、目标布局、优化、对象或链接失败都返回错误。取消在阶段边界协作检查；不能中断正在运行的LLVM pass或链接器。CLI/driver不将旧成功文件返回为新成功，也不发布失败半成品。
- LLVM结构核验不代替ZIL独立Verify。输入是可信编译器产生的IR；不把LLVM当恶意IR隔离沙箱。崩溃时私有暂存可能残留，不产生成功收据。
- 208是作者实测的可选后端；209按用户要求选择为CLI研发默认，非作者审核尚未进行；完整语言NotSatisfied、产品NotRun、B0 Doing不升级。

## 209策略差异

标准Cargo CLI特性现在默认启用`llvm-api`。发行CLI的默认策略在CLI入口选择：编译时有该特性则为API；显式`--backend clang`覆盖选择。底层库`CompileOptions::default()`仍为Clang，嵌入者应显式选择策略，避免Cargo特性统一改变库调用者的隐式选择。没有API特性的参考CLI默认Clang，显式请求API仍拒绝。`zil version`报告实际默认策略。

LLVM库是进程装载依赖：安装错误可能在CLI启动前由动态加载器报告，不能宣传成全部都有ZIL结构化错误。`tools/llvm/check_toolchain.py`用于构建前核对库与Clang版本，不能替代编译器内的版本和目标检查。
