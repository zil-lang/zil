# Compact scalar：完整链与原425三臂接续

当前实测源码 `554bffb68ae7d80edb95289538052e3c2e8ea565`，compiler `84c70f4796f3e814e81a5590ade4bb23ab8dac7ae899f2991ec6742da54d3742`，工具同554；main9170/compiler8d8保持原pin。只记本轮原425结果，不宣告冷编译领先、全面Rust领先或合main；精确receipt/source/完整摘要见[JSON](full-push-compact-checkpoint.json)。

956/ae6478首轮check完整102rows；native只录首3条语义/oracle有效raw，完整ELF硬门失败，exit1原样保留，不纳入合格native计时。Astra[机器码源证据](/tmp/full-push-compact-scalar-machine-code-astra-review.json)证明423个zil.fn中420个（400leaf+20aggregate）完整函数bytes相同；f3/4仅GOT位移，唯一f0由1107→1103B、306→307inst，少4B却多1inst/restore，不能称更快。ELF少4096B主要是布局/页对齐。

554以通用call-bearing域拒绝新complete cleanup，保留已经完成的SSA/ranges；只准完整校核的既有overflow intrinsic/trap域，不硬编码f0、不放宽质量或加优化屏障。02的check/O2各102有效rows=54测量+48父请求，全部9场景、命名父链和原425输入不变；1080native、90条完整ELF、30个main/candidate成功native对完整ELF/exports硬等恢复，actual objects/layout逐请求保留。

| 模式 | 请求 | main中位ms | 候选中位ms | Rust中位ms | GM 候选/main | GM 候选/Rust |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| check | cold | 1060.5 | 1109.0 | 1126.3 | 1.061 | 0.999 |
| check | unchanged | 99.6 | 3.4 | 13.3 | 0.034 | 0.251 |
| check | private | 871.8 | 52.0 | 276.4 | 0.060 | 0.193 |
| check | multi | 548.9 | 81.7 | 211.1 | 0.150 | 0.388 |
| check | broad | 865.6 | 865.0 | 1249.9 | 1.025 | 0.690 |
| check | positions | 941.5 | 701.4 | 999.8 | 0.704 | 0.656 |
| check | error | 487.4 | 46.5 | 282.0 | 0.096 | 0.165 |
| check | recovery | 549.6 | 56.9 | 197.4 | 0.104 | 0.289 |
| check | restore | 111.8 | 24.3 | 238.0 | 0.226 | 0.102 |
| o2 | cold | 6007.0 | 7305.1 | 6509.6 | 1.214 | 1.120 |
| o2 | unchanged | 592.6 | 15.6 | 19.6 | 0.025 | 0.802 |
| o2 | private | 1596.7 | 304.5 | 678.6 | 0.193 | 0.453 |
| o2 | multi | 1670.3 | 492.6 | 998.7 | 0.296 | 0.494 |
| o2 | broad | 5129.0 | 5864.1 | 5491.7 | 1.145 | 1.068 |
| o2 | positions | 1446.6 | 567.8 | 1037.1 | 0.391 | 0.545 |
| o2 | error | 676.1 | 78.4 | 354.4 | 0.121 | 0.206 |
| o2 | recovery | 1338.0 | 298.9 | 477.0 | 0.223 | 0.630 |
| o2 | restore | 479.2 | 61.7 | 495.3 | 0.129 | 0.125 |

**仍未达标：** native cold7305/main6007/Rust6510、broad5864/main5129/Rust5492仍慢；private305/main1597/Rust679、multi493/main1670/Rust999仅是本窗有限观察。GM是两block逐对比值几何均值，不是跨窗旧候选归因；两样本不作显著性或稳定全面领先结论。

门归属：01 caa38测试字段错未编过；02 de9a8 native/API85通过但Source布尔guard8/1失败；03 71c Source9修复、driver211/backend266通过但Clippy一style失败；04 956 strict/release成功后机器ELF门失败；05 554 backend266/Source9/真实retained-source1、strict/fmt/release成功。API85沿用de9a8、driver211沿用71c，所有原失败与实际source/receipt均保留，不虚报重跑。

优化/O2、预算、独立Verify、CGU16/jobs1/cacheall不降级。三臂真实分区内容不能由相同计数推等；Cargo每请求新进程，Zil resident且启动另计。Zil LLVM23.1.4的stock/SDK18 DSO与Rust1.99.1内嵌LLVM23.1.1、CPU/RSS边界不同；源码写入/归档/oracle在编译计时外，首次profile校核hash在候选cold内，OS cache未清。成功间保留output、错误旧exe不执行；02未重做额外output破坏场景。

V28整体归档待封；SDK18/V23及native-link/V27固定复用。V27已固定[3e06887f44907a75df871025b78bd5e2756ce151](full-push-recovery-v27.json)；最新9remote/8local/8worktree的7ce退役决策已[追踪](full-push-old-route-convergence-decision.json)，不混算更早轮次。Root唯一feature分支继续TypedPlan整链，目前只有out/drafts、未集成未测试，不报成果。
