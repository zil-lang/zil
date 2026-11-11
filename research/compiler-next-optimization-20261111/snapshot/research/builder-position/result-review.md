# LLVM builder插入点复用84请求：非作者原始结果复核

结论：**功能及同码控制全部通过，完整编译收益与builder阶段收益均未建立，不采用、不扩测。** 本审只读取已有raw、复算统计和独立整数oracle；未运行编译器/native或新采样，没有删除不利格、借环境解释回退，也不与结构化sink的另一个窗口互比微小差额。

## 精确身份与受控复算

原始run：`out/next-optimization/builder-screen-9f0/`。baseline为`99035931d358add0eaed6d4535035c38413701dd`；候选`9f0deb4c4142d0fff2ef622442518fc1d7a65d5c`，tree `257a29612531576af3f9fcf1f703fb824970f695`。候选binary SHA256 `91ce84672bf8e3554876d46c3fb1d8ed4a8c013cad628f2a80c88f5df0e985f4`；baseline `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d`。实际binary与`binding-builder-9f0.json`中的source/tree/build-log绑定已核，build log hash也重算一致。

本目录`recompute-result.py`复用刚完成的结构化sink独审逻辑，仅受控替换run路径、固定候选source/binary断言并追加builder_ns求和；没有更改语料、样本选择、计时值或统计方法。它不读取summary来计算结果。84条row逐项等于各自record，当前stdout相应report字段一致；42对的顺序和全部请求身份符合plan。6份冻结源、model/runner及工具身份重哈希，已有raw日志/顶层文件1183份读前后hash不变。

## 验证、预检与输出

- 84/84请求有效、Success、exit0，无timeout/errors；84次独立Verify均为1且各有一次verify executed事件。failures.json为空，没有过滤invalid行后再Pass。
- 456个实际unit receipt全部当前preflight_ns>0；其中54个builder均LLVM23.1.4、完整`default<O2>|merge-functions=after|aliases=false`、verify_before_after=true。402个object-cache命中仍完整当前预检，当前LLVM builder核验标记为false、producer前后核验为true，未混淆。
- 42对完整ELF原字节相等；materialIR仅去第二行唯一verified-fingerprint后42/42相等，raw sidecar 0/42相等如实保留。84份日志ELF/sidecar实际hash等于记录。
- 每对actual_units/jobs/fallback、cache hit/miss/corrupt/store/write-failure拓扑相同；逐unit input_kind及object_bytes序列也相同。object-patch为0。
- 252份真实stdin/stdout/stderr全部核对；独立写出的shared-leaf与owning-state数学模型复算18种case/snapshot/input，与所有输出一致，退出码记录均0。没有调用被测编译器生成oracle，也没有新增native执行。

## 完整wall、CPU、RSS

每arm42请求固定工作量的wall总和为**35838.124785ms→36582.893473ms，+2.078146%**；进程CPU总和41514.726ms→42793.549ms，**+3.080408%**。这只是固定screen工作总量，不是原频率加权score，也不是相对Rust结论。

每格均保留全部观测的算术均值；wall/CPU单位ms，RSS单位KiB。cold/private每格仅1点，warm每arm7点（jobs1为3点），edit-warm为3点（jobs1为1点）。

| 语料 | jobs | 请求 | n/arm | A wall | B wall | B/A变化 | A CPU | B CPU | A RSS | B RSS |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| shared4096 | 2 | cold | 1 | 5087.967 | 5579.641 | +9.663% | 8740.950 | 9729.488 | 560380.0 | 558276.0 |
| shared4096 | 2 | warm | 7 | 592.646 | 583.467 | −1.549% | 705.882 | 703.139 | 312676.0 | 312676.0 |
| shared4096 | 2 | private | 1 | 4909.895 | 4651.613 | −5.260% | 5127.914 | 4863.972 | 538600.0 | 520788.0 |
| shared4096 | 2 | edit-warm | 3 | 582.587 | 530.122 | −9.005% | 699.831 | 667.543 | 312676.0 | 312676.0 |
| shared1024 | 2 | cold | 1 | 773.788 | 1108.127 | +43.208% | 1286.908 | 1596.595 | 312676.0 | 312676.0 |
| shared1024 | 2 | warm | 7 | 137.365 | 154.036 | +12.136% | 161.952 | 177.986 | 312676.0 | 312676.0 |
| shared1024 | 2 | private | 1 | 627.673 | 841.268 | +34.030% | 653.282 | 846.289 | 312676.0 | 312676.0 |
| shared1024 | 2 | edit-warm | 3 | 224.813 | 136.374 | −39.339% | 245.843 | 158.208 | 312676.0 | 312676.0 |
| owned8 | 2 | cold | 1 | 110.245 | 109.146 | −0.996% | 109.689 | 106.912 | 312676.0 | 312676.0 |
| owned8 | 2 | warm | 7 | 32.216 | 31.897 | −0.988% | 31.287 | 31.576 | 312676.0 | 312676.0 |
| owned8 | 2 | private | 1 | 108.318 | 100.872 | −6.875% | 103.542 | 100.115 | 312676.0 | 312676.0 |
| owned8 | 2 | edit-warm | 3 | 35.080 | 34.192 | −2.530% | 34.573 | 33.880 | 312676.0 | 312676.0 |
| shared4096 | 1 | cold | 1 | 9369.338 | 9421.589 | +0.558% | 9313.753 | 9363.130 | 519024.0 | 518760.0 |
| shared4096 | 1 | warm | 3 | 630.878 | 649.398 | +2.936% | 627.598 | 636.773 | 312676.0 | 312676.0 |
| shared4096 | 1 | private | 1 | 4506.156 | 4630.838 | +2.767% | 4473.537 | 4609.586 | 502928.0 | 503196.0 |
| shared4096 | 1 | edit-warm | 1 | 589.084 | 703.740 | +19.464% | 587.766 | 699.341 | 312676.0 | 312676.0 |

16格各8格改善/回退，所有值仍保留，不能摘取shared1024 edit-warm的−39.339%而省略其cold/private的+43.208%/+34.030%。RSS范围A 312676–560380KiB、B 312676–558276KiB；这是wait4单进程高水位口径，可能含launcher/fork floor，非aggregate live峰值，没有确定内存收益。

### builder阶段也没有建立收益

直接对所有unit receipt的builder_ns求和，A为**280.644815ms**，B为**323.941232ms**，+15.427478%。这不是独占wall（不同worker的unit elapsed会重叠），不能从总wall扣除，也不能拿机械减少插入点API调用的源码事实代替阶段计时证据。cache-hit receipt的builder_ns为0，部分warm改善没有实际调用这个被修改builder阶段，不能把其全部时间差归因于本机制。现有证据没有建立“虽整体失败但微小阶段成功”的结论，也没有定位某个环境因素为唯一原因。

## 决策与证据hash

维持有限screen NoGo：不采纳、不追加样本、不扩矩阵、不更改权重或阈值。没有Rust臂或重复native运行质量计时，短I/O与同ELF证据不能被改称runtime Rust90比较。结构化sink和本候选分别使用独立测量窗口，两个窗口的baseline有差别，不能互相做微收益排名；两者各自原不利结果都保留，main990不变。

| 文件 | SHA256 |
| --- | --- |
| plan.json | `484fd8bf27c5616d61bead1785f8aac2b1bd7e839b010806628e94ca3af3242e` |
| rows.jsonl | `078f98ead912273bc39868cbd0d23959d9c2d66f634b7c8d56f647539f8cc5f2` |
| pairs.jsonl | `ed16da7abe5d382a15b8e2ccabe14a9631b5abfec991ad6cad65971584ae9784` |
| failures.json | `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570` |
| completion.json | `7538db2ce2d0385d2458c8964c1edd96e16a3aff7fc0c6de4534e272786695f4` |
