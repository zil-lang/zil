# bounded source imports：独立草案

本目录仅承接从main990独立发展的cost32 bounded source imports机制，不携带a60 layout投影。生产边界仍是显式incremental-o2+after、原cost32及原source资格条件；default/off、完整O2、独立Verify、当前完整机器预检和身份检查不能放宽。本阶段只完成脚本副本、AST及静态逻辑检查，尚未prepare、编译、运行native或采样；旧multi-edit脚本、协议和raw均不修改。

## 先运行质量门，再允许扩大

`prepare_runtime.py`未来接受冻结A/B/R/clang身份，生成8个case：N64/256 × base/k1/k16/k64。所有叶子纯函数字节及main中的纯函数前缀必须与原v2 corpus完全相同，只替换main为100000轮：`seed=(input+i)%251`、`acc=(acc+value(seed))%1000003`。每次seed随i改变；不存在固定seed调用被循环外提的写法。代码的合法优化仍由各编译器决定。

独立Python数值模型不读编译器输出，不调用源码生成器的model/body_parameters；对251个seed求值，以`100000=398×251+102`聚合完整周期和旋转尾部。所有算术非负：N256每次value至多510003，acc+value至多1510005，input+i至多100254，均远小于i32上限；每个叶子中间值亦受mod251约束。输入41用于warmup和正式计时，00/ff各作额外功能oracle。

每case每arm：1次warmup+6次formal+2次额外功能，共9次native。正式顺序六轮为ABR、BRA、RAB、ARB、RBA、BAR；每arm在每个位置出现两次，任意两arm的先后关系各三次。8case总计24次clean编译、24次warmup、144次formal、48次额外功能，216次native。完整wall/CPU/RSS、stdout/stderr、失败、超时和长尾全部保存，不剔除样本。每case按六次formal完整wall的算术均值固定要求B/A≤1.10且Rust/B≥.90，8case必须全部通过；另报8case比值的geomean，不能替代逐case门。

`runtime_gate.py`默认不执行。未来`--execute`前必须核冻结脚本、source、A/B/R二进制及clang；ZIL命令显式`--clang`，当前ZIL_CLANG须resolve同一pin并SHA一致，Rust使用`-C linker=同一clang`。ZIL每次clean编译必须有当前Verify1、LLVM23.1.4、full default<O2>+after及每unit完整当前预检。A/B机器码可以不同，但每个独立native oracle都必须成立。

每个process调用无论正常、timeout、cancel或launch异常，均先落`process-attempt.json`，保存argv、退出/异常、elapsed、已reap CPU/RSS和独立cleanup信息，不保存环境变量。返回后先落raw-metrics再解析JSON/验证输出。取消保留partial attempt并写fatal失败收据；门失败或未完成绝不扩展后续编译/并发矩阵。

## 编译screen复用原语料与频率

`fixture.py`与已修正unsafe语法的原v2生成器字节完全一致。新`prepare.py`未来会逐字段检查原`final-inc-after-v2/protocol.json`：N64/256、k1/4/16/64、三state、ABBA/BAAB、两block、CPU0/1、CGU8/jobs2、动态纯函数源码、oracle及假设频率cold=.10/warm=.45/resident=.45均不变。prepare若发现参数、语料或oracle不同即拒绝，不允许为新机制调语料。

新机制可能合理改变机器码，不能沿用a60跨arm同ELF门。现在每snapshot每arm各做一次fresh/no-cache完整O2 clean oracle；该arm所有seed/edit必须与自身clean ELF精确一致，materialIR仅去唯一verified-fingerprint行后精确一致。跨arm仍记录ELF/materialIR同异，但差异自身不构成通过或失败，独立功能oracle和先行运行质量门才决定能否继续。

每策略192次ZIL测量、128次seed、20次arm-local clean oracle，共340次ZIL编译；可选Rust32次测量+16次seed，共48次编译。每次产物作三输入功能验证，共1020次ZIL native；可选Rust另144次native。这些是短功能检查，不能替代先行216次真实运行质量门。

`screen.py --execute`必须给`--runtime-summary`：它验证8case全部过门、24/216/144完整执行计数、所有输出有效，并匹配exact A/B source claims、编译器SHA、Rust（若接入）、clang和原corpus SHA；失败或不一致拒绝执行。该收据随新run保存。原完整wall加权方法保持不变，先加权绝对均值再求比值，不以同异码分类挑选样本。

## 将来的命令顺序（本阶段未执行）

先由非作者审核本目录脚本，再由root绑定新生产源码及对应构建收据。加载既有toolchain/env.sh，显式传同一个clang；先prepare runtime并运行runtime_gate，通过后才prepare/运行340编译screen，之后才考虑独立c2/c4候选并发。所有输出目录必须新建。

```bash
python3 out/bounded-imports/prepare_runtime.py --out RUNTIME_PROTOCOL_DIR \
  --baseline FROZEN_BASELINE --candidate FROZEN_CANDIDATE --rustc FROZEN_RUSTC \
  --clang FROZEN_CLANG --baseline-source BASELINE_FULL_SHA --candidate-source CANDIDATE_FULL_SHA
python3 out/bounded-imports/runtime_gate.py --protocol RUNTIME_PROTOCOL_DIR/protocol.json \
  --out RUNTIME_RUN_DIR --execute
python3 out/bounded-imports/prepare.py --out COMPILE_PROTOCOL_DIR \
  --cpus 0 1 --frequency-policy observed-uncontrolled --clang FROZEN_CLANG
python3 out/bounded-imports/screen.py --protocol COMPILE_PROTOCOL_DIR/protocol.json \
  --baseline FROZEN_BASELINE --candidate FROZEN_CANDIDATE \
  --baseline-source BASELINE_FULL_SHA --candidate-source CANDIDATE_FULL_SHA \
  --rustc FROZEN_RUSTC --runtime-summary RUNTIME_RUN_DIR/summary.json \
  --out COMPILE_RUN_DIR --execute
```

`concurrent_screen.py`由另一作者单独负责，本目录其余脚本没有写它。它需复用新screen的validate_zil/artifact_identity/native_checks/validate_clang_pin接口，使用各arm自身clean oracle，并同样以匹配的runtime通过收据为执行前置。并发2/4候选和一次多文件修改保持两条独立轴。
