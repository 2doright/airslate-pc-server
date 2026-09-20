# Issue Triage

目标是让每个可执行 Issue 快速回答三个问题：**是什么、影响哪里、现在该怎么处理**。标签应服务检索和决策，不追求数量。

## 标签体系

### 类型

优先复用 GitHub 默认标签：

- `bug`：可复现的错误行为
- `enhancement`：新增或改进能力
- `documentation`：仅文档相关
- `question`：不适合作为执行项的问题；通常迁移或引导到 Discussions
- `duplicate`：已有相同跟踪项

### 状态

建议新增：

- `status: needs-triage`：尚未完成维护者分类
- `status: needs-info`：缺少继续判断所需的信息
- `status: blocked`：被外部依赖、平台限制或其他 Issue 阻塞

### 范围

建议新增：

- `area: windows`
- `area: macos`
- `area: usb`
- `area: network`
- `area: input`

前端、发布等范围只有在 Issue 数量明显增加后再拆分，避免过早产生低使用率标签。

### 优先级

建议新增：

- `priority: high`：影响核心路径、数据/安全或大量用户，维护者计划优先处理
- `priority: normal`：已确认且应进入常规计划
- `priority: low`：有效，但影响有限或近期没有计划

优先级表示维护顺序，不表示严重程度或承诺发布日期。

### 维护与贡献

继续使用 `good first issue`、`help wanted`；依赖升级可使用 `dependencies`。自动生成发布说明时，不应出现的 PR 可标记 `skip-changelog`。

## Triage 流程

1. 判断内容应留在 Issue 还是转到 Discussions。
2. 确认能否复现或需求是否足够明确；缺信息时标记 `status: needs-info`。
3. 为可执行项保留一个主要类型标签，并添加最相关的 area 标签。
4. 只有在维护者已经判断处理顺序时再添加 priority。
5. 重复项保留原始跟踪 Issue，新提交标记 `duplicate` 并链接过去。

不要求每个 Issue 拥有所有维度的标签。标签缺失比错误标签更容易修正。
