# 多点编译候选的顺序退役草案

两份YAML仅为本地草案，未安装到分支、未触发workflow、未修改任何远端ref。main/goal保持不变。模板为已成功的`1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56:.github/workflows/compiler-resumption-retire.yml`。

| 草案 | 目标分支 | 精确源head（OLD） |
|---|---|---|
| `compiler-layout-projection-retire.yml` | `task/compiler-layout-projection` | `a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf` |
| `compiler-bounded-imports-retire.yml` | `task/compiler-bounded-imports` | `1679decf177c9a6eda5a64063c525f06e44e56a2` |

每份草案最终都安装为其自身目标分支的`.github/workflows/compiler-resumption-retire.yml`。相对成功模板只替换workflow显示名称、target branch、OLD和PRIOR_ARCHIVE占位；反向还原这四组替换后已逐字节等于原模板。没有新增anchor机制或验收门。

## 确切字段与构造顺序

设P是保全前固定`archive/source-history`的真实head。以下E/Wlayout/A1/Wbounded/A2全是构造顺序中的符号，不是臆造SHA；root完成对应Git对象后填写实际完整SHA。

1. **先保全E。** E的parent为`[P, a60, 1679]`，保全全部源码、脚本、原始正负证据、首败及精简索引。确认远端固定archive实际指向E。
2. **构造Wlayout。** 精确唯一parent为a60，只改变`.github/workflows/compiler-resumption-retire.yml`。在workflow中填写`PRIOR_ARCHIVE=E`的实际完整SHA；此时目标任务ref仍停在a60。
3. **构造并先发布A1。** parent恰为`[E, Wlayout]`，manifest至少满足下列原guard字段；发布固定archive到A1，核对其真实head。
4. **触发layout退役。** 将layout任务ref从a60推进到Wlayout，触发push路径。workflow要求自身`GITHUB_SHA=Wlayout`、唯一parent为a60、diff只含自身workflow、实际target ref仍精确Wlayout、当前archive仍A1。待该run完成、分支404且receipt保全后再进行下一项。
5. **随后构造Wbounded。** 精确唯一parent为1679，只改变相同workflow路径；此时填写`PRIOR_ARCHIVE=A1`真实SHA，bounded任务ref仍停在1679。
6. **构造并先发布A2。** parent恰为`[A1, Wbounded]`，manifest按同一原guard填写；发布固定archive到A2并核真实head。
7. **触发bounded退役。** 将bounded任务ref从1679推进到Wbounded。待其成功、404及receipt保全后结束。

必须按此顺序：若先把A2推成当前archive再触发layout，原guard会因当前archive不是`[E,Wlayout]`而拒绝。不要放宽检查以跨过这一拒绝。main/goal不参加这些ref操作。

## 每个直接archive的manifest

A1必须含：

```json
{
  "prior_archive": "<E实际完整SHA>",
  "commit_parents": ["<E实际完整SHA>", "<Wlayout实际完整SHA>"],
  "commit_tips": ["<Wlayout实际完整SHA>"],
  "remote_refs": {
    "refs/heads/task/compiler-layout-projection": "<Wlayout实际完整SHA>"
  }
}
```

A2同理：`prior_archive=A1`、`commit_parents=[A1,Wbounded]`、`commit_tips`包含Wbounded、`remote_refs[refs/heads/task/compiler-bounded-imports]=Wbounded`。保留其他既有保全内容，不删除负面结果。这里`remote_refs`目标项标识即将发布并由lease检查的精确recipe head；创建archive时任务ref尚未推进，不能把该计划值误写成当时已观测到的远端快照。

## 为什么没有哈希循环

W只包含已存在的PRIOR_ARCHIVE，不包含A1/A2自己的SHA。W的期望head由运行时`GITHUB_SHA`确定；后创建的archive以W为parent，并在manifest中写入W。关系为先P/E、再W、再archive、最后推进任务ref，不存在`W→archive→W`对象哈希循环。不要把未构造的最终archive literal写回W。

## 原安全边界原样保留

权限仍仅contents:write、actions:read、pull-requests:read；仓库/事件/分支固定，自身checkout及recipe ancestry精确检查。实际归档必须是当前archive head，且恰有PRIOR与自身W两个parent；manifest验证保全该精确head。拒绝任何未检查分页、其他活动run、head/base open PR或目标移动；删除前再次核archive与task head。删除只执行自身固定ref的`--force-with-lease=REF:EXPECTED`，随后必须404并核archive未移动；所有成功与失败均保存原artifact receipt。没有其他分支删除逻辑。

当前唯一未定字段是各自PRIOR_ARCHIVE占位，root在上述构造点填写。source OLD已知且固定；EXPECTED不手填。最终填写后文件hash会改变，须重新冻结和复核，不把当前草案hash当最终recipe内容hash。
