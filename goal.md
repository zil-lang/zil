# ZIL 语言功能全景与 Alpha 路线图（Goal）

> 更新：2026-11-25（Asia/Shanghai） · 沿用全仓统一的 **`G1`–`G10` 功能域编号**（直接对齐历史提交、PR、规范与 `validation/language-release-gates.json` 的 12 个基础能力门，不再引入第二套编号），采用「总览索引 → 分域已完成/待完成明细 → 8 类日常程序综合验收 → 非 Alpha 阻断专项」结构。
>
> 规范权威：[`_zil_design.md`](https://github.com/zil-lang/zil/blob/main/_zil_design.md) · [`docs/guide/supported-language.md`](https://github.com/zil-lang/zil/blob/main/docs/guide/supported-language.md) · [`validation/language-release-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/language-release-gates.json) · [`validation/language-alpha-profile.json`](https://github.com/zil-lang/zil/blob/main/validation/language-alpha-profile.json) · [`validation/product-gates.json`](https://github.com/zil-lang/zil/blob/main/validation/product-gates.json)  
> 协作规则：[工作流规则](workflow.md) · [源码与恢复参考](README.md) · [分支与备份规则](AGENTS.md) · 历史快照：[`goal@e8faf57b2`](https://github.com/zil-lang/zil/blob/e8faf57b2/goal.md)

---

## 0. 执行摘要与已完成合入总表

- **首个正式发布的 Linux x86_64 Pre-release**：[**`v1.0.0-alpha.1`**](https://github.com/zil-lang/zil/releases/tag/v1.0.0-alpha.1)（绑定提交 `main@719ea01` / `719ea01d979f1f34e9fee0f72bb8c1481c594835`，标记 `--prerelease --latest=false`）：
  - **同提交 12+8 全门冻结验收**（[`alpha-acceptance.yml` #45754945988](https://github.com/zil-lang/zil/actions/runs/45754945988)，6m11s 全绿）：Native API State/Store `16/128/56` ✅、Fixed Projects `21/21`（`152/152 required`，`196 historical`，`44 archival due to lost stdin`）✅、Roam `42/42` ✅、Roam `24/24` ✅、**Everyday Apps `8/8`（16 项双后端测试）** ✅、**Basic-Language Capabilities `12/12`（217 项双后端测试）** ✅。
  - **同提交自动打包 + 空环境独立验证 + 门禁发布**（[`alpha-release.yml` #45755676030](https://github.com/zil-lang/zil/actions/runs/45755676030)，打包 3m02s + 空环境验证 40s + 门禁发布 20s 全绿）：已自动验证同提交验收绿灯并上传 4 份发行资产（`zil-v1.0.0-alpha.1-linux-x86_64.tar.gz`、`RELEASE-MANIFEST.json`、`SHA256SUMS.txt`、`clean-env-verify-receipt.json`），全新无源码/无 Rust `ubuntu-24.04` 空环境验证 `zil 1.0.0-alpha.1 (ZIL-LANG-1.4; Rust 2024; ZIL 1.0 Alpha; default backend: llvm-api)` 全部通过 ✅。
- **当前主线 HEAD & Windows x64 完整 Alpha 闭环**：`main@278c9c2`（**`G10.4` Windows x64 原生支持 W1–W5 已全部合入并全绿**，正式达成 **Windows x64 完整 Alpha 支持** 里程碑，与 Linux x86_64 保持同一套语言语义与 `zil-check` 独立核验）：
  - **W1**（[PR #163](https://github.com/zil-lang/zil/pull/163) `fa0a904`，[`#45756737871`](https://github.com/zil-lang/zil/actions/runs/45756737871) 1m33s 全绿）：目标管线 + COFF/PE 构建链接 + 真实 `windows-latest` CI。
  - **W2**（[PR #164](https://github.com/zil-lang/zil/pull/164) `894ce85`，[`#45765507517`](https://github.com/zil-lang/zil/actions/runs/45765507517) 58s 全绿）：多单元 COFF 零 `LNK2005` + MSVC x64 ABI 全矩阵 + 64 KiB `__chkstk` + 同步 `Registration` + 5 序原子 + Win32 `HeapAlloc` OOM 零泄漏清理。
  - **W3**（[PR #165](https://github.com/zil-lang/zil/pull/165) `0297b56`，[`#45769026928`](https://github.com/zil-lang/zil/actions/runs/45769026928) 1m17s 全绿）：Win32 `SRWLOCK` `Mutex<T>`（20,000 次跨线程争用）、UCRT `_beginthreadex` `ThreadScope`、Win32 Fiber `Task`（`FIBER_FLAG_FLOAT_SWITCH` 浮点保持、`PendingExit` 单次 Drop、嵌套子任务取消、40 轮压力与跨线程 Fiber 迁移）及真实 Win32 后台线程 `AsyncRegistration` 五态 CAS 晚通知排空。
  - **W4**（[PR #166](https://github.com/zil-lang/zil/pull/166) `1574f69`）：按目标选择 7 个 Windows 标准库模块（`lib/std/windows/{io,fs,path,env,time,net,float}.zil`，二进制模式 `_O_BINARY`/`_setmode`、Win32 `GetFileAttributesExA`/`MoveFileExA`/`FindFirstFileA`、盘符/UNC 路径、Winsock2 TCP + `-lws2_32`、UCRT C-locale 浮点格式化）并原生跑通全部 8 类日常应用。
  - **W5**（[PR #167](https://github.com/zil-lang/zil/pull/167) `7f7c9e9` + [PR #168](https://github.com/zil-lang/zil/pull/168) `278c9c2`）：固定官方 LLVM 23.1.4 Windows SDK（`LLVM-C.dll` + `LLVM-C.lib` + `clang.exe`，`LLVMGetVersion == [23, 1, 4]`）、`zil.exe` 默认 `llvm-api` 后端与 `GetModuleHandleW("LLVM-C.dll")` 缓存身份校验、确定性 `zil-v1.0.0-alpha.1-windows-x86_64.zip` 打包（`29,125,620` 字节，`sha256: 6f73fe053220c0e7344e650f6b196f4f02f39e51e6c1356396b7db6a338db39f`，46 文件）及真机端到端 [`windows-alpha-release.yml` #45780581400](https://github.com/zil-lang/zil/actions/runs/45780581400) 双作业全绿（`package-windows-x86_64` 6m19s：**12/12 基础能力 + 8/8 日常应用** `Clang` + `LLVM-C.dll` 双后端 O0/O2 全通过；`verify-clean-env-windows` 19s：无源码 checkout / 无 Rust / 无 `ZIL_LLVM_LIB_DIR` 干净 Windows 环境独立安装与执行全通过 ✅）。
- **持续推进边界**：Linux 与 Windows x64 Alpha 均保持强制零退化回归基线；后续独立推进 `G10.5` 编译性能专项、`G9.4–G9.6` ZUI 或正式 Windows Release 资产挂接。

### 本轮已完成并合入 `main` 的全部 PR（按合入顺序）

| 序号 | 事项 | 关联编号 | PR / 提交 | 结果 |
| --- | --- | --- | --- | --- |
| 1 | Alpha 合同 12+8 对齐（`validation/language-alpha-profile.json`） | `G10.7` | [PR #138](https://github.com/zil-lang/zil/pull/138) (`27b5da5`) | ✅ 已合入 `main` |
| 2 | 留证瘦身（清 20 份过程 JSON）+ 分层缓存 CI + `tools/dev-check.py` | `G10.1` | [PR #140](https://github.com/zil-lang/zil/pull/140) (`edf80be`) | ✅ 已合入 `main`（CI `documentation` 11s / `workspace` ~3–4m） |
| 3 | G6.3-B `Replace` 状态协议联合语义（`zil-semantics::protocol` 单一权威） | `G6.3-B` | [PR #139](https://github.com/zil-lang/zil/pull/139) (`6d4708d`) | ✅ 已合入 `main` |
| 4 | `zil-check` 终态模块化 + 合同测试拆分为并行二进制 | `G6.2` / `G10.1` | [PR #142](https://github.com/zil-lang/zil/pull/142) (`c52cfdb`) | ✅ 已合入 `main` |
| 5 | `std.env` CLI 参数（`arg_count`/`arg`/`args`）+ `std.string` 高频辅助（`split`/`lines`/`trim*`/`starts_with`/`ends_with`/`contains`/`replace_all`） | `G9.1` / `G9.3b` | [PR #143](https://github.com/zil-lang/zil/pull/143) (`1c03a10`) | ✅ 已合入 `main` |
| 6 | `std.fs` 目录/元数据/重命名（`metadata`/`is_dir`/`is_file`/`file_size`/`mkdir*`/`remove_dir`/`rename`/`read_dir`）+ `std.path` | `G9.3b` | [PR #144](https://github.com/zil-lang/zil/pull/144) (`fd5cc7d`) | ✅ 已合入 `main` |
| 7 | G6.3-C `Store` 状态协议联合语义（Push/Pop 成败恢复 + 非修改借用） | `G6.3-C` | [PR #145](https://github.com/zil-lang/zil/pull/145) (`bf2dbda`) | ✅ 已合入 `main` |
| 8 | `std.net` POSIX TCP Socket（`TcpListener`/`TcpStream`）+ 固有 `impl` `result(create)` 与 `&mut Managed -> &mut Desc` 修复 | `G9.3b` | [PR #146](https://github.com/zil-lang/zil/pull/146) (`58eabc8`) | ✅ 已合入 `main` |
| 9 | 8 类日常应用 1–4（文本统计与文件搜索、CSV、JSON、CLI Todo）+ OOM 注入清理与配对安全反例 | `G10.7` | [PR #148](https://github.com/zil-lang/zil/pull/148) (`51f9b33`) | ✅ 已合入 `main` |
| 10 | G5.3 `dyn Trait<'a>` 生命周期参数方法返回 region 替换 + `DynImpl` 独立区域健全性闭环 | `G5.3` | [PR #147](https://github.com/zil-lang/zil/pull/147) (`c4719c7`) | ✅ 已合入 `main` |
| 11 | 8 类日常应用 5–7（HTTP 小服务与客户端、文件批处理、图依赖分析器）+ 配对安全反例 | `G10.7` | [PR #149](https://github.com/zil-lang/zil/pull/149) (`76db951`) | ✅ 已合入 `main` |
| 12 | G6.3-D `Task` / `Registration` 条件创建、`Valid` 只读语义、`TaskStart` 完整定义与 `TaskValid` 代码生成修复 | `G6.3-D` | [PR #150](https://github.com/zil-lang/zil/pull/150) (`3d5c309`) | ✅ 已合入 `main` |
| 13 | G6.3-E `Cancel` 边状态协议联合语义与嵌套可挂起调用确定性取消展开 | `G6.3-E` | [PR #151](https://github.com/zil-lang/zil/pull/151) (`04225ae`) | ✅ 已合入 `main` |
| 14 | 8 类日常应用 8（异步任务调度器 `async-task-scheduler`）+ 40 轮取消压力与配对安全反例 | `G10.7` | [PR #152](https://github.com/zil-lang/zil/pull/152) (`7236b35`) | ✅ 已合入 `main` |
| 15 | 同步 `docs/guide/supported-language.md` 反映全部已合入 Alpha 能力与 8 类日常应用 | `G10.1` | [PR #153](https://github.com/zil-lang/zil/pull/153) (`1f67627`) | ✅ 已合入 `main` |
| 16 | 修复 `alpha-acceptance.yml` 浅克隆按需拉取 Fixed Projects 历史提交 | `G10.7` | [PR #154](https://github.com/zil-lang/zil/pull/154) (`2100daa`) | ✅ 已合入 `main` |
| 17 | 修复 JSON UTF-16 代理对解码与控制字符转义、补强 CSV RFC 4180 与 HTTP/1.1 11 项协议边界 | `G10.7` | [PR #155](https://github.com/zil-lang/zil/pull/155) (`953507d`) | ✅ 已合入 `main` |
| 18 | 修复 `zil-semantics` 泛型 lending trait 方法与外层具名引用参数语义，新增多输入借入/泛型 lending 全量双后端与 Raw/Wire 套件 | `G3.4` | [PR #156](https://github.com/zil-lang/zil/pull/156) (`b6385be`) | ✅ 已合入 `main` |
| 19 | 新增 Linux x86_64 确定性发布打包、`alpha-release.yml` 无源码/无 Rust 空环境验证与 `supported-language.md` 三分类核对表 | `G10.0` / `G10.7` | [PR #157](https://github.com/zil-lang/zil/pull/157) (`bc4fd51`) | ✅ 已合入 `main` |
| 20 | 升级 `alpha-acceptance.yml` 与 `alpha_acceptance_summary.py` 强制 fail-closed 核验 12/12 能力族 + 8/8 日常应用 + 诚实 Fixed Projects 分母 | `G10.7` | [PR #158](https://github.com/zil-lang/zil/pull/158) (`c48fae4`) | ✅ 已合入 `main` |
| 21 | 修复发布归档 `libLLVM.so.23.1` ELF SONAME + `libLLVM-23.so` 软链与 `Zil.toml` 项目目录验证 | `G10.0` | [PR #159](https://github.com/zil-lang/zil/pull/159) (`c63b3df`) | ✅ 已合入 `main` |
| 22 | 发布打包自动为 8 个日常样例生成 `Zil.lock`，空环境验证覆盖 `zil lock` + `zil build` 全链路 | `G10.0` | [PR #160](https://github.com/zil-lang/zil/pull/160) (`99058c1`) | ✅ 已合入 `main` |
| 23 | 统一版本号为 `1.0.0-alpha.1`、标记 `--prerelease --latest=false`、增加同提交 `alpha-acceptance.yml` 强制门禁 | `G10.0` / `G10.7` | [PR #161](https://github.com/zil-lang/zil/pull/161) (`b71938f`) | ✅ 已合入 `main` |
| 24 | 为无 checkout 发布作业补齐 `gh release create --repo`，自动签发 [`v1.0.0-alpha.1`](https://github.com/zil-lang/zil/releases/tag/v1.0.0-alpha.1) | `G10.0` | [PR #162](https://github.com/zil-lang/zil/pull/162) (`719ea01`) | ✅ 已合入 `main` + 正式 Pre-release 上线（[#45754945988](https://github.com/zil-lang/zil/actions/runs/45754945988) & [#45755676030](https://github.com/zil-lang/zil/actions/runs/45755676030)） |
| 25 | **Win64 W1**：全链路接通 `TargetProfile`/`WindowsLinkOptions`、MSVC COFF/PE 构建链接、W3 未实现并发显式 `Unsupported` 与真实 `windows-latest` 原生 CI | `G10.4`（W1） | [PR #163](https://github.com/zil-lang/zil/pull/163) (`fa0a904`) | ✅ 已合入 `main` + 真实 `windows-latest` 全绿（[#45756737871](https://github.com/zil-lang/zil/actions/runs/45756737871)） |
| 26 | **Win64 W2**：多单元 COFF 链接去重（零 `LNK2005`）、MSVC x64 ABI 全矩阵（16 参 shadow space/NaN/POD/10 参 `CCallback`/64 KiB `__chkstk`）、同步 `Registration`、5 序原子与 Win32 `HeapAlloc` OOM 零泄漏清理 | `G10.4`（W2） | [PR #164](https://github.com/zil-lang/zil/pull/164) (`894ce85`) | ✅ 已合入 `main` + 真实 `windows-latest` 全绿（[#45765507517](https://github.com/zil-lang/zil/actions/runs/45765507517)） |
| 27 | **Win64 W3**：Win32 `SRWLOCK` `Mutex<T>`、UCRT `_beginthreadex` `ThreadScope`、Win32 Fiber `Task`（`FIBER_FLAG_FLOAT_SWITCH`/`PendingExit`/嵌套取消/40 轮压力/跨线程迁移）与真实 Win32 后台线程 `AsyncRegistration` | `G10.4`（W3） | [PR #165](https://github.com/zil-lang/zil/pull/165) (`0297b56`) | ✅ 已合入 `main` + 真实 `windows-latest` 全绿（[#45769026928](https://github.com/zil-lang/zil/actions/runs/45769026928)） |
| 28 | **Win64 W4**：7 个 Windows 标准库模块（Win32/UCRT/Winsock2 `lib/std/windows/`）、`-lws2_32`、按目标内嵌信任根与全部 8 类日常应用 Windows 原生执行 | `G10.4`（W4） | [PR #166](https://github.com/zil-lang/zil/pull/166) (`1574f69`) | ✅ 已合入 `main` |
| 29 | **Win64 W5**：官方 LLVM 23.1.4 `LLVM-C.dll`/`LLVM-C.lib` 双后端（`Clang` + `LlvmApi` × O0/O2）、`windows-native.yml` 双后端门禁、`run_alpha_profile_suites.py --platform windows`、`zil-v*-windows-x86_64.zip` 确定性打包与 `windows-alpha-release.yml` 干净环境验收（含 `path.zil` UNC 与 `stdlib_fs_and_path_native` 修复） | `G10.4`（W5） | [PR #167](https://github.com/zil-lang/zil/pull/167) (`7f7c9e9`) | ✅ 已合入 `main` + 真实 `windows-latest` 双后端全绿（[#45777819709](https://github.com/zil-lang/zil/actions/runs/45777819709)） |
| 30 | 修复 `l2_borrowed_dyn_provider_native` 在 Windows 32 位完整退出码（`400_000`）断言，闭环端到端 **12/12 + 8/8 双后端 + 干净环境** Windows Alpha 验收 | `G10.4`（W5） | [PR #168](https://github.com/zil-lang/zil/pull/168) (`278c9c2`) | ✅ 已合入 `main` + 端到端发行与空环境验收全绿（[#45780581400](https://github.com/zil-lang/zil/actions/runs/45780581400)） |

---

## 1. 语言功能全景总览（统一 `G1`–`G10` 索引）

状态图例：`✅ 已闭环`｜`🟡 待冻结签发`｜`🟠 部分实现（Post-Alpha）`｜`⚪ 未开始（非 Alpha 阻断）`。

| 域 | 语言功能域 | 对应发布门（`language-release-gates.json`） | 当前态 | 核心已支持能力（一句话） | 剩余项（Alpha vs Post-Alpha） |
| --- | --- | --- | --- | --- | --- |
| [G1](#g1--基础类型聚合与模式匹配) | 基础类型、聚合与模式匹配 | `values-and-patterns` | ✅ 已闭环 | 10 整数 + `f32/f64`、`Record`/`Package`/`Enum`/`Option`/`ErrorUnion`/`T \| U`、引用模式与条件部分 Drop，经 8 类日常应用联验 | Alpha 无缺口；浮点字面量模式与 `match` 表达式属 Post-Alpha |
| [G2](#g2--泛型与-trait-api) | 泛型与 Trait API | `generic-libraries` | ✅ 已闭环（Alpha） | 固有 `impl`（含 `result(create)`）、泛型/Supertrait/关联类型/默认方法、整数 `const` 泛型、blanket `where`、拥有/借用迭代与泛型 `Queue<T>` | Alpha 无缺口；`for<'a>`、非可调用/多约束 `impl Trait` 返回与开放泛型方法槽属 Post-Alpha |
| [G3](#g3--借用拥有与生命周期组合) | 借用、拥有与生命周期组合 | `ownership-composition`（数据模型） | ✅ 已闭环（Alpha） | 逐字段借用、嵌套 `Package`、512 槽共享地址数组、`Fn`/`FnMut`/`FnOnce` 捕获，以及 **G3.4 多输入共享/独占/多生命周期借入返回、借入 `Option`/`Enum`/`ErrorUnion` 与泛型 trait/`Store<T>` lending**（PR #156） | Alpha 无缺口；高阶生命周期 `for<'a>` 属 Post-Alpha |
| [G4](#g4--编译期计算与元编程) | 编译期计算与元编程 | `project-workflow`（编译期部分） | ✅ 已闭环（Alpha） | 泛型 `Type` 返回、CT 反射与全序号身份、真实 Definition 上下文、`MetaBlock` + `yield` | Alpha 无缺口；`#if`/`#for` 与运行期/ambient `Type` 属 Post-Alpha |
| [G5](#g5--动态抽象与异构派发) | 动态抽象与异构派发 | `dynamic-abstraction` | ✅ 已闭环（Alpha） | 借入 `dyn Trait<'a>`（PR #147 闭环生命周期返回 region 替换与 `DynImpl` 健全性）、参数化 `dyn Trait<T>`、`OwnedDyn<dyn Trait, Context<'a>>` 与 `Store<OwnedDyn>`（HTTP Router & 异步调度器 Sink 实测） | Alpha 无缺口；`dyn` 常量泛型实参与开放泛型方法槽属 Post-Alpha |
| [G6](#g6--状态协议与效应合同) | 状态协议与效应合同 | `ownership-composition`（协议核心） | ✅ 已闭环（Alpha） | 64 态用户协议（G6.1）、投影失效上界/历史撤销 `H`/嵌套资源别名（G6.2 `27/27`）、**G6.3 全套 4/4**（`Replace` PR #139、`Store` PR #145、`Task/Registration` PR #150、`Cancel` PR #151）共享 `zil-semantics` 单一事实源 | Alpha 无缺口；超 4 层嵌套路径与超 64 态协议域属 Post-Alpha |
| [G7](#g7--外部接口c-abi-与已登记回调) | 外部接口、C ABI 与已登记回调 | `foreign-lifecycle` | ✅ 已闭环（Alpha） | 完整 POD C-ABI/`CCallback`/`errno`、五态 CAS `c_register`/`c_register_async`、`RISKS.json` 14 项、与 G6.3-D/E 协议及取消边完整联合 | Alpha 无缺口；非 Linux ABI 见 `G10.4` |
| [G8](#g8--异步任务与多线程并发) | 异步任务与多线程并发 | `async-ownership` + `threads-and-synchronization` | ✅ 已闭环（Alpha） | `ucontext_t` 协程、嵌套可挂起调用确定性取消展开（PR #151）、`TaskStart`/`TaskValid`（PR #150）、Roam `24/24`、`pthread` `ThreadScope`/`Mutex<T>`/5 序原子与强 CAS，经 HTTP/批处理/异步调度器三应用联验 | Alpha 无缺口；跨线程任务迁移与跨模块非标量 `pub Suspend` 属 Post-Alpha |
| [G9](#g9--标准库资源图与-zui) | 标准库、资源图与 ZUI | `standard-library` + `resource-graphs` | ✅ 已闭环（Alpha 标准库） | 28 个内嵌信任根模块（UTF-8/`std.string` 高频辅助/数值/POD&Owning 哈希集合/`iter`/`graph`/`io`/`buffered_io`/`std.fs` 全套目录元数据/`std.path`/`std.env.args`/`std.net` POSIX TCP/`time`）+ `G9.2` 软件 UI 核心 | Alpha 标准库 4/4 已闭环；`G9.4–G9.6` ZUI（窗口/GPU/IME）不阻断 Alpha |
| [G10](#g10--工程工作流原生交付与发布验收) | 工程工作流、原生交付与发布验收 | `project-workflow` + `native-delivery` + `release-acceptance` | ✅ Linux & Windows x64 Alpha 双平台闭环 | Linux x86_64 [`v1.0.0-alpha.1`](https://github.com/zil-lang/zil/releases/tag/v1.0.0-alpha.1) Pre-release 已发布（[#45754945988](https://github.com/zil-lang/zil/actions/runs/45754945988) & [#45755676030](https://github.com/zil-lang/zil/actions/runs/45755676030)）；**`G10.4` Windows x64 W1–W5 已全部合入并端到端全绿**（PR #163–#168，[`#45780581400`](https://github.com/zil-lang/zil/actions/runs/45780581400)：12/12 + 8/8 双后端 O0/O2 + 确定性 zip 打包 + 无源码/无 Rust 干净环境独立验收） | `G10.5` 编译与运行性能专项、`G9.4–G9.6` ZUI 与 `G10.6` 终态产品门独立推进 |

---

## 2. 分功能域明细（`G1`–`G10`：已完成 vs 待完成）

### G1 · 基础类型、聚合与模式匹配

- **发布门**：`values-and-patterns` · **规范**：`ZIL-SYNTAX`、`ZIL-MODES`、`ZIL-PACKAGE`、`ZIL-CLEANUP` · **总态**：✅ 已闭环

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G1.1 | 标量、整数/浮点与显式转换 | ✅ 已闭环 | Alpha 必选 | 10 种整数（`u8..usize`/`i8..isize`）+ `f32/f64`、`bool`/`char`/`unit`、checked/wrapping 转换、无符号数组/切片越界检查 | 无（保持双后端 O0/O2 回归） |
| G1.2 | 聚合、错误处理与匿名联合 | ✅ 已闭环 | Alpha 必选 | `Record`/`Package`/`Tuple`/`Array`/`Slice`/`Enum`/`Option`/`ErrorUnion`（`?` 传播）、匿名联合 `T \| U`（拥有载荷 Drop、规范排序与重复/非穷尽/双消费拒绝） | 无（`owned-sum-consumer` 全 32 组合闭环） |
| G1.3 | 拥有字段替换与借用/Drop 协同 | ✅ 已闭环 | Alpha 必选 | 整值/字段 `Replace` 责任转交、loan/pin 与 `old` 释放、失败清理闭环，且随 PR #139 闭环状态协议与历史隔离 | 无 |
| G1.4 | 模式匹配、引用模式与条件部分移动 | ✅ 已闭环（Alpha） | Alpha 必选 / Post-Alpha | 命名/位置解构、记录模式简写/省略、`match` 守卫保留拥有 scrutinee、`a..=b`、嵌套引用模式 `&pat`/`&mut pat`/`&&pat` 与 `drop_elab` 补齐，经 8 类日常应用联验 | **Post-Alpha**：更深多层地址/资源包嵌套模式、`match` 表达式/或模式/带标签 `break` |

### G2 · 泛型与 Trait API

- **发布门**：`generic-libraries` · **规范**：`ZIL-TRAIT`、`ZIL-MODULE`、`ZIL-RECEIVER` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G2.1 | 泛型参数、固有 `impl` 与整数 `const` 泛型 | ✅ 已闭环 | Alpha 必选 | 类型/生命周期/`<#N: usize>`、固有 `impl`（含 PR #146 补齐 `result(create)`/`result(give)` 与 `&mut Managed -> &mut Desc`）、具体类型/枚举静态关联函数、跨模块 nominal 身份 | 无 |
| G2.2 | Trait、Supertrait、关联项与默认方法 | ✅ 已闭环（Alpha） | Alpha 必选 | 默认方法体、Supertrait 蕴含与循环拒绝、`T: Iter<Item = U>` 关联类型/常量绑定与跨 trait 消歧、泛型接收者方法，经 8 类日常应用双后端原生联验 | 无 |
| G2.3 | 关联投影与 `Self`/类型依赖常量表达式 | 🟠 部分实现 | Post-Alpha | 有限 `Self` 依赖常量已合入 | 一般关联投影、任意类型/`Self` 依赖常量求值与约束方程 |
| G2.4 | 泛型 Trait 方法、Blanket Impl 与广义 Lending | ✅ 已闭环（Alpha） | Alpha 必选 / Post-Alpha | Trait 泛型方法、blanket impl `where` 校验、未用定义期义务诊断、`Store<Item>` 普通拥有 lending、泛型 `Queue<T: StoreElement>` 与容器迭代组合闭环 | **Post-Alpha**：更广 supertrait 泛型实参、一般 blanket 约束方程与开放泛型方法槽 |

### G3 · 借用、拥有与生命周期组合

- **发布门**：`ownership-composition`（数据模型） · **规范**：`ZIL-ORIGIN`、`ZIL-ACCESS`、`ZIL-CAPTURE`、`ZIL-PACKAGE` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G3.1 | 逐字段借用与嵌套拥有资源包 | ✅ 已闭环 | Alpha 必选 | `Struct`/`Package`/`Tuple` 逐字段借用（`Carries::set_field`）、字段覆写释放旧 loan、多共享/独占借入叶、嵌套 qualified 子 `Package`、`move_out` 后补回 | 无 |
| G3.2 | 固定共享地址数组与快照 | ✅ 已闭环 | Alpha 必选 | 非零 `[&'a T; N]`（含 512 槽）、常量/动态整数索引、普通 helper 传参返回、字段快照逐槽初始化核验 | 空数组与独占数组复制（Post-Alpha） |
| G3.3 | 闭包捕获与路径敏感补齐 | ✅ 已闭环 | Alpha 必选 | 真实捕获 `Fn`/`FnMut`/`FnOnce`、外层上下文、路径敏感 `FnMut` 捕获与 `match`/`let`/`break` 补齐、逐 role 独立核验 | 无 |
| G3.4 | 多输入借用返回与泛型 Lending | ✅ 已闭环（[PR #156](https://github.com/zil-lang/zil/pull/156)） | **Alpha 必选** | `zil-semantics` 共享语义（`borrow.rs`/`transfer.rs`）闭环多输入共享/独占/多生命周期返回（`select_shared`/`select_mut`/`pick_first`/`Pair<'a,'b>`）、借入 `Option`/`Enum`/`ErrorUnion`、泛型 trait `impl<life a>` `&self -> &[u8]` lending 与泛型 `Store<T>` lending；双后端 + 11 安全反例 + Raw/Wire 独立 `zil-check` 全绿 | 高阶生命周期 `for<'a>`（Post-Alpha） |
| G3.5 | 拥有/借入捕获与责任综合矩阵 | ✅ 已闭环 | Alpha 必选 | 闭合 `FnMut` 捕获与 `OwnedDyn` 释放矩阵，并随 G6.3 全套与 8 类日常应用（含 OOM 注入清理）双后端原生联验 | 无 |

### G4 · 编译期计算与元编程

- **发布门**：`project-workflow`（编译期部分） · **规范**：`ZIL-COMPTIME`、`ZIL-MODULE` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G4.1–G4.3 | CT 泛型 `Type`、全序号身份与 Definition 上下文 | ✅ 已闭环 | Alpha 必选 | 普通泛型 `Type` 返回、混合 const/life/type 全序号身份与原源重实体化、真实 Definition 上下文与物理字节计费 | 无 |
| G4.4 | `MetaBlock` + `yield` 与完整实例上下文 | ✅ 已闭环（Alpha） | Alpha 必选 / Post-Alpha | `MetaBlock` + `yield`（经 verified `zil-interp` 执行、拒绝非法拥有载荷 `E-RESOURCE-TYPE`） | **Post-Alpha**：一般成员/生成作用域、重定位与 `#if`/`#for` 元语句 |
| G4.5 | 高级 CT 值导出与 Rich Callable | 🟠 部分实现 | Post-Alpha | 基础纯值 CT 反射已可用 | rich callable、ambient `Type` 导出、更一般可序列化值与拥有载荷组合 |

### G5 · 动态抽象与异构派发

- **发布门**：`dynamic-abstraction` · **规范**：`ZIL-TRAIT`、`ZIL-ROUTE`、`ZIL-BYTE-STORAGE` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G5.1–G5.2 | 借入 `dyn Trait`、静态 vtable 与对象安全 | ✅ 已闭环 | Alpha 必选 | `&dyn Trait` / `&mut dyn Trait`、参数化 `dyn Trait<T>` / `Assoc = T`、object-safety 校验、`FnRole::DynSlotImpl` 授权核验、共享/独占接收者 | 有限泛型 method slot（Post-Alpha） |
| **G5.3** | **显式拥有擦除存储 `OwnedDyn` 与生命周期区域替换** | ✅ 已闭环（[PR #147](https://github.com/zil-lang/zil/pull/147)） | **Alpha 必选** | `OwnedDyn<dyn Trait, Context<'a>>`、共同借入 `Store<OwnedDyn<..., Context<'a>>>`、按值 `self: Self` 堆箱零泄漏释放；PR #147 完整闭环 `dyn Trait<'a>` trait 生命周期参数方法返回 region 替换（`static-callback-getter-cycle-control.zil` 双后端通过）并修补 `DynSlot` 借用解包、`EXTERNAL` 调用借用与 `check_dyn_impl_region_contract` Raw/Wire 独立核验；在 HTTP Router 与异步调度器 Sink 中端到端实测 | **Post-Alpha**：未选 owner 错误联合捕获与非接收者 provider |

### G6 · 状态协议与效应合同

- **发布门**：`ownership-composition`（协议核心） · **规范**：`ZIL-CONTRACT`、`ZIL-ORIGIN`、`ZIL-CLEANUP` · **总态**：✅ 已闭环（Alpha）

> **架构红线已守住**：全链路 `Typeck -> Body IR -> Analyze -> Verify -> Lower -> Native` 共享 [`zil-semantics`](https://github.com/zil-lang/zil/tree/main/crates/zil-semantics) 单一事实源，未为 `zil-check` 另造语义系统；G6.2 `27/27` 零退化。

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G6.1 | 有限状态协议基础（内建 + 64 态用户协议） | ✅ 已闭环 | Alpha 必选 | `requires(state/apart)`、`provides(state/retired/on_pending/on_ready/on_ok/on_err)`、跨 `match` 分支精化、未标注调用方前置核验、Raw/Wire 独立证明 | 超 64 态或泛型参数化状态域（Post-Alpha） |
| G6.2 | 投影失效上界、历史撤销 `H` 与嵌套资源别名 | ✅ 已闭环 | Alpha 必选 | `effects(invalidates(...))`、`effects(rollback(...))`、完整类型图 `mut_cap` 扫描、`Managed`/`Desc` 自动描述与嵌套资源跨参数关联（**27/27 闭环**；PR #142 完成 `zil-check` 终态模块化） | 超 4 层嵌套字段路径、跨模块导出上界扩展（Post-Alpha） |
| **G6.3-B** | **联合语义 1：`Replace`** | ✅ 已闭环（[PR #139](https://github.com/zil-lang/zil/pull/139)） | **Alpha 必选** | `zil-semantics::protocol` 单一模型、`Replace` 溯源分离、旧新值不共用历史 `H`、旧态与责任转入 `old`、新值清旧 `retired`；Source/Raw/Wire + 双后端原生全绿 | 无 |
| **G6.3-C** | **联合语义 2：`Store`** | ✅ 已闭环（[PR #145](https://github.com/zil-lang/zil/pull/145)） | **Alpha 必选** | `StoreProtocolSummary` 共享摘要；`StorePush`/`StorePop` 区分 `.ok`/`.err` 失败恢复（含中间修改失效）与空容器 fail-closed；`StoreBorrow` 借用不等于实际修改并经 `store_borrow_parents` 写回同步；双后端原生 + 7 Source 反例 + 3 Raw/Wire 控制全绿 | 无 |
| **G6.3-D** | **联合语义 3：`Task` / `Registration`** | ✅ 已闭环（[PR #150](https://github.com/zil-lang/zil/pull/150)） | **Alpha 必选** | `TaskCreate` 条件成功与 `.ok` 上下文捕获隔离；`TaskValid`/`AsyncRegistrationValid`/`RegistrationValid` 严格只读且 `Valid == false` 永不证明 `Closed`（区隔 `null_uninstalled_async_reg` 与 `TaskFinishDestroy`）；完整定义并实现 `IntrinsicOp::TaskStart`（IR/Semantics/Verify/LLVM）；修复 `TaskValid` 在 `Desc`/`Managed` 上的潜伏代码生成缺陷 | 无 |
| **G6.3-E** | **联合语义 4：`Cancel`** | ✅ 已闭环（[PR #151](https://github.com/zil-lang/zil/pull/151)） | **Alpha 必选** | `HandleProtocolSlot::on_cancel_possible_mutation` 共享规则；`zil-typeck` 为所有可挂起 `Call`/`TraitCall`/`ClosureCall` 绑定真实取消展开边；`zil-check` `build_cancel_state` 既不沿用调用前状态也不套用正常返回后置，保守保留副作用、回滚、失效与 `Operand::Move` 所有权转移，同时保持纯只读 `effects(invalidates())` 精确性 | 无 |

### G7 · 外部接口、C ABI 与已登记回调

- **发布门**：`foreign-lifecycle` · **规范**：`ZIL-ASYNC`、Design §12.4、`RISKS.json` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G7.1 | 同步 `Registration`、C ABI 与 `CCallback` | ✅ 已闭环 | Alpha 必选 | 10 整数 + `f32/f64` C-ABI、递归 POD Record、`CCallback`（Wire 49）、`c_null()`（Wire 48）、`ForeignHandle ==/!=`、glibc `errno`、同步稳定闭包登记与显式强关闭 | 非 Linux ABI 见 `G10.4` |
| G7.2–G7.3 | `AsyncRegistration` 五态门控与晚通知 Drain | ✅ 已闭环（Alpha） | Alpha 必选 / Post-Alpha | 五态 CAS 门控（`OpenIdle`..`Closed`）、`driver_lock`、`RISKS.json` 14 项 1-to-1 绑定、8 份 C-host 与 `raw-holder.zil` pthread 实测，并随 PR #150/#151 闭环 `Valid`/状态协议/取消边联合 | **Post-Alpha**：一般跨线程借入捕获环境（`Address<Closure>`/`&mut`） |
| G7.4 | `Registration` 与异步任务取消/失败组合 | ✅ 已闭环 | **Alpha 必选** | 随 PR #150/#151 完整闭环 `Task`/`AsyncRegistration` 条件创建、失败与取消边状态协议联合 | 无 |

### G8 · 异步任务与多线程并发

- **发布门**：`async-ownership` + `threads-and-synchronization` · **规范**：`ZIL-ASYNC`、`spec/affine-mutex.md` · **总态**：✅ 已闭环（Alpha）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G8.1–G8.2 | 结构化 `ThreadScope`、协程栈与静态拥有 `Task` | ✅ 已闭环 | Alpha 必选 | `pthread_create`/`pthread_join`、`ucontext_t` 协程、4 KiB `PROT_NONE` 保护页、`std.task.*`、`task_await`、Roam `24/24` + `24+9` | 无 |
| G8.3 & G8.5 | `Mutex<T>` / `Guard`、有序原子与 `Transfer`/`Share` | ✅ 已闭环（Alpha） | Alpha 必选 / Post-Alpha | `pthread_mutex_t` `Mutex<T>`/affine `Guard`、`MemoryOrder` 5 序与 `Sync*Ordered`（Wire 43–47）、强 CAS、保留 `Transfer`/`Share`、`thread_scope` 裸 `c_handle` 与 `Mutex<Resource>` FFI Drop 拒绝，并经 HTTP 服务/文件批处理并发实测 | **Post-Alpha**：用户泛型自动派生 `Transfer`/`Share` |
| G8.4 & G8.6 | `PendingExit` 取消回滚、任务协议与调度组合 | ✅ 已闭环（Alpha） | **Alpha 必选** | `PendingExit` 单次 Drop、`EnumPartialDrop`、`task_create`/`task_create_owned` OOM 清理、逐路径 CFG `resume(transfer)` 证明、嵌套可挂起调用取消展开（PR #151），并通过第 8 个日常应用 `async-task-scheduler` 双后端原生及 40 轮取消压力验收（PR #152） | **Post-Alpha**：一般借入 frame port、`FnOnce` 挂起根精细区分、跨模块 `pub Suspend` 非标量 ABI |

### G9 · 标准库、资源图与 ZUI

- **发布门**：`standard-library` + `resource-graphs` · **规范**：`spec/pod-hash-collections.md`、`spec/owning-hash-collections.md`、`spec/numeric-text-and-buffered-io.md`、`spec/checked-resource-graph.md` · **总态**：✅ 已闭环（Alpha 标准库）

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G9.1 | 基础文本、数值格式与资源图 | ✅ 已闭环 | **Alpha 必选** | `std.utf8`、`std.string`（PR #143 补齐 `starts_with`/`ends_with`/`contains`/`trim_start`/`trim_end`/`trim`/`split`/`lines`/`replace_all`）、`std.text_edit`、`std.int`、`std.float`、`std.graph`，经文本搜索/CSV/JSON/图依赖分析器实测 | 完整 Unicode 排版（Post-Alpha） |
| G9.3a | 容器、哈希集合与迭代器 | ✅ 已闭环 | Alpha 必选 | `std.store`、`std.collections`、`std.iter`、`pod_hash_*`/`i64_hash_*`/`string_map`、`owning_map`/`owning_hash_map`/`owning_hash_set`，经 8 类日常应用与 OOM 注入联验 | 高阶集合算法与跨过程 `apart` 精度（Post-Alpha） |
| **G9.3b** | **系统 I/O、路径、CLI 参数与网络基础库** | ✅ 已闭环（PR #143/#144/#146） | **Alpha 必选** | ① `std.env`（`arg_count`/`arg`/`args` + `@main(argc, argv)` 桥接，PR #143）；② `std.fs`（`Metadata`/`metadata`/`is_dir`/`is_file`/`file_size`/`mkdir`/`mkdir_all`/`remove_dir`/`rename`/`read_dir`）+ `std.path`（`join`/`file_name`/`parent`/`extension`/`normalize`，PR #144）；③ `std.net` POSIX TCP Socket（`Ipv4Addr`/`TcpListener`/`TcpStream` + 1100 轮 fd 回收压测，PR #146）；④ `std.io`/`std.buffered_io`/`std.time` | TLS/UDP/非阻塞异步网络（Post-Alpha） |
| G9.2 | ZUI 软件核心库 | ✅ 已合入 | 非 Alpha 阻断 | 整数布局、RGBA 软件合成、IME preedit 提交/取消与失败保留普通库 | 见 `G9.4–G9.6` |
| G9.4–G9.6 | ZUI 窗口、GPU 渲染与应用层 | 🟠 部分实现 / ⚪ 未开始 | **非 Alpha 阻断** | `G9.4` 已有初步窗口实验 | `G9.4` 真实 OS 窗口/事件循环/OS IME；`G9.5` GPU 首帧与字体塑形；`G9.6` 实体/组件与确定性关闭合流 |

### G10 · 工程工作流、原生交付与发布验收

- **发布门**：`project-workflow` + `native-delivery` + `release-acceptance` · **规范**：`docs/guide/build.md`、`validation/language-alpha-profile.json`、`validation/product-gates.json` · **总态**：✅ Linux `v1.0.0-alpha.1` 已发布；`G10.4` Windows x64 W1–W5 已全部闭环并完成端到端空环境验证

| ID | 子功能 | 状态 | 里程碑 | 已完成（当前可用能力） | 待完成（剩余差距与边界） |
| --- | --- | --- | --- | --- | --- |
| G10.0 & G10.2–G10.3 | Linux x86_64 双后端、发布打包与空环境验证 | ✅ 已发布（[`v1.0.0-alpha.1`](https://github.com/zil-lang/zil/releases/tag/v1.0.0-alpha.1)） | Linux Alpha 必选 | Clang + LLVM 23.1.4 C API × O0/O2、1/4/8 并行链接、Fixed Projects `21/21`（`152/152 required` / `196 historical`, `44 archival`）、State/Store `16/128/56`、版本号统一 `1.0.0-alpha.1`（PR #161/#162）、同提交门禁自动发布 GitHub Pre-release [`v1.0.0-alpha.1`](https://github.com/zil-lang/zil/releases/tag/v1.0.0-alpha.1)（[`#45755676030`](https://github.com/zil-lang/zil/actions/runs/45755676030) 全绿，4 份已校验资产） | 无（保持 Linux Alpha 回归零退化） |
| G10.1 | 项目工作流、分层 CI 与留证治理 | ✅ 已闭环 | Alpha 必选 | `zil check/build/run/test/lock`、常驻模式；清理 20 份过程 JSON、CI 提速至 `documentation` ~11s / `workspace` ~3–4m、新增 `tools/dev-check.py`、`docs/guide/supported-language.md` 12 项能力三分类核对表与 fail-closed `alpha-acceptance.yml` | 无 |
| G10.7 | 12+8 全门冻结验收与 Alpha 签发 | ✅ 已签发（`v1.0.0-alpha.1`） | Linux Alpha 必选 | `validation/language-alpha-profile.json` 12+8 合同对齐（PR #138）；8 类日常应用全部合入并补齐 JSON UTF-16 代理对/CSV/HTTP 规范边界（PR #155）；`main@719ea01` 冻结验收 [`#45754945988`](https://github.com/zil-lang/zil/actions/runs/45754945988)（6m11s 全绿）与正式 Pre-release 发布 [`#45755676030`](https://github.com/zil-lang/zil/actions/runs/45755676030) 同提交全绿 | 无（Win64 对应验收见 `G10.4-W5`） |
| **G10.4** | **Windows x64 原生支持（W1–W5）** | ✅ **已全部闭环（W1–W5 全绿）** | **P0 平台里程碑（已达成）** | **W1–W5 已全部合入 `main@278c9c2` 并端到端全绿**（PR #163–#168）：COFF/PE + 多单元去重 + MSVC x64 ABI/64 KiB `__chkstk`/Win32 `HeapAlloc` OOM + Win32 `SRWLOCK`/`_beginthreadex`/Fiber `Task`/`AsyncRegistration` + 7 个 Windows 标准库模块（Win32/UCRT/Winsock2）+ 官方 LLVM 23.1.4 `LLVM-C.dll`/`LLVM-C.lib` 双后端（`Clang` + `LlvmApi` × O0/O2）+ 确定性 `zil-v1.0.0-alpha.1-windows-x86_64.zip` 打包 + 真机 [`windows-alpha-release.yml` #45780581400](https://github.com/zil-lang/zil/actions/runs/45780581400)（**12/12 + 8/8 双后端全通过** + 无源码/无 Rust 干净环境独立安装验收全绿） | 保持 Windows x64 双后端 CI 持续门禁；可按需将 Windows zip 正式挂接至 GitHub Pre-release |
| G10.5 | 编译与运行性能突破专项 | 🟠 进行中 | Post-Alpha 专项 | LLVM 23.1.4 Stage 1 桥接已合入；独立研究分支 `research/full-push-native`、`integrate/scalar-codegen` 推进中 | Stage 2（去除 `body_structured.rs` 文本桥接直接降级）及同语义 Rust 相对性能/编译净突破 |
| G10.6 | 终态产品门（`product-gates.json`） | ⚪ 未开始 | 终态发布 | `validation/product-gates.json` 合同已固定 | 零 safe UB、Rust 相对性能/编译门、可重复净突破、B0 与 ZUI 终态冻结独立复验 |

#### G10.4 明细：Windows x64 原生支持分阶段路线图（W1–W5，P0 ✅ 全部闭环）

> **诚实声明口径**：已完成 **W1–W5 全部阶段**并在真实 `windows-latest` 跑通同提交 **12/12 基础能力 + 8/8 日常应用** `Clang` + `LLVM-C.dll` 双后端验收、确定性 zip 打包及无源码/无 Rust 干净环境独立安装验收（[`#45780581400`](https://github.com/zil-lang/zil/actions/runs/45780581400)），正式达成 **Windows x64 完整 Alpha 支持**。全程保持单一语言语义、`zil-check` 零绕过与 Linux Alpha 零退化。

| 阶段 | 范围与核心交付 | 验收门槛（必须跑在真实 `windows-latest`） | 当前态 |
| --- | --- | --- | --- |
| **W1** | 目标管线与构建打通：`TargetProfile`/`WindowsLinkOptions` 全链路接入 `BodyBuildOptions`/`body_emit`/`body_build`；`x86_64-pc-windows-msvc` COFF/PE 发射与链接；剔除 Linux POSIX 声明；原生 `windows-latest` CI | `zil.exe` 原生构建 + `win64_pipeline_native`（O0/O2 执行、`OwnedDyn`、C 观察器 `Drop`、`ByteBuffer`/`Store`、`ErrorUnion`、`_errno`/UCRT `printf`、`SafeReject` 预代码生成拦截）+ `zil check/build/run/lock/test` 全绿 | ✅ **已合入**（[PR #163](https://github.com/zil-lang/zil/pull/163) / [#45756737871](https://github.com/zil-lang/zil/actions/runs/45756737871)） |
| **W2** | Windows x64 MSVC ABI 与核心运行时：多单元 COFF 去重（零 `LNK2005`）、4 寄存器 + 32 字节 shadow space、16 参栈溢出、聚合体按值/隐藏指针传返、`CCallback`、大栈帧 64 KiB `__chkstk` 探测、同步 `Registration`、5 序原子与 Win32 `HeapAlloc` OOM 零泄漏清理 | `win64_abi_and_runtime_native`（9 项真实 C 宿主机跨语言 ABI/OOM/多单元 COFF 矩阵）+ `SafeReject` 配对负例全绿 | ✅ **已合入**（[PR #164](https://github.com/zil-lang/zil/pull/164) / [#45765507517](https://github.com/zil-lang/zil/actions/runs/45765507517)） |
| **W3** | Windows 并发与异步运行时：Win32 Fiber（`ConvertThreadToFiber`/`CreateFiberEx(..., FIBER_FLAG_FLOAT_SWITCH=1)`/`SwitchToFiber`/`DeleteFiber`）承载 `Task`；UCRT `_beginthreadex` + `WaitForSingleObject`/`CloseHandle` `ThreadScope`；Win32 `SRWLOCK` `Mutex<T>` 与仿射 `Guard`；`AsyncRegistration` 五态 CAS 与晚通知排空 | `win64_concurrency_and_async_native`（20,000 次 `SRWLOCK` 争用、`Task` 跨线程迁移、`f64` 浮点保持、`PendingExit` 单次 Drop、嵌套子任务取消、40 轮压力与真实 Win32 后台线程 `AsyncRegistration`）全绿 | ✅ **已合入**（[PR #165](https://github.com/zil-lang/zil/pull/165) / [#45769026928](https://github.com/zil-lang/zil/actions/runs/45769026928)） |
| **W4** | 标准库平台抽象层：保持 `std.io`/`std.fs`/`std.path`/`std.env`/`std.time`/`std.net`/`std.float` 公共 API 不变，按目标选择 Linux POSIX 或 Windows Win32/UCRT/Winsock2 实现（`lib/std/windows/`、二进制模式 `_sopen_s`/`_setmode`、`GetFileAttributesExA`/`MoveFileExA`/`FindFirstFileA`、盘符/UNC 路径、Winsock2 `WSAStartup`/`SOCKET` 与 `-lws2_32`） | `std` 全套件 + 8 类日常应用全部在真实 `windows-latest` 原生编译执行并通过边界测试（含 TCP HTTP 回环、文件批处理与异步调度器） | ✅ **已合入**（[PR #166](https://github.com/zil-lang/zil/pull/166) / [PR #167](https://github.com/zil-lang/zil/pull/167) 补齐 `path.zil` UNC 修复与严格 PowerShell 错误传播） |
| **W5** | Windows x64 双后端与正式发行闭环：官方 LLVM 23.1.4 `LLVM-C.dll`/`LLVM-C.lib` 双后端（Clang + LLVM API × O0/O2）、12/12 基础能力 + 8/8 日常应用全门验收、`zil-v1.0.0-alpha.1-windows-x86_64.zip` 确定性打包与无源码/无 Rust 干净 `windows-latest` 安装验证 | `package-windows-x86_64`（12/12 + 8/8 双后端全通过 + 确定性 zip `sha256: 6f73fe0532...`）+ `verify-clean-env-windows`（无 checkout/Rust/Cargo/`ZIL_LLVM_LIB_DIR` 空环境全通过）双作业全绿 | ✅ **已闭环**（[PR #167](https://github.com/zil-lang/zil/pull/167) + [PR #168](https://github.com/zil-lang/zil/pull/168) / 端到端全绿 [`#45780581400`](https://github.com/zil-lang/zil/actions/runs/45780581400)） |

#### G10.7 明细：8 类真实日常程序综合验收矩阵（`8/8` 已合入 `main`）

| # | 应用（Gate ID） | 源码目录与测试 | 覆盖功能域 | 当前态 |
| --- | --- | --- | --- | --- |
| 1 | **文本统计与文件搜索** (`text-stats-and-file-search`) | `examples/everyday-apps/text-stats-and-file-search` (`everyday_apps_core_native.rs`) | G1、G9、G10 | ✅ 已合入（[PR #148](https://github.com/zil-lang/zil/pull/148)） |
| 2 | **CSV 处理工具** (`csv-processor`) | `examples/everyday-apps/csv-processor` (`everyday_apps_core_native.rs`) | G1、G2、G9 | ✅ 已合入（[PR #148](https://github.com/zil-lang/zil/pull/148)） |
| 3 | **JSON 解析与序列化器** (`json-processor`) | `examples/everyday-apps/json-processor` (`everyday_apps_core_native.rs`) | G1、G3、G9、G10 | ✅ 已合入（[PR #148](https://github.com/zil-lang/zil/pull/148)） |
| 4 | **CLI Todo 应用** (`cli-todo-app`) | `examples/everyday-apps/cli-todo-app` (`everyday_apps_core_native.rs`) | G6（`Replace`/`Store`）、G9、G10 | ✅ 已合入（[PR #148](https://github.com/zil-lang/zil/pull/148)） |
| 5 | **HTTP 小服务与客户端** (`http-mini-service-and-client`) | `examples/everyday-apps/http-mini-service-and-client` (`everyday_apps_systems_native.rs`) | G7、G8、G9（TCP）、G5 | ✅ 已合入（[PR #149](https://github.com/zil-lang/zil/pull/149)） |
| 6 | **文件批处理工具** (`file-batch-processor`) | `examples/everyday-apps/file-batch-processor` (`everyday_apps_systems_native.rs`) | G9（目录/路径/重命名）、G8、G3 | ✅ 已合入（[PR #149](https://github.com/zil-lang/zil/pull/149)） |
| 7 | **图依赖分析器** (`graph-dependency-analyzer`) | `examples/everyday-apps/graph-dependency-analyzer` (`everyday_apps_systems_native.rs`) | G9（`std.graph`）、G2 | ✅ 已合入（[PR #149](https://github.com/zil-lang/zil/pull/149)） |
| 8 | **异步任务调度器** (`async-task-scheduler`) | `examples/everyday-apps/async-task-scheduler` (`everyday_app_async_scheduler_native.rs`) | G8、G7、G6（`Task`/`Cancel`）、G5 | ✅ 已合入（[PR #152](https://github.com/zil-lang/zil/pull/152)） |

---

## 3. 看板治理与验收红线

1. **单一真相与防退化**：子项通过不自动关闭父能力；安全边界缺陷永远 P0 阻断；所有改动保持 G6.2 `27/27`、Fixed Projects `21/21`、State/Store `16/128/56`、Roam `42/42 + 24/24` 零退化。
2. **活证据优先**：日常 PR 以 Git SHA + 可执行测试 + 绿灯 CI 证明正确性，不向 `validation/` 追加过程 JSON 留证；静态验收收据仅在冻结发布候选或里程碑签发时生成一份。
3. **反虚高计量与独立 Verify**：全仓 `tests/**/*.zil` `zil check --json` `status == "Success"` 计数不是通过率；`zil-check` 必须独立核验 Source 与 Raw/Wire 有效证书并拒绝伪造证书。
4. **原地维护**：PR 合入或状态变化时仅原地更新对应单元格，不在 `goal.md` 堆积流水账。
