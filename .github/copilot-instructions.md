# GitHub Copilot Instructions

## Commit Workflow

When the user asks to commit changes ("提交版本", "commit", "帮我提交代码"), follow the smart commit workflow defined in `.agent/commit.md`:

1. **Detect** change scope (frontend/backend/both)
2. **Validate** with appropriate commands:
   - Frontend: `pnpm --dir web lint && pnpm --dir web test:unit && pnpm --dir web build`
   - Backend: `cargo check --manifest-path backend/Cargo.toml && OPSBOX_NO_PROXY=1 cargo test --manifest-path backend/Cargo.toml`
3. **Generate** conventional commit message in English
4. **Commit** after user approval
5. **Ask** for push confirmation

Always use English for commit messages. Stop on validation failure.
