# 192 三项旧断言迁移：非作者证据审核

**Pass。** 审核提交为 `0e4e052d9956cbc6184277046e51437ae7b8e8b8`，父提交为 `ea82cf71f63608bee392b6888cb00616e091bccf`。本审核者仅检查源码与 root 已有执行记录，没有自执行这些测试，也没有改动生产或测试。

Git 对比确认生产 `crates/*/src` 与 Cargo.lock 不变。三份测试文件的所有语言源码字符串序列分别有 29、15、8 项，前后逐字相同；其摘要与文件前后摘要记入 `review.json`，完整受审测试补丁在 `reviewed.patch`。两个 raw 变造的指令修改、候选输入与执行分支也没有变化。

| 原断言 | 实际行为与迁移判断 |
|---|---|
| impl120 将有限资源兄弟字段替换认作整包冲突 | 原输入的 R 只有 i32 payload、空 Drop，192 已允许独立核验这个实际名义字段。替换 r 不影响 b 的 ByteBuffer 窗口。原全仓已经实际 Success/lower 且 Verify 运行一次；迁移继续使用要求 Success 和一次 Verify 的原 helper。 |
| review176 将 Copy 字段伪装成 callback 地址 | 原 raw 输入仍把实际字段 index 改成 Copy 字段，SSA 结果仍谎称 callback 地址。192 允许实际 Copy 字段借址后，结果类型核验精确 SafeReject / E-VERIFY-TYPE；未改成接受或泛化错误类别。 |
| review179 将实际 Copy index 伪装成 callback | 原字段和结果不匹配的变造保持不变；raw 与 Wire 两路都继续要求 SafeReject / E-VERIFY-TYPE。 |

原同字段、多来源、不可变父值、共享调用升级等反例没有改动。实际诊断路径是独立 Verify 根据真实字段求得地址类型，随后用结果类型一致性门拒绝伪造，并非只修改输出文字。

核验 root 原始命令、clean source 和日志 SHA：旧全仓为 **1925/3**；迁移后限定三套为 **8+7+4 = 19/0**。最终正例报告及两个变造的 raw/Wire 四份实际诊断随证据保存。旧失败完整保留，新的正式全仓由 root 负责，不能从这次限定复跑推断全部完成。

14 份原记录经长度与 SHA-256 复核后逐字复制。candidate.bin 为不可信 IR/Wire 输入，非可执行产物。没有执行文件或对象文件。此审核不涉及 Rust 性能对照或产品净突破结论，完整语言与 ZUI 前置仍需按主线分别闭合。
