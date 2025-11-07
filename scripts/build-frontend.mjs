#!/usr/bin/env node
// 跨平台前端构建脚本，会在 web 下执行 `pnpm build`，产物输出到 backend/opsbox-server/static。
// Windows 下无需 bash，直接：`node scripts/build-frontend.mjs`

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const webDir = resolve(__dirname, '..', 'web');

const pnpmCmd = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const args = ['--dir', webDir, 'build'];

const child = spawn(pnpmCmd, args, { stdio: 'inherit' });
child.on('exit', (code) => process.exit(code ?? 1));
