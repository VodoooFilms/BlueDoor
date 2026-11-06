# macOS installer work — incomplete Vault translations

This branch preserves the local desktop Vault translation type definitions separately from the Android audio preview.

Base: codex/windows-x64-cpu (8b5e07c), which contains the desktop app and macOS release scripts.
Scope: only the 155 added lines declaring Copy.vault and VaultCopy; Android audio changes are excluded.

Status: WIP, not installer-ready. The Spanish and English COPY objects do not yet provide the required vault values. The current local workspace reports two TypeScript errors for those missing values. The Vite frontend build passes, but does not perform type checking. No native macOS build or installer validation was performed for this branch.

Complete the translations and wire them into VaultView, then run pnpm typecheck, pnpm build, relevant Vault tests, and macOS release validation before distributing an installer.
