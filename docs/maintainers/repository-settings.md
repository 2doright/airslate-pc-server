# Repository Settings

本文件记录代码无法表达、但应与仓库维护规范保持一致的 GitHub 设置。当前项目以个人/小团队维护为主，规则目标是防止误操作和明显回归，不引入不必要的审批摩擦。

## 1. Pull Request 合并策略

在 **Settings → General → Pull Requests**：

- 只保留 **Allow squash merging**
- Squash merge commit title 使用 **Pull request title**
- 开启 **Automatically delete head branches**
- 开启 **Always suggest updating pull request branches**
- Auto-merge 可按需开启；不作为必需流程

主分支只保留一个提交代表一个 PR，便于回滚、生成发布说明和阅读历史。

## 2. main Ruleset

面向 `main` 的 branch ruleset 保持：

- Require a pull request before merging
- Required approvals：**0**
- Require conversation resolution before merging
- Require status checks to pass
- Block force pushes
- Restrict deletions

Required checks：

- `Frontend`
- `Rustfmt`
- `Rust / Windows`
- `Rust / macOS`
- `Dependency Review`

个人维护阶段不强制 1 个 approval，否则维护者自己的正常 PR 会被无意义阻塞。未来出现稳定协作者后，再把关键路径提高到 1 个 approval，并使用 CODEOWNERS 请求对应审查。

不建议当前开启 merge queue，也不要求每次合并前都强制更新到最新 `main`；对当前提交频率，这两项带来的重复 CI 成本大于收益。

### Clippy 收紧路径

当前 Windows/macOS CI 都会运行 Clippy，但暂不使用 `-D warnings`。现有代码存在少量平台相关 dead-code 与新版 Clippy 风格告警。先用独立维护 PR 清理这些告警；清零后再将命令升级为：

```bash
cargo clippy --all-targets --all-features --locked -- -D warnings
```

不要通过批量 `#[allow]` 或降低 lint 级别制造“零告警”。

## 3. Actions 权限

在 **Settings → Actions → General**：

- Workflow permissions 默认保持 **Read repository contents and packages**
- 不开启 “Allow GitHub Actions to create and approve pull requests”，除非后续有明确自动化需求
- 需要写权限的 workflow 在文件内显式声明最小权限

Label、Release、Star History 等写入型 workflow 都应在各自文件中只申请所需权限。

## 4. Security

在 **Settings → Security / Code security and analysis** 按可用能力开启：

- Dependency graph
- Dependabot alerts
- Dependabot security updates
- Secret scanning
- Push protection
- Private vulnerability reporting

Dependency graph 已启用，因此 PR CI 中保留 `actions/dependency-review-action`。当前策略只阻断 PR 新引入的 **high / critical** 已知漏洞。

## 5. 标签管理

`.github/labels.yml` 是仓库标签的声明式来源。标签同步 workflow 在该文件进入 `main` 后创建或更新标签，并迁移旧命名。

当前标签分为三类：

- 工作性质：`bug`、`enhancement`、`documentation`、`question`、`dependencies`、`ci`
- 产品与技术范围：`frontend`、`rust`、`usb`、`network`、`input`、`windows`、`macos`
- 维护动作：`needs-info`、`blocked`、`duplicate`、`invalid`、`wontfix`、`good first issue`、`help wanted`、`skip-changelog`

旧标签迁移：

- `area: windows` → `windows`
- `area: macos` → `macos`
- `area: usb` → `usb`
- `area: network` → `network`
- `area: input` → `input`
- `status: needs-info` → `needs-info`
- `status: blocked` → `blocked`

`status: needs-triage` 与 `priority: high/normal/low` 不再使用并由迁移 workflow 删除。优先级和排期放在 GitHub Projects / Milestones，不通过标签长期维护。
