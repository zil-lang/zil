# 响应式编译专项：中断恢复入口

## 当前真实状态

任务G10.5.1.15；分支research/reactive-query-graph。main基线99035931d358add0eaed6d4535035c38413701dd；最新受测生产源码12e648af1f50ffcb18fc32b8f4dc50a680900182。本文及README/receipt更新只保全已执行事实，不是新的生产实现。

本轮实际接入parser密封Source、模块Normalize/Restore/Lower产物/预算receipt、AST共享、独立SourceMap共享及Types稳定版本；B直接memo和C响应图公平对照。357lib＋CLI1、严格Clippy、release及非作者精确源码/日志复核已完成。r6真实22模块私有只Lower1、批量3，Map深复制1/3，CoreEq0、CFG5/6同A；Core组装仍复制22模块。

最新check六block306row/162measured/0invalid已完成且分析器返回complete_valid_evidence=true；完整原始文件独审首次读取即遇环境失效，未完成。控制台墙钟投影已另存tools/research/reactive-query-graph/interruption-checkpoint-20261111.json，不能当完整raw。最新O2已启动但未收到任何完成收据，绝不计native通过。

**阻塞：执行环境status unavailable。exec-server transport disconnected，恢复45s超时；再次只读pwd返回No usable execution environment。没有暴露可调用的环境恢复工具。GitHub仍可读写，源码和本恢复入口均已远端保全。** 不声明进程已停止或仍在后台运行；旧session77850状态未知。

## 工作树与持久化边界

- 实现/worktree：/workspace/scratch/e28839ace920/reactive；中断前HEAD12e648a、tracked/untracked源码检查干净。
- 验证树：/workspace/scratch/e28839ace920/producer-validation-1c7b6cd；目录名是旧标签，实际HEAD12e648a、clean。构建target /workspace/scratch/e28839ace920/targets/reactive-producer。
- binary：/workspace/scratch/e28839ace920/binaries/zil-producer-12e648a；SHA256 c64da5a70a938a21811c52b95f2ccc946599315fbf5b615896d35d74b58e65b6。
- goal树：/workspace/scratch/e28839ace920/goal-final；中断前正常ff到1434986e0d435c64e46e967a9a5460a1a7fede69，clean。后续远端文档提交尚不能在失效沙盒ff，恢复必须重新读refs。
- 生产源码/测试/ABC测量器已推送，无已知唯一未推送生产源码。后期out/runs原始日志、review脚本/笔记尚未外部备份；这些是未保全内容，不能用本摘要替代。
- 旧Library版本0仅到7b2红测之前后检查点；ZIP SHA256 ae533035eee20c532b22b3d6797c45ab3331d97d811e0d291d4644112051defc，90983519字节/1084条目。ID libfile_fde3_f-11852a3f427881918ed8650db66dc3f3_bd147c3485448191a9df16b3c3cea7dd；不能声称包括ea0/12e新数据。

## 已完成运行，恢复时优先保全

| 路径（实现树out/runs内） | 身份与边界 |
|---|---|
| producer-validation-12e648a | 357lib＋CLI1/Clippy/release真实exit0；非作者实读 |
| native-abc-cost-20x20x16-r6 | 21请求/12measured、全部Verify1；profile单block，native0；计数checker已执行 |
| native-abc-map-20x20x16-check-r1 | 306请求/162measured，分析器完整；新独审未完成 |
| native-abc-map-20x20x16-O2-r1 | 已启动，完成情况未知；不能算通过 |
| native-abc-producer-20x20x16-{check,O2}-r1 | 上一ea0阶段各306请求；O2 1620native全过、非作者完整复核；不能替代12e |
| native-producer-long-ea0ce45-r1 | ea0 B/C102请求＋5fresh公开JSON，native0；独审已完成 |
| producer-measurement-review-881e67f | 新旧独审、源码hash、首审计工具错误及修正；最新map-check审计尚未创建 |
| producer-review-1c7b6cd | source-map-r6-check.json及check_source_map_run.py成功写/执行；只是计数核验，不是作者工具独审 |

保留所有更早首失败：重复FileId/预算反例、真实IO未走producer、变宽与批量限额误失效、测试诊断预期错误、Clippy失败、PID临时目录碰撞及32/24规模默认额度失败。详细身份在[冻结前施工记录](https://github.com/zil-lang/zil/blob/12e648af1f50ffcb18fc32b8f4dc50a680900182/.task/reactive-query-graph.md)和receipt；不重复旧JS模型全实验。

## 下一条实际命令与顺序

环境恢复后的第一条为只读核查，避免重复启动仍未知的重窗口：

```bash
ps -eo pid,etimes,args | rg 'native_abc_measure|native_long_session|cargo|rustc|zil-producer'
```

再核各工作树status、远端refs、磁盘/内存和旧O2文件是否存在。不要reset/clean，不切正在构建的树，不删除其他分支。先打包已完成raw及未完成O2状态并持久化，再做额外长测量。

若旧O2已完整，原目录只读执行分析器并独审，不重跑已完成块：

```bash
python3 tools/research/reactive-query-graph/native_abc_summarize.py out/runs/native-abc-map-20x20x16-O2-r1
```

若缺失/部分完成，保留旧目录、明确InfraError/未完成后，用新的r2身份完整配对；同一重窗口，不与Cargo或其他测量并行。示例在原工具链环境仍存在时：

```bash
source /workspace/scratch/abf690ce880c/toolchain/env.sh
python3 tools/research/reactive-query-graph/native_abc_measure.py \
  --binary /workspace/scratch/e28839ace920/binaries/zil-producer-12e648a \
  --source 12e648af1f50ffcb18fc32b8f4dc50a680900182 \
  --out /workspace/scratch/e28839ace920/reactive/out/runs/native-abc-map-20x20x16-O2-r2 \
  --case scalar --modules 20 --functions 20 --steps 16 --blocks 6 --mode O2 \
  --request cold --request unchanged --request private --request multi \
  --request broad --request positions --request error --request recovery --request restore
```

若沙盒已重建，先从远端克隆/独立worktree恢复，按照当前rust-toolchain.toml、docs/guide/build.md和脚本恢复受支持工具链；必须重新核binary SHA，不用别的源码binary冒充12e。当前构建使用CARGO_PROFILE_DEV_DEBUG=0、CARGO_PROFILE_TEST_DEBUG=0及独立target。测试必须用全新TMPDIR/ZIL_TEST_REPORT_DIR，避免历史PID碰撞；原失败输入不删除。

其后必要控制仍待执行：当前12e mixed2×2×16的cold/multi/trait/ct/recovery/restore；resource_closures的cold/positions/recovery，O2/1CGU1job/1block全部独立native输入。新默认关对main、50编辑长会话、40模块宽度native及1/8CGU矩阵均不得写成已完成。先保证当前切片端到端与缺口证据，未见净收益不盲目扩大矩阵。

## 决策与协作边界

上一ea0 O2私有C−A平均省171.096ms，但C−B区间跨0；多文件C−A增加275.550ms。最新12e控制台check私有C−A省32.600ms、多文件增加172.726ms、广泛增加429.656ms，原始文件独审待恢复。B和C均尚未满足总体净收益采用门。

继续优先消除Core整项目复制、CT材料化和缓存发布的无效全量工作，再引入昂贵消费者的实际分面依赖。普通递归不等于SCC固定点；全局候选/ID/上下文守卫仍有保守范围。不得绕过当前Verify、预算或首诊断，不把旧合法IR等同于当前Source。

goal已另认领research/incremental-demand-propagation从ea0续接Core共享/材料化receipt；保留它的record与看板附注，不删除、合并、覆盖其分支。ThinLTO分支8072f23187a77f2e63686696111a8e0e71e8be7b同样保护。修改goal只动本任务字段，并保留其他接续附注；读最新SHA、lease失败重读重放。源码/原始数据恢复后再推进验收，当前不合main，不声称全架构或整体Rust目标完成。
