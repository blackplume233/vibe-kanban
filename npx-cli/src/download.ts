import http from 'http';
import https from 'https';
import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import os from 'os';

// Substituted in the staged release package by scripts/release/stage-npm.mjs.
export const BINARY_TAG = '__BINARY_TAG__'; // e.g., v0.1.46.1778000000

// Binaries are GitHub Release assets of the BINARY_TAG release, stored flat:
//   <base>/<tag>/manifest.json
//   <base>/<tag>/<platform>-<binary>.zip
// VIBE_KANBAN_DOWNLOAD_BASE_URL points at a mirror with the same layout
// (a corporate proxy, or a local server when testing a release).
const DEFAULT_DOWNLOAD_BASE_URL = '__RELEASE_BASE_URL__';
export const DOWNLOAD_BASE_URL = (
  process.env.VIBE_KANBAN_DOWNLOAD_BASE_URL || DEFAULT_DOWNLOAD_BASE_URL
).replace(/\/+$/, '');
const NPM_LATEST_URL = 'https://registry.npmjs.org/vibe-kanban/latest';
const MAX_REDIRECTS = 5;
export const CACHE_DIR = path.join(os.homedir(), '.vibe-kanban', 'bin');

// Local development mode: use binaries from npx-cli/dist/ instead of downloading
// Only activate if dist/ exists (i.e., running from source after local-build.sh)
export const LOCAL_DIST_DIR = path.join(__dirname, '..', 'dist');
export const LOCAL_DEV_MODE =
  fs.existsSync(LOCAL_DIST_DIR) || process.env.VIBE_KANBAN_LOCAL === '1';

export interface BinaryInfo {
  sha256: string;
  size: number;
}

export interface BinaryManifest {
  platforms: Record<string, Record<string, BinaryInfo>>;
}

type ProgressCallback = (downloaded: number, total: number) => void;

export class HttpError extends Error {
  constructor(
    readonly statusCode: number | undefined,
    readonly url: string,
    action: string
  ) {
    super(`HTTP ${statusCode} ${action} ${url}`);
  }
}

export function assetUrl(name: string): string {
  return `${DOWNLOAD_BASE_URL}/${BINARY_TAG}/${name}`;
}

export function binaryAssetName(platform: string, binaryName: string): string {
  return `${platform}-${binaryName}.zip`;
}

function httpGet(url: string, onResponse: (res: http.IncomingMessage) => void) {
  const client = url.startsWith('http:') ? http : https;
  return client.get(
    url,
    { headers: { 'User-Agent': 'vibe-kanban-cli' } },
    onResponse
  );
}

// GitHub release downloads answer with a 302 to a signed CDN URL.
function redirectTarget(res: http.IncomingMessage, url: string): string | null {
  const status = res.statusCode ?? 0;
  if (status < 300 || status >= 400 || !res.headers.location) return null;
  res.resume();
  return new URL(res.headers.location, url).toString();
}

function fetchJson<T>(url: string, redirects = 0): Promise<T> {
  return new Promise((resolve, reject) => {
    httpGet(url, (res) => {
      const next = redirectTarget(res, url);
      if (next) {
        if (redirects >= MAX_REDIRECTS) {
          return reject(new Error(`Too many redirects fetching ${url}`));
        }
        return fetchJson<T>(next, redirects + 1)
          .then(resolve)
          .catch(reject);
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new HttpError(res.statusCode, url, 'fetching'));
      }
      let data = '';
      res.on('data', (chunk: string) => (data += chunk));
      res.on('end', () => {
        try {
          resolve(JSON.parse(data) as T);
        } catch {
          reject(new Error(`Failed to parse JSON from ${url}`));
        }
      });
    }).on('error', reject);
  });
}

function downloadFile(
  url: string,
  destPath: string,
  expectedSha256: string | undefined,
  onProgress?: ProgressCallback,
  redirects = 0
): Promise<string> {
  const tempPath = destPath + '.tmp';
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(tempPath);
    const hash = crypto.createHash('sha256');

    const cleanup = () => {
      try {
        fs.unlinkSync(tempPath);
      } catch {}
    };

    httpGet(url, (res) => {
      const next = redirectTarget(res, url);
      if (next) {
        file.close();
        cleanup();
        if (redirects >= MAX_REDIRECTS) {
          return reject(new Error(`Too many redirects downloading ${url}`));
        }
        return downloadFile(
          next,
          destPath,
          expectedSha256,
          onProgress,
          redirects + 1
        )
          .then(resolve)
          .catch(reject);
      }

      if (res.statusCode !== 200) {
        res.resume();
        file.close();
        cleanup();
        return reject(new HttpError(res.statusCode, url, 'downloading'));
      }

      const totalSize = parseInt(res.headers['content-length'] || '0', 10);
      let downloadedSize = 0;

      res.on('data', (chunk: Buffer) => {
        downloadedSize += chunk.length;
        hash.update(chunk);
        if (onProgress) onProgress(downloadedSize, totalSize);
      });
      res.pipe(file);

      file.on('finish', () => {
        file.close();
        const actualSha256 = hash.digest('hex');
        if (expectedSha256 && actualSha256 !== expectedSha256) {
          cleanup();
          reject(
            new Error(
              `Checksum mismatch: expected ${expectedSha256}, got ${actualSha256}`
            )
          );
        } else {
          try {
            fs.renameSync(tempPath, destPath);
            resolve(destPath);
          } catch (err) {
            cleanup();
            reject(err);
          }
        }
      });
    }).on('error', (err) => {
      file.close();
      cleanup();
      reject(err);
    });
  });
}

export async function ensureBinary(
  platform: string,
  binaryName: string,
  onProgress?: ProgressCallback
): Promise<string> {
  // In local dev mode, use binaries directly from npx-cli/dist/
  if (LOCAL_DEV_MODE) {
    const localZipPath = path.join(
      LOCAL_DIST_DIR,
      platform,
      `${binaryName}.zip`
    );
    if (fs.existsSync(localZipPath)) {
      return localZipPath;
    }
    throw new Error(
      `Local binary not found: ${localZipPath}\n` +
        `Run pnpm run build:npx first to build the binaries.`
    );
  }

  const cacheDir = path.join(CACHE_DIR, BINARY_TAG, platform);
  const zipPath = path.join(cacheDir, `${binaryName}.zip`);

  if (fs.existsSync(zipPath)) return zipPath;

  fs.mkdirSync(cacheDir, { recursive: true });

  const manifest = await fetchJson<BinaryManifest>(assetUrl('manifest.json'));
  const binaryInfo = manifest.platforms?.[platform]?.[binaryName];

  if (!binaryInfo) {
    throw new Error(`Binary ${binaryName} not available for ${platform}`);
  }

  const url = assetUrl(binaryAssetName(platform, binaryName));
  await downloadFile(url, zipPath, binaryInfo.sha256, onProgress);

  return zipPath;
}

// The version the npm `latest` dist-tag points at.
export async function getLatestVersion(): Promise<string | undefined> {
  const pkg = await fetchJson<{ version?: string }>(NPM_LATEST_URL);
  return pkg.version;
}
