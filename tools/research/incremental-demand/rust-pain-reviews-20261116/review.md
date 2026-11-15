# Rust 痛点与实现对照复核

2026-11-16（Asia/Shanghai）。这是对既有结果的复核，没有执行新编译、修改编译器或产生新性能数字。ZIL 测量始终绑定 `027695318436fcb1f8994d0dc9802a0b2349c362` / window05；实际对照读取同版 rustc `58cf8f9c35b355199727627d07ae34a4e2846d67`、Cargo `5f94df4789f005f9a352888e8355ffc645b7ed0e` 源码。两份 Astra 审核和原始派生数据分别见 [ZIL 成本定位](astra-review.md)、[Rust 实现对照](rust-implementation-review.md)、[时间与空间](audit.json)。

复核结论：既有优化真实减少了有限合成项目的原生冷编译等待和短编辑历史占盘，但尚未证明解决 Rust 的核心产品痛点——大工程中的持续编辑等待，以及为维持增量性能必须保留的磁盘工作集。完整18请求总等待领先24%/24%/17%仍是有效的限定结论；不能把它改称所有冷/热场景领先，更不能推成工业大项目或相对 C++ 的量级优势。

Rust 的实际痛点需要按使用场景理解：

- 新 checkout、清理产物、切换工具链/目标/profile/features 后，依赖图及受影响实例可能大量重建。泛型单态化、宏展开、LLVM 优化和链接各有成本，不能只用源码行数预测。
- 已保留缓存时，完全未修改的 Cargo Fresh 与修改源码后的 rustc 增量编译是两条路径；常驻进程内复用又是另一种起点。三者必须分别报告。
- 大 `target` 不全来自增量缓存。还包括依赖的 rlib/rmeta、对象、调试信息、构建脚本/proc-macro、测试目标，以及多个profile/target/features组合。即便关闭增量，也可能保留很大的构建产物。
- 清空整个 target 会移除有用产物；只清旧历史、只清增量数据、保留当前可达工作集的代价不同。必须同时观察清理后下一次私改/接口改，而非只测无修改命令。

这些区别来自与实测相同 Cargo 版本的 [profiles](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/doc/book/src/reference/profiles.md#L216)、[build cache](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/doc/book/src/reference/build-cache.md)、[clean](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/doc/book/src/commands/cargo-clean.md) 及实际实现。该版 dev 默认有完整调试信息并开启增量；release 默认不开增量。上轮明确选择 O2/CGU16/jobs1、release 增量开启、无调试信息，且仅一个无第三方依赖的 crate。该选择适合固定原生对照，未覆盖通常更大的 debug/test、多包 workspace 或依赖下载缓存，不能据此宣称解决那些场景。

| 痛点 | 既有事实 | 复核判断 |
| --- | --- | --- |
| 原生冷编译 | 5105档 ZIL 3.835s / Rust 6.719s | 此限定输入真实改善；尚无 ZIL 分钟级工业项目成功证据 |
| 常驻私改 | 5105档8次私改 ZIL 10.754s / Rust 9.859s | 仍落后；1000/2500档的局部收益不能抵消此单项失败 |
| 保留磁盘缓存后重启 | 5105档 ZIL 2.232s / Cargo 0.0217s | 端到端明显落后；Rust 六个重启样本全部 Fresh、没有启动 rustc |
| 用户构建产物占盘 | 2500档边界峰12.846 / 36.670MiB，8私改增长1.041 / 11.527MiB | 短历史真实变小；未证明长期有界、清理后继续快或 debug/test 同样领先 |
| 清理与热性能的联合取舍 | 有空缓存 cold 与保留缓存 restart 两个点 | 未测等预算、历史回收、部分清理后再编辑，不能声称核心矛盾已解决 |
| 大项目准入 | 10105/25105函数项目均 lex token 超限；默认另有全项目10000函数上限 | 在现有数值与全项目预算语义不变时不可能通过；不能归为单纯性能重复工作 |
| IDE/check、泛型/宏/async/多包项目 | 当前027成功项目的正式新窗口为原生O2；扩大check是容量失败观察 | 未覆盖的当前配置不能由旧源码check数字或原生结果代替 |

同一原始Source、两block中位数的联合观察如下；磁盘为对应请求完成后的用户产物实际分配量，包含诊断IR，全局inode去重。它不是达到该时间所需的最小缓存量，也不是构建瞬时峰。

| 档位 | 空缓存cold ZIL/Rust s | 保留缓存restart ZIL/Rust s | cold占盘 ZIL/Rust MiB | restart占盘 ZIL/Rust MiB |
| --- | ---: | ---: | ---: | ---: |
| 1000 | .763 / 1.042 | .457 / .0310 | 3.100 / 5.938 | 9.061 / 19.176 |
| 2500 | .802 / 1.332 | .379 / .0384 | 4.459 / 12.398 | 12.846 / 33.311 |
| 5105 | 3.835 / 6.719 | 2.232 / .0217 | 17.422 / 29.855 | 46.820 / 58.691 |

尤其不能推断“Rust需要58.69MiB增量数据才有21ms”：Cargo Fresh检查已有指纹/依赖/输出，不调用rustc；修改源码后才真正使用编译器增量缓存。本次只读盘点5105/block00保留的incremental目录，去重后dep-graph.bin约36.109MiB、query-cache.bin约20.688MiB，对象约.828MiB、pre-LTO bitcode约.195MiB。本例主要空间是依赖图与查询值，不是最终二进制，也不能把大部分空间归为bitcode。这是原始保留文件的分类诊断，不是新测量或回收试验。

已逐项对照的 Rust 实现，以及对 ZIL 施工的约束：

| 机制 | 实际 Rust 路径 | ZIL 当前差距与可借鉴处 |
| --- | --- | --- |
| 无改动 Fresh | [Cargo fingerprint::prepare_target](https://github.com/rust-lang/cargo/blob/5f94df4789f005f9a352888e8355ffc645b7ed0e/src/compiler/fingerprint/mod.rs#L459)，499返回new_fresh；2044比较指纹和fs_status | 解释21ms产品路径；ZIL独立当前Verify合同仍保留，不能照抄为跳过Verify的快捷入口 |
| 真实依赖与结果未变截断 | [DepGraph::with_task / try_mark_green](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_middle/src/dep_graph/graph.rs#L791) | ZIL接口仅2个语义结果改变，却清空全header环境相关产品。沿实际读取的header/ABI/名称绑定传播；结果完整等价则停止传播，保留负依赖和动态读取顺序 |
| 先判断CGU复用，后构造LLVM | [determine_cgu_reuse](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/base.rs#L1238)，842选择cached或codegen；[ModuleLlvm::new](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_llvm/src/base.rs#L64)在后 | ZIL16对象全命中仍先lower5105、整图变换/分区。优先让完整当前语义授权后的unit输入选择早于昂贵机器展开，而非先把整个BackendSession落盘 |
| 按需恢复查询值与缓存晋升 | [execution.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_query_impl/src/execution.rs#L459)、[persist/save.rs](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/save.rs#L60) | Rust已有索引/按需恢复；save72明确指出小改时晋升仍有不少解码/重编码。ZIL可研究稳定共享块延续以减少此工作，但需处理当前身份映射、schema和完整性，不能凭Rust TODO宣称已超越 |
| 增量文件共享与历史回收 | [persist/fs.rs copy_files](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/fs.rs#L364)、[session GC](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_incremental/src/persist/fs.rs#L590)、save92回收旧work products | Rust已有hard-link/copy、成功发布后GC、锁保护使用者并保留当前/最近可复用session。实现GC本身不是领先；要比较同等热能力的保留工作集与回收后等待 |
| 主动函数去重/合并与导入 | [MonoItem collector](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_monomorphize/src/collector.rs#L354)、[CGU partition](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_monomorphize/src/partitioning.rs#L220)、[O2 MergeFunctions](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/back/write.rs#L239)、LLVM wrapper655 | Rust已有生成项去重和LLVM合并。ZIL的受限完整Function等价发生在导入/分区之前，减少后续构造；不是“Rust没有去重”。5100eligible→156classes高度依赖本输入重复率，独特函数/泛型收益未证 |

Rust `lto=false` 也不等于显式关闭所有本地LTO：本窗实际rustc命令没有`-Clto`，该版本[Session::lto](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_session/src/session.rs#L844)在O2/CGU16等条件下默认ThinLocal，绿色CGU先选PreLto产物。ZIL不能仅凭源unit本体未变就复用受导入/跨unit优化影响的最终对象。Rust的实际存盘存在性/稳定hash规则也不是ZIL独立Verify、原始完整体检查和损坏校验的替代品。

下一次交付应沿一条链实施：实际读取的函数/header/ABI/名称绑定 → 结果等价截断 → class代表及反向调用/完整导入依赖更新 → 当前unit输入与既有对象选择 → 仅变更unit展开机器模型/检查/full O2 → 当前引用索引发布与有界历史回收。保留ZIL现有表示及ClosedBody/WorldBound授权链，不新增通用IR/另一套证书系统；未知、缺件、坏件和取消走原正确生产/发布规则。Analyze与Verify各自证明依赖。

Astra定位的首要累计成本是5105八私改machine+partition **4.777s**，不是已经可全部省掉的时间；接口对应Expand+machine+partition约1.388s，重启machine+partition约1.412s。私改根unit对象工作3.828s，其余15unit仅.121s；优化空unit命中数量不足以解决主要等待。完整导入曾恢复运行质量，缩小导入必须另做整链实验，不能删O2/降运行质量换速度。

验收应同时报告“保留全部 / 清历史保当前 / 删除缓存”后restart→私改→接口改的绝对等待和占盘，并扩展长编辑历史、低重复体、真实多包/泛型/宏/async及debug/test。主原生比较仍actual main/候选/Rust、CGU16/O2/jobs1，检查分开，独立Verify、默认预算、完整O2和运行质量门不减；实际默认debug/test另设身份，缺少同等调试功能则标未覆盖。不能全清Rust却保留ZIL工作集，也不能由八次增长外推长期线性增长或永久有界。

容量合同单列：当前默认1,000,000 token与全项目10,000函数；10105/25105真实函数输入在语义不变时无法满足函数上限。包级独立编译/接口边界或容量合同是显式产品设计，不能每文件重置预算、裁源或提高额度后冒充性能优化。源码对照不能消除此逻辑约束。

后续每个落后点和主动优化方向均需记录：固定Rust版本的实际文件/函数、对应ZIL消费者、不同代价来自何处、借鉴与有意保留的安全差异、整链改动和统一验收证据。没有找到实际对应或尚未核对的部分标未知；不以“Rust没有该优化”作为无源码依据的前提。本次已闭上述机制对照，不宣称审完Rust编译器全部优化。
