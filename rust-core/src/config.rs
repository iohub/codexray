use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::services::hybrid_search::HybridSearchConfig as SvcHybridConfig;

/// 全文检索配置（用户可见的 config.json schema）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HybridSearchConfig {
    #[serde(default = "default_true")]
    pub enable_bm25: bool,
    #[serde(default = "default_100")]
    pub bm25_top_k: usize,
    #[serde(default = "default_100")]
    pub vector_top_k: usize,
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f64,
    #[serde(default = "default_20")]
    pub rrf_top_k: usize,
    #[serde(default = "default_short_code_threshold")]
    pub short_code_threshold: usize,
    #[serde(default = "default_short_code_penalty")]
    pub short_code_penalty: f64,
    /// Timeout for the entire hybrid search operation (milliseconds). 0 = no timeout.
    #[serde(default)]
    pub timeout_ms: u64,
}

fn default_true() -> bool { true }
fn default_100() -> usize { 100 }
fn default_20() -> usize { 20 }
fn default_rrf_k() -> f64 { 60.0 }
fn default_short_code_threshold() -> usize { 30 }
fn default_short_code_penalty() -> f64 { 0.5 }

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            enable_bm25: default_true(),
            bm25_top_k: default_100(),
            vector_top_k: default_100(),
            rrf_k: default_rrf_k(),
            rrf_top_k: default_20(),
            short_code_threshold: default_short_code_threshold(),
            short_code_penalty: default_short_code_penalty(),
            timeout_ms: 0,
        }
    }
}

impl From<&HybridSearchConfig> for SvcHybridConfig {
    fn from(c: &HybridSearchConfig) -> Self {
        SvcHybridConfig {
            enable_sparse: c.enable_bm25,
            rrf_k: c.rrf_k,
            dense_limit: c.vector_top_k,
            sparse_limit: c.bm25_top_k,
            rrf_top_k: c.rrf_top_k,
            short_code_threshold: c.short_code_threshold,
            short_code_penalty: c.short_code_penalty,
            timeout_ms: c.timeout_ms,
        }
    }
}

/// 嵌入配置
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_provider() -> String { "openai-compatible".to_string() }
fn default_model() -> String { "Qwen/Qwen3-Embedding-4B".to_string() }
fn default_api_base_url() -> String { "https://api.siliconflow.cn/v1".to_string() }
fn default_dimensions() -> usize { 2560 }

/// 索引配置
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IndexConfig {
    #[serde(default = "default_min_code_block_length")]
    pub min_code_block_length: usize,
    #[serde(default)]
    pub hybrid: HybridSearchConfig,
    #[serde(default)]
    pub reranker: RerankerConfig,
}

fn default_min_code_block_length() -> usize { 16 }

/// 重排序配置
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RerankerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_reranker_model")]
    pub model: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default = "default_reranker_api_base_url")]
    pub api_base_url: String,
    #[serde(default = "default_10")]
    pub top_n: usize,
    #[serde(default = "default_5")]
    pub candidate_multiplier: usize,
    #[serde(default = "default_30")]
    pub timeout_secs: u64,
}

fn default_reranker_model() -> String { "Qwen/Qwen3-Reranker-4B".to_string() }
fn default_reranker_api_base_url() -> String { "https://api.siliconflow.cn/v1/rerank".to_string() }
fn default_10() -> usize { 10 }
fn default_5() -> usize { 5 }
fn default_30() -> u64 { 30 }

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            min_code_block_length: default_min_code_block_length(),
            hybrid: HybridSearchConfig::default(),
            reranker: RerankerConfig::default(),
        }
    }
}

/// 全局配置（~/.codexray/config.json）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub index: IndexConfig,
    /// 已安装 git hooks 的项目 { project_path → [hook_names] }
    #[serde(default)]
    pub installed_hooks: HashMap<String, Vec<String>>,
}

impl Config {
    /// 全局配置文件路径
    pub fn global_config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray").join("config.json")
    }

    /// 二进制存储目录
    pub fn bin_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray").join("bin")
    }

    /// 项目索引目录: ~/.codexray/projects/<project_hash>/
    pub fn project_index_dir(project_hash: &str) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray").join("projects").join(project_hash)
    }

    /// Codexray 全局配置根目录: ~/.codexray/
    pub fn codexray_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray")
    }

    /// 项目数据根目录: ~/.codexray/projects/
    pub fn projects_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray").join("projects")
    }

    /// 项目 LanceDB 向量数据库目录: ~/.codexray/projects/<project_hash>/lancedb/
    pub fn lancedb_dir(project_hash: &str) -> PathBuf {
        Self::project_index_dir(project_hash).join("lancedb")
    }

    /// 项目 BM25 全文索引目录: ~/.codexray/projects/<project_hash>/tantivy_bm25/
    pub fn bm25_dir(project_hash: &str) -> PathBuf {
        Self::project_index_dir(project_hash).join("tantivy_bm25")
    }

    /// 全局缓存目录: ~/.codexray/cache/
    pub fn cache_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".codexray").join("cache")
    }

    /// 加载全局配置
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::global_config_path();
        info!("Loading configuration from: {:?}", config_path);

        if !config_path.exists() {
            return Err(format!(
                "Config file not found at {:?}. Run 'codexray' to set up.",
                config_path
            ).into());
        }

        let contents = fs::read_to_string(&config_path).map_err(|e| {
            format!("Failed to read config file at {:?}: {}", config_path, e)
        })?;

        let config: Config = serde_json::from_str(&contents).map_err(|e| {
            format!("Failed to parse config file: {}", e)
        })?;

        Ok(config)
    }

    /// 保存全局配置
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::global_config_path();

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, contents)?;

        info!("Configuration saved to: {:?}", config_path);
        Ok(())
    }

    /// 从当前目录检测项目根（向上找第一个 .git/ 目录）
    pub fn detect_project_root() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;
        loop {
            if current.join(".git").exists() {
                return Some(current);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// 计算项目 hash
    pub fn compute_project_hash(project_root: &PathBuf) -> String {
        format!("{:x}", md5::compute(project_root.to_string_lossy().as_bytes()))
    }
}
