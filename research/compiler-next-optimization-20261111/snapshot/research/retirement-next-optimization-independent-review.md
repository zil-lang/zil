# 四候选退役占位草案：非作者独立静态审查

结论：**冻结占位草案静态审查通过，但不可直接执行。** 本轮raw持久化及新A0尚未由本审确认，四个PRIOR均未填；不得把旧archive93当新A0。本审仅本地读文件、解析AST、计算hash，未git add/commit、查询或修改远端、执行workflow、移动任何ref。报告置于草案ROOT外，没有改变其freeze集合。

## 冻结身份及模板一致性

被审目录`research/retirement-next-optimization/`；draft-freeze SHA256 `83b62077fa68a84a7ccfc8fec805280ff83af0e926fa9329ac126d0133585b55`。freeze列出的8份文件全部逐项重哈希一致。

成功模板6723字节，SHA256 `68ea8a721246cb764c43277ea52f249c9b54ffab42f7784934273ffd840b2411`；独立计算Git blob SHA1为`346a4db8d8223c71ee50cac0c85f7ce972d12354`，与固定官方blob身份一致；它还与archive93恢复的旧成功`multi-edit/retire-composition.yml`逐字节相同。本地main仓库浅历史不能直接读取1df历史对象，本审未据此杜撰git-show成功，也未新增远端请求；模板内容的独立依据是已恢复旧文件、完整字节及Git blob身份。

四份草案均只反向替换显示name、branch（push filter和Python常量两处）、OLD、PRIOR；替换次数严格`[1,2,1,1]`，还原结果逐字节等于该成功模板，末尾LF相同。四份嵌入Python均AST解析通过，REPO/BRANCH/OLD/PRIOR/WORKFLOW常量与identities逐一相符；没有执行Python。

| 顺序 / 目标 | OLD | 草案SHA256 | PRIOR状态 |
| --- | --- | --- | --- |
| structured-emission | ee0cc45261dbfadf25dce567f0fc1fa0b4d4b6f5 | 8c82943d987fd0e59670e8f1d64790d120c1c7595645553165c503c26b7389b3 | A0占位 |
| builder-position | 9f0deb4c4142d0fff2ef622442518fc1d7a65d5c | 00d083ef9b1f271e609e433e055818406408704cd9fae72e9c5471e0942bf275 | A1占位 |
| thinlto-prototype | eb5b150903b67d8b9011f6adc2d3ac67b1c419a6 | 65336698d38665919ee39f889ce5343e944908149fde00cf73fa94f6942aca3e | A2占位 |
| thinlto-complete-visibility | 2549cbec8e1f3096d4c8c8a960459c83095ad13d | 6ed875b575bae8d30d3ed30d768889f1e8ac75a263afda8513a815969ce9168e | A3占位 |

所有branch均有固定`task/compiler-`前缀，未增第五个目标或通配删除入口。

## 未改变的原guard

| 保护项 | 逐字节继承的实际检查 |
| --- | --- |
| 权限/事件 | 仅contents write、actions read、pull-requests read；固定zil-lang/zil、push、自身ref与原workflow路径；checkout/upload固定commit、原concurrency组及cancel=false保持 |
| recipe ancestry | EXPECTED取实际GITHUB_SHA，HEAD必须等于EXPECTED，EXPECTED不得等于OLD；commit父列表精确为[OLD]；compare ahead1/behind0且files列表只含该workflow |
| 精确任务head | 真实目标ref须等于EXPECTED，own run的head_sha/head_branch/event对应；删除前再次检查，移动则停止 |
| 当前直接archive | 从真实archive/source-history head读取；不得等于PRIOR，恰有两个parents且集合为{PRIOR,EXPECTED}；guard核集合而非额外约定顺序 |
| manifest | prior_archive、commit_parents、commit_tips、remote_refs[自身REF]均绑定真实PRIOR/W；不能只以祖先可达替代当前直接归档 |
| 活动run/PR | 六种状态分页count必须等于已读rows，除自身run/EXPECTED外任何活动run均拒绝；head/base open PR任一非空即拒绝 |
| lease删除 | 只删除自身固定REF，强制精确`--force-with-lease=REF:EXPECTED`；returncode非0停止，无无lease降级 |
| 删除确认 | target必须HTTP404，其他HTTP错误不当成功；archive删除前后都不得移动 |
| 收据 | 原try/except/finally与always上传不变，缺artifact报错；不额外承诺硬终止或runner丢失也必能保存完整收据 |

## README构造顺序正确，仍需具体化后重新审

顺序为先持久化本轮全部源码/正负raw/复现工具并建立A0，然后`W_S→A1→完成S退役`，再`W_B→A2→完成B退役`，再`W_default→A3→完成default退役`，最后`W_strict→A4→完成strict退役`。每个W创建时任务ref仍停OLD，W唯一parent为该OLD且tree只改原workflow；先发布直接archive Ai，再推进任务ref触发W；完成、404和receipt保全后才使用Ai作下一PRIOR。

W内只写已存在的PRIOR，不写尚未产生的Ai；EXPECTED由GITHUB_SHA在运行时确定，避免对象哈希循环。不能先把archive推进A4再触发前面的W，原guard会拒绝，不能放宽它。manifest remote_refs中的W是即将发布且由lease保护的recipe计划值，不冒充建archive时的已观测快照。main/goal不参加任务ref操作。

PRIOR填入真实完整SHA后草稿hash会改变，须重新冻结和反向核验，再审W/A实际对象、完整证据持久化及执行前状态；当前Pass不替代那些尚未完成的步骤。

## 只读快照及旧composition边界

本审读的是作者已冻结的`remote-readback.json`，观测时间2026-11-11 10:43:17 UTC，并非本审新增实时查询：其7条branch记录中四目标OLD均匹配，open PR为空、六状态active count均0，archive仍93。该快照只证明当时记录，不替代workflow执行时重新检查，也不证明本轮A0已存在。

快照中composition精确ref为404、branch列表不含它，历史run42972887557 completed/success、head为1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56；保存的parent图确认93→5c97386→a018e254→a48374cc→c43cda66，其中c43直接包含1df及c6，c6直接包含旧研究5aa8057f9cc0bff651fc6a44147b4e6e2dd307de。README没有重建已退役ref，也没有把旧研究分支说成待生产合并。

四候选不采用、strict完整可见性成立仍NoGo的事实与退役安全是两件事；草稿和本审均不修改benchmark或将研究证明转化为生产接受。当前仅确认**占位草案内容与原安全流程一致**，没有执行删除。
