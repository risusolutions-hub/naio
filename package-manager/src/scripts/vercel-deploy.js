import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const envPath = path.join(root, '.env');
const vercelBin = path.join(root, 'node_modules', 'vercel', 'dist', 'index.js');
const dataDir = path.join(root, 'data');
const dataBackup = path.join(root, '..', '.niao-nms-data-backup');

const SKIP_KEYS = new Set(['PORT', 'HOST', 'DATA_DIR']);

function loadEnv(file) {
  if (!fs.existsSync(file)) return {};
  const out = {};
  for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq < 1) continue;
    const key = trimmed.slice(0, eq).trim();
    let val = trimmed.slice(eq + 1).trim();
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1);
    }
    out[key] = val;
  }
  return out;
}

function run(args) {
  const res = spawnSync(process.execPath, args, { stdio: 'inherit', cwd: root });
  if (res.status !== 0) process.exit(res.status ?? 1);
}

function hideDataDir() {
  if (fs.existsSync(dataDir) && !fs.existsSync(dataBackup)) {
    fs.renameSync(dataDir, dataBackup);
    console.log('[vercel-deploy] temporarily moved data/ out of upload');
  }
}

function restoreDataDir() {
  if (fs.existsSync(dataBackup) && !fs.existsSync(dataDir)) {
    fs.renameSync(dataBackup, dataDir);
    console.log('[vercel-deploy] restored data/');
  }
}

try {
  console.log('\n[vercel-deploy] sync public assets');
  run([path.join(root, 'src/scripts/sync-public-assets.js')]);

  hideDataDir();

  const envArgs = [];
  for (const [key, val] of Object.entries(loadEnv(envPath))) {
    if (SKIP_KEYS.has(key) || !val) continue;
    envArgs.push('-e', `${key}=${val}`);
  }

  console.log('\n[vercel-deploy] link project (new account)');
  run([vercelBin, 'link', '--yes', '--project', 'niao-nms']);

  console.log('\n[vercel-deploy] deploying to production');
  run([vercelBin, 'deploy', '--prod', '--yes', ...envArgs]);

  console.log('\n[vercel-deploy] assigning nms.taurus-tech.in');
  run([vercelBin, 'alias', 'set', 'niao-nms-indol.vercel.app', 'nms.taurus-tech.in']);
} finally {
  restoreDataDir();
}

console.log('\n[vercel-deploy] done → https://nms.taurus-tech.in');
