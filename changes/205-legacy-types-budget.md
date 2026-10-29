# 205：三个旧 callback Store 输入的真实 Types 预算迁移

主树205工单及 `changes/205-workspace01-followup.md` 预先登记此迁移。规范摘要仍为 `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`；默认 Limits、产品门、语言输入和成功/拒绝含义均保持。

原测试 `pre202_callback_store_paths_keep_their_recorded_types_boundaries` 在204正式完整工作区 `85f01d622d9c31bffe165e5720bbf09658af5084` 实际通过。测试文件及两份 include 源与205首轮 `0317a093ade0118741d334c45438ade07e9d7228` 逐字相同；嵌套 helper 仍用原固定前缀与两次精确替换产生。此基线来自已保存的204原命令与输出，没有冒称重新执行旧编译器。

205首轮整仓的旧原断言实际失败：第一输入测得6326/16，对比4241/16；该断言阻止后两个测量。随后仅将原断言移到三个完整测量之后，在 clean `4e69c996257c776482b624c1c48c0d67f00802c0` 得到三份原N实际Fail。合入旧完整State右值转交的真实修复后，在 clean `e09748eab8f0f70b8e917dc7f3ed9d5fa05b853c` 再测，三份工作量不变，原N断言再次实际Fail。这两次Fail均保留，不改成Pass。

| 原输入 | 原工作额度 | 205实测工作额度 | 局部额度 | 函数额度 |
|---|---:|---:|---:|---:|
| old-original02 | 4241 | 6326 | 16 | 5 |
| old-two-elements | 11851 | 29030 | 39 | 7 |
| old-nested-helper | 11792 | 29566 | 40 | 8 |

每一列额度分别对同一完整 Core 调用公开 Types API，先取得默认额度下的完整 Typed，再核对最小N与fresh N完整输出相等；N−1必须仅产生Types ResourceLimit。局部和函数列没有依据此次变更放大。旧工作额度继续写入每份 `boundary.json`，最终只修改这三个工作断言。

精确源、baseline身份和原命令留在 `run-logs/205-root/budget-legacy01`；合修复后仍带旧断言的原始失败在 `budget-legacy02`。完整记录随205独立证据包交付；非作者复测和root正式整仓以各自新身份另记，不由此文预填通过。本次真实额外定义检查工作不是编译提速结论。
