# 084 全部正式配对结果

各单元保留7对；比值为逐对candidate/baseline的中位数，与两列中位数相除不同。小于1表示候选较快；区间为预登记的探索bootstrap，不能当产品门。

## 完整请求API

| 语料 | 模式 | 场景 | 基线中位ms | 候选中位ms | 配对比 | 探索95%区间 |
|---|---|---|---:|---:|---:|---|
| byte_arithmetic | build | cold | 64.255 | 68.967 | 1.1321 | [0.9188, 1.1764] |
| byte_arithmetic | build | unchanged | 57.280 | 59.352 | 0.9913 | [0.9465, 1.1466] |
| byte_arithmetic | build | private_edit | 63.747 | 64.774 | 0.9877 | [0.9131, 1.0898] |
| byte_arithmetic | build | public_edit | 66.115 | 67.711 | 0.9811 | [0.9173, 1.0256] |
| byte_arithmetic | verify | cold | 6.544 | 6.536 | 1.0172 | [0.6097, 1.0373] |
| byte_arithmetic | verify | unchanged | 1.014 | 0.945 | 0.9770 | [0.8593, 1.0552] |
| byte_arithmetic | verify | private_edit | 4.873 | 5.259 | 1.0108 | [0.9767, 1.1171] |
| byte_arithmetic | verify | public_edit | 6.013 | 5.961 | 1.0071 | [0.9827, 1.1236] |
| helper_ui | build | cold | 70.761 | 74.521 | 1.0937 | [0.9666, 1.1232] |
| helper_ui | build | unchanged | 61.825 | 60.652 | 0.9891 | [0.8688, 1.0554] |
| helper_ui | build | private_edit | 69.654 | 66.961 | 1.0178 | [0.9039, 1.0841] |
| helper_ui | build | public_edit | 69.855 | 72.080 | 1.0549 | [0.8982, 1.0809] |
| helper_ui | verify | cold | 10.599 | 10.052 | 1.0087 | [0.9644, 1.0903] |
| helper_ui | verify | unchanged | 1.564 | 1.560 | 1.0034 | [0.9727, 1.1760] |
| helper_ui | verify | private_edit | 8.128 | 8.677 | 1.0169 | [0.9875, 1.0882] |
| helper_ui | verify | public_edit | 9.820 | 9.379 | 0.9550 | [0.9309, 1.0785] |
| modules128 | verify | cold | 30.363 | 29.685 | 1.0043 | [0.9436, 1.0393] |
| modules128 | verify | unchanged | 9.282 | 9.509 | 1.0248 | [0.9755, 1.0311] |
| modules128 | verify | private_edit | 23.736 | 23.693 | 0.9896 | [0.9366, 1.0294] |
| modules128 | verify | public_edit | 29.095 | 29.426 | 1.0233 | [0.9860, 1.0484] |
| modules16 | verify | cold | 4.315 | 4.316 | 1.0003 | [0.9626, 1.0394] |
| modules16 | verify | unchanged | 1.357 | 1.322 | 0.9766 | [0.8631, 1.0529] |
| modules16 | verify | private_edit | 3.251 | 3.360 | 1.0366 | [0.9855, 1.1331] |
| modules16 | verify | public_edit | 4.020 | 3.920 | 1.0313 | [0.9652, 1.0857] |
| modules512 | verify | cold | 128.850 | 132.691 | 1.0298 | [0.9769, 1.1800] |
| modules512 | verify | unchanged | 40.890 | 39.594 | 0.9828 | [0.9446, 1.1640] |
| modules512 | verify | private_edit | 107.097 | 112.503 | 0.9834 | [0.8025, 1.1551] |
| modules512 | verify | public_edit | 123.457 | 132.533 | 1.0411 | [1.0048, 1.1418] |
| modules64 | verify | cold | 15.676 | 14.871 | 0.9418 | [0.9221, 0.9674] |
| modules64 | verify | unchanged | 4.900 | 4.656 | 0.9557 | [0.9384, 0.9771] |
| modules64 | verify | private_edit | 12.498 | 11.853 | 0.9536 | [0.8586, 0.9922] |
| modules64 | verify | public_edit | 14.242 | 14.165 | 0.9889 | [0.9577, 0.9934] |
| modules64-default-capacity | verify | cold | 15.619 | 15.149 | 0.9776 | [0.9036, 1.0127] |
| modules64-default-capacity | verify | unchanged | 14.314 | 14.065 | 1.0190 | [0.8898, 1.0392] |
| modules64-default-capacity | verify | private_edit | 14.285 | 14.247 | 0.9837 | [0.8621, 1.0342] |
| modules64-default-capacity | verify | public_edit | 14.828 | 14.276 | 0.9636 | [0.9125, 0.9941] |
| owned_component | build | cold | 70.676 | 76.972 | 1.0129 | [0.9753, 1.1449] |
| owned_component | build | unchanged | 71.721 | 63.381 | 0.9297 | [0.7170, 1.1125] |
| owned_component | build | private_edit | 69.722 | 73.600 | 1.0534 | [0.9952, 1.0884] |
| owned_component | build | public_edit | 70.767 | 72.820 | 1.0462 | [0.8759, 1.0652] |
| owned_component | verify | cold | 12.287 | 12.176 | 0.9591 | [0.9476, 1.0133] |
| owned_component | verify | unchanged | 1.747 | 1.815 | 1.0194 | [0.9700, 1.0697] |
| owned_component | verify | private_edit | 10.102 | 11.544 | 1.1207 | [1.0653, 1.3214] |
| owned_component | verify | public_edit | 11.214 | 11.723 | 1.0712 | [0.8885, 1.1377] |
| scalar | verify | cold | 0.791 | 0.744 | 0.9569 | [0.8808, 1.0435] |
| scalar | verify | unchanged | 0.113 | 0.113 | 0.9996 | [0.9419, 1.0530] |
| scalar | verify | private_edit | 0.366 | 0.347 | 0.9355 | [0.6820, 0.9815] |
| scalar | verify | public_edit | 0.446 | 0.453 | 0.9806 | [0.9669, 1.0529] |

## 四请求完整进程及峰值

父进程VmHWM为四请求合计高水位，不是每阶段独占，也不含外部clang子进程。进程wall含报告/源身份记录、编辑恢复及会话销毁。

| 语料 | 模式 | 基线中位ms | 候选中位ms | 配对比 | 探索95%区间 | 基线/候选VmHWM中位KiB |
|---|---|---:|---:|---:|---|---:|
| byte_arithmetic | build | 259.546 | 267.076 | 1.0290 | [0.9586, 1.1091] | 7264 / 7296 |
| byte_arithmetic | verify | 22.428 | 22.004 | 0.9664 | [0.8003, 1.1305] | 7000 / 7048 |
| helper_ui | build | 274.714 | 277.895 | 1.0265 | [0.9671, 1.0704] | 7688 / 7724 |
| helper_ui | verify | 34.238 | 33.488 | 1.0016 | [0.9757, 1.0178] | 7356 / 7384 |
| modules128 | verify | 101.748 | 99.942 | 1.0165 | [0.8913, 1.0547] | 16376 / 16372 |
| modules16 | verify | 16.163 | 16.108 | 1.0142 | [0.8947, 1.0620] | 6192 / 6192 |
| modules512 | verify | 435.753 | 451.526 | 0.9958 | [0.9595, 1.0906] | 51720 / 51652 |
| modules64 | verify | 53.079 | 51.414 | 0.9609 | [0.9466, 0.9769] | 10492 / 10508 |
| modules64-default-capacity | verify | 64.020 | 62.949 | 0.9797 | [0.8858, 1.0112] | 7548 / 7672 |
| owned_component | build | 288.958 | 289.973 | 1.0067 | [0.9483, 1.0593] | 7888 / 7844 |
| owned_component | verify | 40.723 | 43.525 | 1.0634 | [0.9113, 1.1462] | 7520 / 7592 |
| scalar | verify | 5.034 | 6.245 | 0.9654 | [0.8401, 1.5611] | 4700 / 4676 |

逐进程/逐请求的原始数值、每次父进程峰值、输出身份与完整报告位置见[evidence/impl/084/performance-samples.json](../../../evidence/impl/084/performance-samples.json)。完整原始包包含所有阶段文件、Build可执行文件、源码和预热。
