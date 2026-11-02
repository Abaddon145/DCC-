import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export function verifyOutcome(kind, result) {
  if (result.status !== 0 || result.error || result.signal) throw new Error(`${kind} test process failed`);
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  const count = kind === 'rust'
    ? output.match(/test result:\s+ok\.\s+([1-9]\d*) passed;\s+0 failed;/)?.[1]
    : output.match(/Tests\s+([1-9]\d*) passed\s*\(/)?.[1];
  if (!count) throw new Error(`${kind} tests did not report a passing, nonempty suite`);
  return Number(count);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const parent = path.join(root, 'release-artifacts/yingliu-merge');
  fs.mkdirSync(parent, { recursive: true });
  const output = fs.mkdtempSync(path.join(parent, 'migration-qa-'));
  const checks = [
    { kind: 'rust', command: process.platform === 'win32' ? 'cargo.exe' : 'cargo', args: ['test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', 'learning_import', '--', '--test-threads=1'] },
    { kind: 'frontend', command: process.execPath, args: [path.join(root, 'node_modules/vitest/vitest.mjs'), 'run', '--configLoader', 'runner', 'src/features/learning/LearningImportPanel.test.tsx'] },
  ];
  const passed = {};
  try {
    for (const check of checks) {
      const result = spawnSync(check.command, check.args, { cwd: root, encoding: 'utf8', windowsHide: true, timeout: 600_000, maxBuffer: 16 * 1024 * 1024 });
      fs.writeFileSync(path.join(output, `${check.kind}.log`), `${result.stdout ?? ''}\n${result.stderr ?? ''}\nstatus=${result.status}; signal=${result.signal}; error=${result.error ?? ''}\n`);
      passed[check.kind] = verifyOutcome(check.kind, result);
    }
    console.log(JSON.stringify({ scope: 'synthetic Rust and React tests; not desktop or real Vault acceptance', output, passed }, null, 2));
  } catch (error) {
    console.error(`Migration QA failed; logs: ${output}\n${error}`);
    process.exitCode = 1;
  }
}
