import fs from 'node:fs/promises';
import path from 'node:path';
import {
  buildTarball,
  listPackages,
  listVersions,
  readCatalog,
  readPackageManifest,
  readVersionManifest,
  tarballPath,
} from '../services/storage.js';
import { remoteTarballUrl, packageFromCatalogEntry } from '../services/remote-registry.js';
import { config } from '../config.js';
import { publicDir } from '../lib/paths.js';
import {
  buildSitePayload,
  hostedSourceExists,
  hostedSourcePath,
  resolveReleaseDetail,
} from '../services/site.js';
import {
  getLatestReleaseFromDb,
  getPackageFromDb,
  getVersionFromDb,
  isMongoPrimary,
  listReleasesFromDb,
  DEFAULT_PLATFORMS,
  normalizePublicDist,
  distTarballUrl,
} from '../db/registry-db.js';
import { isDbReady } from '../db/mongo.js';

const homePage = path.join(publicDir, 'home.html');

export async function registryRoutes(app) {
  app.get('/', async (_req, reply) => {
    const html = await fs.readFile(homePage, 'utf8');
    const payload = await buildSitePayload();
    const injected = html.replace(
      '<!--SITE_DATA-->',
      `<script id="site-data" type="application/json">${JSON.stringify(payload).replace(/</g, '\\u003c')}</script>`,
    );
    reply.type('text/html').send(injected);
  });

  app.get('/v1/site', async () => buildSitePayload());

  app.get('/health', async () => ({
    ok: true,
    time: new Date().toISOString(),
    serverless: config.isServerless,
    version: config.niaoVersion,
    mongo: isDbReady(),
  }));

  app.get('/v1/releases/niao', async () => {
    if (isMongoPrimary()) {
      const releases = await listReleasesFromDb();
      const latest = await getLatestReleaseFromDb();
      if (releases.length > 0) {
        return { releases, latest: latest?.version || config.niaoVersion };
      }
    }
    const release = await resolveReleaseDetail(config.niaoVersion);
    if (release) {
      return { releases: [release], latest: release.version };
    }
    return {
      releases: [{ version: config.niaoVersion, status: 'active', is_latest: true }],
      latest: config.niaoVersion,
    };
  });

  app.get('/v1/releases/niao/:version', async (req, reply) => {
    const { version } = req.params;
    const release = await resolveReleaseDetail(version);
    if (release) return release;
    reply.code(404);
    return { error: `release not found: ${version}` };
  });

  app.get('/v1/releases/niao/:version/:variantId', async (req, reply) => {
    const { version, variantId } = req.params;
    const release = await resolveReleaseDetail(version);
    const variant = release?.variants?.find((v) => v.id === variantId);
    if (variant?.url) {
      return reply.redirect(variant.url, 302);
    }
    const platform = DEFAULT_PLATFORMS.find((p) => p.id === variantId);
    const ext = variant?.ext || platform?.ext || 'zip';
    return reply.redirect(`${config.filesUrl}/releases/niao-${version}-${variantId}.${ext}`, 302);
  });

  app.get('/v1/releases/niao/:version/source.tgz', async (req, reply) => {
    const { version } = req.params;
    if (await hostedSourceExists(version)) {
      const file = await hostedSourcePath(version);
      const buf = await fs.readFile(file);
      reply
        .header('Content-Type', 'application/gzip')
        .header('Content-Disposition', `attachment; filename="niao-${version}-toolchain.tgz"`)
        .send(buf);
      return;
    }
    const tag = `v${version}`;
    return reply.redirect(`${config.githubRepo}/archive/refs/tags/${tag}.tar.gz`, 302);
  });

  app.get('/v1/catalog', async (_req, reply) => {
    try {
      return await readCatalog();
    } catch (err) {
      reply.code(404);
      return { error: err.message || 'catalog not found — run npm run seed or sync static files' };
    }
  });

  app.get('/v1/packages', async () => {
    let catalog = null;
    try {
      catalog = await readCatalog();
    } catch {
      // listPackages may still work from mongo/local
    }

    const names = await listPackages();
    const packages = [];
    for (const name of names) {
      try {
        const entry = catalog?.libs?.[name];
        if (entry && isMongoPrimary()) {
          const fromDb = await getPackageFromDb(name);
          if (!fromDb) {
            packages.push({
              ...packageFromCatalogEntry(name, entry),
              latest: entry.versions?.at(-1) || entry.version,
            });
            continue;
          }
        } else if (entry && !isMongoPrimary()) {
          packages.push({
            ...packageFromCatalogEntry(name, entry),
            latest: entry.versions?.at(-1) || entry.version,
          });
          continue;
        }
        const pkg = await readPackageManifest(name);
        const versions = await listVersions(name);
        packages.push({ ...pkg, versions, latest: versions.at(-1) || pkg.version });
      } catch {
        // skip broken entries
      }
    }
    return { packages };
  });

  app.get('/v1/packages/:name', async (req, reply) => {
    const { name } = req.params;
    try {
      const pkg = await readPackageManifest(name);
      const versions = await listVersions(name);
      return { ...pkg, versions, latest: versions.at(-1) || pkg.version };
    } catch {
      reply.code(404);
      return { error: `package not found: ${name}` };
    }
  });

  app.get('/v1/packages/:name/:version', async (req, reply) => {
    const { name, version } = req.params;
    try {
      if (isMongoPrimary()) {
        const ver = await getVersionFromDb(name, version);
        if (ver) {
          return {
            name,
            version,
            status: ver.status,
            package: ver.package,
            lib: ver.lib,
            dist: normalizePublicDist(ver.dist),
          };
        }
      }
      const pkg = await readPackageManifest(name);
      const lib = await readVersionManifest(name, version);
      const tarball = await buildTarball(name, version);
      return {
        name,
        version,
        package: pkg,
        lib,
        dist: {
          tarball: tarball.tarballUrl,
          shasum: tarball.sha256,
          size: tarball.size,
        },
      };
    } catch (err) {
      reply.code(404);
      return { error: err.message || `version not found: ${name}@${version}` };
    }
  });

  app.get('/v1/packages/:name/:version/tarball', async (req, reply) => {
    const { name, version } = req.params;
    try {
      if (isMongoPrimary()) {
        const ver = await getVersionFromDb(name, version);
        const tarballUrl = distTarballUrl(ver?.dist);
        if (tarballUrl && config.remoteReads) {
          return reply.redirect(tarballUrl, 302);
        }
      }
      const tgz = tarballPath(name, version);
      if (await fs.access(tgz).then(() => true).catch(() => false)) {
        const buf = await fs.readFile(tgz);
        reply
          .header('Content-Type', 'application/gzip')
          .header('Content-Disposition', `attachment; filename="${name}-${version}.tgz"`)
          .send(buf);
        return;
      }
      if (config.remoteReads) {
        return reply.redirect(remoteTarballUrl(name, version), 302);
      }
      reply.code(404).send({ error: `tarball not found: ${name}@${version}` });
    } catch {
      reply.code(404).send({ error: `tarball not found: ${name}@${version}` });
    }
  });

  // Static mirror for FTP/CDN-style access
  app.get('/packages/:name/:version/lib.json', async (req, reply) => {
    const { name, version } = req.params;
    const file = path.join(config.packagesDir, name, version, 'lib.json');
    try {
      const data = await fs.readFile(file, 'utf8');
      reply.type('application/json').send(data);
    } catch {
      reply.code(404).send({ error: 'not found' });
    }
  });
}
