# 编译效率专项：组合实现与最终对照

最终生产实现：`5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`，分支 `research/incremental-compilation`。从开工 main `21058d0e331de764e3a1b9123c3e1edf5e55a1aa` 到最终版本直接比较，512模块常驻暖 check 从 **105.515 ms 降到38.183 ms（2.76倍）**，128模块暖 build 从 **57.034 ms 降到43.331 ms（耗时降低24.0%）**。冷启动CLI总体没有建立稳定突破，私有修改收益也远小于无修改请求。

这次交付包含实际实现、修复首轮内存回退、真实源码发现的O2错误修复、四个完整计量窗口、独立审核与可复现原始证据。它没有完成函数级语义增量，也没有把有限语料提升为整个产品的Rust四目标结论。

## 查明并处理的问题

| 原问题 | 已交付处理 | 仍然存在的边界 |
|---|---|---|
| 默认32项缓存超过容量后下次请求整批清空；28/29模块出现断崖 | 按访问次序淘汰raw历史，保留剩余根可达依赖收据 | 项数上限不是字节上限；512模块仍不能保留全部Parse |
| 连续修改时新旧整项目IR同时存活 | 新epoch开始前释放过时大项目快照；生命周期测试和完整复测 | root数字相同的不同项目可能保守互相淘汰；不是总内存预算 |
| Parse因剩余额度变化重复执行相同源码 | 成功费用收据；当前额度重新准入并扣费，失败回原解析器 | 不缓存失败的接纳结论；语义阶段仍用完整额度身份 |
| 指纹临时字节串、Verify和Lower多次深复制 | 保留v27字节编码的流式哈希、不可变Arc输入、借用LIR | 仍有哈希和全量遍历，不把指纹作为安全授权 |
| 每模块展开都重建全项目声明索引 | 项目级有序只读索引共享 | 展开、CT、类型和分析仍按整项目重算 |
| LLVM命名类型DAG重复走指数条路径 | 单请求成功子图高度备忘，额度不足回原顺序DFS | 200,000例精确旧/新结果对照属于机器IR层，不声称源码性能收益 |
| 无关extern声明进入所有CGU、导致所有对象失效 | 按结构化机器IR实际引用选择导入声明 | 保留所有NamedType、全部未用非法声明检查；细节见SLICE1 |
| 原基线LLVM API O2错误折叠负整数返回路径 | 在LLVMConstInt前按目标位宽规范化位模式 | 这是必要的正确性修复；旧错误产物不参加等价加速计算 |

组合优化的身份、预算、生命周期、失败/取消及安全边界见 [SLICE2](SLICE2.md)，首片见 [SLICE1](SLICE1.md)。CP33/34/35分别适配自旧研究提交 `675d1ac9`、`9fe23d71`、`de0eefdc`；没有把旧版本语言实现覆盖回来，也不叠乘各候选历史收益。

## 最终性能结果

主表来自最后完整的 main→最终窗口。每项为5个交错区组内均值的中位数，单位ms；每区组ABBA/BAAB。私有修改为5个连续新版本，未用同一改动反复制造命中。完整单元、p95观测值、RSS及所有不利结果在 [MEASUREMENTS](MEASUREMENTS.md) 和[证据目录](../../../evidence/research/incremental-compilation/slice2/README.md)。这里只陈述点估计，未作正式产品阈值或统计准入判断。

| 常驻Session，默认容量32 | 开工main | 最终 | 耗时变化 |
|---|---:|---:|---:|
| 32模块，暖check | 4.574 | 1.724 | −62.3% |
| 128模块，暖check | 20.111 | 8.250 | −59.0% |
| 512模块，暖check | 105.515 | 38.183 | −63.8% |
| 512模块，私有修改check | 108.016 | 104.204 | −3.5% |
| 32模块，暖build | 30.159 | 25.926 | −14.0% |
| 128模块，暖build | 57.034 | 43.331 | −24.0% |
| 128模块，私有修改build | 59.823 | 58.935 | −1.5% |

512模块连续私有修改后的Session RSS中位数为65,246→64,828 KiB。第一组合版本曾升到91,242 KiB并出现时延回退；原始窗口完整保留，释放过时owners后另做整窗复测。此处RSS是请求后的常驻量，不是峰值。CLI由同一子进程wait4报告最大RSS，不与Session混用；小CLI受启动器/内核约25MiB观测下界影响。

最终对首片基线的另一独立窗口中，512模块暖check为105.360→37.950 ms、私有修改111.983→103.595 ms。两个最终窗口私有修改改善幅度不同，不能挑较好的7.5%替换主表3.5%。128模块私有check在两个窗口也有明显波动。暖check在各完整窗口保留了约2.4–3倍的收益；大容量8192暖check控制在最终两窗只改善约1.3%/2.2%，说明主要收益确实来自原默认容量断崖。

完整CLI包含启动、读源、当前Verify、后端、链接和发布。最终main对照中，32模块冷build为47.777→47.392 ms，128模块为81.743→82.068 ms；32模块无修改build为36.251→36.771 ms。小幅改善和回退并存。memory-fix / stage2-final-frozen窗口128模块无修改CLI check曾退步17.9%，同样保留。不能把常驻暖检查加速解释为独立CLI、任意私有编辑或所有冷编译都快了数倍。

八组真实源码又完成320次CLI编译、320次本机执行，另有16次clang文本路径O0/O2参考编译和执行。它们覆盖字节流、资源闭包、泛型Store、混合捕获、CT选择、类型数组、where Copy、共享Product。语义有效单元的耗时比约0.962–1.051，未证明普遍冷编译提速；where Copy O2的5.1%不利点保留。byte_io旧API O2的10个错误结果全部记录，速度比为null。

## 测量与证据身份

| 窗口 | 基线 | 候选生产源码 | 目的 |
|---|---|---|---|
| initial / stage2-frozen-v2 | 首片5d7c1eeb | b230cfdd（工具提交d66a3142） | 暴露512模块内存回退 |
| memory-fix / stage2-final-frozen | 首片5d7c1eeb | 646099a7 | 修正owners寿命后的完整重跑 |
| final / stage2-corrected-frozen | 首片5d7c1eeb | 5cf1c417 | 加入常量正确性修复后的完整重跑 |
| main-final / stage2-main-final | 开工main21058d0e | 5cf1c417 | 直接量化整个专项，无收益叠乘 |

每窗1,560次完整请求：400次ZIL CLI、960次ZIL Session、200次直接rustc；1,360次成功ZIL请求均有当前Verify，620次构建产物作native结果控制。四窗分别保留，不混成同一个候选的大样本。首片ELF编译自665a0a06，与5d7c1eeb生产源码完全相同，仅后续测试断言/文档不同；ELF SHA在每窗manifest。最终受测ELF、probe、工具、环境与不可变规范文件摘要均留证。

每窗300次CLI native保留原始stdout；320次Session native由probe在输出计量行前断言退出码/stdout，未单独保存每请求完整report或native stdout。Session行保留阶段执行/复用及Verify事件，`verification_runs=1`也由probe断言。前两窗CLI文件后来重建覆盖，原hash和源码身份保留，当前不能再次对旧ELF文件重哈希；对应旧probe仍保留。这些历史产物不冒称随Git分发。

机器为Linux x86-64，固定CPU0/1，不清OS文件缓存，不重置热状态。Rust 1.99.1使用LLVM23.1.1；ZIL固定LLVM23.1.4。每叶8个普通函数，CLI32/128模块，Session32/128/512模块，容量32及8192控制。源码构造、测试进程的原生执行在计时外；build内部链接和发布在计时内。阶段事件间隔含driver中间工作，不能当排他CPU剖面。

Rust参考为直接rustc、O0、overflow checks、8CGU及每序列持久的`-C incremental`。check仅emit metadata，并非Cargo check。完整CLI的候选/Rust比保留于表格，但此普通整数语料在旧ZIL上已有优势；不据此宣称本次新赢得全部Rust编译成本、相同运行效率或真实Cargo工作区。

另试验GNU ld与LLD的20次完整构建及native检查，只得到约2%的暖请求点估计改善，首轮启动异常和原行保留。未改默认链接器，未把它计入当前实现收益。

## 正确性与独立审核

最终5cf1c417生产源码上，`verify_scoped.sh`的 **486项限定测试和受影响库Clippy（-D warnings）通过**。验证包含原错误拒绝、费用N/N−1、取消/失败后fresh等价、连续编辑与容量、指纹旧编码、LIR篡改、当前核验次数、泛型/CT/资源及原生对象缓存；这不是正式整仓验收。独立功能审核依次绑定8fb69d61、b230cfdd、646099a7和5cf1c417，均有精确收据；重复执行不累计为新覆盖。

真实byte_io在旧API O2返回2、输出字节正确；O0与clang文本路径返回0。最小反例单独IPSCCP即可复现。向`LLVMConstInt(i32, (-2) as u64, SignExtend=0)`传入非规范高位违反底层位模式构造前提；打印/bitcode往返会掩盖状态，clone不会。修复明确截到1/8/16/32/64位，不关闭优化、不插入文本往返，也不跳过Verify。独立90组C API构造控制、全部整数位宽的相同/相邻位模式、真实负分支/循环/含NUL与高字节流都留证。

跨编译器共用同一对象缓存的作者及非作者控制分别证明：旧版cold/warm仍返回错误2；新ELF第一次miss、返回正确0，随后hit仍正确0。命名空间既有的编译器ELF身份绑定已经隔离旧坏对象，无公共缓存schema变更。提案见 [changes](../../../changes/research-narrow-integer-constants.md)。

## 更大幅度私有编辑突破需要什么

当前最主要的结构问题仍是：整个Project输入摘要直接决定Expand/CT/Types/Analyze身份。私有函数变一个token，绝大多数未变函数仍参与重算。512模块最终控制中，一次私有修改仍执行484次Parse、复用28次，Expand/CT/Types/Analyze各完整执行一次。把这些整项目调用再包一层memo或只增加容量不能解决传播边界。

1. 建立独立于局部紧凑编号的稳定声明/实例身份，按Signature、Body、Layout、Effect、CTValue拆分结果；先贯通普通函数，再扩展泛型/CT。公共签名相同不能授权复用依赖泛型正文、CT值或效果的消费者。
2. 记录实际查询读取和负名字查找；计算结果相同即停止传播。失败/取消需事务回滚，删除边要撤回旧事实，递归效果按真实SCC重算。图切割/zit适合帮助计算受影响闭包和完整合同边界，不能代替语言依赖建模。
3. 将当前核验之后的machine工作产品与链接动作分开建模。对象全命中仍不免去当前machine构造和链接，链接缓存还必须覆盖启动对象、库、脚本、工具、选项及发布语义，不能仅看对象文件名。
4. 当前Verify保留O(N)扫描下界。先研究可独立检查的分段摘要/归纳证书，再讨论接近O(变化量)的全链路；不以对象命中、哈希相等或Analyze旧结论替代当前安全授权。快速开发后端只在后端成本足以抵偿工程与代码质量代价时值得引入。

已按固定来源研究Rust查询/稳定身份、Zig依赖种类、Salsa、Adapton、Build Systems à la Carte、Cranelift和差分数据流，详见 [外部研究](audit/external-notes.md)；历史图切割、zit、COMPILE5/7及CP32失败教训见 [开工审计](audit/compile-speed-review.md)。这些研究支持上述边界设计，没有外部速度数字被移植成ZIL实证。本次尚未实现的函数级语义增量明确保留为未完成。

## Rust四目标复核

| 目标 | 本轮实证 | 未证明 |
|---|---|---|
| 安全 | 当前独立Verify保留、拒绝/预算/失败边界对照，修复真实O2误编译 | 完整语言安全与全部平台 |
| 性能 | 真实原生行为、资源控制和优化等级保留 | 同语义Rust运行时间/内存全面可比 |
| 语法/维护 | 无语言语法变更，保留owned Lower/后端入口并增加借用入口 | 框架级表达和维护成本优势 |
| 编译 | 默认容量常驻暖check及部分暖build有重复、完整请求收益 | 普遍冷编译突破、函数级增量、正式产品门 |
| 至少一项突破 | 限定常驻工作负载得到约2.4–3倍检查加速 | 四目标在同一最终产品上的合取及产品级净突破 |

`Cargo.lock`、公共wire schema、默认逻辑预算及`validation/product-gates.json`保持原合同。正式任务继续Doing；本分支没有合入main，也没有由施工方运行正式全量或改写集成方的验收状态。

## 复现

云环境已保存安装/启动配置；本机实际前缀为`. /workspace/.zil-env/env.sh`，工具链固定如上。两个源码提交分别构建release，放在不同target目录；用`build_probe.py --target DIR --out PROBE`分别链接对应库。测试运行`bash tools/research/incremental-compilation/verify_scoped.sh`。

最终完整命令：

```sh
python3 tools/research/incremental-compilation/combined_bench.py \
  --baseline /workspace/.zil-env/research/target-slice1-baseline/release/zil \
  --baseline-source 21058d0e331de764e3a1b9123c3e1edf5e55a1aa \
  --candidate /workspace/.zil-env/research/target-stage2/release/zil \
  --baseline-probe /workspace/.zil-env/research/stage2-main-baseline-probe \
  --candidate-probe /workspace/.zil-env/research/stage2-corrected-probe \
  --rustc /workspace/.zil-env/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/rustc \
  --out /workspace/.zil-env/research/stage2-main-final
```

输出目录要求不存在。完整原始行、源码与工具摘要、LLVM IR、编译报告、native结果及失败窗口已进入Git证据压缩包；可重建的可执行文件/对象缓存不打包。解压后的FILES.json逐文件绑定SHA-256，外层manifest绑定各压缩包。
