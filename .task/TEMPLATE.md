# 工单格式说明

本发布固定 B0、T01–T09、W1–W4 共14个ID，不使用本模板擅自扩单。需要调整分工时由集成者修改既有单的明确目录与交接产物，保留规范定义权。

每单 JSON 必须含 id、milestone、title、status、task_revision、spec_hash、normative_hash、base_revision、blocked_by、requirements、acceptance、profile、write_set、required_artifacts、provided_artifacts、plan_case_ids、case_ids、test_plan_manifest、test_commands、conformance_command、done_when、evidence。

状态字段从 Planned 开始；Ready 需要具体基线、当前输入摘要和已完成前置；Done 需要实际证据与验收裁决。测试计划指向既有规范，不新增安全语义。`case_ids` 为现有规范源样本，`plan_case_ids` 为本单待实现的测试夹具要求；二者都不能在未运行时记 Pass。

正文依次写输入与前置、实现与交付、接口交接、验收命令与样本、完成条件、本单边界、请验记录。工单中的命令以 B0 建立的统一 runner 为契约，缺 runner 或 fixture 不算通过。
