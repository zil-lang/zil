# 208：语言177＋Ante-R1＋207的LLVM API可用首版

日期：2026-10-28。范围是可选的LLVM API对象后端，不是完整语言或产品验收，不宣称本轮已获得编译速度百分比收益。

## 1. 实际基线与采纳范围

从174–177实际Git历史恢复`08cb9e6ff57492e11eec6c8592d37d601ee71f10`，保持schema/Wire21。在这套现有语言实现上移植204所有权转移优化、206有界并行代码生成及207声明裁剪。201/202不进入本版。供体207为`de7ce29528bab6567b9c524788cb2e62e29f6dc6`。

用户上传`_zil_design_1.4.md`逐字归档；其规范标识ZIL-LANG-1.3保持。根规范只更新原173/Wire20的修订状态段，正文规则与原上传一致。具名接口/诊断/依赖附件、摘要与生成阅读视图同步；没有新增语言指令或放宽产品门。

新增的来源路径精化、按项/SCC查询、责任形状、稳定声明身份、结构化CGU、机器对象缓存仍是设计义务，不能因更新文档而记作实现。177自身已实现的State拥有Store、只读元素窗、具体回调字段通过现有LIR进入本后端。

## 2. 为什么这样分层

```
当前源码 → Types/Analyze → 独立Verify → 绑定当前核验的LIR
                                     ↓
                              既有共用LLVM发射器
                                     ↓
                       LLVM文本／既有CGU过渡分区
                        ↙                         ↘
                Clang参考编译               进程内LLVM API
                                           单元Context+TargetMachine
                                           解析→核验→优化→核验→对象
                        ↘                         ↙
                          共有链接与driver发布事务
```

Rust的参考点是codegen、join与link职责边界；Ante的参考点是每单元独立上下文、对象字节结果，以及O0并行/O2整体优化的区分。没有复制Ante的宿主目标假设、把按体积贪心分箱当稳定身份、或核验错误后只打印再继续发射的路径。外部引用路径、commit/blob记录于REFERENCES.json。

新增`zil-llvm-api`是无外部Rust依赖的叶子crate，FFI只在私有`native.rs`；原有安全crate和Verify仍禁止unsafe。安全接口返回拥有对象字节与实际配置receipt，不外泄LLVM原生指针。每单元独立Context/Module/TargetMachine并RAII释放，目标注册一次初始化，线程之间不传递原生句柄。原LLVM发射器及Verify源码与177逐字一致。

首版已接通`zil build/run --backend llvm-api`，支持既有O0/O2及显式有界CGU并行。API在编译器进程内完成优化和机器码生成，Clang只用于Linux链接；不是启动Python工作进程，也不再为每单元启动Clang编译。Win64使用显式lld-link。

**仍然解析LLVM文本，尚非LIR直接LLVMBuilder。** 原207的文本扫描分区保留为过渡，未知形式保守回退单模块。新接口没有倒逼重写前端，也没有偷偷去掉文本成本。

## 3. 正确性与失败处理

固定libLLVM17.0.6，generic x86-64 CPU、空附加features、PIC/default code model和显式目标三元组。按目标而非宿主设置data-layout，拒绝三元组冲突或不兼容已有布局。优化前后进行LLVM结构核验，核验失败即返回错误；它不替代每次完整请求的ZIL独立Verify。

解析、核验、优化、对象及链接失败不会返回新的可执行产物。driver在私有暂存里完成链接和最终取消检查，再按已有事务发布。旧成功产物保留但不作为失败请求结果。取消是阶段间协作检查，不能安全中断正在运行的LLVM pass或外部链接器。LLVM是可信原生依赖，不是恶意IR沙箱；进程崩溃可残留私有临时文件，不产生成功收据。

feature关闭时选择API明确返回Unsupported/E-LLVM-API-UNAVAILABLE，不能静默切回Clang。默认仍为Clang、1单元；API启用构建的CLI装载时需要libLLVM动态库，即使某次选择Clang。

## 4. 本轮真实执行结果

| 验证 | 实际结果 | 解释 |
|---|---:|---|
| 启用API的完整workspace | 1660通过，0失败，0忽略 | 154个非空测试组；包含大量既有Clang默认测试 |
| 不启用API的完整workspace | 1642通过，0失败，0忽略 | 实测没有LLVM依赖的配置及明确未启用拒绝 |
| API叶子专属 | 9通过 | ELF/COFF、非法IR、支配关系核验、布局、目标、取消及并发 |
| API driver专属 | 11通过 | 当前语言组合、函数值、溢出、借用拒绝、失败产物及缓存/取消 |
| 177消费者真实API对象链路 | 16次构建、128次原生执行通过 | O0/O2×单/多单元×两入口×普通/分配探针 |
| 分配失败注入 | 上述执行中56次 | 输出/退出/清理顺序及allocated/frees/live均匹配旧独立业务oracle |
| Cargo实际依赖图 | 15crate、38边通过 | 读取真实cargo metadata，不是只验证政策声明 |
| 格式及all-target Clippy | 通过 | 早期继承测试的一条lint失败记录保留 |

两套workspace共享多数用例，不相加为独立测试总数。20项API专属测试已包含在1660中；128次消费者执行另列，也不伪装为128个语言特性。完整workspace默认测试保留Clang参照，因此不能把1660全部称为API目标覆盖。

分配测试使用177已有`consumer.py`的业务预期，探针仅链接**API实际生成的对象**；未把发布的`.ll`交Clang重编后冒充API。每个构建核对一次独立Verify、API配置receipt与仅链接的实际进程参数；ordinary/probe的原IR摘要相同。

Win64：独立COFF对象、显式kernel32导入库后的PE32+链接成功；缺少导入库的失败原件保留。没有Windows原生执行或Windows宿主安装验收。177的Store组合在Win64仍由Verify明确Unsupported，没有借后端替换扩大语言接受集。

全局历史handoff最初1153条问题；新增208格式问题已单独修正，不改旧任务声明来冲掉历史错误。最终handoff记录随交付保留，历史问题不是本轮新增编译器失败，也不能登记为全局handoff通过。

## 5. 结果身份和限制

受测实现提交`e59322ff83f44978f0ffc80d873acb82acf35363`，crates tree `18ad86e5edc936fad1ac761b7d2dda3d61e37702`。受测API CLI SHA256：`0371c7d76e498fc9e72c77c3f8c4b3312878cd76cbf4ea15b3e4dd4e1f2b15d9`。后续交付提交只补文档、生成阅读视图及纯源码身份输入支持，生产crates未变化。

未进行正式冻结配对性能实验，不把单次receipt时长当提速结论，不乘上旧204/206/207的百分比。API与Clang使用同一前端/LIR及LLVM版本，但优化pipeline逐pass等价尚未证明；运行质量与完整编译/CPU/内存成本需后续独立测量。206的O2多单元运行质量问题未被本次API接入自动修复，不切默认多单元或默认API。

所有新增实现、测试和复核由同一执行者完成，非作者独立审核NotRun；因此任务保持施工/作者实测状态，不虚构审核合入。完整语言NotSatisfied、产品NotRun、B0 Doing保持。源码包冷恢复验证另以交付外部RECOVERY-208.json绑定实际ZIP，避免循环自证或把未运行检查预记为Pass。

## 6. 后续可直接承接的接口

第一优先保持对象生成/链接边界稳定，补目标ABI与产物质量差分。随后从现有LIR类型族逐类接入直接Builder，复用当前目标配置、对象receipt、取消及发布接口。稳定符号、结构化CGU和对象缓存另线推进，不要求等待Builder迁移完成，也不能拿缓存哈希替代当前验证授权。

安装与真实例子见`docs/LLVM-API.md`，可信边界见`policy/llvm-api-ffi.md`。本版交付的可用含义是：指定Linux工具链下能够从当前ZIL源码经独立Verify、进程内API对象生成、实际链接得到可运行程序，并能明确失败；不是全部终态设计完成。
