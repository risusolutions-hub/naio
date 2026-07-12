import fs from 'node:fs/promises';
import path from 'node:path';
import { config } from '../config.js';
import {
  DEFAULT_PLATFORMS,
  releaseInstallerFileName,
} from '../db/registry-db.js';

const probeCache = new Map();

async function probeUrl(url) {
  if (!url) return { ok: false, size: 0 };
  if (probeCache.has(url)) return probeCache.get(url);

  let result = { ok: false, size: 0 };
  try {
    const res = await fetch(url, {
      method: 'HEAD',
      signal: AbortSignal.timeout(12_000),
      redirect: 'follow',
    });
    if (res.ok) {
      const size = Number(res.headers.get('content-length')) || 0;
      result = { ok: true, size };
    } else if (res.status === 405 || res.status === 403) {
      const get = await fetch(url, {
        method: 'GET',
        headers: { Range: 'bytes=0-0' },
        signal: AbortSignal.timeout(12_000),
        redirect: 'follow',
      });
      result = { ok: get.ok, size: Number(get.headers.get('content-length')) || 0 };
    }
  } catch {
    result = { ok: false, size: 0 };
  }

  probeCache.set(url, result);
  return result;
}

async function localFileSize(filePath) {
  try {
    const stat = await fs.stat(filePath);
    return stat.isFile() && stat.size > 0 ? stat.size : 0;
  } catch {
    return 0;
  }
}

function platformFor(variant) {
  return DEFAULT_PLATFORMS.find((p) => p.id === variant.id) || variant;
}

export async function resolveVariantAvailability(variant, version) {
  const p = platformFor(variant);
  const ext = variant.ext || p.ext || 'zip';
  const archivePath = path.join(config.dataDir, 'releases', `niao-${version}-${p.id}.${ext}`);
  const installerPath = path.join(
    config.dataDir,
    'releases',
    releaseInstallerFileName(version, p.id, p),
  );

  let archiveAvailable = Number(variant.size) > 0 || Boolean(variant.shasum);
  let installerAvailable = Number(variant.installer_size) > 0 || Boolean(variant.installer_shasum);
  let size = Number(variant.size) || 0;
  let installerSize = Number(variant.installer_size) || 0;

  if (!archiveAvailable && variant.url) {
    const localSize = await localFileSize(archivePath);
    if (localSize > 0) {
      archiveAvailable = true;
      size = localSize;
    } else {
      const probe = await probeUrl(variant.url);
      archiveAvailable = probe.ok;
      if (probe.size > 0) size = probe.size;
    }
  }

  const sameUrl = variant.installer_url && variant.installer_url === variant.url;
  if (!installerAvailable && variant.installer_url && !sameUrl) {
    const localSize = await localFileSize(installerPath);
    if (localSize > 0) {
      installerAvailable = true;
      installerSize = localSize;
    } else {
      const probe = await probeUrl(variant.installer_url);
      installerAvailable = probe.ok;
      if (probe.size > 0) installerSize = probe.size;
    }
  }

  return {
    ...variant,
    size,
    installer_size: installerSize,
    archive_available: archiveAvailable,
    installer_available: installerAvailable,
    available: archiveAvailable || installerAvailable,
  };
}

export async function resolveAllVariants(variants, version) {
  return Promise.all(
    (variants || []).map((v) => resolveVariantAvailability({ ...v }, version)),
  );
}

export async function filterAvailableVariants(variants, version) {
  const resolved = await resolveAllVariants(variants, version);
  return resolved.filter((v) => v.available);
}

export function clearProbeCache() {
  probeCache.clear();
}
