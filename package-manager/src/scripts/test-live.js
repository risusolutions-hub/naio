/**
 * Comprehensive live registry test — API, CDN, checksums, nm client flow.
 * Usage: node src/scripts/test-live.js [apiUrl] [filesUrl]
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import * as tar from 'tar';
import dotenv from 'dotenv';

dotenv.config();

const api = (process.argv[2] || 'https://nms.taurus-tech.in').replace(/\/$/, '');
const files = (process.argv[3] || 'https://nm.c4compare.com').replace(/\/$/, '');
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const tmpDir = path.join(__dirname, '../../.test-live-tmp');
const nmBin = process.env.NM_BIN || path.resolve(__dirname, '../../../target/release/nm.exe');

let passed = 0;
let failed = 0;
const failures = [];

function ok(name, detail = '') {
  passed++;
  console.log(`  ✓ ${name}${detail ? ` — ${detail}` : ''}`);
}

function fail(name, err) {
  failed++;
  const msg = err instanceof Error ? err.message : String(err);
  failures.push({ name, msg });
  console.error(`  ✗ ${name}: ${msg}`);
}

async function fetchJson(url, opts = {}) {
  const res = await fetch(url, {
    ...opts,
    headers: { accept: 'application/json', ...(opts.headers || {}) },
    signal: AbortSignal.timeout(60_000),
  });
  const text = await res.text();
  let data;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = text;
  }
  return { res, data, text };
}

async function fetchBytes(url, { follow = true } = {}) {
  const res = await fetch(url, {
    redirect: follow ? 'follow' : 'manual',
    signal: AbortSignal.timeout(120_000),
  });
  const buf = Buffer.from(await res.arrayBuffer());
  return { res, buf };
}

async function testSection(title, fn) {
  console.log(`\n▸ ${title}`);
  await fn();
}

async function main() {
  console.log(`\n${'═'.repeat(52)}`);
  console.log(' Niao Registry — Live Verification');
  console.log(` API:   ${api}`);
  console.log(` Files: ${files}`);
  console.log(`${'═'.repeat(52)}`);

  let catalog = null;
  let health = null;

  await testSection('API health & metadata', async () => {
    const { res, data } = await fetchJson(`${api}/health`);
    if (!res.ok || !data?.ok) throw new Error(`health failed: ${JSON.stringify(data)}`);
    health = data;
    ok('GET /health', `serverless=${data.serverless}, mongo=${data.mongo}, v${data.version}`);

    const root = await fetchJson(`${api}/`);
    if (!root.res.ok) throw new Error(`GET / HTTP ${root.res.status}`);
    ok('GET / (home page)', root.res.headers.get('content-type')?.includes('html') ? 'HTML' : 'OK');

    const site = await fetchJson(`${api}/v1/site`);
    if (!site.res.ok) throw new Error(`site HTTP ${site.res.status}`);
    ok('GET /v1/site', `release ${site.data?.latestRelease ?? site.data?.version ?? '?'}`);
  });

  await testSection('Catalog & package index', async () => {
    const { res, data } = await fetchJson(`${api}/v1/catalog`);
    if (!res.ok || !data?.libs) throw new Error(`catalog HTTP ${res.status}`);
    catalog = data;
    const names = Object.keys(data.libs);
    if (names.length < 1) throw new Error('catalog has no libs');
    ok('GET /v1/catalog', `${names.length} libraries`);

    const pkgs = await fetchJson(`${api}/v1/packages`);
    if (!pkgs.res.ok) throw new Error(`packages HTTP ${pkgs.res.status}`);
    ok('GET /v1/packages', `${pkgs.data.packages?.length ?? 0} packages`);
  });

  await testSection('Static CDN mirror', async () => {
    const { res, data } = await fetchJson(`${files}/catalog.json`);
    if (!res.ok) throw new Error(`CDN catalog HTTP ${res.status}`);
    const cdnCount = Object.keys(data.libs || {}).length;
    if (cdnCount < 1) throw new Error('CDN catalog empty');
    ok('CDN catalog.json', `${cdnCount} libraries`);

    const apiCount = Object.keys(catalog?.libs || {}).length;
    if (apiCount > 0 && cdnCount !== apiCount) {
      fail('catalog parity API vs CDN', `API=${apiCount}, CDN=${cdnCount}`);
    } else {
      ok('catalog parity API vs CDN', `${apiCount} libs match`);
    }
  });

  const sampleLibs = ['nllm', 'nrag', 'json', 'nos'].filter((n) => catalog?.libs?.[n]);
  if (sampleLibs.length === 0) {
    sampleLibs.push(...Object.keys(catalog.libs).slice(0, 4));
  }

  await testSection(`Package metadata (${sampleLibs.join(', ')})`, async () => {
    for (const name of sampleLibs) {
      try {
        const { res, data } = await fetchJson(`${api}/v1/packages/${name}`);
        if (!res.ok) throw new Error(JSON.stringify(data));
        const latest = data.latest || data.version;
        ok(`GET /v1/packages/${name}`, `latest ${latest}`);
      } catch (e) {
        fail(`GET /v1/packages/${name}`, e);
      }
    }
  });

  await testSection('Version metadata, tarball URLs & checksums', async () => {
    await fs.mkdir(tmpDir, { recursive: true });

    for (const name of sampleLibs) {
      const entry = catalog.libs[name];
      const version = entry.versions?.at(-1) || entry.version;
      try {
        const { res, data } = await fetchJson(`${api}/v1/packages/${name}/${version}`);
        if (!res.ok || !data.dist?.tarball || !data.dist?.shasum) {
          throw new Error(JSON.stringify(data));
        }
        ok(`GET /v1/packages/${name}/${version}`, `sha256 ${data.dist.shasum.slice(0, 12)}…`);

        const tarballUrl = data.dist.tarball;
        const { res: tRes, buf } = await fetchBytes(tarballUrl);
        if (!tRes.ok) throw new Error(`tarball HTTP ${tRes.status} from ${tarballUrl}`);
        const hash = crypto.createHash('sha256').update(buf).digest('hex');
        if (hash !== data.dist.shasum) {
          throw new Error(`checksum mismatch for ${name}@${version}`);
        }
        ok(`download ${name}@${version}`, `${buf.length} bytes, checksum OK`);

        const tgz = path.join(tmpDir, `${name}-${version}.tgz`);
        await fs.writeFile(tgz, buf);
        const extractDir = path.join(tmpDir, `extract-${name}`);
        await fs.rm(extractDir, { recursive: true, force: true });
        await fs.mkdir(extractDir, { recursive: true });
        await tar.x({ file: tgz, cwd: extractDir });
        const libJson = path.join(extractDir, name, version, 'lib.json');
        const lib = JSON.parse(await fs.readFile(libJson, 'utf8'));
        if (lib.name !== name) throw new Error('bad lib.json name');
        ok(`extract ${name}/${version}/lib.json`);
      } catch (e) {
        fail(`${name}@${version} tarball flow`, e);
      }
    }
  });

  await testSection('API tarball redirect endpoint', async () => {
    const name = sampleLibs[0];
    const version = catalog.libs[name].versions?.at(-1) || catalog.libs[name].version;
    try {
      const head = await fetch(`${api}/v1/packages/${name}/${version}/tarball`, {
        redirect: 'manual',
        signal: AbortSignal.timeout(30_000),
      });
      const loc = head.headers.get('location');
      if (head.status >= 300 && head.status < 400) {
        if (!loc || loc === '302' || !loc.startsWith('http')) {
          throw new Error(`bad redirect Location: ${loc ?? '(missing)'}`);
        }
        ok(`GET tarball redirect`, `${head.status} → ${loc.slice(0, 60)}…`);
        const { res, buf } = await fetchBytes(`${api}/v1/packages/${name}/${version}/tarball`);
        if (!res.ok || buf.length < 50) throw new Error(`followed redirect HTTP ${res.status}`);
        ok('follow API tarball redirect', `${buf.length} bytes`);
      } else if (head.ok) {
        ok('GET tarball direct', `HTTP ${head.status}`);
      } else {
        throw new Error(`HTTP ${head.status}, Location=${loc}`);
      }
    } catch (e) {
      fail('API /tarball redirect', e);
    }
  });

  await testSection('Niao release endpoints', async () => {
    const ver = health?.version || catalog?.niao_version || '0.2.2';
    try {
      const { res, data } = await fetchJson(`${api}/v1/releases/niao`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      ok('GET /v1/releases/niao', `latest ${data.latest}`);
    } catch (e) {
      fail('GET /v1/releases/niao', e);
    }

    try {
      const { res, data } = await fetchJson(`${api}/v1/releases/niao/${ver}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      ok(`GET /v1/releases/niao/${ver}`, `${data.variants?.length ?? 0} variants`);
    } catch (e) {
      fail(`GET /v1/releases/niao/${ver}`, e);
    }

    try {
      const head = await fetch(`${api}/v1/releases/niao/${ver}/windows-x64`, {
        redirect: 'manual',
        signal: AbortSignal.timeout(30_000),
      });
      const loc = head.headers.get('location');
      if (head.status >= 300 && head.status < 400 && loc?.startsWith('http')) {
        ok('release binary redirect', `→ ${loc.split('/').pop()}`);
      } else if (head.ok) {
        ok('release binary direct', `HTTP ${head.status}`);
      } else {
        throw new Error(`HTTP ${head.status}, Location=${loc}`);
      }
    } catch (e) {
      fail('release windows-x64 redirect', e);
    }
  });

  if (await fs.access(nmBin).then(() => true).catch(() => false)) {
    await testSection('nm client install (live registry)', async () => {
      const testLibs = sampleLibs.slice(0, 2);
      for (const lib of testLibs) {
        try {
          const result = spawnSync(nmBin, ['install', lib, '--force'], {
            env: { ...process.env, NIAO_REGISTRY: api },
            encoding: 'utf8',
            timeout: 120_000,
          });
          if (result.status !== 0) {
            throw new Error((result.stderr || result.stdout || '').trim() || `exit ${result.status}`);
          }
          ok(`nm install ${lib}`, 'success');
        } catch (e) {
          fail(`nm install ${lib}`, e);
        }
      }
    });
  } else {
    console.log('\n▸ nm client install — skipped (nm binary not found)');
  }

  await fs.rm(tmpDir, { recursive: true, force: true }).catch(() => {});

  console.log(`\n${'─'.repeat(52)}`);
  console.log(`Results: ${passed} passed, ${failed} failed`);
  if (failures.length) {
    console.log('\nFailures:');
    for (const f of failures) {
      console.log(`  • ${f.name}: ${f.msg}`);
    }
  }
  console.log('');
  process.exit(failed > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
