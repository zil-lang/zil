# 完整计量单元

全部固定单元原样列出，未筛除不利结果。比值=候选/基线，低于1为耗时降低；ms为区组均值中位数，非置信区间。RSS及p95原始统计见各summary.json；Session私有编辑汇合每序列5次新修改。

## main→最终

| 接口 | 模块/容量 | 模式 | 请求 | 基线ms | 候选ms | 候选/基线 | 候选/直接rustc |
|---|---|---|---|---:|---:|---:|---:|
| cli | 32/— | build | cold | 47.777 | 47.392 | 0.9919 | 0.5835 |
| cli | 32/— | build | private1 | 39.447 | 39.746 | 1.0076 | 0.5232 |
| cli | 32/— | build | private2 | 39.984 | 40.561 | 1.0144 | 0.5923 |
| cli | 32/— | build | restore | 36.356 | 36.898 | 1.0149 | 0.4917 |
| cli | 32/— | build | unchanged | 36.251 | 36.771 | 1.0143 | 0.6128 |
| cli | 32/— | check | cold | 14.610 | 12.677 | 0.8677 | 0.3925 |
| cli | 32/— | check | private1 | 12.934 | 12.473 | 0.9643 | 0.5012 |
| cli | 32/— | check | private2 | 13.643 | 11.708 | 0.8582 | 0.4526 |
| cli | 32/— | check | restore | 14.710 | 12.179 | 0.8279 | 0.4729 |
| cli | 32/— | check | unchanged | 13.122 | 12.302 | 0.9375 | 0.5463 |
| cli | 128/— | build | cold | 81.743 | 82.068 | 1.0040 | 0.5900 |
| cli | 128/— | build | private1 | 68.390 | 67.658 | 0.9893 | 0.5856 |
| cli | 128/— | build | private2 | 68.036 | 69.299 | 1.0186 | 0.7450 |
| cli | 128/— | build | restore | 65.816 | 64.321 | 0.9773 | 0.5672 |
| cli | 128/— | build | unchanged | 65.288 | 64.204 | 0.9834 | 0.7814 |
| cli | 128/— | check | cold | 32.961 | 33.524 | 1.0171 | 0.5273 |
| cli | 128/— | check | private1 | 34.532 | 33.885 | 0.9813 | 0.8036 |
| cli | 128/— | check | private2 | 34.145 | 32.684 | 0.9572 | 0.7978 |
| cli | 128/— | check | restore | 34.263 | 31.583 | 0.9218 | 0.7632 |
| cli | 128/— | check | unchanged | 33.388 | 33.163 | 0.9933 | 0.8940 |
| session | 32/32 | build | cold | 40.464 | 39.131 | 0.9671 | — |
| session | 32/32 | build | private | 31.939 | 30.818 | 0.9649 | — |
| session | 32/32 | build | restore | 29.199 | 30.062 | 1.0296 | — |
| session | 32/32 | build | unchanged | 30.159 | 25.926 | 0.8596 | — |
| session | 32/32 | check | cold | 5.436 | 5.277 | 0.9707 | — |
| session | 32/32 | check | private | 4.678 | 4.245 | 0.9074 | — |
| session | 32/32 | check | restore | 4.729 | 4.185 | 0.8848 | — |
| session | 32/32 | check | unchanged | 4.574 | 1.724 | 0.3770 | — |
| session | 128/32 | build | cold | 76.910 | 76.238 | 0.9913 | — |
| session | 128/32 | build | private | 59.823 | 58.935 | 0.9852 | — |
| session | 128/32 | build | restore | 56.621 | 57.134 | 1.0091 | — |
| session | 128/32 | build | unchanged | 57.034 | 43.331 | 0.7597 | — |
| session | 128/32 | check | cold | 22.990 | 22.212 | 0.9662 | — |
| session | 128/32 | check | private | 25.023 | 20.206 | 0.8075 | — |
| session | 128/32 | check | restore | 20.904 | 20.577 | 0.9843 | — |
| session | 128/32 | check | unchanged | 20.111 | 8.250 | 0.4102 | — |
| session | 128/8192 | check | cold | 21.788 | 22.122 | 1.0153 | — |
| session | 128/8192 | check | private | 18.956 | 18.240 | 0.9622 | — |
| session | 128/8192 | check | restore | 6.271 | 6.317 | 1.0074 | — |
| session | 128/8192 | check | unchanged | 5.873 | 5.795 | 0.9868 | — |
| session | 512/32 | check | cold | 111.960 | 112.162 | 1.0018 | — |
| session | 512/32 | check | private | 108.016 | 104.204 | 0.9647 | — |
| session | 512/32 | check | restore | 111.030 | 106.502 | 0.9592 | — |
| session | 512/32 | check | unchanged | 105.515 | 38.183 | 0.3619 | — |

## 首片→最终正确性修复

| 接口 | 模块/容量 | 模式 | 请求 | 基线ms | 候选ms | 候选/基线 | 候选/直接rustc |
|---|---|---|---|---:|---:|---:|---:|
| cli | 32/— | build | cold | 47.388 | 45.936 | 0.9694 | 0.5492 |
| cli | 32/— | build | private1 | 41.161 | 39.841 | 0.9679 | 0.5146 |
| cli | 32/— | build | private2 | 39.523 | 39.857 | 1.0084 | 0.5816 |
| cli | 32/— | build | restore | 37.516 | 36.986 | 0.9858 | 0.4973 |
| cli | 32/— | build | unchanged | 37.393 | 37.498 | 1.0028 | 0.6176 |
| cli | 32/— | check | cold | 12.996 | 12.317 | 0.9477 | 0.3822 |
| cli | 32/— | check | private1 | 12.575 | 12.163 | 0.9672 | 0.4556 |
| cli | 32/— | check | private2 | 13.210 | 12.122 | 0.9176 | 0.4498 |
| cli | 32/— | check | restore | 12.855 | 11.904 | 0.9260 | 0.4748 |
| cli | 32/— | check | unchanged | 12.003 | 12.341 | 1.0281 | 0.5400 |
| cli | 128/— | build | cold | 88.814 | 88.388 | 0.9952 | 0.6236 |
| cli | 128/— | build | private1 | 73.915 | 71.615 | 0.9689 | 0.5891 |
| cli | 128/— | build | private2 | 70.710 | 73.245 | 1.0359 | 0.7134 |
| cli | 128/— | build | restore | 75.776 | 69.901 | 0.9225 | 0.5502 |
| cli | 128/— | build | unchanged | 70.085 | 67.899 | 0.9688 | 0.7715 |
| cli | 128/— | check | cold | 33.281 | 30.799 | 0.9254 | 0.4663 |
| cli | 128/— | check | private1 | 31.943 | 32.625 | 1.0214 | 0.7347 |
| cli | 128/— | check | private2 | 31.721 | 31.904 | 1.0058 | 0.7559 |
| cli | 128/— | check | restore | 32.174 | 31.839 | 0.9896 | 0.7648 |
| cli | 128/— | check | unchanged | 31.725 | 31.647 | 0.9975 | 0.8467 |
| session | 32/32 | build | cold | 40.045 | 39.023 | 0.9745 | — |
| session | 32/32 | build | private | 32.084 | 30.674 | 0.9561 | — |
| session | 32/32 | build | restore | 29.328 | 29.319 | 0.9997 | — |
| session | 32/32 | build | unchanged | 33.335 | 26.152 | 0.7845 | — |
| session | 32/32 | check | cold | 5.459 | 5.495 | 1.0064 | — |
| session | 32/32 | check | private | 4.894 | 4.377 | 0.8943 | — |
| session | 32/32 | check | restore | 4.816 | 4.292 | 0.8911 | — |
| session | 32/32 | check | unchanged | 4.780 | 1.595 | 0.3336 | — |
| session | 128/32 | build | cold | 77.715 | 77.377 | 0.9957 | — |
| session | 128/32 | build | private | 59.531 | 58.244 | 0.9784 | — |
| session | 128/32 | build | restore | 56.271 | 54.986 | 0.9772 | — |
| session | 128/32 | build | unchanged | 57.512 | 42.247 | 0.7346 | — |
| session | 128/32 | check | cold | 22.157 | 21.800 | 0.9839 | — |
| session | 128/32 | check | private | 20.304 | 19.023 | 0.9369 | — |
| session | 128/32 | check | restore | 19.854 | 19.189 | 0.9665 | — |
| session | 128/32 | check | unchanged | 20.085 | 7.938 | 0.3952 | — |
| session | 128/8192 | check | cold | 23.890 | 23.374 | 0.9784 | — |
| session | 128/8192 | check | private | 20.427 | 19.573 | 0.9582 | — |
| session | 128/8192 | check | restore | 6.650 | 6.407 | 0.9635 | — |
| session | 128/8192 | check | unchanged | 6.159 | 6.024 | 0.9780 | — |
| session | 512/32 | check | cold | 115.547 | 111.009 | 0.9607 | — |
| session | 512/32 | check | private | 111.983 | 103.595 | 0.9251 | — |
| session | 512/32 | check | restore | 110.611 | 109.407 | 0.9891 | — |
| session | 512/32 | check | unchanged | 105.360 | 37.950 | 0.3602 | — |

## 首片→内存修复

| 接口 | 模块/容量 | 模式 | 请求 | 基线ms | 候选ms | 候选/基线 | 候选/直接rustc |
|---|---|---|---|---:|---:|---:|---:|
| cli | 32/— | build | cold | 50.035 | 48.288 | 0.9651 | 0.5492 |
| cli | 32/— | build | private1 | 41.670 | 40.057 | 0.9613 | 0.5075 |
| cli | 32/— | build | private2 | 41.992 | 40.090 | 0.9547 | 0.5667 |
| cli | 32/— | build | restore | 39.162 | 37.758 | 0.9642 | 0.4748 |
| cli | 32/— | build | unchanged | 39.799 | 37.853 | 0.9511 | 0.5960 |
| cli | 32/— | check | cold | 14.297 | 14.294 | 0.9998 | 0.4085 |
| cli | 32/— | check | private1 | 14.055 | 15.713 | 1.1180 | 0.5775 |
| cli | 32/— | check | private2 | 13.801 | 13.656 | 0.9894 | 0.5173 |
| cli | 32/— | check | restore | 13.640 | 13.881 | 1.0177 | 0.4954 |
| cli | 32/— | check | unchanged | 14.040 | 13.335 | 0.9498 | 0.5232 |
| cli | 128/— | build | cold | 86.760 | 87.030 | 1.0031 | 0.5840 |
| cli | 128/— | build | private1 | 72.063 | 73.909 | 1.0256 | 0.6002 |
| cli | 128/— | build | private2 | 73.336 | 73.531 | 1.0027 | 0.7637 |
| cli | 128/— | build | restore | 71.629 | 69.050 | 0.9640 | 0.5920 |
| cli | 128/— | build | unchanged | 70.123 | 68.890 | 0.9824 | 0.8460 |
| cli | 128/— | check | cold | 31.905 | 34.547 | 1.0828 | 0.5020 |
| cli | 128/— | check | private1 | 33.021 | 32.714 | 0.9907 | 0.7057 |
| cli | 128/— | check | private2 | 33.449 | 34.228 | 1.0233 | 0.7810 |
| cli | 128/— | check | restore | 34.220 | 33.456 | 0.9777 | 0.7765 |
| cli | 128/— | check | unchanged | 32.726 | 38.597 | 1.1794 | 1.0070 |
| session | 32/32 | build | cold | 39.395 | 39.559 | 1.0042 | — |
| session | 32/32 | build | private | 31.670 | 30.653 | 0.9679 | — |
| session | 32/32 | build | restore | 29.895 | 28.735 | 0.9612 | — |
| session | 32/32 | build | unchanged | 29.950 | 26.540 | 0.8862 | — |
| session | 32/32 | check | cold | 5.450 | 5.238 | 0.9611 | — |
| session | 32/32 | check | private | 4.668 | 4.029 | 0.8632 | — |
| session | 32/32 | check | restore | 4.661 | 4.162 | 0.8930 | — |
| session | 32/32 | check | unchanged | 4.530 | 1.590 | 0.3510 | — |
| session | 128/32 | build | cold | 79.520 | 74.792 | 0.9405 | — |
| session | 128/32 | build | private | 58.215 | 57.420 | 0.9863 | — |
| session | 128/32 | build | restore | 56.209 | 54.778 | 0.9745 | — |
| session | 128/32 | build | unchanged | 55.245 | 41.639 | 0.7537 | — |
| session | 128/32 | check | cold | 23.167 | 23.503 | 1.0145 | — |
| session | 128/32 | check | private | 21.244 | 20.607 | 0.9701 | — |
| session | 128/32 | check | restore | 21.597 | 19.679 | 0.9112 | — |
| session | 128/32 | check | unchanged | 21.205 | 8.285 | 0.3907 | — |
| session | 128/8192 | check | cold | 22.238 | 22.055 | 0.9918 | — |
| session | 128/8192 | check | private | 19.453 | 18.691 | 0.9608 | — |
| session | 128/8192 | check | restore | 6.471 | 6.393 | 0.9879 | — |
| session | 128/8192 | check | unchanged | 6.119 | 5.764 | 0.9420 | — |
| session | 512/32 | check | cold | 114.803 | 112.786 | 0.9824 | — |
| session | 512/32 | check | private | 113.895 | 104.034 | 0.9134 | — |
| session | 512/32 | check | restore | 109.836 | 105.403 | 0.9596 | — |
| session | 512/32 | check | unchanged | 112.243 | 39.452 | 0.3515 | — |

## 首片→初始组合（保留回退）

| 接口 | 模块/容量 | 模式 | 请求 | 基线ms | 候选ms | 候选/基线 | 候选/直接rustc |
|---|---|---|---|---:|---:|---:|---:|
| cli | 32/— | build | cold | 49.940 | 48.169 | 0.9645 | 0.5816 |
| cli | 32/— | build | private1 | 40.077 | 40.102 | 1.0006 | 0.5222 |
| cli | 32/— | build | private2 | 40.052 | 39.995 | 0.9986 | 0.5943 |
| cli | 32/— | build | restore | 37.467 | 38.231 | 1.0204 | 0.5104 |
| cli | 32/— | build | unchanged | 38.714 | 37.578 | 0.9707 | 0.6266 |
| cli | 32/— | check | cold | 13.282 | 12.168 | 0.9161 | 0.3877 |
| cli | 32/— | check | private1 | 12.550 | 12.514 | 0.9971 | 0.4947 |
| cli | 32/— | check | private2 | 13.149 | 11.833 | 0.8999 | 0.4801 |
| cli | 32/— | check | restore | 12.603 | 12.659 | 1.0045 | 0.5153 |
| cli | 32/— | check | unchanged | 12.630 | 12.806 | 1.0139 | 0.5797 |
| cli | 128/— | build | cold | 88.699 | 84.879 | 0.9569 | 0.5910 |
| cli | 128/— | build | private1 | 74.424 | 67.911 | 0.9125 | 0.5816 |
| cli | 128/— | build | private2 | 71.091 | 68.834 | 0.9682 | 0.7501 |
| cli | 128/— | build | restore | 67.945 | 64.831 | 0.9542 | 0.5650 |
| cli | 128/— | build | unchanged | 67.974 | 66.482 | 0.9781 | 0.7838 |
| cli | 128/— | check | cold | 34.253 | 31.737 | 0.9265 | 0.4853 |
| cli | 128/— | check | private1 | 33.145 | 31.475 | 0.9496 | 0.7168 |
| cli | 128/— | check | private2 | 33.739 | 31.605 | 0.9368 | 0.7295 |
| cli | 128/— | check | restore | 34.937 | 32.989 | 0.9443 | 0.7719 |
| cli | 128/— | check | unchanged | 33.862 | 31.126 | 0.9192 | 0.7868 |
| session | 32/32 | build | cold | 42.938 | 39.018 | 0.9087 | — |
| session | 32/32 | build | private | 31.293 | 31.540 | 1.0079 | — |
| session | 32/32 | build | restore | 31.214 | 29.637 | 0.9495 | — |
| session | 32/32 | build | unchanged | 31.613 | 26.238 | 0.8300 | — |
| session | 32/32 | check | cold | 5.629 | 5.567 | 0.9890 | — |
| session | 32/32 | check | private | 4.603 | 4.342 | 0.9434 | — |
| session | 32/32 | check | restore | 4.702 | 4.137 | 0.8798 | — |
| session | 32/32 | check | unchanged | 4.719 | 1.714 | 0.3631 | — |
| session | 128/32 | build | cold | 76.744 | 78.108 | 1.0178 | — |
| session | 128/32 | build | private | 59.008 | 59.678 | 1.0114 | — |
| session | 128/32 | build | restore | 55.822 | 54.641 | 0.9788 | — |
| session | 128/32 | build | unchanged | 56.067 | 43.224 | 0.7709 | — |
| session | 128/32 | check | cold | 23.334 | 23.045 | 0.9876 | — |
| session | 128/32 | check | private | 21.240 | 21.569 | 1.0155 | — |
| session | 128/32 | check | restore | 21.048 | 21.163 | 1.0054 | — |
| session | 128/32 | check | unchanged | 21.454 | 8.424 | 0.3927 | — |
| session | 128/8192 | check | cold | 21.843 | 23.957 | 1.0968 | — |
| session | 128/8192 | check | private | 19.167 | 18.872 | 0.9846 | — |
| session | 128/8192 | check | restore | 6.317 | 6.359 | 1.0066 | — |
| session | 128/8192 | check | unchanged | 6.230 | 5.930 | 0.9519 | — |
| session | 512/32 | check | cold | 116.264 | 123.392 | 1.0613 | — |
| session | 512/32 | check | private | 111.364 | 116.797 | 1.0488 | — |
| session | 512/32 | check | restore | 116.748 | 107.885 | 0.9241 | — |
| session | 512/32 | check | unchanged | 111.461 | 42.035 | 0.3771 | — |

## 真实语料冷CLI（首片→最终）

16组clang文本路径O0/O2参考结果一致；每API单元10次baseline与10次candidate。旧byte_io O2的10个结果错误，保留耗时但不算等价速度比。

| 源码 | 优化 | 基线ms | 候选ms | 候选/基线 | 旧错误次数 |
|---|---|---:|---:|---:|---:|
| byte_io/main.zil | O0 | 39.225 | 39.201 | 0.9994 | 0 |
| byte_io/main.zil | O2 | 86.249 | 84.347 | 无效：旧产物错误 | 10 |
| resource_closures/main.zil | O0 | 44.022 | 43.147 | 0.9801 | 0 |
| resource_closures/main.zil | O2 | 53.508 | 52.455 | 0.9803 | 0 |
| generic_store178/main.zil | O0 | 47.052 | 47.738 | 1.0146 | 0 |
| generic_store178/main.zil | O2 | 73.021 | 72.161 | 0.9882 | 0 |
| mixed_capture182/main.zil | O0 | 35.524 | 35.967 | 1.0125 | 0 |
| mixed_capture182/main.zil | O2 | 44.924 | 46.128 | 1.0268 | 0 |
| ct_selection219/main.zil | O0 | 39.134 | 39.529 | 1.0101 | 0 |
| ct_selection219/main.zil | O2 | 47.184 | 47.049 | 0.9971 | 0 |
| ct_types228/arrays_defaults.zil | O0 | 34.951 | 34.023 | 0.9735 | 0 |
| ct_types228/arrays_defaults.zil | O2 | 39.977 | 39.477 | 0.9875 | 0 |
| where_copy229/main.zil | O0 | 33.516 | 33.023 | 0.9853 | 0 |
| where_copy229/main.zil | O2 | 36.348 | 38.218 | 1.0514 | 0 |
| shared_product224/main.zil | O0 | 37.552 | 38.669 | 1.0297 | 0 |
| shared_product224/main.zil | O2 | 40.711 | 39.162 | 0.9619 | 0 |
