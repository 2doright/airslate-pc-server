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

在 **Settings → Rules → Rulesets** 新建面向 `main` 的 branch ruleset：

- Require a pull request before merging
- Required approvals：**0**
- Require conversation resolution before merging
- Require status checks to pass
- Block force pushes
- Restrict deletions

待 PR #26 的 CI 名称稳定后，将以下检查设为 required：

- `Frontend`
- `Rustfmt`
- `Rust / Windows`
- `Rust / macOS`

个人维护阶段不强制 1 个 approval，否则维护者自己的正常 PR 会被无意义阻塞。未来出现稳定协作者后，再把关键路径提高到 1 个 approval，并使用 CODEOWNERS 请求对应审查。

不建议当前开启 merge queue，也不建议要求每次合并前都强制更新到最新 `main`；对当前提交频率，这两项带来的重复 CI 成本大于收益。

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

现有 Release、Star History 等写入型 workflow 已采用显式 `permissions`，应继续保持这种方式。

## 4. Security

在 **Settings → Security / Code security and analysis** 按可用能力开启：

- Dependency graph
- Dependabot alerts
- Dependabot security updates
- Secret scanning
- Push protection
- Private vulnerability reporting

Dependency graph 开启后，再增加 `actions/dependency-review-action` 作为 PR 检查；未开启前不要把它设为 required check，否则所有 PR 都会失败。

## 5. 标签初始化

建议创建以下自定义标签；类型标签继续复用 GitHub 默认的 `bug`、`enhancement`、`documentation`、`question`、`duplicate`：

| 标签 | 建议颜色 | 用途 |
| --- | --- | --- |
| `status: needs-triage` | `FBCA04` | 尚未完成维护者分类 |
| `status: needs-info` | `D4C5F9` | 需要提交者补充信息 |
| `status: blocked` | `B60205` | 被外部条件阻塞 |
| `area: windows` | `0075CA` | Windows 平台 |
| `area: macos` | `5319E7` | macOS 平台 |
| `area: usb` | `0E8A16` | USB 有线连接 |
| `area: network` | `1D76DB` | 无线连接与网络 |
| `area: input` | `0052CC` | 笔输入、压感、手势 |
| `priority: high` | `B60205` | 优先处理 |
| `priority: normal` | `FBCA04` | 常规计划 |
| `priority: low` | `C2E0C6` | 影响有限或暂缓 |
| `dependencies` | `0366D6` | 依赖升级 |
| `skip-changelog` | `EDEDED` | 不进入自动发布说明 |

不要为每个模块预先创建标签。只有当某一类 Issue 已经多到需要稳定筛选时，再拆分新的 `area:` 标签。
