# 第三、四阶段完整测量表

本表按各自冻结窗口列出全部 176 个 CLI/Session 单元及 16 个真实源码单元；不跨窗口合并或叠乘。最终候选为 9fe5f55e。阶段2的 MEASUREMENTS.md 保持历史身份。详情见 [当前报告](CURRENT.md)。

时间为区组内每臂均值的中位数，单位 ms；比值 candidate/baseline，小于1较快。RSS 为 KiB，CLI 是子进程峰值，Session 是请求后观测值，二者不能混用。rustc 仅为同一生成器的直接命令参考，不是 Cargo/全语言产品对等结论。p95 和全部原始记录保留在 evidence 的 summary/raw 档案。

## stage3-main-final

baseline `21058d0e331de764e3a1b9123c3e1edf5e55a1aa`；candidate `5e0613cc4d09bc99dcd1fba00a371eed691f8a37`；5 blocks，1560 请求。墙钟不利单元 6/44。

| 接口/模式 | 模块/容量 | 请求 | baseline ms | candidate ms | 比值 | baseline/candidate RSS | rustc ms |
|---|---:|---|---:|---:|---:|---:|---:|
| cli/build | 32/0 | cold | 49.222 | 48.060 | 0.9764 | 25320/25588 | 85.518 |
| cli/build | 32/0 | private1 | 41.552 | 39.903 | 0.9603 | 25320/25320 | 76.561 |
| cli/build | 32/0 | private2 | 40.279 | 40.254 | 0.9994 | 25320/25320 | 68.838 |
| cli/build | 32/0 | restore | 38.238 | 38.183 | 0.9986 | 25320/25320 | 74.824 |
| cli/build | 32/0 | unchanged | 39.420 | 38.180 | 0.9685 | 25320/25320 | 59.947 |
| cli/check | 32/0 | cold | 13.845 | 13.151 | 0.9499 | 25320/25320 | 33.978 |
| cli/check | 32/0 | private1 | 15.528 | 12.233 | 0.7878 | 25320/25320 | 28.408 |
| cli/check | 32/0 | private2 | 13.139 | 12.618 | 0.9603 | 25320/25320 | 27.006 |
| cli/check | 32/0 | restore | 13.299 | 12.703 | 0.9552 | 25320/25320 | 26.006 |
| cli/check | 32/0 | unchanged | 13.293 | 13.302 | 1.0007 | 25320/25320 | 24.496 |
| cli/build | 128/0 | cold | 90.333 | 86.116 | 0.9533 | 39286/39206 | 150.340 |
| cli/build | 128/0 | private1 | 72.106 | 68.437 | 0.9491 | 37404/37036 | 116.822 |
| cli/build | 128/0 | private2 | 70.828 | 70.990 | 1.0023 | 37408/37096 | 92.275 |
| cli/build | 128/0 | restore | 68.089 | 66.505 | 0.9767 | 32418/32552 | 116.758 |
| cli/build | 128/0 | unchanged | 74.626 | 66.801 | 0.8951 | 32544/32550 | 83.660 |
| cli/check | 128/0 | cold | 33.574 | 36.068 | 1.0743 | 25320/27048 | 65.973 |
| cli/check | 128/0 | private1 | 37.324 | 33.305 | 0.8923 | 25320/27052 | 44.342 |
| cli/check | 128/0 | private2 | 37.426 | 33.820 | 0.9036 | 25320/27048 | 44.125 |
| cli/check | 128/0 | restore | 35.766 | 35.129 | 0.9822 | 25320/27050 | 43.739 |
| cli/check | 128/0 | unchanged | 38.500 | 33.624 | 0.8733 | 25320/27050 | 42.243 |
| session/build | 32/32 | cold | 39.764 | 41.180 | 1.0356 | 26064/25502 | — |
| session/build | 32/32 | private | 31.081 | 30.401 | 0.9781 | 24654/24758 | — |
| session/build | 32/32 | restore | 29.681 | 29.094 | 0.9802 | 25284/24394 | — |
| session/build | 32/32 | unchanged | 28.868 | 25.811 | 0.8941 | 24632/23992 | — |
| session/check | 32/32 | cold | 5.345 | 5.421 | 1.0143 | 17756/17772 | — |
| session/check | 32/32 | private | 4.552 | 4.089 | 0.8983 | 17910/17882 | — |
| session/check | 32/32 | restore | 4.597 | 4.224 | 0.9188 | 17916/17904 | — |
| session/check | 32/32 | unchanged | 4.959 | 1.269 | 0.2559 | 17844/17772 | — |
| session/build | 128/32 | cold | 75.711 | 77.183 | 1.0194 | 41872/38404 | — |
| session/build | 128/32 | private | 61.515 | 57.664 | 0.9374 | 39708/38064 | — |
| session/build | 128/32 | restore | 61.900 | 55.987 | 0.9045 | 39562/37670 | — |
| session/build | 128/32 | unchanged | 56.417 | 39.702 | 0.7037 | 39512/37336 | — |
| session/check | 128/32 | cold | 22.431 | 22.315 | 0.9948 | 27182/26996 | — |
| session/check | 128/32 | private | 20.181 | 19.368 | 0.9597 | 27254/27168 | — |
| session/check | 128/32 | restore | 20.413 | 19.514 | 0.9559 | 27174/27160 | — |
| session/check | 128/32 | unchanged | 19.623 | 5.228 | 0.2664 | 27402/26996 | — |
| session/check | 128/8192 | cold | 22.147 | 22.050 | 0.9956 | 27212/26972 | — |
| session/check | 128/8192 | private | 19.038 | 18.177 | 0.9547 | 50544/46766 | — |
| session/check | 128/8192 | restore | 6.218 | 5.611 | 0.9024 | 63630/59914 | — |
| session/check | 128/8192 | unchanged | 6.081 | 5.159 | 0.8483 | 27212/26972 | — |
| session/check | 512/32 | cold | 112.757 | 108.828 | 0.9652 | 64778/63182 | — |
| session/check | 512/32 | private | 105.656 | 103.180 | 0.9766 | 65260/64528 | — |
| session/check | 512/32 | restore | 109.136 | 104.273 | 0.9554 | 65190/64410 | — |
| session/check | 512/32 | unchanged | 108.236 | 21.629 | 0.1998 | 65374/63182 | — |

## stage3-isolation

baseline `5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`；candidate `5e0613cc4d09bc99dcd1fba00a371eed691f8a37`；5 blocks，1560 请求。墙钟不利单元 19/44。

| 接口/模式 | 模块/容量 | 请求 | baseline ms | candidate ms | 比值 | baseline/candidate RSS | rustc ms |
|---|---:|---|---:|---:|---:|---:|---:|
| cli/build | 32/0 | cold | 47.858 | 50.356 | 1.0522 | 25572/25480 | 80.806 |
| cli/build | 32/0 | private1 | 41.912 | 40.730 | 0.9718 | 25216/25216 | 77.730 |
| cli/build | 32/0 | private2 | 42.778 | 40.509 | 0.9470 | 25216/25216 | 69.013 |
| cli/build | 32/0 | restore | 40.619 | 37.931 | 0.9338 | 25216/25216 | 80.128 |
| cli/build | 32/0 | unchanged | 37.537 | 40.881 | 1.0891 | 25216/25216 | 61.769 |
| cli/check | 32/0 | cold | 12.836 | 12.253 | 0.9545 | 25216/25216 | 31.809 |
| cli/check | 32/0 | private1 | 12.695 | 11.851 | 0.9335 | 25216/25216 | 25.692 |
| cli/check | 32/0 | private2 | 12.459 | 12.574 | 1.0092 | 25216/25216 | 24.717 |
| cli/check | 32/0 | restore | 12.450 | 12.544 | 1.0076 | 25216/25216 | 24.401 |
| cli/check | 32/0 | unchanged | 12.911 | 11.743 | 0.9095 | 25216/25216 | 22.517 |
| cli/build | 128/0 | cold | 85.137 | 84.883 | 0.9970 | 39258/39146 | 142.453 |
| cli/build | 128/0 | private1 | 70.297 | 69.475 | 0.9883 | 37048/37038 | 111.034 |
| cli/build | 128/0 | private2 | 71.461 | 68.633 | 0.9604 | 37116/37074 | 90.402 |
| cli/build | 128/0 | restore | 65.585 | 64.545 | 0.9841 | 32544/32450 | 112.515 |
| cli/build | 128/0 | unchanged | 65.393 | 66.440 | 1.0160 | 32548/32562 | 80.706 |
| cli/check | 128/0 | cold | 38.621 | 33.214 | 0.8600 | 27046/27050 | 67.201 |
| cli/check | 128/0 | private1 | 36.051 | 34.293 | 0.9512 | 27046/27082 | 44.394 |
| cli/check | 128/0 | private2 | 32.892 | 33.034 | 1.0043 | 27046/27052 | 43.179 |
| cli/check | 128/0 | restore | 33.017 | 34.721 | 1.0516 | 27044/27052 | 41.864 |
| cli/check | 128/0 | unchanged | 32.810 | 33.118 | 1.0094 | 27046/27048 | 40.059 |
| session/build | 32/32 | cold | 40.822 | 38.329 | 0.9389 | 25468/25562 | — |
| session/build | 32/32 | private | 30.668 | 31.973 | 1.0425 | 24670/24850 | — |
| session/build | 32/32 | restore | 29.006 | 30.900 | 1.0653 | 24114/24754 | — |
| session/build | 32/32 | unchanged | 26.065 | 26.780 | 1.0274 | 24010/24052 | — |
| session/check | 32/32 | cold | 5.373 | 5.301 | 0.9866 | 17732/17738 | — |
| session/check | 32/32 | private | 4.175 | 4.232 | 1.0134 | 17850/17858 | — |
| session/check | 32/32 | restore | 4.115 | 4.181 | 1.0163 | 17864/17870 | — |
| session/check | 32/32 | unchanged | 1.566 | 1.285 | 0.8204 | 17732/17738 | — |
| session/build | 128/32 | cold | 75.016 | 77.716 | 1.0360 | 38028/38314 | — |
| session/build | 128/32 | private | 56.993 | 57.984 | 1.0174 | 37400/37732 | — |
| session/build | 128/32 | restore | 53.799 | 56.700 | 1.0539 | 37254/37278 | — |
| session/build | 128/32 | unchanged | 42.648 | 39.199 | 0.9191 | 37010/36988 | — |
| session/check | 128/32 | cold | 23.816 | 22.010 | 0.9241 | 27032/27004 | — |
| session/check | 128/32 | private | 20.653 | 19.266 | 0.9328 | 27188/27248 | — |
| session/check | 128/32 | restore | 20.693 | 20.129 | 0.9728 | 27184/27256 | — |
| session/check | 128/32 | unchanged | 8.556 | 5.379 | 0.6287 | 27060/27004 | — |
| session/check | 128/8192 | cold | 22.215 | 21.341 | 0.9606 | 27010/26966 | — |
| session/check | 128/8192 | private | 18.234 | 18.079 | 0.9915 | 46770/46756 | — |
| session/check | 128/8192 | restore | 6.278 | 5.563 | 0.8860 | 59926/59900 | — |
| session/check | 128/8192 | unchanged | 5.746 | 5.167 | 0.8992 | 27010/26966 | — |
| session/check | 512/32 | cold | 125.258 | 128.376 | 1.0249 | 63266/63320 | — |
| session/check | 512/32 | private | 109.426 | 124.151 | 1.1346 | 64392/64794 | — |
| session/check | 512/32 | restore | 115.998 | 124.935 | 1.0770 | 64386/64872 | — |
| session/check | 512/32 | unchanged | 39.896 | 29.361 | 0.7359 | 63266/63320 | — |

## stage4-main-final

baseline `21058d0e331de764e3a1b9123c3e1edf5e55a1aa`；candidate `9fe5f55e64c7d537b9d585d3d40e42c9e248308e`；10 blocks，3120 请求。墙钟不利单元 5/44。

| 接口/模式 | 模块/容量 | 请求 | baseline ms | candidate ms | 比值 | baseline/candidate RSS | rustc ms |
|---|---:|---|---:|---:|---:|---:|---:|
| cli/build | 32/0 | cold | 49.384 | 46.296 | 0.9375 | 25432/25578 | 83.952 |
| cli/build | 32/0 | private1 | 41.604 | 43.519 | 1.0460 | 25432/25432 | 80.817 |
| cli/build | 32/0 | private2 | 41.664 | 43.400 | 1.0417 | 25432/25432 | 70.820 |
| cli/build | 32/0 | restore | 38.500 | 40.111 | 1.0418 | 25432/25432 | 81.880 |
| cli/build | 32/0 | unchanged | 39.626 | 39.857 | 1.0058 | 25432/25432 | 62.369 |
| cli/check | 32/0 | cold | 13.238 | 13.239 | 1.0001 | 25432/25432 | 32.781 |
| cli/check | 32/0 | private1 | 12.376 | 12.270 | 0.9914 | 25432/25432 | 27.282 |
| cli/check | 32/0 | private2 | 12.563 | 12.344 | 0.9826 | 25432/25432 | 26.193 |
| cli/check | 32/0 | restore | 12.548 | 12.336 | 0.9831 | 25432/25432 | 25.478 |
| cli/check | 32/0 | unchanged | 12.570 | 12.271 | 0.9762 | 25432/25432 | 22.778 |
| cli/build | 128/0 | cold | 89.370 | 85.148 | 0.9528 | 39288/39204 | 152.046 |
| cli/build | 128/0 | private1 | 72.768 | 70.711 | 0.9717 | 37402/37058 | 119.699 |
| cli/build | 128/0 | private2 | 74.832 | 71.581 | 0.9566 | 37404/37162 | 100.727 |
| cli/build | 128/0 | restore | 71.473 | 64.822 | 0.9069 | 32542/32556 | 124.378 |
| cli/build | 128/0 | unchanged | 69.962 | 66.035 | 0.9439 | 32416/32556 | 88.721 |
| cli/check | 128/0 | cold | 34.019 | 33.213 | 0.9763 | 25432/27052 | 67.594 |
| cli/check | 128/0 | private1 | 33.839 | 31.530 | 0.9317 | 25432/27052 | 42.867 |
| cli/check | 128/0 | private2 | 33.859 | 31.549 | 0.9318 | 25432/27056 | 43.509 |
| cli/check | 128/0 | restore | 33.828 | 32.043 | 0.9472 | 25432/27052 | 43.457 |
| cli/check | 128/0 | unchanged | 33.481 | 31.566 | 0.9428 | 25432/27056 | 37.329 |
| session/build | 32/32 | cold | 40.443 | 38.995 | 0.9642 | 26012/25526 | — |
| session/build | 32/32 | private | 31.421 | 29.423 | 0.9364 | 25200/24818 | — |
| session/build | 32/32 | restore | 29.511 | 28.020 | 0.9495 | 24876/24700 | — |
| session/build | 32/32 | unchanged | 29.409 | 24.941 | 0.8481 | 24542/23962 | — |
| session/check | 32/32 | cold | 5.465 | 5.108 | 0.9346 | 17722/17716 | — |
| session/check | 32/32 | private | 4.842 | 3.789 | 0.7826 | 17882/17846 | — |
| session/check | 32/32 | restore | 4.790 | 3.891 | 0.8124 | 17882/17864 | — |
| session/check | 32/32 | unchanged | 4.718 | 0.840 | 0.1780 | 17810/17716 | — |
| session/build | 128/32 | cold | 76.771 | 70.883 | 0.9233 | 41802/38332 | — |
| session/build | 128/32 | private | 57.856 | 53.192 | 0.9194 | 39586/38010 | — |
| session/build | 128/32 | restore | 56.982 | 52.148 | 0.9152 | 39602/37482 | — |
| session/build | 128/32 | unchanged | 55.531 | 35.479 | 0.6389 | 39376/37318 | — |
| session/check | 128/32 | cold | 23.654 | 20.655 | 0.8732 | 27216/26990 | — |
| session/check | 128/32 | private | 21.176 | 19.810 | 0.9355 | 27308/27140 | — |
| session/check | 128/32 | restore | 20.998 | 19.211 | 0.9149 | 27208/27158 | — |
| session/check | 128/32 | unchanged | 20.484 | 3.536 | 0.1726 | 27436/27026 | — |
| session/check | 128/8192 | cold | 23.232 | 20.814 | 0.8959 | 27168/26968 | — |
| session/check | 128/8192 | private | 19.603 | 16.937 | 0.8640 | 50500/46742 | — |
| session/check | 128/8192 | restore | 6.560 | 3.768 | 0.5744 | 63582/59898 | — |
| session/check | 128/8192 | unchanged | 6.430 | 3.315 | 0.5155 | 27168/26968 | — |
| session/check | 512/32 | cold | 112.300 | 104.075 | 0.9268 | 64768/63272 | — |
| session/check | 512/32 | private | 107.113 | 94.652 | 0.8837 | 65284/64734 | — |
| session/check | 512/32 | restore | 106.814 | 94.379 | 0.8836 | 65180/64840 | — |
| session/check | 512/32 | unchanged | 106.917 | 13.443 | 0.1257 | 65364/63272 | — |

## stage4-isolation

baseline `a4bec09fbaff29a353ad21571e183166de0968dc`；candidate `9fe5f55e64c7d537b9d585d3d40e42c9e248308e`；10 blocks，3120 请求。墙钟不利单元 4/44。

| 接口/模式 | 模块/容量 | 请求 | baseline ms | candidate ms | 比值 | baseline/candidate RSS | rustc ms |
|---|---:|---|---:|---:|---:|---:|---:|
| cli/build | 32/0 | cold | 45.841 | 46.197 | 1.0078 | 25526/25574 | 81.735 |
| cli/build | 32/0 | private1 | 39.401 | 39.435 | 1.0009 | 25228/25228 | 76.695 |
| cli/build | 32/0 | private2 | 39.826 | 39.005 | 0.9794 | 25228/25228 | 67.526 |
| cli/build | 32/0 | restore | 37.892 | 36.294 | 0.9578 | 25228/25228 | 74.776 |
| cli/build | 32/0 | unchanged | 36.532 | 38.009 | 1.0404 | 25228/25228 | 60.296 |
| cli/check | 32/0 | cold | 12.970 | 12.142 | 0.9362 | 25228/25228 | 31.948 |
| cli/check | 32/0 | private1 | 12.127 | 11.594 | 0.9560 | 25228/25228 | 24.651 |
| cli/check | 32/0 | private2 | 12.666 | 12.101 | 0.9553 | 25228/25228 | 24.758 |
| cli/check | 32/0 | restore | 12.647 | 11.688 | 0.9241 | 25228/25228 | 24.198 |
| cli/check | 32/0 | unchanged | 12.947 | 11.720 | 0.9052 | 25228/25228 | 21.806 |
| cli/build | 128/0 | cold | 83.129 | 80.477 | 0.9681 | 39236/39196 | 139.684 |
| cli/build | 128/0 | private1 | 68.817 | 67.980 | 0.9878 | 37052/37042 | 111.215 |
| cli/build | 128/0 | private2 | 68.583 | 66.050 | 0.9631 | 37142/37160 | 91.251 |
| cli/build | 128/0 | restore | 64.711 | 63.160 | 0.9760 | 32534/32520 | 109.836 |
| cli/build | 128/0 | unchanged | 65.425 | 63.597 | 0.9721 | 32550/32552 | 78.895 |
| cli/check | 128/0 | cold | 31.704 | 31.017 | 0.9783 | 27050/27056 | 65.168 |
| cli/check | 128/0 | private1 | 32.074 | 30.136 | 0.9396 | 27052/27056 | 43.051 |
| cli/check | 128/0 | private2 | 31.326 | 30.386 | 0.9700 | 27050/27052 | 41.397 |
| cli/check | 128/0 | restore | 31.489 | 31.576 | 1.0028 | 27048/27056 | 40.872 |
| cli/check | 128/0 | unchanged | 31.299 | 30.547 | 0.9760 | 27052/27054 | 38.163 |
| session/build | 32/32 | cold | 39.382 | 37.542 | 0.9533 | 25510/25522 | — |
| session/build | 32/32 | private | 30.843 | 30.062 | 0.9747 | 24718/24758 | — |
| session/build | 32/32 | restore | 29.032 | 28.618 | 0.9858 | 24602/24732 | — |
| session/build | 32/32 | unchanged | 26.375 | 24.949 | 0.9460 | 23936/23968 | — |
| session/check | 32/32 | cold | 5.423 | 4.887 | 0.9011 | 17724/17726 | — |
| session/check | 32/32 | private | 4.166 | 3.622 | 0.8694 | 17844/17850 | — |
| session/check | 32/32 | restore | 4.219 | 3.701 | 0.8771 | 17852/17874 | — |
| session/check | 32/32 | unchanged | 1.366 | 0.848 | 0.6207 | 17724/17726 | — |
| session/build | 128/32 | cold | 77.899 | 72.782 | 0.9343 | 38094/38304 | — |
| session/build | 128/32 | private | 56.719 | 52.574 | 0.9269 | 37350/37458 | — |
| session/build | 128/32 | restore | 55.186 | 50.670 | 0.9182 | 37256/37282 | — |
| session/build | 128/32 | unchanged | 39.877 | 35.973 | 0.9021 | 36888/36836 | — |
| session/check | 128/32 | cold | 22.000 | 19.970 | 0.9077 | 27008/27012 | — |
| session/check | 128/32 | private | 19.437 | 17.339 | 0.8921 | 27168/27200 | — |
| session/check | 128/32 | restore | 19.699 | 17.602 | 0.8935 | 27160/27164 | — |
| session/check | 128/32 | unchanged | 5.300 | 3.242 | 0.6117 | 27044/27012 | — |
| session/check | 128/8192 | cold | 21.633 | 19.996 | 0.9243 | 27008/26978 | — |
| session/check | 128/8192 | private | 18.328 | 16.366 | 0.8929 | 46798/46754 | — |
| session/check | 128/8192 | restore | 5.710 | 3.676 | 0.6438 | 59940/59916 | — |
| session/check | 128/8192 | unchanged | 5.175 | 3.153 | 0.6093 | 27008/26978 | — |
| session/check | 512/32 | cold | 113.961 | 103.803 | 0.9109 | 63364/63180 | — |
| session/check | 512/32 | private | 105.896 | 97.618 | 0.9218 | 64370/64412 | — |
| session/check | 512/32 | restore | 106.138 | 96.840 | 0.9124 | 64352/64404 | — |
| session/check | 512/32 | unchanged | 23.154 | 13.896 | 0.6001 | 63364/63180 | — |

## stage4-mixed

8 个真实示例 × O0/O2，5 blocks，320 个测量构建及原生执行，另16个 clang O0/O2 参考构建及原生执行。旧 byte_io O2 的10个错误结果保留；其速度比无效。其余15个单元中6个时间不利。

| 源码 | 优化 | baseline ms | candidate ms | 比值 | 旧结果错误数 |
|---|---|---:|---:|---:|---:|
| byte_io/main.zil | O0 | 37.560 | 37.066 | 0.9869 | 0 |
| byte_io/main.zil | O2 | 78.061 | 80.372 | 无效（误编译） | 10 |
| resource_closures/main.zil | O0 | 39.636 | 38.888 | 0.9811 | 0 |
| resource_closures/main.zil | O2 | 53.319 | 51.922 | 0.9738 | 0 |
| generic_store178/main.zil | O0 | 52.267 | 49.675 | 0.9504 | 0 |
| generic_store178/main.zil | O2 | 83.227 | 78.745 | 0.9461 | 0 |
| mixed_capture182/main.zil | O0 | 36.647 | 34.869 | 0.9515 | 0 |
| mixed_capture182/main.zil | O2 | 48.573 | 46.033 | 0.9477 | 0 |
| ct_selection219/main.zil | O0 | 37.055 | 39.003 | 1.0526 | 0 |
| ct_selection219/main.zil | O2 | 47.235 | 45.971 | 0.9732 | 0 |
| ct_types228/arrays_defaults.zil | O0 | 32.810 | 33.526 | 1.0218 | 0 |
| ct_types228/arrays_defaults.zil | O2 | 35.704 | 36.408 | 1.0197 | 0 |
| where_copy229/main.zil | O0 | 32.710 | 33.693 | 1.0301 | 0 |
| where_copy229/main.zil | O2 | 33.646 | 34.026 | 1.0113 | 0 |
| shared_product224/main.zil | O0 | 35.302 | 36.907 | 1.0455 | 0 |
| shared_product224/main.zil | O2 | 44.503 | 42.174 | 0.9477 | 0 |

## Stage4 Session 进程 CPU 诊断

下面48行使用相同区组汇总规则。CLOCK_PROCESS_CPUTIME_ID 包含编译器线程，不包含子链接器或原生控制进程；因此 build 不是完整构建树 CPU。墙钟计时边界未改变。

| 窗口 | 模式 | 模块/容量 | 请求 | baseline CPU ms | candidate CPU ms | 比值 |
|---|---|---:|---|---:|---:|---:|
| stage4-main-final | build | 32/32 | cold | 27.531 | 25.411 | 0.9230 |
| stage4-main-final | build | 32/32 | private | 9.851 | 8.003 | 0.8124 |
| stage4-main-final | build | 32/32 | restore | 8.841 | 7.036 | 0.7958 |
| stage4-main-final | build | 32/32 | unchanged | 8.559 | 3.807 | 0.4448 |
| stage4-main-final | check | 32/32 | cold | 5.475 | 5.113 | 0.9338 |
| stage4-main-final | check | 32/32 | private | 4.842 | 3.786 | 0.7819 |
| stage4-main-final | check | 32/32 | restore | 4.791 | 3.892 | 0.8124 |
| stage4-main-final | check | 32/32 | unchanged | 4.714 | 0.841 | 0.1784 |
| stage4-main-final | build | 128/32 | cold | 70.081 | 62.431 | 0.8908 |
| stage4-main-final | build | 128/32 | private | 38.196 | 33.077 | 0.8660 |
| stage4-main-final | build | 128/32 | restore | 34.153 | 29.364 | 0.8598 |
| stage4-main-final | build | 128/32 | unchanged | 34.335 | 12.538 | 0.3652 |
| stage4-main-final | check | 128/32 | cold | 23.643 | 20.640 | 0.8730 |
| stage4-main-final | check | 128/32 | private | 21.173 | 19.813 | 0.9358 |
| stage4-main-final | check | 128/32 | restore | 21.001 | 19.214 | 0.9149 |
| stage4-main-final | check | 128/32 | unchanged | 20.482 | 3.518 | 0.1717 |
| stage4-main-final | check | 128/8192 | cold | 23.205 | 20.792 | 0.8960 |
| stage4-main-final | check | 128/8192 | private | 19.603 | 16.938 | 0.8640 |
| stage4-main-final | check | 128/8192 | restore | 6.561 | 3.770 | 0.5747 |
| stage4-main-final | check | 128/8192 | unchanged | 6.432 | 3.313 | 0.5151 |
| stage4-main-final | check | 512/32 | cold | 112.219 | 104.017 | 0.9269 |
| stage4-main-final | check | 512/32 | private | 107.107 | 94.651 | 0.8837 |
| stage4-main-final | check | 512/32 | restore | 106.816 | 94.379 | 0.8836 |
| stage4-main-final | check | 512/32 | unchanged | 106.912 | 13.447 | 0.1258 |
| stage4-isolation | build | 32/32 | cold | 22.005 | 21.355 | 0.9704 |
| stage4-isolation | build | 32/32 | private | 9.215 | 8.153 | 0.8848 |
| stage4-isolation | build | 32/32 | restore | 7.994 | 7.432 | 0.9296 |
| stage4-isolation | build | 32/32 | unchanged | 5.094 | 3.700 | 0.7263 |
| stage4-isolation | check | 32/32 | cold | 5.427 | 4.891 | 0.9012 |
| stage4-isolation | check | 32/32 | private | 4.163 | 3.620 | 0.8696 |
| stage4-isolation | check | 32/32 | restore | 4.218 | 3.702 | 0.8776 |
| stage4-isolation | check | 32/32 | unchanged | 1.367 | 0.850 | 0.6212 |
| stage4-isolation | build | 128/32 | cold | 70.566 | 63.017 | 0.8930 |
| stage4-isolation | build | 128/32 | private | 38.041 | 33.031 | 0.8683 |
| stage4-isolation | build | 128/32 | restore | 33.650 | 29.192 | 0.8675 |
| stage4-isolation | build | 128/32 | unchanged | 18.152 | 13.040 | 0.7184 |
| stage4-isolation | check | 128/32 | cold | 21.985 | 19.949 | 0.9074 |
| stage4-isolation | check | 128/32 | private | 19.439 | 17.340 | 0.8920 |
| stage4-isolation | check | 128/32 | restore | 19.701 | 17.604 | 0.8936 |
| stage4-isolation | check | 128/32 | unchanged | 5.299 | 3.244 | 0.6121 |
| stage4-isolation | check | 128/8192 | cold | 21.609 | 19.972 | 0.9243 |
| stage4-isolation | check | 128/8192 | private | 18.330 | 16.367 | 0.8929 |
| stage4-isolation | check | 128/8192 | restore | 5.711 | 3.677 | 0.6438 |
| stage4-isolation | check | 128/8192 | unchanged | 5.172 | 3.150 | 0.6091 |
| stage4-isolation | check | 512/32 | cold | 113.811 | 103.751 | 0.9116 |
| stage4-isolation | check | 512/32 | private | 105.893 | 97.465 | 0.9204 |
| stage4-isolation | check | 512/32 | restore | 106.136 | 96.841 | 0.9124 |
| stage4-isolation | check | 512/32 | unchanged | 23.142 | 13.895 | 0.6004 |
