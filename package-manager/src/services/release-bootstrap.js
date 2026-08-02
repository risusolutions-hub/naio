import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { config } from '../config.js';
import {
  compareVersions,
  DEFAULT_PLATFORMS,
  releaseInstallerFileName,
  releaseInstallerUrl,
  releaseVariantUrl,
  RELEASE_STATUS,
} from '../db/registry-db.js';
import { readReleaseIndex, readReleaseManifest, upsertReleaseIndex } from './release-index.js';
import { fetchRemoteJson } from './remote-registry.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const knownVersionsPath = path.join(__dirname, '../data/release-versions.json');

export async function loadKnownVersions() {
  try {
    const raw = await fs.readFile(knownVersionsPath, 'utf8');
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return [...new Set(parsed)].sort(compareVersions);
    if (Array.isArray(parsed?.versions)) return [...new Set(parsed.versions)].sort(compareVersions);
  } catch {
    // optional seed file
  }
  return [config.niaoVersion].filter(Boolean);
}

function releaseTemplate(version, variants = null) {
  const tag = `v${version}`;
  const github = config.githubRepo;
  return {
    version,
    status: RELEASE_STATUS.ACTIVE,
    is_latest: false,
    changelog: '',
    components: ['niao', 'nm', 'vm', 'nfe'],
    source: {
      github,
      tag,
      zip_url: `${github}/archive/refs/tags/${tag}.zip`,
      tarball_url: `${github}/archive/refs/tags/${tag}.tar.gz`,
      hosted_url: `${config.filesUrl}/releases/niao-${version}-toolchain.tgz`,
    },
    toolchain: {
      url: `${config.filesUrl}/releases/niao-${version}-toolchain.tgz`,
    },
    variants:
      variants ||
      DEFAULT_PLATFORMS.map((p) => ({
        id: p.id,
        label: p.label,
        platform: p.platform,
        arch: p.arch,
        ext: p.ext || 'zip',
        status: RELEASE_STATUS.ACTIVE,
        url: releaseVariantUrl(version, p.id, p.ext || 'zip'),
        installer_ext: p.installer_ext || 'sh',
        installer_label: p.installer_label || 'install.sh',
        installer_url: releaseInstallerUrl(version, p.id, p),
      })),
  };
}

async function fetchRemoteVersionDetail(version) {
  const paths = [
    `/v1/releases/niao/${encodeURIComponent(version)}.json`,
    `/releases/${encodeURIComponent(version)}.json`,
  ];
  for (const pathname of paths) {
    try {
      return await fetchRemoteJson(pathname);
    } catch {
      // try next
    }
  }
  return null;
}

/** Ensure index.json lists every known active release (keeps older versions until discontinued). */
export async function bootstrapReleaseHistory({ knownVersions = null } = {}) {
  const versions = knownVersions || (await loadKnownVersions());
  const manifest = await readReleaseManifest();
  const existing = await readReleaseIndex();
  const byVersion = new Map();

  for (const row of existing?.releases || []) {
    if (row?.version) byVersion.set(row.version, row);
  }

  for (const version of versions) {
    const remote = await fetchRemoteVersionDetail(version).catch(() => null);
    const current = byVersion.get(version) || {};
    const template = releaseTemplate(version, remote?.variants || current.variants);

    byVersion.set(version, {
      ...template,
      ...current,
      ...(remote || {}),
      version,
      status: current.status || remote?.status || RELEASE_STATUS.ACTIVE,
      variants: remote?.variants || current.variants || template.variants,
    });
  }

  if (manifest?.version) {
    const current = byVersion.get(manifest.version) || releaseTemplate(manifest.version);
    byVersion.set(manifest.version, {
      ...current,
      ...manifest,
      version: manifest.version,
      status: manifest.status || RELEASE_STATUS.ACTIVE,
      variants: manifest.variants || current.variants,
    });
  }

  const releases = [...byVersion.values()].sort((a, b) => compareVersions(b.version, a.version));
  const active = releases
    .filter((r) => (r.status || RELEASE_STATUS.ACTIVE) === RELEASE_STATUS.ACTIVE)
    .sort((a, b) => compareVersions(a.version, b.version));
  const latest = active.at(-1)?.version || manifest?.version || config.niaoVersion;

  for (const release of releases) {
    await upsertReleaseIndex({ ...release, is_latest: release.version === latest });
  }

  return readReleaseIndex();
}

export function isPublicRelease(release) {
  const status = release?.status || RELEASE_STATUS.ACTIVE;
  return status === RELEASE_STATUS.ACTIVE || status === RELEASE_STATUS.DISCONTINUED;
}

export { releaseTemplate };
