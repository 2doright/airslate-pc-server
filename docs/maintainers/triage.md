# Issue Triage

目标是让 Issue 和 Pull Request 用少量、稳定的标签回答两个问题：**这是什么**、**影响哪里**。标签用于检索、自动化和维护动作，不承担排期系统的职责。

## 标签体系

### 工作性质

- `bug`：可复现的错误行为
- `enhancement`：新增或改进能力
- `documentation`：文档相关变更
- `question`：使用或支持问题；通常应引导到 Discussions
- `dependencies`：依赖升级
- `ci`：CI、自动化和仓库维护

### 产品与技术范围

- `frontend`：前端界面和 Web 代码
- `backend`：后端应用代码、传输、输入处理和系统集成
- `usb`：USB 有线连接与 accessory mode
- `network`：局域网发现、UDP 与网络传输
- `input`：笔输入、压感、手势与快捷键
- `windows`：Windows 特有行为
- `macos`：macOS 特有行为
- `linux`：Linux 特有行为

一个 Issue 或 PR 可以同时拥有多个范围标签，例如 Linux 后端 USB 修复可以同时标记 `backend`、`linux` 和 `usb`。

### 维护动作

- `needs-info`：缺少继续判断所需的信息或复现
- `blocked`：被外部依赖、平台限制或其他工作阻塞
- `duplicate`：已有相同跟踪项
- `invalid`：不是有效或可执行的问题
- `wontfix`：已明确不计划处理
- `good first issue`：适合首次贡献
- `help wanted`：明确欢迎外部贡献
- `skip-changelog`：PR 不应进入自动生成的发布说明

## 自动化

- Bug / Feature Issue Form 分别自动添加 `bug` / `enhancement`。
- 新 Issue 根据表单中的系统、连接方式和影响范围自动补充 `windows`、`macos`、`linux`、`usb`、`network`、`input`、`frontend` 或 `backend`。
- Pull Request 根据 changed files 自动添加范围标签。
- Dependabot PR 使用 `dependencies` 加对应的 `backend`、`frontend` 或 `ci`。
- 标签定义由 `.github/labels.yml` 管理，不在 GitHub UI 中维护第二套命名规则。

## Triage 流程

1. 保留一个最主要的工作性质标签，并添加确实有检索价值的范围标签。
2. 缺少复现或关键信息时加 `needs-info`；有明确外部阻塞时加 `blocked`。
3. 重复项标记 `duplicate` 并链接原 Issue；明确不处理时使用 `wontfix` 或关闭为 not planned。
4. 只有明确适合外部贡献时才使用 `good first issue` / `help wanted`。

不使用 `area:*`、`status:*`、`priority:*` 命名空间。优先级和排期需要时放在 GitHub Projects / Milestones，而不是长期维护一组容易过期的标签。
