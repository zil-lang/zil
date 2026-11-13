# Compact / TypedIR producer：124完整三臂续接

当前实测源码/工具 `124567d4d00722113818e46f1e4c662de6eb1d87`，compiler `07a06aac47bc6a04f4f0cfa8245dd2ea055ad70d118f120ccf9f44c0656fd7f0`；main9170/compiler8d8保持原pin。当前TypedIR范围事实直接驱动机器IR生成的整链已集成并完成本窗，不能再称“仅draft、未测试”。精确source/receipt/18格摘要见[JSON](full-push-compact-checkpoint.json)。

typed-producer-three-arm-01的check/O2各102rows、54测量+48父请求，两controller均0；原425、20×20×16、22文件、全部9场景及命名父链保留。下表只选measured=true且request=target_request，逐block唯一配对，绝不混入prepare-cold。O2完成1080native、90条完整ELF，30对main/candidate完整ELF/exports硬等全部通过；原始LLVM允许经源审的表示变化但不放宽机器质量。

| 模式 | 请求 | main中位ms | 候选中位ms | Rust中位ms | GM 候选/main | GM 候选/Rust |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| check | cold | 1053.1 | 1236.8 | 1159.6 | 1.244 | 1.087 |
| check | unchanged | 94.8 | 3.0 | 12.3 | 0.032 | 0.241 |
| check | private | 866.3 | 84.2 | 307.0 | 0.090 | 0.254 |
| check | multi | 658.8 | 155.2 | 281.5 | 0.206 | 0.483 |
| check | broad | 988.6 | 652.8 | 1004.4 | 0.659 | 0.650 |
| check | positions | 1155.0 | 744.9 | 744.2 | 0.646 | 1.012 |
| check | error | 535.1 | 54.4 | 208.0 | 0.105 | 0.264 |
| check | recovery | 500.4 | 61.3 | 194.3 | 0.122 | 0.315 |
| check | restore | 84.8 | 22.7 | 190.6 | 0.268 | 0.120 |
| o2 | cold | 6263.5 | 5856.9 | 6057.7 | 0.935 | 0.968 |
| o2 | unchanged | 488.4 | 15.3 | 18.8 | 0.031 | 0.803 |
| o2 | private | 1416.7 | 265.1 | 647.0 | 0.187 | 0.415 |
| o2 | multi | 1716.7 | 394.8 | 1045.8 | 0.230 | 0.378 |
| o2 | broad | 5500.1 | 5401.2 | 5708.7 | 0.980 | 0.944 |
| o2 | positions | 1545.1 | 652.6 | 826.4 | 0.426 | 0.796 |
| o2 | error | 316.0 | 53.4 | 203.9 | 0.168 | 0.260 |
| o2 | recovery | 1361.3 | 254.5 | 414.3 | 0.187 | 0.614 |
| o2 | restore | 482.9 | 73.2 | 480.9 | 0.150 | 0.152 |

**范围与反例：** native cold5857/main6264/Rust6058、broad5401/main5500/Rust5709仅有小幅本窗优势，两block不足显著性/稳定全面领先结论。check cold候选1237/main1053/Rust1160，配对GM1.244/1.087仍慢；check positions候选745/Rust744，GM1.012也未领先。multi按本次measured raw实际为main1716.7/候选394.8/Rust1045.8，不借554旧窗493/1670/999。

门归属：backend269/driver211/Source9/retained-source1及fmt实际在3a1通过；Clippy01两处风格错误保留。124修正后strict02与release通过，不能冒称backend/driver/Source在124重跑。新窗口的1080native/30fullELF是124自己的实际消费者证据。

预算、独立Verify、完整O2、CGU16/jobs1/cacheall及真实分区差别不变；三臂每成功native row实际16对象，不据计数推内容/安全义务相同。候选10条terminal复用保存历史物理producer证据和完整当前preflight，不造16hits。Cargo每请求新进程，Zil resident且启动另计；Zil LLVM23.1.4的stock/SDK18与Rust1.99.1内嵌LLVM23.1.1不同。源码写入、归档及oracle在编译计时外，首次profile hash在cold内，OS cache未清，CPU/RSS边界不同。

554/84c70旧完整18格及956/ae6478首次硬ELF失败保留在V28；Astra420/423函数exactbytes、唯一f0少4B却多1inst的反例和554通用call-bearing域拒绝恢复不删除。这里只列124新同窗表，不把554→124跨窗差额当因果提速倍数。

V28已固定：INDEX[090729ba21c163521b899ceab756984a3e389135](https://github.com/zil-lang/zil/tree/090729ba21c163521b899ceab756984a3e389135/research/incremental-demand-full-chain-20261113-v28-index)，完整恢复[534bd98fcdad9ad79d6ba8daa0e6a971729ea844](https://github.com/zil-lang/zil/tree/534bd98fcdad9ad79d6ba8daa0e6a971729ea844/research/incremental-demand-full-chain-20261113-v28-recovery)。这些固定旧结论不重绑124；当前124窗口V29也已完整保全：[固定恢复620e1fb](https://github.com/zil-lang/zil/tree/620e1fb3646892ed16c5c21fee924a965f9b52e8/research/incremental-demand-full-chain-20261113-v29-recovery)，6片raw+metadata全量fresh恢复闭合，共10374files/1389772400B；不是云环境本地tar备份。Root唯一feature分支已贯通后续TypedIR直接SSA/Phi与LLVM冷capture融合，待统一门，不混进124测量，不宣告main合入或全面/显著领先。
