# 四个未采用候选的安全退役结果

状态：**Pass，四次实际自删均成功**。最终只读核验时间：2026-11-11T11:47:10.499Z。

|分支|原源码 OLD|退役 W|直接归档 Ai|实际 Actions|
|---|---|---|---|---|
|task/compiler-structured-emission|ee0cc45261dbfadf25dce567f0fc1fa0b4d4b6f5|b0da05ec784d632fa88c00d860382d656db712ec|78ce99886e435dd56c0af632240eded1840827c0|[success 43014067899](https://github.com/zil-lang/zil/actions/runs/43014067899)|
|task/compiler-builder-position|9f0deb4c4142d0fff2ef622442518fc1d7a65d5c|087757507562a47bde7e32ba34f19d1986592cde|34ded7a21bed380b71e5bf85f4eb88569404710f|[success 43014310393](https://github.com/zil-lang/zil/actions/runs/43014310393)|
|task/compiler-thinlto-prototype|eb5b150903b67d8b9011f6adc2d3ac67b1c419a6|19681c97573460252df425cfaefc0e5d3dadb0e0|098789c8cfb7f3f436c8014af1069a7f8314da08|[success 43014487178](https://github.com/zil-lang/zil/actions/runs/43014487178)|
|task/compiler-thinlto-complete-visibility|2549cbec8e1f3096d4c8c8a960459c83095ad13d|f2522c0786d4a1cd3cfb9cee03c2fcc7e31e72b8|f6c6cba6876a84694750e28c104ccc387860a6f8|[success 43014666292](https://github.com/zil-lang/zil/actions/runs/43014666292)|

## 执行与证明

每个 W 的唯一父提交都是该分支授权 OLD，唯一 diff 为 `.github/workflows/compiler-resumption-retire.yml`；四个 recipe 逆替换名称、分支、OLD、PRIOR 后与已成功模板逐字节相同，内嵌 Python AST 通过。没有修改活动 run、PR、当前 head、归档可达性、唯一 workflow diff、精确 lease 或只删自身的守卫。

每次先创建并发布 Ai（恰好两个父提交 `[PRIOR,W]`，根 manifest 绑定 PRIOR/W/remote_refs），再以 OLD 精确 lease 推进该任务分支到 W；工作流重新核验后只对自身 ref 执行 `git push --force-with-lease=refs/heads/<branch>:W origin :refs/heads/<branch>`。执行前逐页核全 repo open PR 和六种 active Actions 状态，没有活动任务；每一步成功且日志实际 receipt 与 API404、归档保持不变一致后才开始下一步。四次均没有失败、绕过或重试删除。

实际日志均证明 `returncode=0`、`verified_absent=true`、`status=Completed exact archived self retirement`。完整 GitHub 对象创建响应、guard、recipe、run/job/artifact、原日志、精确404与 actual receipt 均保存在本目录各 step 下。

## 归档与保留

A0 `5e31ee5f0d8d0219afde8a73ac8c960e04f844b5` / tree `8e4e4e25cdd1a0c17501eebddd793fba95ea818c` 的四父为 `93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4`、S OLD、B OLD、`1cc866a08bac100012e41726067711c94caf1899`；1cc → strict2549 → defaulteb5，四源码均可达。当前轮证据由 `research/compiler-next-optimization-20261111/manifest.json` 和 `raw-checkpoint-v0.json` 绑定：raw 104191976 bytes，SHA256 `9b1677993e14d608cd5f64c99eecc75bf13963a142219bb48caa1b7945bfadef`，Library version0；打包收据 all_sources_before_after_equal/all_tar_members_verified 均 true。本次只核对持久化元数据和精确归档对象，没有解包/重读大型 raw。旧 Thin v1/v2/v3 只记录实际存有的源码和 metadata，不扩大为完整历史字节已恢复。

最终 archive/source-history 为 [f6c6cba6876a84694750e28c104ccc387860a6f8](https://github.com/zil-lang/zil/commit/f6c6cba6876a84694750e28c104ccc387860a6f8)，tree `c87e1a139e6daa0c2ec30cfabde54311876a65ba`。A2/A3/A4 各带入前一步 completed receipt；最后 strict 的实际收据为 `step-04/actual-receipt.json`，待 root 最终证据提交 F 纳入归档，本次不再移动 archive。

## 最终远端状态

逐页 branches 只剩以下五项，四目标与 `task/compiler-composition-validation` 的 exact ref GET 全为404：

- `archive/source-history` = `f6c6cba6876a84694750e28c104ccc387860a6f8`
- `goal` = `32fe4e123b5c75fc943f8f0fe346964dac1c1102`
- `main` = `99035931d358add0eaed6d4535035c38413701dd`
- `research/reactive-query-graph` = `99035931d358add0eaed6d4535035c38413701dd`
- `task/compiler-thinlto-private-imports` = `3d0dc3e1f420ebea68aff66ebbcd86111251bffc`

旧 composition-validation 是 compiler composition/resumption 历史研究验收、失败结果、诊断补丁和脚本分支，历史 source `5aa8057f9cc0bff651fc6a44147b4e6e2dd307de`、退役 W `1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56`、[原退役 run42972887557](https://github.com/zil-lang/zil/actions/runs/42972887557)；当轮没有待合生产候选，本轮只核其仍不存在，没有再次触发。

本次没有 main/goal/private ref 写入，没有切换运行中工作树，没有本地编译、LLVM/native 执行或重型 I/O；没有采用任一 NoGo 候选。
