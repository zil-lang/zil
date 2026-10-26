# 136 当前源码研究结果

## 本轮新增而非旧成绩

从真实源码 Scan 提取一次直接种子与参数投影，生产候选对16个以上资源参数函数改走稀疏事实图。不是131的跨请求撤销算法，也未修改独立Verify。基线为当前生产worklist，额外sweeps仅作正确性oracle。

远端受测源码 `9c1644be786b33abf3549378714865be27606bb0`，本地同源 `7f7fc784935e67882985c938c22b3a3ba4a931d0`。crates/spec/policy/规范的Git对象已核对一致。GitHub Actions 40325991072 和本地各1263项workspace通过、0失败、0忽略；fmt和全workspace/all-targets Clippy通过。独立审核尚未完成。

## 固定完整进程试验

52次独立预检全部通过，216份阶段文件逐字节一致。随后固定26个cell×16配对块×4进程，加52次保留预热，共1716次编译进程、396次原生执行，无失败。每块ABBA/BAAB交替、场景顺序交替；新进程、暖文件缓存、固定单核。原始warmup、全部样本、stderr、阶段IR和程序均在旁置证据归档，不进入Git。

下表百分比是每块两次B均值/两次A均值的配对比值中位数，不等于两边总体中位数相除。区间是2000次固定seed块bootstrap，属于探索测量；不是产品门、不是多重比较校正的正式显著性声明。

|场景|阶段|耗时变化|95%配对比值区间|
|---|---|---:|---|
|tiny|types|-11.48%|0.789–1.041|
|tiny|verify|-6.02%|0.805–1.105|
|below-threshold|types|-0.60%|0.793–1.031|
|below-threshold|verify|-12.96%|0.818–1.003|
|chain16-caller-first|types|-8.57%|0.756–0.982|
|chain16-caller-first|verify|-11.84%|0.783–1.029|
|chain64-caller-first|types|-16.10%|0.807–0.927|
|chain64-caller-first|verify|+11.36%|1.007–1.260|
|chain64-callee-first|types|+1.96%|0.800–1.116|
|chain64-callee-first|verify|-5.99%|0.870–1.000|
|chain24-wide8|types|+0.83%|0.880–1.106|
|chain24-wide8|verify|+6.46%|0.862–1.188|
|cycle16-width4|types|+10.76%|0.923–1.319|
|cycle16-width4|verify|+0.02%|0.856–1.143|
|cycle16-width8|types|-8.01%|0.850–1.035|
|cycle16-width8|verify|+12.20%|0.993–1.764|
|cycle16-width16|types|-10.47%|0.854–1.051|
|cycle16-width16|verify|+1.70%|0.878–1.275|
|cycle32-width8|types|+11.68%|0.827–1.888|
|cycle32-width8|verify|+1.45%|0.844–1.044|
|tiny|O0|+9.78%|0.782–1.284|
|tiny|O2|+11.55%|1.022–1.257|
|chain24-wide8|O0|+1.83%|0.919–1.305|
|chain24-wide8|O2|-6.65%|0.798–0.988|
|cycle16-width8|O0|+7.66%|0.923–1.230|
|cycle16-width8|O2|-10.63%|0.776–0.946|

## 裁决与下一处真正缺口

**Inconclusive / NotEligible：不以本轮数据合入生产默认路径。** 类型阶段和部分完整Build有较好点值，但若干Verify场景退化，未修改的小程序控制组也有明显摆动。不能只挑较快格子宣称整体加速。

确定的算法进展是有限参数事实的源码提取与等价不动点闭环，避免效果变化反复解释调用者；并已把源面、全Core转移枚举、穷举图、预算前缀和真实机器路径接起来。逻辑计费降低不等于实际时间收益。后续应剖分生产Types中的效果提取/求解占比，复用该图而非每次重建，并在同一进程做归因控制；不得跳过完整请求复核。

四目标：未增加程序运行表示；安全结论限于既有receiver推断等价和独立Verify回归；用户语法不变；本轮尚未建立编译总成本优势。完整语言NotSatisfied、产品NotRun。研究代码留在任务分支，不修改main的默认算法。

正式统计/准入门不改。运行 `measure.py --help` 获取复现参数；必须指定编译器构建提交，脚本核对当前源码对象与构建对象。CI事件head是transport父提交，受测HEAD以artifact receipts/commit.txt为准。
