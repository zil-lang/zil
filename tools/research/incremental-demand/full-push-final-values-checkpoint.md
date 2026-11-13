# Final native values: complete two-block three-arm checkpoint

源码 `6facee9aea4daec2457b2f353754d6120eae85e0` / binary `46facb1a7e8cd37946835c414d50aeaa479857cc2476f8fb67555ac7cc62667a`，工具 `ffc1aa9770f0eaafc88129ca56547f57879937d0`；main `9170bb4210554392e36eb7528596612596ebef80` / binary `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e`。完整机器结果复用、显式封闭链接profile、early SSA/已证明算术与typed IR运输已共同进入这次候选；仍是研究分支，不合main、不宣称全面领先Rust。精确原始路径、哈希、逐block值与父链见[JSON](full-push-final-values-checkpoint.json)。

check/O2各2block、102有效rows（54测量+48父请求），均含原425、22文件、20×20×16、全部9场景及命名父链。当前独立Verify/错误断言保留。O2共1080原生I/O执行，90条完整ELF归档；main/candidate全部30成功对的完整ELF与函数exports相等。30对raw及仅去第二行fingerprint的LLVM均不同：这是经源审的SSA/算术改写允许的IR表示变化；全ELF硬等、实际16CGU/物理对象/layout及原生oracle质量门没有删除。

候选10条before-codegen terminal reuse有0当前object reports/0真实cache lookups；160当前preflight单元复用，绑定同worker历史LLVM/ELF及物理CGU，不虚构16hits。其余20候选成功row保留320当前unit reports；main有480。三臂每成功row实际各16对象，但内容/划分与安全义务不据计数假定等价。main/candidate各72个(block,key)物理container，分别36个唯一object payload；Rust实际current-session对象与历史session分开保留。

下表单位ms，GM为两个逐block候选/main、候选/Rust比值的几何均值；逐block原值和配对比完整保留在JSON，不是跨窗口比值。

| 模式 | 请求 | main中位 | 候选中位 | Rust中位 | GM 候选/main | GM 候选/Rust |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| check | cold | 1075.5 | 880.1 | 1495.4 | 0.817 | 0.561 |
| check | unchanged | 90.6 | 3.5 | 18.2 | 0.038 | 0.192 |
| check | private | 684.8 | 52.0 | 228.0 | 0.076 | 0.233 |
| check | multi | 886.5 | 106.5 | 305.0 | 0.121 | 0.375 |
| check | broad | 691.6 | 931.1 | 1186.7 | 1.298 | 0.755 |
| check | positions | 840.1 | 527.1 | 786.1 | 0.640 | 0.673 |
| check | error | 503.0 | 47.9 | 206.7 | 0.095 | 0.231 |
| check | recovery | 612.0 | 57.9 | 319.1 | 0.095 | 0.183 |
| check | restore | 84.4 | 24.8 | 252.3 | 0.294 | 0.103 |
| o2 | cold | 5916.5 | 8539.1 | 6776.1 | 1.440 | 1.258 |
| o2 | unchanged | 581.2 | 14.4 | 14.8 | 0.023 | 0.909 |
| o2 | private | 1805.0 | 401.1 | 621.5 | 0.222 | 0.648 |
| o2 | multi | 2124.5 | 771.6 | 1159.1 | 0.346 | 0.633 |
| o2 | broad | 5758.0 | 7200.6 | 5362.9 | 1.260 | 1.344 |
| o2 | positions | 1446.8 | 952.2 | 1066.4 | 0.649 | 0.881 |
| o2 | error | 365.2 | 59.3 | 267.9 | 0.162 | 0.236 |
| o2 | recovery | 1670.4 | 391.1 | 460.0 | 0.233 | 0.845 |
| o2 | restore | 519.0 | 98.9 | 703.1 | 0.190 | 0.148 |

**负结果保留：** native cold约8539ms/main5917/Rust6776，broad7201/main5758/Rust5363，两个block均慢于两方；check broad也仍慢于main。native noop block0为9.40/Rust14.21ms，block1却为19.30/Rust15.43ms，不能称稳定领先。每格仅两个样本，无显著性、全场景领先或此前候选归因结论。

**退出与独立正确性：** check helper exit0；O2九表102rows全部matrix-valid，但helper exit2如实保留：末尾独立delete/overwrite共6请求中5pass，main/candidate两操作及Rust delete通过，Rust overwrite仍留下非ELF，harness拒绝执行并记录false。额外60次原生执行不计入九表1080次。此反例既不掩盖，也不误报成九表失败。

01门在d721上native82/1失败（typed transport fixture left0/right1）及strict Clippy两处失败原样保留；6face的02门native83/backend262、真实source单test含10requests、strict Clippy/fmt及release通过。Analyze304/Verify652/driver211沿用实际d721的未受影响收据，不冒称6face新跑。原release与补tree的immutable binding收据均列于JSON，未覆写原raw。Astra的early scalar、final product、typed-stage源审按各自精确component SHA引用，未以源审替代运行。

Zil runtime LLVM23.1.4（stock2bf与SDK18 785分别实映射），Rust1.99.1内嵌LLVM23.1.1；CGU16/O2/jobs1、Rust incremental/lto=false与Zil cacheall/cap256固定。Cargo每请求新进程、Zil resident且启动另计；源码写入、对象/符号/hash归档和原生执行在编译计时外。首次profile admission/hash在候选冷请求内；完整SDK文件在native窗口后归档。OS cache未清空，CPU/RSS边界不同，不推内存优势。三臂普通请求均保留成功output，error绝不执行旧exe；恢复破坏仅在末尾独立场景。

SDK profile `192bd5739d2420fb3f15afe5c1672ec4934c94046259a79d642e9ffc828f65e6` 的首次固定归档为 [`70f92d8677ee501fc5000c77fd1d48f5dc2407e8`](https://github.com/zil-lang/zil/tree/70f92d8677ee501fc5000c77fd1d48f5dc2407e8/research/incremental-demand-full-chain-20261113-v27-001)，V27整体metadata/index仍pending，不冒称整体恢复闭合或性能接受。下一compact-scalar整链继续施工，尚无新测量，不能用本表替它报收益。
