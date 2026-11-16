# 重启布局整链：精简结果

被测源码11078c56dc8450b8c47312577dd9918bcdf6d9a3，tree20620ad111efd02ff1975aaea949d6b7fbc1117a；binary SHA256 d9bef5078b5b17b6c2f58fe500519ebdd284556ddbf5f88b87f185511a1437e6。实际main9170bb4210554392e36eb7528596612596ebef80，Rust1.99.1，同CGU16/O2/compiler jobs1，两平衡区块，check/native分开。

| 场景 | ZIL秒 | Rust秒 |
|---|---:|---:|
| native5105完整18请求流程 | 31.794 | 35.568 |
| native5105八私改合计 | 11.017 | 11.198 |
| retain5105连续重启+首改 | 5.110 | 1.478 |
| check5000wide完整流程 | 7.906 | 7.763 |
| check5000wide冷 | 1.866 | 0.827 |

native5105用户盘量区块边界峰值中位36.020/R72.855MiB；八私改增长1.164/R28.480MiB。非瞬时峰值/工业GB级证明。246原始请求/82功能格/22运行质量完成，candidate/Rust质量通过；48main5105限额行不是成功。原始命令和完整日志仍在本地out/runs/restart-layout-performance-*-01，精确pins在out/runs/restart-layout-performance-pins-01.json。

实现了class磁盘layout当前完整校验与replay、实际key与Materialization、同build真实不可变owner的完整preflight复用、closed push discovery/hydration与locator/segment重复copy消除。独立Verify/原预算/完整O2保持，研究默认关，SDK18未重建，未合main。

验收：ab57 driver227/backend322/nativepublication2/tool22过；最终11078仅删除未调用cfg(test)适配5行，生产同一逻辑，strict/fmt/release重新过。首次strict失败保留。精确命令在本地out/runs/restart-layout-chain-gates-{01,02}-*/binding.json与各收据。

profile诊断restart4.243s中function-cache-read1.046s、sourceingress.213s、backend2.004s(machine.827/partition.718/linkkey.273)，signed未分配37.7ms。实际layouthit、16objects全hit、preflight1检查/1复用、Types4执行/5101CFG恢复。不同窗口与缓存/写回状态不作因果速度比较；read不是纯decode证据。仍真实解析102文件，不是重启零工作。

全面超Rust未达成；重启首改为最大当前短板，接口改与check冷负结果保持。旧305/98fa/021997数字不转记。按用户最新指令不再默认打包/上传全量证据，仅保存必要结论；此前已生成包暂留本地out。后续直接参照Rust OnDiskCache/CGU partition继续整链施工。
