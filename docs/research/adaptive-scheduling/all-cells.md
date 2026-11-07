# 100 全部50个完整成本单元

数值为21个同轮H/BASE比值的中位数；括号为固定种子100、2000次有放回抽样的探索性95%区间。未作多重比较校正；低于1表示H更快。比值不是两个独立中位数相除。所有单元保留，不建立加权总分。

## 完整进程：10单元

| 输入 | 模式 | H/BASE |
|---|---|---|
| bytes-main | build | 1.001836 [0.981138, 1.044459] |
| bytes-main | verify | 1.016418 [0.993092, 1.024623] |
| forward128 | verify | 1.013080 [0.993156, 1.036008] |
| modules512 | verify | 0.997951 [0.981761, 1.034047] |
| owned-bytes-main | build | 0.992342 [0.933131, 1.021230] |
| owned-bytes-main | verify | 1.012165 [0.975842, 1.043759] |
| owned-main | build | 1.005348 [0.986727, 1.012421] |
| owned-main | verify | 0.998704 [0.982295, 1.016336] |
| reverse128 | verify | 0.724669 [0.697742, 0.738673] |
| tiny | verify | 1.019987 [0.824728, 1.240691] |

## compile_path API：40单元

| 输入 | 模式 | 情景 | H/BASE |
|---|---|---|---|
| bytes-main | build | cold | 0.992253 [0.966980, 1.025093] |
| bytes-main | build | unchanged | 0.987721 [0.944948, 1.013375] |
| bytes-main | build | private_edit | 0.981901 [0.941522, 1.064527] |
| bytes-main | build | public_edit | 1.010899 [0.991053, 1.050654] |
| bytes-main | verify | cold | 1.012235 [0.969599, 1.070559] |
| bytes-main | verify | unchanged | 1.007513 [0.986796, 1.082496] |
| bytes-main | verify | private_edit | 1.003085 [0.989408, 1.029267] |
| bytes-main | verify | public_edit | 1.018105 [0.986119, 1.028135] |
| forward128 | verify | cold | 1.027132 [0.997197, 1.045650] |
| forward128 | verify | unchanged | 1.010498 [0.999657, 1.029456] |
| forward128 | verify | private_edit | 1.012238 [0.990413, 1.041777] |
| forward128 | verify | public_edit | 1.022878 [0.994461, 1.032877] |
| modules512 | verify | cold | 1.005096 [0.959111, 1.042224] |
| modules512 | verify | unchanged | 1.002145 [0.930935, 1.048408] |
| modules512 | verify | private_edit | 1.020211 [0.973864, 1.061288] |
| modules512 | verify | public_edit | 1.036823 [1.020533, 1.056133] |
| owned-bytes-main | build | cold | 1.002480 [0.947346, 1.049372] |
| owned-bytes-main | build | unchanged | 1.012254 [0.896313, 1.035991] |
| owned-bytes-main | build | private_edit | 0.993927 [0.972534, 1.005877] |
| owned-bytes-main | build | public_edit | 0.996146 [0.914774, 1.011987] |
| owned-bytes-main | verify | cold | 1.031342 [1.022438, 1.046140] |
| owned-bytes-main | verify | unchanged | 1.015172 [0.910138, 1.066721] |
| owned-bytes-main | verify | private_edit | 0.987646 [0.937900, 1.048981] |
| owned-bytes-main | verify | public_edit | 1.016715 [0.962041, 1.047754] |
| owned-main | build | cold | 1.031178 [0.975764, 1.064567] |
| owned-main | build | unchanged | 0.986369 [0.949231, 1.003316] |
| owned-main | build | private_edit | 1.031231 [1.005183, 1.087669] |
| owned-main | build | public_edit | 0.990167 [0.955705, 1.033422] |
| owned-main | verify | cold | 1.001702 [0.966049, 1.021299] |
| owned-main | verify | unchanged | 0.990548 [0.950647, 1.024978] |
| owned-main | verify | private_edit | 1.003014 [0.966093, 1.035098] |
| owned-main | verify | public_edit | 1.012081 [0.994896, 1.020272] |
| reverse128 | verify | cold | 0.692823 [0.653919, 0.706563] |
| reverse128 | verify | unchanged | 1.014435 [0.998970, 1.077080] |
| reverse128 | verify | private_edit | 0.690901 [0.674524, 0.700890] |
| reverse128 | verify | public_edit | 0.688282 [0.669088, 0.711629] |
| tiny | verify | cold | 0.991094 [0.970511, 1.025932] |
| tiny | verify | unchanged | 0.988163 [0.965957, 1.035381] |
| tiny | verify | private_edit | 0.994018 [0.882119, 1.082975] |
| tiny | verify | public_edit | 1.029475 [0.993775, 1.071840] |

原始21次值、父进程VmHWM和身份见机器summary与完整raw归档。
