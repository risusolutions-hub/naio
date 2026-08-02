/**
 * Sync release metadata (manifest → index → Mongo → static mirror → optional FTP).
 * Run after uploading binaries when you skip a full `npm run release`.
 */
import dotenv from 'dotenv';
import { config } from '../config.js';
import { connectMongo, closeMongo } from '../db/mongo.js';
import { RELEASE_STATUS, upsertReleaseDoc } from '../db/registry-db.js';
import { buildStaticApiMirror } from '../services/static-api.js';
import { ftpConfigured, syncToFtp } from '../services/ftp.js';
import { readReleaseIndex, readReleaseManifest } from '../services/release-index.js';
import { bootstrapReleaseHistory } from '../services/release-bootstrap.js';
import { clearReleaseCache } from '../services/release-registry.js';
import { clearRemoteCache } from '../services/remote-registry.js';

dotenv.config();

async function syncMongoReleases(index) {
  if (!config.mongo.uri) {
    console.log('  ⊘ Mongo not configured');
    return;
  }

  try {
    await connectMongo();
    for (const release of index?.releases || []) {
      if ((release.status || RELEASE_STATUS.ACTIVE) === RELEASE_STATUS.DRAFT) continue;
      await upsertReleaseDoc({
        version: release.version,
        status: release.status || RELEASE_STATUS.ACTIVE,
        is_latest: release.is_latest === true,
        changelog: release.changelog || `Release ${release.version}`,
        variants: release.variants || [],
        source: release.source,
      });
    }
    console.log(`  ✓ Mongo niao_releases (${index.releases.length} versions)`);
  } catch (err) {
    console.warn(`  ⊘ Mongo: ${err.message}`);
  } finally {
    await closeMongo().catch(() => {});
  }
}

async function main() {
  const manifest = await readReleaseManifest();
  if (!manifest?.version) {
    throw new Error('releases/manifest.json not found — run npm run write-manifest first');
  }

  console.log(`\nSync release metadata for v${manifest.version}\n`);

  const index = await bootstrapReleaseHistory();
  console.log(
    `  ✓ index.json (${index.releases.length} releases: ${index.releases.map((r) => r.version).join(', ')}, latest ${index.latest})`,
  );

  await syncMongoReleases(index);

  await buildStaticApiMirror();
  console.log('  ✓ static v1 mirror');

  clearReleaseCache();
  clearRemoteCache();

  if (process.argv.includes('--ftp') || process.env.FTP_AUTO_SYNC === 'true') {
    if (!ftpConfigured()) {
      console.log('  ⊘ FTP not configured');
    } else {
      const result = await syncToFtp();
      console.log(`  ✓ FTP (${result.uploaded} files)`);
    }
  } else {
    console.log('  tip: pass --ftp to upload to nm.c4compare.com');
  }

  console.log('\nDone.\n');
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
