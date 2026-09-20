# 贡献指南

感谢你改进 AirSlate PC Server。项目优先保持变更范围清晰、验证可重复，并让 Issue、PR 与发布记录之间能够追踪。

## Issue 与 Discussions

- 可复现的缺陷使用 **Bug 报告** Issue 表单。
- 明确的产品能力建议使用 **功能建议** Issue 表单。
- 安装、使用、兼容性咨询，以及尚未形成明确需求的想法，请优先使用 Discussions。
- 提交前先搜索已有 Issue / Discussions，避免重复跟踪同一问题。

## 本地验证

前端测试使用 Node.js 22 或更高版本；测试会直接加载 TypeScript 源文件。

前端：

```bash
npm ci --prefix frontend
npm --prefix frontend test
npm --prefix frontend run build
```

Rust：

```bash
cargo fmt --all -- --check
cargo test --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

项目同时面向 Windows 与 macOS。平台相关改动应尽量在对应平台完成真实验证；PR CI 会补充仓库级自动检查。

## 分支与提交

建议使用短生命周期分支：

- `feat/<topic>`：新增能力
- `fix/<topic>`：缺陷修复
- `refactor/<topic>`：不改变外部行为的重构
- `docs/<topic>`：文档
- `chore/<topic>`：工程、依赖或维护工作

提交与 PR 标题建议使用 Conventional Commits 风格，例如：

```text
feat(usb): support device discovery
fix(input): keep stylus up event balanced
docs: clarify Windows Ink requirement
chore: update GitHub Actions
```

## Pull Request

一个 PR 尽量只解决一个问题。请：

- 关联对应 Issue；小型维护变更可直接说明目的。
- 为行为变更补充或更新测试。
- 对用户可见变化更新 `CHANGELOG.md`。
- UI 变化附截图或录屏。
- 不把无关格式化、重构或依赖升级混入功能 PR。

所有进入 `main` 的常规代码变更都应通过 PR，并通过仓库 CI。审查意见处理完成后再合并。

仓库推荐使用 **Squash and merge**：PR 标题作为主线历史中的单个变更说明，PR 内可保留开发过程中的细粒度提交。
