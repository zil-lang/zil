# 两个退役草案的独立静态审查

结论：**当前草案静态审查通过，无需放宽原guard；尚不可直接执行，唯一待填写值是各自PRIOR_ARCHIVE。** 两份YAML不是本审查者编写。此次只读草案、README、本地Git对象并解析嵌入Python，没有推送远端、执行workflow、构建、采样或改动其他报告。以下只确认检查逻辑存在且未改变，不声称已查询执行时远端archive、活动run或PR状态。

## 模板与逐字节证据

本地模板`../multi-edit/retire-composition.yml`原字节等于成功提交`1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56:.github/workflows/compiler-resumption-retire.yml`。分别对草案反向替换四组值：workflow显示名称、branch、OLD、PRIOR_ARCHIVE；出现次数严格为`[1,2,1,1]`，反向结果均与模板**逐字节一致**，没有其他改动。branch的两处是push filter和嵌入Python常量。

| 文件 | SHA256 |
| --- | --- |
| 成功模板 | `68ea8a721246cb764c43277ea52f249c9b54ffab42f7784934273ffd840b2411` |
| compiler-layout-projection-retire.yml | `79aa1546c3ad01e0756d2016bb01a81c81c07a1f317d9ef5989fc1bdec4b4695` |
| compiler-bounded-imports-retire.yml | `cc1daba6034dcd6e310750f468a7e89bb90c5bb7b1b57644247b5dc498a0bca3` |
| README.md | `e4f4e896115f7208f199c13d939e0905aa1d51053c651c041998319cc6c4fc79` |

两份嵌入Python均经`ast.parse`成功，未执行其代码；解析出的REPO、BRANCH、OLD、WORKFLOW及唯一PRIOR占位符合README。源码OLD以本地Git完整对象复核：

| 分支 | OLD | tree |
| --- | --- | --- |
| task/compiler-layout-projection | `a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf` | `d260d616a23af8f9c716fbe26c38c200e0eeeb5f` |
| task/compiler-bounded-imports | `1679decf177c9a6eda5a64063c525f06e44e56a2` | `1e599b1bc2f0f6a0c438088a30faee9ff4018271` |

## 保护条件逐项确认

| 项目 | 保留的实际检查 |
| --- | --- |
| 权限与触发 | 仅contents:write、actions:read、pull-requests:read；固定zil-lang/zil、push事件、自身branch和workflow路径。checkout与upload-artifact的固定commit不变；共享concurrency组及cancel-in-progress:false不变。 |
| 精确recipe ancestry | EXPECTED只取GITHUB_SHA，HEAD须等于EXPECTED、EXPECTED不得等于OLD；GitHub commit的parents必须精确为`[OLD]`；compare须ahead_by=1、behind_by=0，files列表只能含该workflow。 |
| 目标身份 | GITHUB_REF必须等于固定REF；真实target head必须等于EXPECTED；own run的head_sha、head_branch及event必须对应当前recipe。没有其他branch删除入口。 |
| 当前直接archive | 从真实`archive/source-history`head读取，不能只验祖先；不得等于PRIOR；真实commit恰有两个parents且集合为`{PRIOR,EXPECTED}`。原guard比较parent集合，不额外承诺parent顺序。 |
| manifest | 从上述精确archive SHA读取；prior_archive等于PRIOR、commit_parents集合为`{PRIOR,EXPECTED}`、commit_tips包含EXPECTED，且remote_refs[REF]精确等于EXPECTED。 |
| 活动run与PR | 检查六种活动状态；每次要求total_count等于已读取rows数，防止未检查分页；除自身run/EXPECTED外任何活动run均拒绝。head和base open PR任一非空即拒绝，不靠第一页空结果跳过已有项目。 |
| 删除前复核 | inactive检查后重新确认archive及target未移动；不删除未经归档或已移动的head。 |
| 精确lease | 仅执行`git push --porcelain --force-with-lease=REF:EXPECTED origin :REF`；非零returncode即失败，没有无lease降级。 |
| 删除后 | 必须取得target查询HTTP 404，其他HTTP错误不算成功；archive必须仍等于已检查值。 |
| 收据 | 原try/except/finally逻辑不变：成功或Exception失败均写receipt并打印；always artifact上传、缺失即error，保留30天。未扩大成对硬终止或runner丢失的额外保证。 |

## README的构造顺序与未定字段

README与原guard一致，固定顺序是`E → Wlayout → A1 → layout删除及receipt → Wbounded → A2 → bounded删除及receipt`。E先保全源码与正负证据；Wlayout唯一parent为a60、PRIOR=E；A1直接parents为E和Wlayout。完成layout退役后，Wbounded唯一parent为1679、PRIOR=A1；A2直接parents为A1和Wbounded。不得先发布A2再触发layout，否则原“当前直接archive”guard会拒绝，不能修改guard绕过。

当前两个PRIOR均为`__ROOT_SET_PRIOR_ARCHIVE_AFTER_EVIDENCE_PRESERVATION__`，其余source identity已固定。最终填入真实完整SHA后文件hash会改变，必须重新冻结并做同样反向核验；本次Pass只针对上列草案hash，不替代执行时guard。EXPECTED仍由GITHUB_SHA确定，不能手填最终archive SHA引入哈希循环。README也已明确manifest中的目标remote_refs是将由lease验证的recipe head计划值，不冒充构造archive时已经观测到的远端快照。main/goal不参与这些ref操作。
