# Project Instructions

## Git Workflow

- 开始修改前检查 `git status`。
- 不覆盖或删除用户已有未提交修改。
- Small Change 完成后检查 `git diff`。
- 提交前检查 `git diff --stat` 和相关 diff，确认没有 scope expansion。
- 不使用 `git add .` 自动提交未知内容，除非明确检查过 status。
- 不执行 `git reset --hard`、`git clean -fd` 等破坏性命令，除非用户明确授权。
- 不修改历史 commit。
- 不 force push。
- 不自动 push remote。
- 用户没有明确要求时，不自动创建 commit。
- 对小修改，Git diff 应用于确认是否发生不必要的跨模块修改。
