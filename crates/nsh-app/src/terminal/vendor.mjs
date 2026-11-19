// Run after `pnpm install --frozen-lockfile` at the repository root.
import { readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);
const files = [
  ['@xterm/xterm', '5.5.0', 'lib/xterm.js', 'xterm.js'],
  ['@xterm/xterm', '5.5.0', 'css/xterm.css', 'xterm.css'],
  ['@xterm/xterm', '5.5.0', 'LICENSE', 'xterm.LICENSE'],
  ['@xterm/addon-fit', '0.10.0', 'lib/addon-fit.js', 'addon-fit.js'],
  ['@xterm/addon-fit', '0.10.0', 'LICENSE', 'addon-fit.LICENSE'],
];

for (const [name, version, source, destination] of files) {
  const root = dirname(dirname(require.resolve(name)));
  const metadata = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  if (metadata.name !== name || metadata.version !== version) {
    throw new Error(`Expected ${name}@${version}, found ${metadata.name}@${metadata.version}`);
  }
  const contents = await readFile(join(root, source), 'utf8');
  await writeFile(
    new URL(`./assets/${destination}`, import.meta.url),
    contents.replace(/^\/\/# sourceMappingURL=.*\r?\n?/m, '')
  );
}
