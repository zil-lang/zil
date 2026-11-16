# Rust定向重启专项：精简结论

原生被测源码 `480e7d6fe425ad9cce4d0e69b1cf3ca869572198`，binary SHA256 `3d8eed704d54453477b052da61f3208bc87054b6db2f99784e0f9f413ca6d4e8`；实际main `9170bb4210554392e36eb7528596612596ebef80`；Rust1.99.1，同O2/CGU16/compiler jobs1，两平衡区块。

| 原生5105请求 | ZIL秒 | Rust秒 |
|---|---:|---:|
| 冷 | 5.186 | 6.724 |
| 无修改 | 0.074 | 0.028 |
| 缓存进程重启 | 3.491 | 0.038 |
| 紧接重启的首改 | 1.150 | 1.295 |
| 连续重启+首改 | 4.641 | 1.334 |

原生36请求/12功能格/5运行质量完成，candidate/Rust质量通过。main5105默认限额失败不是快成功。用户缓存边界峰值中位 32.682/Rust 58.691MiB；不含SDK/宿主target/研究文件，不是工业GB级证明。

独立check窗口仍属于前一源码 `bee7ee218df3c673bc7660e1af8aea709505272d`：5000wide全流程 5.564/8.211s，冷 1.177/1.198s；102请求/34格。最后修改只涉及closed native驻留入口，check未重测，不把前版数字转记新源码。

实现：参考Rust OnDiskCache共享数据/索引，metadata小块批量pack+offset、CDC去重和有界选择性整理；同一次真实wire解码owner跨semantic/function恢复复用，仍检查完整字节（除可重标诊断名）、完整limits和原6费；当前图观察与实际layout复用，拒绝布局同时丢弃旧槽位；closed push驻留前端省重复整盘读取，closed native当前语法候选仍从本次parser owners形成。

诊断源码bee7的真实重启中5101个raw函数复用decoded owner、第二次decode尝试0，仍有Expand/CT/Types/Analyze/Verify和machine/partition工作。实际private class域变化时仍可能重新规划，不宣称所有首改layout hit。新的最终窗口按完整等待判定，全面超Rust仍未达成。

受影响检查：IR89/Types115(64e1，后续仅等价op-ref lint)、backend325(249cf)、driver228/native2/strict/fmt/release最终版本；机械修正与未变子树复用按原源码收据保留。原始首次失败保留：IR夹具分段、32编辑盘量、拒绝布局旧槽位、严格lint；未放松预算/断言。独立Verify/完整O2/默认研究开关保持，SDK18未重建，未合main。

本地必要定位：out/runs/cache-restart-chain-gates-*；out/runs/cache-restart-performance-retain-02/rows.jsonl与summary.json；out/runs/cache-restart-performance-pins-02.json。命令为cache_lifecycle_matrix.py --cache-state retain measure --sizes 5105 --pins对应pins，完整argv在manifest。较早bee7两窗路径cache-restart-performance-{retain,check-5000-wide}-01；profile在cache-restart-bootstrap-diagnostic-01。不默认打包/上传raw，SDK恢复沿固定e85/V32。

当前480e重启阶段中位（毫秒，父/子不相加）：parse=0.1, expand=678.1, types=150.0, analyze=117.1, verify=80.7, lower=10.6, codegen=1556.7, ct=88.7。backend子项 machine_emit_ns=685.6, machine_transform_ns=277.8, partition_ns=429.5, object_ns=10.8。剩余重点是实际上游重建与机器生成/transform，不是增加对象命中。
