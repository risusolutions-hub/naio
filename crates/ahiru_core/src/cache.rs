use dashmap::DashMap;
use niao_db::redis::Client as RedisClient;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum CacheDriver {
    Memory,
    #[cfg(feature = "redis")]
    Redis,
}

#[derive(Clone)]
pub struct CacheManager {
    memory: Arc<DashMap<String, String>>,
    #[cfg(feature = "redis")]
    redis: Option<Arc<Mutex<RedisClient>>>,
    default_driver: CacheDriver,
}

impl CacheManager {
    pub fn memory() -> Self {
        Self {
            memory: Arc::new(DashMap::new()),
            #[cfg(feature = "redis")]
            redis: None,
            default_driver: CacheDriver::Memory,
        }
    }

    #[cfg(feature = "redis")]
    pub async fn connect_redis(url: &str) -> Result<Self, String> {
        let client = RedisClient::open(url).map_err(|e| e.to_string())?;
        Ok(Self {
            memory: Arc::new(DashMap::new()),
            redis: Some(Arc::new(Mutex::new(client))),
            default_driver: CacheDriver::Redis,
        })
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        match self.default_driver {
            CacheDriver::Memory => self.memory.get(key).map(|v| v.clone()),
            #[cfg(feature = "redis")]
            CacheDriver::Redis => {
                if let Some(conn) = &self.redis {
                    let mut c = conn.lock().unwrap();
                    c.get(key).ok().flatten()
                } else {
                    None
                }
            }
        }
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), String> {
        match self.default_driver {
            CacheDriver::Memory => {
                self.memory.insert(key.to_string(), value.to_string());
                Ok(())
            }
            #[cfg(feature = "redis")]
            CacheDriver::Redis => {
                if let Some(conn) = &self.redis {
                    let mut c = conn.lock().unwrap();
                    conn_set(&mut c, key, value)
                } else {
                    Err("redis unavailable (E2301)".into())
                }
            }
        }
    }

    pub async fn incr(&self, key: &str) -> Result<i64, String> {
        match self.default_driver {
            CacheDriver::Memory => {
                let mut entry = self.memory.entry(key.to_string()).or_insert("0".into());
                let n: i64 = entry.parse().unwrap_or(0) + 1;
                *entry = n.to_string();
                Ok(n)
            }
            #[cfg(feature = "redis")]
            CacheDriver::Redis => {
                if let Some(conn) = &self.redis {
                    let mut c = conn.lock().unwrap();
                    c.incr(key, 1).map_err(|e| e.to_string())
                } else {
                    Err("redis unavailable (E2301)".into())
                }
            }
        }
    }

    pub async fn del(&self, key: &str) -> Result<(), String> {
        match self.default_driver {
            CacheDriver::Memory => {
                self.memory.remove(key);
                Ok(())
            }
            #[cfg(feature = "redis")]
            CacheDriver::Redis => {
                if let Some(conn) = &self.redis {
                    let mut c = conn.lock().unwrap();
                    c.del(key).map_err(|e| e.to_string())
                } else {
                    Err("redis unavailable (E2301)".into())
                }
            }
        }
    }

    pub async fn ping(&self) -> bool {
        match self.default_driver {
            CacheDriver::Memory => true,
            #[cfg(feature = "redis")]
            CacheDriver::Redis => {
                if let Some(conn) = &self.redis {
                    let mut c = conn.lock().unwrap();
                    c.ping()
                        .map(|s| s.eq_ignore_ascii_case("PONG"))
                        .unwrap_or(false)
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(feature = "redis")]
fn conn_set(c: &mut RedisClient, key: &str, value: &str) -> Result<(), String> {
    c.set(key, value).map_err(|e| e.to_string())
}

pub type SharedCacheManager = Arc<CacheManager>;
