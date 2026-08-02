import path from 'node:path';
import fs from 'node:fs/promises';
import { config } from '../config.js';
import {
  compareVersions,
  DEFAULT_PLATFORMS,
  getLatestReleaseFromDb,
  getReleaseFromDb,
  isMongoPrimary,
  listReleasesFromDb,
  releaseInstallerUrl,
  releaseVariantUrl,
  RELEASE_STATUS,
} from '../db/registry-db.js';
import { fetchRemoteJson } from './remote-registry.js';
import { readReleaseIndex, readReleaseManifest } from './release-index.js';
import { bootstrapReleaseHistory, isPublicRelease, loadKnownVersions, releaseTemplate } from './release-bootstrap.js';

function releaseFromManifest(manifest) {
  if (!manifest?.version) return null;
  return {
    version: manifest.version,
    status: manifest.status || RELEASE_STATUS.ACTIVE,
    is_latest: manifest.is_latest !== false,
    changelog: manifest.changelog || '',
    components: manifest.components || ['niao', 'nm', 'vm', 'nfe'],
    source: {
      zip_url: manifest.source?.zip_url,
      tarball_url: manifest.source?.tarball_url,
      hosted_url: manifest.toolchain?.url || manifest.source?.hosted_url || null,
      github: manifest.source?.github || config.githubRepo,
    },
    variants: (manifest.variants || []).map((v) => ({
      ...v,
      url: v.url || releaseVariantUrl(manifest.version, v.id, v.ext || 'zip'),
      installer_url: v.installer_url || releaseInstallerUrl(manifest.version, v.id, v),
    })),
  };
}

function releaseFromEnv(version) {
  const envMap = {
    'windows-x64': config.releaseBinaries.windows,
    'linux-x64': config.releaseBinaries.linux,
    'linux-arm64': config.releaseBinaries.linux_arm64,
    'macos-x64': config.releaseBinaries.macos,
    'macos-arm64': config.releaseBinaries.macos_arm64,
  };
  const tag = `v${version}`;
  const github = config.githubRepo;
  return {
    version,
    status: RELEASE_STATUS.ACTIVE,
    is_latest: true,
    components: ['niao', 'nm', 'vm', 'nfe'],
    source: {
      zip_url: `${github}/archive/refs/tags/${tag}.zip`,
      tarball_url: `${github}/archive/refs/tags/${tag}.tar.gz`,
      hosted_url: `${config.filesUrl}/releases/niao-${version}-toolchain.tgz`,
      github,
    },
    variants: DEFAULT_PLATFORMS.map((p) => ({
      id: p.id,
      label: p.label,
      platform: p.platform,
      arch: p.arch,
      ext: p.ext || 'zip',
      status: RELEASE_STATUS.ACTIVE,
      url: envMap[p.id] || releaseVariantUrl(version, p.id, p.ext || 'zip'),
      installer_ext: p.installer_ext || 'sh',
      installer_label: p.installer_label || 'install.sh',
      installer_url: releaseInstallerUrl(version, p.id, p),
    })),
  };
}

function variantScore(variant = {}) {
  let score = 0;
  if (variant.url) score += 1;
  if (variant.shasum) score += 4;
  if (Number(variant.size) > 0) score += 2;
  if (variant.installer_url) score += 1;
  if (variant.installer_shasum) score += 2;
  return score;
}

function releaseScore(release = {}) {
  const variants = release.variants || [];
  const variantTotal = variants.reduce((sum, v) => sum + variantScore(v), 0);
  let score = variantTotal;
  if (release.changelog) score += 1;
  if (release.source?.hosted_url) score += 1;
  if (release.released_at) score += 1;
  return score;
}

function mergeVariant(existing = {}, incoming = {}) {
  const pick = variantScore(incoming) >= variantScore(existing) ? incoming : existing;
  const other = pick === incoming ? existing : incoming;
  return {
    ...other,
    ...pick,
    id: pick.id || other.id,
    url: pick.url || other.url,
    installer_url: pick.installer_url || other.installer_url,
    shasum: pick.shasum || other.shasum || '',
    installer_shasum: pick.installer_shasum || other.installer_shasum || '',
    size: Number(pick.size) || Number(other.size) || 0,
    installer_size: Number(pick.installer_size) || Number(other.installer_size) || 0,
  };
}

function mergeVariants(existing = [], incoming = []) {
  const byId = new Map();
  for (const variant of existing) {
    if (variant?.id) byId.set(variant.id, variant);
  }
  for (const variant of incoming) {
    if (!variant?.id) continue;
    const prev = byId.get(variant.id);
    byId.set(variant.id, prev ? mergeVariant(prev, variant) : variant);
  }
  return [...byId.values()];
}

function mergeRelease(existing, incoming) {
  if (!existing) return incoming;
  if (!incoming) return existing;

  const primary = releaseScore(incoming) >= releaseScore(existing) ? incoming : existing;
  const secondary = primary === incoming ? existing : incoming;

  return {
    ...secondary,
    ...primary,
    version: primary.version || secondary.version,
    status: primary.status || secondary.status || RELEASE_STATUS.ACTIVE,
    changelog: primary.changelog || secondary.changelog || '',
    components: primary.components || secondary.components,
    source: { ...(secondary.source || {}), ...(primary.source || {}) },
    variants: mergeVariants(secondary.variants, primary.variants),
    released_at: primary.released_at || secondary.released_at,
  };
}

function recomputeLatestFlags(releases) {
  const active = releases
    .filter((r) => (r.status || RELEASE_STATUS.ACTIVE) === RELEASE_STATUS.ACTIVE)
    .sort((a, b) => compareVersions(a.version, b.version));
  const latestVersion = active.at(-1)?.version || releases[0]?.version || config.niaoVersion;
  return releases
    .map((r) => ({ ...r, is_latest: r.version === latestVersion }))
    .sort((a, b) => compareVersions(b.version, a.version));
}

function shouldFetchRemote() {
  if (config.remoteReads) return true;
  const files = config.filesUrl.replace(/\/$/, '');
  if (!files || files.includes('localhost')) return false;
  return true;
}

async function fetchRemoteReleases() {
  const paths = [
    '/v1/releases/niao/index.json',
    '/releases/index.json',
  ];

  const collected = new Map();

  for (const pathname of paths) {
    try {
      const data = await fetchRemoteJson(pathname);
      const rows = Array.isArray(data?.releases) ? data.releases : [];
      for (const row of rows) {
        const release = releaseFromManifest(row);
        if (release) collected.set(release.version, mergeRelease(collected.get(release.version), release));
      }
      if (rows.length > 0) break;
    } catch {
      // try next path
    }
  }

  const versions = new Set([
    ...collected.keys(),
    ...(await loadKnownVersions()),
  ]);

  for (const version of versions) {
    const paths = [
      `/v1/releases/niao/${encodeURIComponent(version)}.json`,
      `/releases/${encodeURIComponent(version)}.json`,
    ];
    for (const pathname of paths) {
      try {
        const detail = await fetchRemoteJson(pathname);
        const release = releaseFromManifest(detail);
        if (release) {
          collected.set(version, mergeRelease(collected.get(version), release));
          break;
        }
      } catch {
        // try next path
      }
    }
    if (!collected.has(version)) {
      collected.set(version, releaseFromManifest(releaseTemplate(version)));
    }
  }

  try {
    const manifest = await fetchRemoteJson('/releases/manifest.json');
    const release = releaseFromManifest(manifest);
    if (release) {
      collected.set(release.version, mergeRelease(collected.get(release.version), release));
    }
  } catch {
    // manifest optional
  }

  return [...collected.values()];
}

async function loadLocalReleases() {
  const releases = [];

  const index = await readReleaseIndex();
  if (Array.isArray(index?.releases)) {
    for (const row of index.releases) {
      const release = releaseFromManifest(row);
      if (release) releases.push(release);
    }
  }

  const manifest = await readReleaseManifest();
  const fromManifest = releaseFromManifest(manifest);
  if (fromManifest) {
    const idx = releases.findIndex((r) => r.version === fromManifest.version);
    if (idx >= 0) {
      releases[idx] = mergeRelease(releases[idx], fromManifest);
    } else {
      releases.push(fromManifest);
    }
  }

  return releases;
}

async function loadMongoReleases() {
  if (!isMongoPrimary()) return [];
  return listReleasesFromDb();
}

async function collectReleases() {
  const byVersion = new Map();

  const add = (release) => {
    if (!release?.version) return;
    const prev = byVersion.get(release.version);
    byVersion.set(release.version, mergeRelease(prev, release));
  };

  for (const release of await loadMongoReleases()) add(release);
  for (const release of await loadLocalReleases()) add(release);

  if (shouldFetchRemote()) {
    for (const release of await fetchRemoteReleases()) add(release);
  }

  for (const version of await loadKnownVersions()) {
    if (!byVersion.has(version)) {
      add(releaseFromManifest(releaseTemplate(version)));
    }
  }

  if (byVersion.size === 0) {
    add(releaseFromEnv(config.niaoVersion));
  } else if (!byVersion.has(config.niaoVersion)) {
  // Env version may exist on CDN even if metadata is incomplete — synthesize URLs.
    add(releaseFromEnv(config.niaoVersion));
  }

  return recomputeLatestFlags(
    [...byVersion.values()].filter(isPublicRelease),
  );
}

let cache = { at: 0, releases: null };
const CACHE_MS = 30_000;

export function clearReleaseCache() {
  cache = { at: 0, releases: null };
}

export async function listAllReleases({ fresh = false } = {}) {
  if (!fresh && cache.releases && Date.now() - cache.at < CACHE_MS) {
    return cache.releases;
  }
  const releases = await collectReleases();
  cache = { at: Date.now(), releases };
  return releases;
}

export async function getLatestRelease({ fresh = false } = {}) {
  const releases = await listAllReleases({ fresh });
  return releases.find((r) => r.is_latest) || releases[0] || null;
}

export async function getRelease(version, { fresh = false } = {}) {
  const releases = await listAllReleases({ fresh });
  const found = releases.find((r) => r.version === version);
  if (found) return found;

  if (isMongoPrimary()) {
    const fromDb = await getReleaseFromDb(version);
    if (fromDb) return fromDb;
  }

  if (version === config.niaoVersion) {
    return releaseFromEnv(version);
  }

  return null;
}

export async function hostedSourcePath(version = config.niaoVersion) {
  return path.join(config.dataDir, 'releases', `niao-${version}-toolchain.tgz`);
}

export async function hostedSourceExists(version = config.niaoVersion) {
  try {
    await fs.access(await hostedSourcePath(version));
    return true;
  } catch {
    return false;
  }
}
