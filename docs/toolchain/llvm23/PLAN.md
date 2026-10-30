# LLVM 23 一次性全库升级

## 目的与基线

2026-10-30（Asia/Shanghai）核实远端main为ad509b5003d9d2a303428f9d01732292988084d8，
research/compile-performance为fb87916d5e7f64a460408f75893928511c0fa35b。
两者ZIL原生后端仍链接LLVM17、运行时严格要求17.0.6；宿主Rust1.99.1自带LLVM23.1.1
是独立工具，不能算升级完成。

用户授权的一次性分支chore/llvm-23-upgrade从最新main建立。编译专项CP02默认关闭成本
仍为Hold；本次不携带CP02或其他算法候选，以便独立交付和归因。

## 固定版本与写集

目标为官方稳定LLVM/Clang/LLD23.1.4。tools/llvm/version.txt统一版本pin，构建脚本
生成原生运行时常量和链接名；checker核对真实DSO、Clang、LLD、Rust、Cargo及profile。
固定default<O0>/default<O2>、x86-64 generic、PIC、单CGU默认和Linux系统链接器选择，
保留LLVM前后核验及每请求独立ZIL Verify。错误版本和隐式回退不可接受。

范围覆盖FFI构建与收据、真实上游ABI核查、当前安装与CI入口、版本检查、文档、profile
及本单证据。历史实验脚本和收据保留旧工具身份，不批量替换版本号。Linux x86_64宿主
与Win64交叉目标分列，不能据此宣布Windows宿主或Windows本机验收。

根Cargo.lock只有15个本地workspace包，没有外部依赖。Rust/Cargo1.99.1已是稳定最新版。
独立Roam工具的serde_json1.0.154及其锁定传递依赖、可选Playwright1.64.1也无需更新。
CI checkout/upload-artifact新稳定版本作为独立维护提交，不记作编译性能收益。

## 发布包与共享库

官方Linux X64发布包不含libLLVM共享库。apt.llvm.org当日23分支提供的是尚未稳定发布的
23.1.5开发快照，因此不作为23.1.4替代。使用完整SHA256核对后的官方23.1.4 PIC静态
组件，按上游llvm-shlib whole-archive、符号版本和SONAME规则链接所需X86共享库。
安装收据单列本地链接、组件哈希和实际系统运行库，不把它标作官方预编译DSO。

## 规范输入迁移

spec_hash保持245cf29a95d721da6268c43191204777a1015639cba56e8e9025aeac012d0a2c。
policy/dependencies.json在规范输入集合内；把其中受审计上游改为LLVM23使normative_hash
从4d5480fd3f7de30f6a846489e624ce01bb64d7e835c7e01930bd3410a2206db1
变为8b4b3d219999c8f2904b276e051745bf65f9761e421630bdb4bbe0ec2781e02d。
语言准入、ABI规则、预算和产品门不变。历史工单摘要不回填；集成时由集成者登记在途任务
的工具链输入迁移。

## 切片及验收

1. 固定官方发布、下载摘要和独立前缀，记录实际版本、DSO身份、API符号和资源目录。
2. 核查全部已用C函数、硬编码枚举和所有权；迁移构建、运行时拒绝、收据与checker。
3. 限定回归：LLVM叶子text-reference、driver llvm_api208/object_cache212、无默认feature拒绝、
   新主线完整Product/closure消费者、版本误配及跨版本缓存隔离。
4. 集成方运行既有格式、默认workspace、参考workspace、release CLI、208真实消费者/分配失败、
   all-target Clippy六项闭包，增加显式text-reference测试，保留实际失败和原日志。
5. 非作者绑定精确提交审核。主线接受、运行质量和性能结论分列，未运行不记Pass。
6. 验收后通过PR进入main，再让research/compile-performance吸收新main并开启LLVM23基线。
   一次性分支可删除，长期研究分支继续保留。

对象缓存已有真实编译器、dladdr定位的DSO、LLVM版本和目标优化配置身份。不任意改格式，
实际检查17缓存不被23命中、23重复请求可命中且当前Verify仍执行。

## 性能研究衔接

升级本身不作为性能Go。17/23对照须固定同一语言源码、Rust、target、CGU/worker、缓存
策略和输入；编译wall/CPU、原生运行质量、对象体积分列。同名LLVM pipeline会随版本
改变，不能从版本更高或局部样本推出整体收益。CP01/CP04负结果保持原LLVM17身份。

## 官方来源

- 稳定LLVM发布：<https://llvm.org/>；<https://github.com/llvm/llvm-project/releases/tag/llvmorg-23.1.4>
- C API精确源码：<https://github.com/llvm/llvm-project/tree/llvmorg-23.1.4/llvm/include/llvm-c>
- ParseIR所有权：<https://github.com/llvm/llvm-project/blob/llvmorg-23.1.4/llvm/lib/IRReader/IRReader.cpp>
- Rust：<https://doc.rust-lang.org/releases.html>
- CI维护：<https://github.com/actions/checkout/releases/tag/v7.0.1>；<https://github.com/actions/upload-artifact/releases/tag/v7.0.2>

具体执行和未完成项见本目录RESULT.md及evidence/toolchain/llvm23，不以此计划冒充完成。
