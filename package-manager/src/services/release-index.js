import fs from 'node:fs/promises';
import path from 'node:path';
import { config } from '../config.js';
import { compareVersions } from '../db/registry-db.js';

const releasesDir = () => path.join(config.dataDir, 'releases');
const indexPath = () => path.join(releasesDir(), 'index.json');
const manifestPath = () => path.join(releasesDir(), 'manifest.json');

export async function readReleaseIndex() {
  try {
    const raw = await fs.readFile(indexPath(), 'utf8');
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export async function readReleaseManifest() {
  try {
    const raw = await fs.readFile(manifestPath(), 'utf8');
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

/** Merge a release record into index.json and mark the highest active version as latest. */
export async function upsertReleaseIndex(release) {
  if (!release?.version) {
    throw new Error('release.version required');
  }

  await fs.mkdir(releasesDir(), { recursive: true });

  const existing = (await readReleaseIndex()) || { releases: [] };
  const releases = Array.isArray(existing.releases) ? [...existing.releases] : [];
  const idx = releases.findIndex((r) => r.version === release.version);

  const record = {
    ...(idx >= 0 ? releases[idx] : {}),
    ...release,
    version: release.version,
    status: release.status || 'active',
    updated_at: new Date().toISOString(),
  };

  if (idx >= 0) releases[idx] = record;
  else releases.push(record);

  const active = releases
    .filter((r) => (r.status || 'active') === 'active')
    .sort((a, b) => compareVersions(a.version, b.version));
  const latest = active.at(-1)?.version || release.version;

  for (const r of releases) {
    r.is_latest = r.version === latest;
  }

  const index = {
    latest,
    updated_at: new Date().toISOString(),
    releases: releases.sort((a, b) => compareVersions(b.version, a.version)),
  };

  await fs.writeFile(indexPath(), JSON.stringify(index, null, 2) + '\n');
  return index;
}
