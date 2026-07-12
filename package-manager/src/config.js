import dotenv from 'dotenv';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

dotenv.config();

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '..');

function isServerless() {
  return Boolean(
    process.env.VERCEL ||
      process.env.AWS_LAMBDA_FUNCTION_NAME ||
      process.env.LAMBDA_TASK_ROOT ||
      process.env.AWS_EXECUTION_ENV ||
      process.env.FUNCTIONS_WORKER_RUNTIME,
  );
}

function resolveDataDir() {
  const envDir = process.env.DATA_DIR;
  const serverless = isServerless();

  if (serverless) {
    // /var/task is read-only — only /tmp is writable on Lambda/Vercel
    if (!envDir || envDir === './data' || envDir === 'data' || envDir.startsWith('/var/task')) {
      return path.join(os.tmpdir(), 'niao-nms', 'data');
    }
    return path.isAbsolute(envDir) ? envDir : path.resolve(root, envDir);
  }

  if (envDir) {
    return path.isAbsolute(envDir) ? envDir : path.resolve(root, envDir);
  }
  return path.resolve(root, 'data');
}

const dataDir = resolveDataDir();
const serverless = isServerless();

export const config = {
  port: Number(process.env.PORT || 3000),
  host: process.env.HOST || '0.0.0.0',
  /** API + admin server (Node Fastify) */
  apiUrl: (process.env.API_URL || process.env.PUBLIC_URL || 'http://localhost:3000').replace(/\/$/, ''),
  /** Static package files (FTP / CDN) */
  filesUrl: (process.env.FILES_URL || process.env.API_URL || process.env.PUBLIC_URL || 'http://localhost:3000').replace(/\/$/, ''),
  nodeEnv: process.env.NODE_ENV || 'development',
  isServerless: serverless,
  /** When true, registry reads fall back to FILES_URL if local data is missing */
  remoteReads: serverless || process.env.REMOTE_READS === 'true',
  root,
  dataDir,
  packagesDir: path.join(dataDir, 'packages'),
  tarballsDir: path.join(dataDir, 'tarballs'),
  catalogPath: path.join(dataDir, 'catalog.json'),
  admin: {
    username: process.env.ADMIN_USERNAME || 'admin',
    password: process.env.ADMIN_PASSWORD || '',
    jwtSecret: process.env.JWT_SECRET || 'dev-only-change-in-production',
    jwtExpires: process.env.JWT_EXPIRES || '8h',
  },
  ftp: {
    host: process.env.FTP_HOST || '',
    user: process.env.FTP_USER || '',
    password: process.env.FTP_PASSWORD || '',
    remoteDir: process.env.FTP_REMOTE_DIR || '/public_html',
    secure: process.env.FTP_SECURE === 'true',
    passive: process.env.FTP_PASSIVE !== 'false',
    timeoutMs: Number(process.env.FTP_TIMEOUT_MS || 90_000),
    autoSync: process.env.FTP_AUTO_SYNC !== 'false',
    /** Parallel FTP connections for small files (lib.json, package.json, etc.) */
    concurrency: Math.max(1, Math.min(32, Number(process.env.FTP_CONCURRENCY || 24))),
    /** Files at or below this size upload in parallel; larger files go one-by-one */
    smallFileMaxBytes: Number(process.env.FTP_SMALL_FILE_MAX || 512 * 1024),
    /** Delay between parallel worker logins (ms) — avoids connection bursts on shared hosts */
    loginStaggerMs: Number(process.env.FTP_LOGIN_STAGGER_MS || 150),
    /** Retries for connect / upload on transient network errors */
    retryAttempts: Math.max(1, Number(process.env.FTP_RETRY_ATTEMPTS || 3)),
    /** Parallel connections while creating remote directories before upload */
    dirConcurrency: Math.max(1, Math.min(12, Number(process.env.FTP_DIR_CONCURRENCY || 8))),
  },
  niaoVersion: process.env.NIAO_VERSION || '0.2.3',
  websiteUrl: (process.env.NIAO_WEBSITE_URL || process.env.NEXT_PUBLIC_SITE_URL || 'https://niao.risu.in').replace(/\/$/, ''),
  social: {
    discord: process.env.NIAO_DISCORD_URL || process.env.NEXT_PUBLIC_DISCORD_URL || 'https://discord.gg/XwmcDqxtm',
    instagram: process.env.NIAO_INSTAGRAM_URL || process.env.NEXT_PUBLIC_INSTAGRAM_URL || 'https://www.instagram.com/risusolutions/',
    linkedin: process.env.NIAO_LINKEDIN_URL || process.env.NEXT_PUBLIC_LINKEDIN_URL || 'https://www.linkedin.com/company/risu-solutions/',
    x: process.env.NIAO_X_URL || process.env.NEXT_PUBLIC_X_URL || 'https://x.com/RisuSolutions',
  },
  githubRepo: (process.env.NIAO_GITHUB_REPO || 'https://github.com/vivekgorasiya98/naio').replace(/\/$/, ''),
  releaseBinaries: {
    windows: process.env.NIAO_RELEASE_WINDOWS || '',
    linux: process.env.NIAO_RELEASE_LINUX || '',
    linux_arm64: process.env.NIAO_RELEASE_LINUX_ARM || '',
    macos: process.env.NIAO_RELEASE_MACOS || '',
    macos_arm64: process.env.NIAO_RELEASE_MACOS_ARM || '',
  },
  mongo: {
    uri: process.env.MONGODB_URI || '',
    dbName: process.env.MONGODB_DB || 'niao-nms',
  },
};
