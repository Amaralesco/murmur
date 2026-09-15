use envconfig::Envconfig;
use std::num::NonZeroU64;
use std::path::PathBuf;
use tracing::Level;

#[derive(Envconfig)]
pub struct Config {
    #[envconfig(default = "info")]
    pub log_level: Level,

    pub zone: String,

    pub jetstream_collections: String,

    #[envconfig(default = "1")]
    pub base_delay_secs: u64,

    #[envconfig(default = "300")]
    pub max_delay_secs: u64,

    #[envconfig(default = "2")]
    pub backoff_multiplier: NonZeroU64,

    #[envconfig(default = "5")]
    pub missed_pings_tolerance: NonZeroU64,
    // Todo: Make sure number of missed_pings_tolerance is at least 2

    #[envconfig(default = "data/readings")]
    pub readings_dir: PathBuf,
}

impl Config {
    /// `kinds=commit` is deliberately fixed: the `Commit` payload struct is only
    /// sound while that is the sole event kind requested.
    pub fn jetstream_url(&self) -> String {
        format!(
            "wss://jetstream.{}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={}",
            self.zone, self.jetstream_collections
        )
    }
}
