import { config } from '../config.js';

const CACHE_TTL_MS = 5 * 60 * 1000;
const cache = new Map();

function filesUrl(pathname) {
  const base = config.filesUrl.replace(/\/$/, '');
  const path = pathname.startsWith('/') ? pathname : `/${pathname}`;
  return `${base}${path}`;
}

function getCached(pathname) {
  const hit = cache.get(pathname);
  if (!hit) return null;
  if (Date.now() - hit.at > CACHE_TTL_MS) return null;
  return hit.data;
}

function setCached(pathname, data) {
  cache.set(pathname, { data, at: Date.now() });
}

async function fetchRemoteJsonOnce(pathname) {
  const url = filesUrl(pathname);
  const res = await fetch(url, {
    headers: { accept: 'application/json' },
    signal: AbortSignal.timeout(30_000),
  });
  if (!res.ok) {
    throw new Error(`remote fetch failed (${res.status}): ${url}`);
  }
  return res.json();
}

export async function fetchRemoteJson(pathname) {
  const cached = getCached(pathname);
  if (cached) return cached;

  let lastErr;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const data = await fetchRemoteJsonOnce(pathname);
      setCached(pathname, data);
      return data;
    } catch (err) {
      lastErr = err;
      const stale = cache.get(pathname)?.data;
      if (stale && String(err.message).includes('429')) return stale;
      if (String(err.message).includes('429') && attempt < 2) {
        await new Promise((r) => setTimeout(r, 800 * (attempt + 1)));
        continue;
      }
      throw err;
    }
  }
  throw lastErr;
}

export function clearRemoteCache() {
  cache.clear();
}

export function packageFromCatalogEntry(name, entry) {
  return {
    name: entry.name || name,
    version: entry.version,
    kind: entry.kind,
    description: entry.description || '',
    import_paths: entry.import_paths || [],
    builtin_count: entry.builtin_count || 0,
    remote: entry.remote === true,
    versions: entry.versions || [entry.version],
    latest: entry.versions?.at(-1) || entry.version,
  };
}

export async function fetchCatalog() {
  try {
    return await fetchRemoteJson('/v1/catalog');
  } catch {
    return fetchRemoteJson('/catalog.json');
  }
}

export async function fetchPackageList() {
  try {
    const data = await fetchRemoteJson('/v1/packages/index.json');
    if (Array.isArray(data?.packages) && data.packages.length > 0) {
      return data.packages.map((p) => p.name).sort();
    }
  } catch {
    // fall through
  }
  const catalog = await fetchCatalog();
  return Object.keys(catalog.libs || {}).sort();
}

export async function fetchPackageMeta(name) {
  const catalog = await fetchCatalog();
  const entry = catalog.libs?.[name];
  if (entry) {
    return packageFromCatalogEntry(name, entry);
  }

  const encoded = encodeURIComponent(name);
  const paths = [`/v1/packages/${encoded}.json`, `/v1/packages/${encoded}`];
  for (const pathname of paths) {
    try {
      return await fetchRemoteJson(pathname);
    } catch {
      // try next path
    }
  }

  throw new Error(`package not found on files host: ${name}`);
}

export async function fetchVersionMeta(name, version) {
  const encName = encodeURIComponent(name);
  const encVer = encodeURIComponent(version);
  const paths = [
    `/v1/packages/${encName}-${encVer}.json`,
    `/v1/packages/${encName}/${encVer}.json`,
    `/v1/packages/${encName}/${encVer}`,
  ];
  for (const pathname of paths) {
    try {
      return await fetchRemoteJson(pathname);
    } catch {
      // try next path
    }
  }
  throw new Error(`version not found on files host: ${name}@${version}`);
}

export function remoteTarballUrl(name, version) {
  return filesUrl(`/v1/packages/${encodeURIComponent(name)}/${encodeURIComponent(version)}/tarball`);
}
