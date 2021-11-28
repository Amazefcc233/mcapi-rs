use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid port {0}")]
    InvalidPort(u16),
    #[error("could not resolve host")]
    ResolveFailed,
    #[error("timeout: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("protocol error: {0}")]
    Protocol(#[from] crate::protocol::Error),
}

pub trait Metadata {
    const NAME: &'static str;

    fn updated_at(&self) -> u64;
    fn set_times(self, last_updated: u64, duration: u64) -> Self;
    fn is_online(&self) -> bool;
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ServerPingPlayers {
    pub max: i32,
    pub now: i32,
    pub sample: Vec<crate::protocol::PlayerSample>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ServerPingServer {
    pub name: Option<String>,
    pub protocol: i32,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ServerPing {
    pub status: String,
    pub online: bool,

    pub motd: String,
    pub motd_json: serde_json::Value,

    pub favicon: Option<String>,
    pub error: Option<String>,

    pub players: ServerPingPlayers,
    pub server: ServerPingServer,

    #[serde(with = "string")]
    pub last_updated: u64,

    #[serde(with = "string")]
    pub duration: u64,
}

impl Metadata for ServerPing {
    const NAME: &'static str = "ping";

    fn updated_at(&self) -> u64 {
        self.last_updated
    }

    fn set_times(mut self, last_updated: u64, duration: u64) -> Self {
        self.last_updated = last_updated;
        self.duration = duration;

        self
    }

    fn is_online(&self) -> bool {
        self.online
    }
}

impl From<crate::protocol::Ping> for ServerPing {
    fn from(data: crate::protocol::Ping) -> Self {
        Self {
            status: "success".to_string(),
            online: true,
            motd: data.get_motd().unwrap_or_default(),
            motd_json: data.description,
            favicon: data.favicon,
            error: None,
            players: ServerPingPlayers {
                max: data.players.max,
                now: data.players.online,
                sample: data.players.sample.unwrap_or_default(),
            },
            server: ServerPingServer {
                name: data.version.name,
                protocol: data.version.protocol,
            },
            last_updated: 0,
            duration: 0,
        }
    }
}

impl From<Error> for ServerPing {
    fn from(err: Error) -> Self {
        Self {
            online: false,
            status: "error".to_string(),
            error: Some(err.to_string()),
            ..Default::default()
        }
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ServerQueryPlayers {
    pub max: usize,
    pub now: usize,
    pub list: Vec<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct ServerQuery {
    pub status: String,
    pub online: bool,
    pub error: Option<String>,

    pub server_mod: String,
    pub plugins: Vec<String>,

    pub players: ServerQueryPlayers,
    #[serde(flatten)]
    pub kv: std::collections::HashMap<String, String>,

    #[serde(with = "string")]
    pub last_updated: u64,

    #[serde(with = "string")]
    pub duration: u64,
}

impl Metadata for ServerQuery {
    const NAME: &'static str = "query";

    fn updated_at(&self) -> u64 {
        self.last_updated
    }

    fn set_times(mut self, last_updated: u64, duration: u64) -> Self {
        self.last_updated = last_updated;
        self.duration = duration;

        self
    }

    fn is_online(&self) -> bool {
        self.online
    }
}

impl ServerQuery {
    fn extract_data(
        mut kv: std::collections::HashMap<String, String>,
        players: Vec<String>,
    ) -> (
        std::collections::HashMap<String, String>,
        ServerQueryPlayers,
    ) {
        let numplayers = kv
            .remove("numplayers")
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .unwrap_or_default();
        let maxplayers = kv
            .remove("maxplayers")
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .unwrap_or_default();

        (
            kv,
            ServerQueryPlayers {
                now: numplayers,
                max: maxplayers,
                list: players,
            },
        )
    }
}

impl From<crate::protocol::Query> for ServerQuery {
    fn from(data: crate::protocol::Query) -> Self {
        let (kv, players) = Self::extract_data(data.kv, data.players);

        Self {
            status: "success".to_string(),
            error: None,
            online: true,

            server_mod: data.server.0,
            plugins: data.server.1,
            kv,
            players,

            last_updated: 0,
            duration: 0,
        }
    }
}

impl From<Error> for ServerQuery {
    fn from(err: Error) -> Self {
        Self {
            online: false,
            status: "error".to_string(),
            error: Some(err.to_string()),
            ..Default::default()
        }
    }
}

pub mod string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
