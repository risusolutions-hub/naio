//! Hugging Face Hub wrapper over the in-tree `hf-hub` crate.

use crate::checksum::{hash_file, verify_file, HashAlgo};
use crate::error::HubResult;
use hf_hub::api::sync::{Api, ApiBuilder, ApiRepo};
use hf_hub::api::RepoInfo;
use hf_hub::{Cache, Repo, RepoType};
use niao_glob::match_str;
use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct HubConfig {
    pub cache_dir: Option<PathBuf>,
    pub token: Option<String>,
    pub endpoint: Option<String>,
    pub retries: usize,
    pub progress: bool,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            cache_dir: None,
            token: None,
            endpoint: None,
            retries: 3,
            progress: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HubClient {
    api: Api,
    cache: Cache,
}

impl HubClient {
    pub fn new(config: HubConfig) -> HubResult<Self> {
        let cache = config
            .cache_dir
            .map(Cache::new)
            .unwrap_or_else(|| Cache::from_env());
        let mut builder = ApiBuilder::from_cache(cache.clone())
            .with_progress(config.progress)
            .with_retries(config.retries);
        if let Some(token) = config.token {
            builder = builder.with_token(Some(token));
        }
        if let Some(endpoint) = config.endpoint {
            builder = builder.with_endpoint(endpoint);
        }
        let api = builder.build()?;
        Ok(Self { api, cache })
    }

    pub fn from_env() -> HubResult<Self> {
        Self::new(HubConfig::default())
    }

    pub fn cache_dir(&self) -> &Path {
        self.cache.path()
    }

    pub fn token(&self) -> Option<String> {
        self.cache.token()
    }

    pub fn model(&self, repo_id: &str, revision: Option<&str>) -> HubRepo {
        HubRepo::new(
            self.api.clone(),
            self.cache.clone(),
            repo_id,
            RepoType::Model,
            revision,
        )
    }

    pub fn dataset(&self, repo_id: &str, revision: Option<&str>) -> HubRepo {
        HubRepo::new(
            self.api.clone(),
            self.cache.clone(),
            repo_id,
            RepoType::Dataset,
            revision,
        )
    }

    pub fn space(&self, repo_id: &str, revision: Option<&str>) -> HubRepo {
        HubRepo::new(
            self.api.clone(),
            self.cache.clone(),
            repo_id,
            RepoType::Space,
            revision,
        )
    }
}

/// Default HF cache directory (`~/.cache/huggingface/hub` or `$HF_HOME/hub`).
pub fn default_cache_dir() -> PathBuf {
    Cache::default().path().clone()
}

/// Cache directory from `$HF_HOME` when set.
pub fn cache_dir_from_env() -> PathBuf {
    Cache::from_env().path().clone()
}

#[derive(Debug, Clone)]
pub struct HubRepo {
    api: Api,
    cache: Cache,
    repo: Repo,
    repo_id: String,
    repo_type: RepoType,
}

impl HubRepo {
    fn new(
        api: Api,
        cache: Cache,
        repo_id: &str,
        repo_type: RepoType,
        revision: Option<&str>,
    ) -> Self {
        let rev = revision.unwrap_or("main").to_string();
        let repo = Repo::with_revision(repo_id.to_string(), repo_type, rev);
        Self {
            api,
            cache,
            repo,
            repo_id: repo_id.to_string(),
            repo_type,
        }
    }

    fn api_repo(&self) -> ApiRepo {
        self.api.repo(self.repo.clone())
    }

    pub fn repo_id(&self) -> &str {
        &self.repo_id
    }

    pub fn revision(&self) -> &str {
        self.repo.revision()
    }

    pub fn repo_type(&self) -> RepoType {
        self.repo_type
    }

    pub fn kind_name(&self) -> &'static str {
        match self.repo_type {
            RepoType::Model => "model",
            RepoType::Dataset => "dataset",
            RepoType::Space => "space",
        }
    }

    pub fn file_url(&self, filename: &str) -> String {
        self.api_repo().url(filename)
    }

    pub fn info(&self) -> HubResult<RepoInfo> {
        Ok(self.api_repo().info()?)
    }

    pub fn list_files(&self) -> HubResult<Vec<String>> {
        Ok(self
            .info()?
            .siblings
            .into_iter()
            .map(|s| s.rfilename)
            .collect())
    }

    pub fn cached_path(&self, filename: &str) -> Option<PathBuf> {
        self.cache.repo(self.repo.clone()).get(filename)
    }

    pub fn download(&self, filename: &str) -> HubResult<DownloadResult> {
        let cached_before = self.cached_path(filename).is_some();
        let path = self.api_repo().get(filename)?;
        let bytes = std::fs::metadata(&path)?.len();
        Ok(DownloadResult {
            path,
            bytes,
            cached: cached_before,
        })
    }

    pub fn snapshot_download(&self, opts: &SnapshotOpts) -> HubResult<SnapshotResult> {
        let files = self.list_files()?;
        let selected: Vec<String> = files
            .into_iter()
            .filter(|f| matches_patterns(f, &opts.allow_patterns, &opts.ignore_patterns))
            .collect();
        let mut paths = Vec::with_capacity(selected.len());
        let mut total_bytes = 0u64;
        for filename in &selected {
            let dl = self.download(filename)?;
            total_bytes += dl.bytes;
            paths.push(dl.path);
        }
        Ok(SnapshotResult {
            paths,
            count: selected.len(),
            bytes: total_bytes,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotOpts {
    pub allow_patterns: Vec<String>,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub path: PathBuf,
    pub bytes: u64,
    pub cached: bool,
}

#[derive(Debug, Clone)]
pub struct SnapshotResult {
    pub paths: Vec<PathBuf>,
    pub count: usize,
    pub bytes: u64,
}

fn matches_patterns(path: &str, allow: &[String], ignore: &[String]) -> bool {
    if !ignore.is_empty() {
        for pat in ignore {
            if match_str(path, pat, false).unwrap_or(false) {
                return false;
            }
        }
    }
    if allow.is_empty() {
        return true;
    }
    allow
        .iter()
        .any(|pat| match_str(path, pat, false).unwrap_or(false))
}

/// Verify a downloaded file against an expected digest.
pub fn verify_path(path: &Path, expected: &str, algo: HashAlgo) -> HubResult<bool> {
    verify_file(path, expected, algo)
}

/// SHA-256 hex digest of a file on disk.
pub fn file_sha256(path: &Path) -> HubResult<String> {
    hash_file(path, HashAlgo::Sha256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cache_dir_exists() {
        let p = default_cache_dir();
        assert!(p.to_string_lossy().contains("huggingface"));
    }

    #[test]
    fn pattern_filter() {
        assert!(matches_patterns("model.safetensors", &[], &[]));
        assert!(matches_patterns(
            "model.safetensors",
            &["*.safetensors".into()],
            &[]
        ));
        assert!(!matches_patterns(
            "tokenizer.json",
            &["*.safetensors".into()],
            &[]
        ));
        assert!(!matches_patterns(
            "data/train.parquet",
            &[],
            &["*.parquet".into()]
        ));
    }

    #[test]
    fn file_url_shape() {
        let client = HubClient::new(HubConfig {
            progress: false,
            ..Default::default()
        })
        .unwrap();
        let repo = client.model("gpt2", None);
        let url = repo.file_url("config.json");
        assert!(url.contains("huggingface.co"));
        assert!(url.contains("gpt2"));
        assert!(url.contains("config.json"));
    }
}
