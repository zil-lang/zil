# 私有导入 pass：Python 协议独立静审与最终冻结补核

## 范围与当前结论

本次只读审查 `research/thinlto-private-imports/` 的 Python 草案，没有 import、prepare、编译、LLVM 工具、native 或性能采样。当前十二份 Python 经 `ast.parse` 语法读取通过；未发现 Python 层阻断。C++ 插件、构建绑定、正式 freeze/protocol 当时尚未生成（README 随后已出现并核对，状态仍为实现中），因此本结论不表示插件契约、hook 链或真实运行已通过；后续必须按实际冻结字节补核。旧 strict/default NoGo 原样保留，不以本草案覆盖。

## 继承合同

与 strict v2 对照，`processes.py`、`visibility.py`、vendor 五文件逐字节相等。固定两 case N64/N256 base、每文件八函数、100000 次循环、period251 数值模型、A/B/R 六排列 ABR/BRA/RAB/ARB/RBA/BAR、每 arm 一 warmup 六 formal 与额外 00/ff 功能输入均未变：共六 clean compile、54 native、36 formal。A 使用 main990/tree1c16744d，B 继续使用 exporter2549/treeebb9931；插件是独立源码/so/buildreceipt 身份，不假称 exporter 新生产提交。

原完整 O2、current Verify、原模型/折叠后模型/实际八 unit preflight、实际 summary/HASH、.3 导入覆盖和八 .4 verify 均保留。force-import-all=true 一次，实际 lld/clang CPU/PIC/merge/aliases 参数未调整，新增参数是已 pin 的 load-pass-plugin。CPU 两个、编译/ThinLTO jobs2 与 linker threads2 未变；这是共享两 CPU 的调度预算，不声称线程总数只有二。

原 B/A≤1.10、Rust/B≥.90 按每 case 完整六正式样本均值执行，全部失败输出与长尾保留；任何缺失、工具失败或验证异常落 Incomplete，不得过滤后称 Pass。compile 时间包含研究验证/证据开销，只作诊断，不是编译加速门；两个纯函数 base case 不给生产、地址/全局或原八 case 覆盖证明。

## 新增身份与隔离

`common.validate` 要求 plugin 工具 bytes/path 与 receipt 一致；receipt status=Pass，源码当前字节等于冻结目录 `PrivateImports.cpp`，源码路径身份、compiler 身份与 pin clang bytes、build-log SHA 均核对。scripts 冻结从递归 `.py` 扩展至 `.py/.cpp`，因此新 C++ 不能在 prepare 后悄改。实际 toolchain invocation 保留 ld.lld/rustc 的 argv0 路径，未回退为 resolve 后通用 driver。

runner 在 A、B、R 前和 native 前移除原五个 research env 及四个 private-import env。四项分别为 manifest path/SHA、request id、receipt dir；只有 B wrapper 在当前完整 source/cost/owner/实际 prelink proof 后生成。native-cache 使用 mkdir(exist_ok=False)；已有目录即失败，不能以已有 backend cache 绕开八 actual hooks。

## Manifest/output 契约（待 C++ 字节对照）

manifest `/1` 含 request_id、protocol SHA、plugin identity、当前 Rust receipt 与 prelink witness identity，以及八 unit 的 index/source_filename/input_bitcode/canonical/prelink definitions/required_prelink/allowed_original。source_filename 必须是唯一、无转义、绝对路径；这只是当前纯 fixture 的拒绝式适配边界，不是通用 LLVM 文件名解析器。

输出恰为每 unit 的 `.started.json/.before.ll/.after.ll/.json` 共 32 个文件；缺失/多余一项即拒绝。正式 receipt 要求 phase=ThinLTOPostLink、相同 request/unit/source_filename、manifest/plugin/Rust/prelink SHA、verify_before/after=true、两份实际 IR identity，以及 imports 列表 `{canonical, private, direct_uses}`。started 文件当前核存在与文件集合，不解读其内容；最终成功事实依赖正式 receipt、before/after 和实际 .3 验证。

Python 将 hook-before AE 集合与真实保存 .3 的 AE 集合核等，Required 须在 AE 或 canonical 中。imports 必须恰覆盖全部 AE，canonical 必须在原 Allowed 内；private 名固定为 `zil.import.{unit}.{canonical}`，不得碰撞现有定义/声明。hook-after 必须是 Internal 私有定义，原 canonical 名不能仍留于本模块定义/声明。该约束对应对本模块纯 AE 实现改名，而不是创建一份 clone 并保留 canonical declaration；实际作者插件必须匹配。

将 after 的精确 private 名逆回 canonical 后，逐字节全文比较 before/after，仅在选中函数 define header 规范化 available_externally/internal/dso_local 三个词。该比较不允许算术、CFG、属性、阈值或其他函数变动；它并不声明 hook-before 全文逐字节等同 .3，后者在 Python 层核的是 AE 名单和独立原 visibility proof。Root 随后补充了独立 direct-use 计数：在 parse 已拒绝非callee符号引用的前提下，逐行去注释计精确 canonical 符号引用、减去唯一定义 header，必须等于 C++ direct_uses，不能仅凭非负 receipt 过门。该规则与当前 parser 的纯直调边界一致；LLVM Use 的具体分类、拒绝地址/别名/metadata/COMDAT 以及原 canonical owner 保留，仍需非作者 C++ 审查。

## 失败与当前未执行项

缺 receipt、错误格式/身份、未知文件/名字、非原预算导入、全文差异等均在 B wrapper 返回成功前拒绝，因此 runner 不会把缺少 hook 证据当完成后继续 native。outer/inner process scope、raw-before-cleanup、timeout/fatal 的继承代码未改；硬 SIGKILL 不能保证所有 finally 均执行的原限制仍在，不声称无条件保存所有半成品。

当前未核：实际 C++/manifest/output 逐字段兼容、插件构建命令/LLVM ABI 与真实 so 绑定、最终 prefreeze/生成 protocol 的 sourcehash/oracle/工具路径及实际 hooks。未做镜像实现测试，也不要求再以复杂 mock 延迟真实首轮集成；静态冻结通过后，真实六编译应先验证实际链，任何失败保留 Incomplete 并停止在 native 前。

## 本次读取的 Python SHA256

| 文件 | SHA256 |
| --- | --- |
| common.py | d8780ed7faf17e7c9cc17ce0f3bf01e5a3cfd012ee59d6bd8b3d4bfc0f7cb2ce |
| prepare.py | 4ce3cf7271ec496acebfb9261a7d5c358ef3b0c9cbe56da6ea29fb5f3d22ed7a |
| run.py | 4ac460515dcf79c5d76acf002c683a60c8f767f8309304a1b5a4dc15ab4a832c |
| clang_wrapper.py | b1127a35c81a4d1712e24b1c08d68b693b71bbabb1aae296c5a293ac2db48979 |
| hook_checks.py | 532bb71f9d5275f118aa6a6c029829554d059fccc5ce682759d5cf935d81d242 |
| processes.py | ac8cbab5aaa2c2b50a891f5dac28c392dbe5228da6d517749af1b13645abbe04 |
| visibility.py | bbd583f3f948f4292c5a484c37b3e8ebd24213d414fcbcab11b890a8eaedde21 |

作者已消息确认 manifest/output 全部字段及八 unit 四文件契约相同：原地 rename+Internal、不 clone、不新增 canonical declaration、0AE unit 也出收据；这只是接口对齐说明，不替代 C++ 最终字节复核。

## 最终冻结与实际协议补核

前述“待生成”属于草案读取时点；本节按实际已发布 source、已构建插件和已 prepare 协议补核，结论为：**未发现 Python/插件收据契约阻断，允许进入固定真实质量运行；尚未执行的 hook 整链与运行门仍须由实际结果证明。** 本审没有运行 compiler、LLVM、plugin 或 native，也不把 Root 另行进行的有效地址反例控制提前写成成功。

| 身份 | 精确值 |
| --- | --- |
| 发布源码 commit | `1cc866a08bac100012e41726067711c94caf1899` |
| source tree | `910d281b288b9266f67c92ae9891b3b9290d16d2` |
| source-freeze-v1 JSON SHA256 | `bb3c573475b29825c9a1a10e0d975488d7e6740eadea13dba4ef898475a6b738` |
| PrivateImports.cpp SHA256 | `a22e5004b5f99150ae197ffa88f406eafb2136c796d5d3cc651a8fc97cc99623` |
| plugin build binding SHA256 | `056e812dbb270f07c40abf9658fa6ef3007ed76bc8f5869941ed49322fdcf2a9` |
| PrivateImports.so SHA256 | `585a84701033e21669a2465df4007150e1fddf36df1acdab3165c48a16734cf6` |
| 实际 protocol SHA256 | `208adf6f5a3e904182afdaf1ef1c6b30a173fb559400ab14d05b2f59e4803a44` |

协议为 `out/next-optimization/private-protocol-v1/protocol.json`，绑定 `private-plugin-build-1cc-v1/binding.json`。读取实际 Git HEAD/tree 与十四份 `research/compiler-thinlto-private-imports/` blob，逐份 SHA 均等于工作区十四份 source-freeze 条目；十二 Python AST 读取通过，协议递归十三份 `.py/.cpp` 集合完整且字节相等，README 由 source freeze 单列。没有仅相信 commit 文本标签。

与 strict `5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728` 比较，整个 `cases` 对象相等，因此 case 名、全部 sourcehash、独立 oracle 数值、native_vectors 与语料元数据均未更改；另对新目录实际 640 份 ZIL/Rust 源文件逐 SHA 核对。source_claims/build_receipts、cpus/jobs、cases_requested、iterations、orders、limits、counts、pipeline_A、scope 和原 compile/runtime 计量定义逐字段相等，旧八个工具身份也逐对象相同；新增 plugin 不伪装成 Rust exporter 的新 binary。实际固定范围仍为两 case、六 clean 编译、54 native/36 formal，CPU0,1，jobs2，B/A≤1.10 且 Rust/B≥.90。

### 构建绑定

source-freeze 的 C++ 与绑定中实际编译源字节相等；插件 SO、compiler、build log、stdout/stderr、plan 的实际 SHA 均核对。build plan、绑定和 process-attempt 的 command 三者精确相同；现存 attempt 为 exit0、reaped=true、无 timeout/exception/residual descendants。clang++ 调用角色保留，文件 SHA 与协议 pin clang 相同；参数为 C++17/O2/shared/PIC/no-exceptions/no-rtti、固定 ABI 和 LLVM23.1.4 headers，链接仅加 `-ldl`。读取保存的 dynamic-dependencies 原始 stdout，NEEDED 仅 libstdc++/libm/libgcc_s/libc，没有 libLLVM；这里没有重新运行 readelf/ldd。SDK archive 身份由既有 binding 记录，不将“未链接另一 LLVM”误称为未来所有 ABI/实际加载已证明。

### 最终 C++ 与 Python 字段契约

实际源码读取四个 ENV，并以 manifest SHA/request id、dladdr 的真实 SO 路径与 SHA、当前 Rust/prelink 收据及八 unit 的 current proof 绑定；source_filename 必须唯一匹配实际 Module，本 wrapper 同时要求八个绝对文件名互异。按 index 匹配完整数组、当前 preflight、原 owner/Allowed/cost/Required，不通过线程序号或执行顺序猜 owner。实际 callback 只在 `ThinLTOPostLink` 且 O2 添加 pass；只保留标准完整后续 O2，没有替换 codegen callback。

inspect 在任何 mutation 前遍历并拒绝 global/alias/ifunc/module asm、COMDAT/personality/prologue/GC、非许可 AE、metadata/addressspace 特殊输入和 private 名碰撞。AE 的每个 Use 均要求 CallInst、`isCallee(&U)`、`getCalledFunction()==&F`、函数类型相同且无 operand bundles；因此 `call @f(ptr @f)` 的地址参数 Use 不会因同属一个 CallInst 而放行。后者正是 Root 另行做有效 LLVM 地址反例控制的目标，但其结果仍另存，不由静审代报。

实际 mutation 恰为原地 `setName` 与 `setLinkage(Internal)`；无 clone/旧 canonical declaration，其他 module 的 canonical owner 不修改。变更后检查名字无自动后缀、旧名不留、属性/CC、block/instruction/operand 指针序列不变；这里只改 linkage，不承诺所有未inline私有定义一定不 emit。新 pass 发生变更时返回 PreservedAnalyses::none，无变更才 all。

实际每 unit 独占创建 `.started.json/.before.ll/.after.ll/.json`，即使零 AE 也出四份；前后 actual verify 必须通过。所有 Python 必需字段与 C++ JSON 字段逐一匹配：format/request/unit/phase/source_filename、manifest/plugin/Rust/prelink SHA、before/after path+SHA、imports 中 canonical/private/direct_uses。额外 visibility/DLL/dso_local 与 protocol SHA 明确记录。C++ 原 module 顺序不因 receipt 排序变化；imports 输出排序与 Python 的 sorted(ae) 相同。

Python 独立 parse+符号计数复核 direct_uses，再将批准私有名逆回 canonical、仅规范化 header 中 available_externally/internal/dso_local，全文需相等。Internal setter 的隐含 visibility/DLL 变化若在当前输入上出现额外未准许拼写变化，则会被全文门拒绝，不会悄然接受。实际 .3 名单/原预算及八 .4 verify 继续独立执行；缺一份成功 hook 或任何新额外文件都不能生成完整 link receipt，runner 只能 Incomplete 并在 native 前停止。

尚未由本审证明的事项仅是实际正式六编译/54native 链、质量结果与性能，及 Root 另行执行的有效地址控制结果。现有静审不增加新采样或复杂 mock 阻塞门，不豁免失败后完整保留，不授权超出固定两 case 的扩测或生产采用。
