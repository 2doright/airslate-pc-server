# AirSlate PC Server 开发说明

## 核心约束

### 编程原则

- Zero-up refactor, breaking changes. NEVER write defensive, fallback, or patch code. Disregard backward compatibility and unreasonable legacy structures.
- Runtime truth, no fake compile passes. NEVER optimize for build-green status by adding patches, fake flags, placeholder transitions, or exception wrappers.
- Event-sourced facts, no simulation. Store state must describe facts, not hopes.
- Root-cause fixing, no symptom-patching

### 技术栈

- Rust + Windows crate
- 前端位于 `frontend/`
- 目标平台为 Windows

## Rust Skills（强制）

本项目通过 `.rust-skills` Git submodule 使用
[`actionbook/rust-skills`](https://github.com/actionbook/rust-skills)。所有 Rust 问题、设计、实现、
调试和代码审查任务都必须使用该 Skill 系统。

### Rust 开发

所有 Rust 开发任务必须：

1. 先完整阅读 `.rust-skills/AGENTS.md`
2. 使用 `.rust-skills/skills/rust-router/SKILL.md` 路由问题
3. 根据路由结果任务类型，继续完整阅读对应的 `.rust-skills/skills/*/SKILL.md`
4. 遵循相关 Skill 的实现、检查和测试要求


## 实现与验证

### 调试要求

- 修改代码后必须执行与风险相称的真实语法检查、测试和构建。
- 测试必须验证运行时事实，不得使用占位实现、伪造状态或仅为通过编译而添加的开关。
- 修复失败的根因，并检查相邻调用路径是否具有相同问题。

### 标准命令

```powershell
npm --prefix ./frontend run build
cargo build
```

## 版本与交付

- 调试通过后更新 `CHANGELOG.md`。
- 修改发布行为时同步维护版本号。
- 最终交付目标是完整的应用服务，以及可发布到 GitHub Release 的 Windows 安装包和 portable 程序。

## 项目文档

- Windows API 笔输相关文档介绍（不是 Rust crate 规范）：`doc/winapi/index.md`
- Rust for Windows API：<https://microsoft.github.io/windows-docs-rs/doc/windows/>
