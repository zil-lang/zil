# 231 模块改名后的保留trait身份修复

作者精确失败候选4ed6609f6中，`main`为`import model;fn keep<T>(x:T)->T where T:model.Send=>x;fn main()->i32=>0`，`model`为`pub trait Send{}`，错误进入Verify。模块生成的规范名绕过仅检查原字符串的保留trait排除；原失败日志和输入保留，不重标。

修复在既有generic_trait_bounds::Index::declaration及where名义bound入口统一排除：仅识别真正模块生成的`$module<digits>_`前缀后原名，用于检查既有内建保留名；不缩短真正名义身份、不接受别名或末段同名。普通inline使用相同已存在入口时也修复这个具名缺陷。新增扫描工作计入原预算；无模块改名旧路径费用保持。

授权231写集从仅开放Index入口扩大为这一个保留名排除修复。需覆盖Send/Sync/Drop/Fn族的inline/where、合法近名（例如User_Send）不误拒绝、真实模块及伪前缀边界、未调用定义和N/N−1/fresh。不扩大一般trait求解器、schema、权限或默认额度。
