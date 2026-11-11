# 本轮四个未采用候选的安全退役草稿

状态：**仅草稿，未执行、未git add/commit、未推送、未调用workflow、未移动任何ref。** 当前archive仍为93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4；它是上轮最终归档，不是本轮所需A0。必须等root先持久化本轮源码、完整正负raw及复现脚本，并建立新archive A0，再填写真实PRIOR。下表所有PRIOR都故意保持占位。

## 四个精确source lease

| 顺序 | 目标分支 | OLD完整SHA | 草稿 | PRIOR待填 |
|---|---|---|---|---|
| 1 | task/compiler-structured-emission | ee0cc45261dbfadf25dce567f0fc1fa0b4d4b6f5 | compiler-structured-emission-retire.yml | 本轮证据归档A0 |
| 2 | task/compiler-builder-position | 9f0deb4c4142d0fff2ef622442518fc1d7a65d5c | compiler-builder-position-retire.yml | structured退役归档A1 |
| 3 | task/compiler-thinlto-prototype | eb5b150903b67d8b9011f6adc2d3ac67b1c419a6 | compiler-thinlto-prototype-retire.yml | builder退役归档A2 |
| 4 | task/compiler-thinlto-complete-visibility | 2549cbec8e1f3096d4c8c8a960459c83095ad13d | compiler-thinlto-complete-visibility-retire.yml | default ThinLTO退役归档A3 |

这些候选均未采用，草稿不承担生产合并。strict已证明Required56/224实际导入且原cost门通过，实际运行仍失败，因此不继续调参数或扩大矩阵；归档应保存这一机制已实现但质量不达标的事实，不能只保存最终NoGo标签。S/B/default各自原始完整结果与失败记录亦由root归档，本草稿不修改benchmark、源码或接受条件。

## 模板来源与字节校验

template-success.yml来自旧composition已成功退役提交[1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56的原workflow](https://github.com/zil-lang/zil/blob/1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56/.github/workflows/compiler-resumption-retire.yml)。远端Git blob=346a4db8d8223c71ee50cac0c85f7ce972d12354，6723字节，SHA256=68ea8a721246cb764c43277ea52f249c9b54ffab42f7784934273ffd840b2411；与恢复的旧成功文件完全一致，末尾LF也保持原字节。

四份草稿只替换workflow显示名称、BRANCH（push filter和Python常量两处）、OLD、PRIOR_ARCHIVE。每份逆替换出现次数为[1,2,1,1]，结果与成功模板逐字节一致；嵌入Python只经ast.parse，未执行。精确draft SHA、字节数、source与占位值见draft-identities.json。没有新造退役门，也没有删除旧门。

保留的原检查包括固定repo/event/ref、固定checkout/upload action SHA、原concurrency组；W唯一parent必须是OLD，compare只允许原workflow路径且ahead1/behind0；当前branch必须仍等于GITHUB_SHA；当前archive恰有PRIOR与W两个直接parents，manifest准确标识它们及将触发的W；六种活动run状态除本run外必须为空，open head/base PR均为空；archive及branch删除前后不移动；最终仅通过精确force-with-lease删除自身；HTTP404才确认不存在；原try/finally收据与always artifact上传保持原样。

模板的remote_refs[目标REF]=W是归档时的计划值，不冒充构造归档时已经观察到该任务ref在W。不能把最终archive SHA直接写入W引入hash循环，也不能把上轮93冒充本轮A0。

## 后续具体化顺序

root完成本轮raw持久化后确定A0，要求源码与证据可达、manifest明确绑定其真实身份；本草稿不预设A0的parent布局，不替root生成证据完成声明。

随后依次处理S、B、default、strict。每一步都必须：

1. 填入该步真实PRIOR，重新冻结并做四字段逆替换审查。
2. 在不移动任务ref的前提下创建W：唯一parent=该source OLD，tree只新增或修改原workflow路径；HEAD仍由真实GITHUB_SHA在执行时绑定。
3. 创建并先发布Ai，直接parents精确为当前PRIOR与该W，manifest满足原模板字段，收录最终recipe；归档仍由root/非作者核验。
4. 再把唯一目标task ref从OLD推进到W触发workflow；等待原run完成，独立读回run/head、目标404、archive保持Ai及实际receipt。
5. 完成并保留收据后，才将Ai作为下一步PRIOR。

因此顺序是A0 → W_S/A1/完成S退役 → W_B/A2/完成B退役 → W_default/A3/完成default退役 → W_strict/A4/完成strict退役。不能先把所有Ai推进到A4再触发前面的workflow，原“当前直接archive”检查会拒绝；不得放宽该检查绕过。main和goal不参与这些任务ref操作。任何head变动、活动run、PR、归档不符或lease失败都按旧模板停止，不降级成无lease删除。

## 本次只读远端快照

remote-readback.json记录API来源与UTC时间；branch列表7条（per_page100未满），四个任务head与上表完全一致；main为99035931d358add0eaed6d4535035c38413701dd，goal为112e4d107aa35be1a61f9da7b57a618a10482978，archive为93adb1e7aec44bce6e93fcce237dbf77ec2b7aa4。本次open PR列表为空，六种活动run查询total_count均为0。它只是当前快照，不替代执行时原guard，也不证明本轮A0已经存在。

## 旧composition分支已经退役

本轮[完整branch列表](https://api.github.com/repos/zil-lang/zil/branches?per_page=100)不含task/compiler-composition-validation；其[精确git/ref查询](https://api.github.com/repos/zil-lang/zil/git/ref/heads/task/compiler-composition-validation)返回HTTP404。历史退役[run42972887557](https://github.com/zil-lang/zil/actions/runs/42972887557)仍为completed/success，head为1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56。因此不生成第五条recipe，也不重建该ref。

该分支原用于编译器组合验收、阶段profiling、诊断补丁及失败候选记录；研究证据head为[5aa8057f9cc0bff651fc6a44147b4e6e2dd307de](https://github.com/zil-lang/zil/commit/5aa8057f9cc0bff651fc6a44147b4e6e2dd307de)，提交题录是记录精确CLI阶段并停止worker-owned unit候选，不是待整体合入main的生产分支。1dfd8b...是其最终自删workflow提交；归档与接受结论分开。

当前93的已读实际parent链为93 → 5c97386... → a018e254... → a48374cc... → c43cda66...；[c43cda662622d81d7a43849ac7bf5266062fc8f7](https://github.com/zil-lang/zil/commit/c43cda662622d81d7a43849ac7bf5266062fc8f7)直接parents包含c6a629877009a408f7c15f56389d50f6358b5162和1dfd8b...；[c6a629877009a408f7c15f56389d50f6358b5162](https://github.com/zil-lang/zil/commit/c6a629877009a408f7c15f56389d50f6358b5162)直接parents包含5aa8057...与未采用源码2222699d7382c6c2d986c256cbc7610329cd421c。故旧研究与自删recipe仍可从当前archive到达。精确全部parents/tree/API URL保存在remote-readback.json，没有重新解包旧raw或重验数千证据文件。

请root和blaze先审当前占位草稿；只有本轮raw持久化、新A0、最终W/A对象及执行前状态均具体化后，才可由root执行原流程。
