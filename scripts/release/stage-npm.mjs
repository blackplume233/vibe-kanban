import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../../npx-cli/', import.meta.url));
const [stage, tag, base] = process.argv.slice(2);
if (!stage || !tag || !base) {
  throw new Error('Usage: node scripts/release/stage-npm.mjs <staging-dir> <tag> <release-download-base>');
}
const bundle = readFileSync(join(source, 'bin/cli.js'), 'utf8');
if (!bundle.includes('__BINARY_TAG__') || !bundle.includes('__RELEASE_BASE_URL__')) {
  throw new Error('Expected a freshly built npm launcher with release placeholders');
}
mkdirSync(join(stage, 'bin'), { recursive: true });
for (const file of ['package.json', 'README.md']) {
  copyFileSync(join(source, file), join(stage, file));
}
writeFileSync(join(stage, 'bin/cli.js'), bundle
  .replaceAll('__BINARY_TAG__', () => tag)
  .replaceAll('__RELEASE_BASE_URL__', () => base));
