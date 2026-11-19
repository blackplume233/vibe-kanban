import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const app = join(root, 'crates/nsh-app');
const nightly = 'nightly-2026-08-01';
const nshRevision = '12af3d778b588c2837267ff6aaed678eb5ad72a2';
const bindgenVersion = '0.2.129';
const targetDir = resolve(process.env.CARGO_TARGET_DIR || join(root, 'target'));
const env = { ...process.env, RUSTUP_TOOLCHAIN: nightly, CARGO_TARGET_DIR: targetDir };
const tools = join(targetDir, 'nsh-tools');
const exe = process.platform === 'win32' ? '.exe' : '';
const tool = (name) => join(tools, 'bin', name + exe);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: 'inherit', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function packageApp(target, out) {
  const cli = process.env.NSH_BIN || tool('nsh');
  if (!isAbsolute(cli) || !existsSync(cli)) {
    throw new Error('Run pnpm run nsh:install or set NSH_BIN to the absolute path of the locked NSH CLI.');
  }
  if (target === 'web') {
    env.NSH_WASM_BINDGEN ||= existsSync(tool('wasm-bindgen')) ? tool('wasm-bindgen') : 'wasm-bindgen';
  }
  run('node', ['design/generate-tokens.mjs', '--check'], { cwd: app });
  const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  run(cli, ['package', '--target', target, '-o', out, '--app-version', version], { cwd: app });
}

function buildNpm() {
  const platform = { win32: 'windows', darwin: 'macos', linux: 'linux' }[process.platform];
  if (!platform || !['x64', 'arm64'].includes(process.arch)) {
    throw new Error(`Unsupported npm build platform: ${process.platform}-${process.arch}`);
  }
  const platformDir = `${platform}-${process.arch}`;
  const out = join(root, 'npx-cli', 'dist', platformDir);
  const web = join(targetDir, 'package-web');
  const assets = join(root, 'scripts/release/package-assets.py');
  const python = process.env.PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
  packageApp('web', web);
  run('cargo', ['build', '--locked', '--release', '-p', 'server', '-p', 'mcp', '-p', 'review',
    '--bin', 'server', '--bin', 'vibe-kanban-mcp', '--bin', 'review']);
  run(python, [assets, 'platform', platformDir, join(targetDir, 'release'), join(web, 'vibe-own'), out, '--cargo-names', '--local']);
  if (platformDir === 'windows-x64') {
    const windows = join(targetDir, 'package-windows');
    packageApp('windows', windows);
    run(python, [assets, 'native', join(windows, 'vibe-own.zip'), out, '--local']);
  }
}

const [command, arg] = process.argv.slice(2);
switch (command) {
  case 'install':
    run('cargo', ['install', '--debug', '--locked', '--root', tools, '--git', 'https://github.com/longbridge/nsh', '--rev', nshRevision, 'nsh-cli']);
    break;
  case 'install-web':
    run('rustup', ['target', 'add', '--toolchain', nightly, 'wasm32-unknown-unknown']);
    run('cargo', ['install', '--debug', '--locked', '--root', tools, '--version', bindgenVersion, 'wasm-bindgen-cli']);
    break;
  case 'web':
  case 'windows':
    packageApp(command, arg ? resolve(arg) : join(targetDir, `package-${command}`));
    break;
  case 'npx':
    buildNpm();
    break;
  case 'dev': {
    const out = join(targetDir, 'package-web');
    packageApp('web', out);
    env.VK_WEB_ROOT = join(out, 'vibe-own');
    env.DISABLE_WORKTREE_CLEANUP ??= '1';
    const features = arg === '--qa' ? ['--features', 'qa-mode'] : [];
    run('cargo', ['run', '--locked', '-p', 'server', '--bin', 'server', ...features]);
    break;
  }
  case 'dev-windows': {
    const out = join(targetDir, 'package-windows');
    packageApp('windows', out);
    const appExe = join(out, 'vibe-own', 'vibe-own.exe');
    run(appExe, [], { cwd: dirname(appExe) });
    break;
  }
  default:
    throw new Error(`Usage: node ${basename(fileURLToPath(import.meta.url))} {install|install-web|web [out]|windows [out]|dev [--qa]|dev-windows|npx}`);
}
