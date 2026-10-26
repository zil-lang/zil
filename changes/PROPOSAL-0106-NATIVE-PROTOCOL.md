# 106：补齐已有Native多模块执行合同的机器夹具

本片不改变接受性、入口ABI或清理顺序。现行ZIL-SCRIPT §15.7规定模块顶层仅声明，
Native程序从显式main开始，成功void/i32及错误联合有确定退出映射，错误前先清理。
旧native-protocol.py明确只覆盖标量CLI；PLAN-T04-003因此仍缺实际多模块协议证据。

新增四模块程序，比较zil run和build后的本机二进制，覆盖嵌套资源的正常/错误退出
顺序、导入模块有同名main但不启动、完整i64具名探针、void返回，以及禁止顶层
执行、缺main、重复main、错误main签名、managed、取消和缺工具。每次运行保留
输入、命令、stdout/stderr、阶段JSON、退出状态和文件摘要，失败不得计Pass。

只完成这一协议夹具，不据此关闭T04整体的模块增量、公共schema或完整语言义务。
