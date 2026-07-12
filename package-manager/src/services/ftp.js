import dns from 'node:dns';
import fs from 'node:fs/promises';
import path from 'node:path';
import { Client } from 'basic-ftp';
import { config } from '../config.js';

// Prefer IPv4 — avoids timeout when IPv6 route is broken
dns.setDefaultResultOrder('ipv4first');

export function formatBytes(bytes) {
  const n = Number(bytes) || 0;
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(2)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatEta(seconds) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  if (m < 60) return `${m}m ${r}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

function progressBar(pct, width = 24) {
  const filled = Math.round((pct / 100) * width);
  return `${'█'.repeat(filled)}${'░'.repeat(width - filled)}`;
}

/** Live console progress for CLI release / sync-ftp. */
export function createConsoleFtpProgress({ label = 'FTP upload' } = {}) {
  const started = Date.now();
  let lastLineLen = 0;
  let prepStarted = false;
  let uploadStarted = false;

  function writeLine(line) {
    process.stdout.write(`\r${line}${' '.repeat(Math.max(0, lastLineLen - line.length))}`);
    lastLineLen = line.length;
  }

  function endLine() {
    if (prepStarted || uploadStarted) {
      process.stdout.write('\n');
      lastLineLen = 0;
    }
  }

  return {
    onProgress(state) {
      switch (state.phase) {
        case 'connecting':
          console.log('  Connecting…');
          return;
        case 'scanning':
          console.log('  Scanning local files…');
          return;
        case 'cleanup':
          console.log('  Cleaning legacy FTP directories…');
          return;
        case 'preparing':
          if (!prepStarted) {
            console.log(`  Preparing ${state.totalDirs || 0} remote directories…`);
            prepStarted = true;
          }
          if (state.totalDirs > 0) {
            const pct = Math.min(100, (state.doneDirs / state.totalDirs) * 100);
            writeLine(
              `  [${progressBar(pct)}] ${pct.toFixed(0)}%  ` +
                `${state.doneDirs}/${state.totalDirs} directories`,
            );
          }
          return;
        case 'uploading':
          if (!uploadStarted) {
            endLine();
            const parallel =
              state.workers && state.workers > 1 ? ` (${state.workers} parallel)` : '';
            console.log(
              `  Uploading ${state.totalFiles} files (${formatBytes(state.totalBytes)})${parallel}…`,
            );
            uploadStarted = true;
          }
          break;
        default:
          return;
      }

      const pct =
        state.totalBytes > 0
          ? Math.min(100, (state.uploadedBytes / state.totalBytes) * 100)
          : 0;
      const elapsedSec = (Date.now() - started) / 1000;
      const rate = state.uploadedBytes / (elapsedSec || 1);
      const leftBytes = Math.max(0, state.totalBytes - state.uploadedBytes);
      const eta = rate > 0 ? leftBytes / rate : 0;
      const leftFiles = Math.max(0, state.totalFiles - state.uploadedFiles);
      const shortFile = state.currentFile
        ? state.currentFile.length > 42
          ? `…${state.currentFile.slice(-41)}`
          : state.currentFile
        : '';
      const workers =
        state.workers && state.workers > 1 ? ` · ${state.workers} parallel` : '';

      writeLine(
        `  [${progressBar(pct)}] ${pct.toFixed(1)}%  ` +
          `${state.uploadedFiles}/${state.totalFiles} files (${leftFiles} left)  ` +
          `${formatBytes(state.uploadedBytes)} / ${formatBytes(state.totalBytes)}  ` +
          `${formatBytes(rate)}/s  ETA ${formatEta(eta)}${workers}  ${shortFile}`,
      );
    },
    done(result) {
      endLine();
      const elapsed = formatEta((Date.now() - started) / 1000);
      console.log(
        `  ✓ ${label}: ${result.uploaded} files, ${formatBytes(result.totalBytes)} in ${elapsed} → ${result.remote}`,
      );
    },
    fail(err) {
      endLine();
      console.error(`  ✗ ${label} failed: ${err.message}`);
    },
  };
}

async function walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await walk(full)));
    } else {
      files.push(full);
    }
  }
  return files;
}

export function ftpConfigured() {
  return Boolean(config.ftp.host && config.ftp.user && config.ftp.password);
}

/** Remove legacy per-package directories that shadow {name}.json rewrite rules on Apache. */
async function cleanupLegacyPackageDirs(client, remotePackagesDir) {
  try {
    await client.cd(remotePackagesDir);
  } catch {
    return;
  }
  const entries = await client.list();
  for (const entry of entries) {
    if (entry.isDirectory && /^[a-z][a-z0-9_-]*$/i.test(entry.name)) {
      const target = path.posix.join(remotePackagesDir, entry.name);
      try {
        await client.removeDir(target, true);
      } catch {
        // best effort
      }
    }
  }
}

function ftpHint(err) {
  const msg = err.message || String(err);
  if (msg.includes('530') || msg.includes('Login')) {
    return ' Check FTP_USER, FTP_PASSWORD, and unlock FTP in StackCP (Manage Hosting → Unlock FTP).';
  }
  if (msg.includes('Timeout') || msg.includes('control socket') || msg.includes('ECONNRESET')) {
    return ' Use FTP_HOST=ftp.stackcp.com (not ftp.stackcp.risu.in). Unlock FTP in StackCP control panel.';
  }
  return '';
}

function isRetryableFtpError(err) {
  const msg = err?.message || String(err);
  return /ECONNRESET|ETIMEDOUT|EPIPE|Timeout|control socket|421|426/i.test(msg);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function withRetry(fn, { attempts = config.ftp.retryAttempts, label = 'FTP' } = {}) {
  let lastErr;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await fn();
    } catch (err) {
      lastErr = err;
      if (attempt >= attempts || !isRetryableFtpError(err)) throw err;
      await sleep(400 * attempt);
    }
  }
  throw lastErr;
}

function resolveRemotePath(remoteRoot, rel) {
  return remoteRoot === '/' ? `/${rel}` : path.posix.join(remoteRoot, rel);
}

function leafEnsureDirs(dirs) {
  const all = [...dirs];
  return all.filter(
    (dir) => !all.some((other) => other !== dir && other.startsWith(`${dir}/`)),
  );
}

function applyFtpSettings(client, { verbose = false } = {}) {
  client.ftp.verbose = verbose;
  client.ftp.passive = config.ftp.passive !== false;
}

async function connectFtpClient({ verbose = false } = {}) {
  const client = new Client(config.ftp.timeoutMs);
  applyFtpSettings(client, { verbose });
  await client.access({
    host: config.ftp.host,
    user: config.ftp.user,
    password: config.ftp.password,
    secure: config.ftp.secure,
  });
  return client;
}

async function connectFtpClientWithRetry(options = {}) {
  return withRetry(() => connectFtpClient(options), { label: 'FTP connect' });
}

async function ensureRemoteDirsParallel(dirs, { onDirDone }) {
  const leaves = leafEnsureDirs(dirs);
  if (leaves.length === 0) return;

  let nextIndex = 0;
  let doneDirs = 0;
  const workerCount = Math.min(config.ftp.dirConcurrency, leaves.length);
  const loginStaggerMs = config.ftp.loginStaggerMs;

  async function worker(workerId) {
    if (workerId > 0 && loginStaggerMs > 0) {
      await sleep(workerId * loginStaggerMs);
    }

    let client = await connectFtpClientWithRetry();
    try {
      while (true) {
        const index = nextIndex;
        nextIndex += 1;
        if (index >= leaves.length) break;

        const dir = leaves[index];
        try {
          await client.ensureDir(dir);
        } catch (err) {
          if (!isRetryableFtpError(err)) throw err;
          client.close();
          client = await connectFtpClientWithRetry();
          await client.ensureDir(dir);
        }
        doneDirs += 1;
        if (doneDirs === 1 || doneDirs === leaves.length || doneDirs % 10 === 0) {
          onDirDone?.({ doneDirs, totalDirs: leaves.length, currentDir: dir });
        }
      }
    } finally {
      client.close();
    }
  }

  await Promise.all(Array.from({ length: workerCount }, (_, i) => worker(i)));
}

/**
 * Upload many small files in parallel — each worker keeps its own FTP connection.
 * Workers start staggered so shared hosts are not hit with many logins at once.
 */
async function uploadEntriesParallel(entries, { remoteRoot, concurrency, onFileDone }) {
  if (entries.length === 0) return;

  let nextIndex = 0;
  const workerCount = Math.min(concurrency, entries.length);
  const loginStaggerMs = config.ftp.loginStaggerMs;

  async function worker(workerId) {
    if (workerId > 0 && loginStaggerMs > 0) {
      await sleep(workerId * loginStaggerMs);
    }

    let client = await connectFtpClientWithRetry();
    try {
      while (true) {
        const index = nextIndex;
        nextIndex += 1;
        if (index >= entries.length) break;

        const entry = entries[index];
        const remotePath = resolveRemotePath(remoteRoot, entry.rel);
        try {
          await client.uploadFrom(entry.file, remotePath);
        } catch (err) {
          if (!isRetryableFtpError(err)) throw err;
          client.close();
          client = await connectFtpClientWithRetry();
          await client.uploadFrom(entry.file, remotePath);
        }
        onFileDone(entry);
      }
    } finally {
      client.close();
    }
  }

  await Promise.all(Array.from({ length: workerCount }, (_, i) => worker(i)));
}

async function uploadEntriesSequential(entries, { remoteRoot, onFileDone }) {
  let client = await connectFtpClientWithRetry();
  try {
    for (const entry of entries) {
      const remotePath = resolveRemotePath(remoteRoot, entry.rel);
      try {
        await client.uploadFrom(entry.file, remotePath);
      } catch (err) {
        if (!isRetryableFtpError(err)) throw err;
        client.close();
        client = await connectFtpClientWithRetry();
        await client.uploadFrom(entry.file, remotePath);
      }
      onFileDone(entry);
    }
  } finally {
    client.close();
  }
}

export async function syncToFtp({ onProgress } = {}) {
  if (!ftpConfigured()) {
    throw new Error('FTP not configured — set FTP_HOST, FTP_USER, FTP_PASSWORD in .env');
  }

  const started = Date.now();
  const state = {
    phase: 'connecting',
    totalFiles: 0,
    uploadedFiles: 0,
    totalBytes: 0,
    uploadedBytes: 0,
    currentFile: '',
    totalDirs: 0,
    doneDirs: 0,
  };
  const report = (patch) => {
    Object.assign(state, patch);
    onProgress?.({ ...state });
  };

  let client = null;

  let remoteRoot = (config.ftp.remoteDir || '/').replace(/\/$/, '');
  if (!remoteRoot) remoteRoot = '/';

  const smallMax = config.ftp.smallFileMaxBytes;
  const concurrency = config.ftp.concurrency;

  try {
    report({ phase: 'connecting' });
    client = await connectFtpClientWithRetry({
      verbose: config.nodeEnv !== 'production' && !onProgress,
    });

    const localRoot = config.dataDir;
    const remotePackages = remoteRoot === '/' ? '/v1/packages' : path.posix.join(remoteRoot, 'v1/packages');
    report({ phase: 'cleanup' });
    await cleanupLegacyPackageDirs(client, remotePackages);

    report({ phase: 'scanning' });
    const files = await walk(localRoot);
    if (files.length === 0) {
      throw new Error('no files in DATA_DIR — run npm run seed first');
    }

    const fileEntries = [];
    let totalBytes = 0;
    for (const file of files) {
      const st = await fs.stat(file);
      const rel = path.relative(localRoot, file).split(path.sep).join('/');
      fileEntries.push({ file, rel, size: st.size });
      totalBytes += st.size;
    }

    if (remoteRoot !== '/') {
      await client.ensureDir(remoteRoot);
    }

    const remoteDirs = new Set();
    for (const entry of fileEntries) {
      const remoteDir = path.posix.dirname(resolveRemotePath(remoteRoot, entry.rel));
      if (remoteDir && remoteDir !== '.' && remoteDir !== '/') {
        remoteDirs.add(remoteDir);
      }
    }

    const leafDirCount = leafEnsureDirs(remoteDirs).length;
    report({ phase: 'preparing', totalDirs: leafDirCount, doneDirs: 0 });
    client.close();
    client = null;

    await ensureRemoteDirsParallel(remoteDirs, {
      onDirDone: ({ doneDirs, totalDirs }) => {
        report({ phase: 'preparing', doneDirs, totalDirs });
      },
    });

    const smallEntries = fileEntries.filter((e) => e.size <= smallMax);
    const largeEntries = fileEntries.filter((e) => e.size > smallMax);

    report({
      phase: 'uploading',
      totalFiles: fileEntries.length,
      totalBytes,
      uploadedFiles: 0,
      uploadedBytes: 0,
      currentFile: '',
      workers: smallEntries.length > 0 ? Math.min(concurrency, smallEntries.length) : 0,
    });

    let uploadedFiles = 0;
    let uploadedBytes = 0;
    const parallelWorkers =
      smallEntries.length > 0 ? Math.min(concurrency, smallEntries.length) : 0;
    const onFileDone = (entry) => {
      uploadedFiles += 1;
      uploadedBytes += entry.size;
      const isLarge = entry.size > smallMax;
      // Throttle progress updates — 900+ small files would flood the terminal.
      if (
        uploadedFiles === 1 ||
        uploadedFiles === fileEntries.length ||
        uploadedFiles === smallEntries.length ||
        uploadedFiles % 25 === 0 ||
        isLarge
      ) {
        report({
          uploadedFiles,
          uploadedBytes,
          currentFile: entry.rel,
          workers: uploadedFiles < smallEntries.length ? parallelWorkers : 0,
        });
      }
    };

    if (smallEntries.length > 0) {
      await uploadEntriesParallel(smallEntries, { remoteRoot, concurrency, onFileDone });
    }

    if (largeEntries.length > 0) {
      report({
        workers: 0,
        currentFile: largeEntries[0].rel,
      });
      await uploadEntriesSequential(largeEntries, { remoteRoot, onFileDone });
    }

    // Ensure Apache rewrite rules are present (some hosts skip dotfiles in bulk upload)
    const htaccess = path.join(localRoot, '.htaccess');
    if (await fs.access(htaccess).then(() => true).catch(() => false)) {
      const remoteHtaccess = remoteRoot === '/' ? '/.htaccess' : path.posix.join(remoteRoot, '.htaccess');
      const already = fileEntries.some((e) => e.rel === '.htaccess');
      if (!already) {
        const st = await fs.stat(htaccess);
        report({ currentFile: '.htaccess', workers: 0 });
        const htClient = await connectFtpClientWithRetry();
        try {
          await htClient.uploadFrom(htaccess, remoteHtaccess);
        } finally {
          htClient.close();
        }
        uploadedFiles += 1;
        uploadedBytes += st.size;
        report({ uploadedFiles, uploadedBytes, currentFile: '.htaccess' });
      }
    }

    const pwdClient = await connectFtpClientWithRetry();
    let pwd = remoteRoot;
    try {
      pwd = await pwdClient.pwd();
    } catch {
      // keep remoteRoot
    } finally {
      pwdClient.close();
    }

    report({ phase: 'done', currentFile: '' });

    return {
      uploaded: uploadedFiles,
      totalBytes: uploadedBytes,
      durationMs: Date.now() - started,
      remote: `${config.ftp.host}:${pwd}`,
      host: config.ftp.host,
      remoteDir: pwd,
    };
  } catch (err) {
    throw new Error(`FTP sync failed: ${err.message}.${ftpHint(err)}`);
  } finally {
    client?.close();
  }
}

export async function testFtpConnection() {
  if (!ftpConfigured()) {
    throw new Error('FTP not configured');
  }
  const client = new Client(config.ftp.timeoutMs);
  applyFtpSettings(client);
  try {
    await withRetry(() =>
      client.access({
        host: config.ftp.host,
        user: config.ftp.user,
        password: config.ftp.password,
        secure: config.ftp.secure,
      }),
    );
    const pwd = await client.pwd();
    const list = await client.list();
    return {
      ok: true,
      host: config.ftp.host,
      pwd,
      entries: list.length,
      sample: list.slice(0, 8).map((e) => e.name),
    };
  } finally {
    client.close();
  }
}
