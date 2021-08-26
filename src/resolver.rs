use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use futures_retry::{ErrorHandler, FutureRetry, RetryPolicy};
use lazy_static::lazy_static;
use lru::LruCache;
use prometheus::{register_counter, Counter};
use rand::prelude::IteratorRandom;
use tokio::sync::Mutex;
use tracing_unwrap::ResultExt;
use trust_dns_resolver::{
    error::{ResolveError, ResolveErrorKind},
    TokioAsyncResolver,
};

lazy_static! {
    static ref RESOLVES: Counter =
        register_counter!("mcapi_dns_resolves_total", "Total number of DNS resolves").unwrap();
}

/// Get default DNS resolver configuration.
fn get_dns_resolver() -> TokioAsyncResolver {
    TokioAsyncResolver::tokio(
        trust_dns_resolver::config::ResolverConfig::cloudflare(),
        Default::default(),
    )
    .expect_or_log("could not create dns resolver")
}

/// Retry method for DNS requests.
///
/// It will retry up to `max_attempts` times, waiting 1 second between attempts.
struct ResolverRetry {
    max_attempts: usize,
}

impl ResolverRetry {
    /// Create a new `ResolverRetry` with the given number of maximum attempts.
    fn new(max_attempts: usize) -> Self {
        Self { max_attempts }
    }
}

impl ErrorHandler<ResolveError> for ResolverRetry {
    type OutError = ResolveError;

    fn handle(
        &mut self,
        attempt: usize,
        err: ResolveError,
    ) -> futures_retry::RetryPolicy<Self::OutError> {
        if attempt >= self.max_attempts {
            return RetryPolicy::ForwardError(err);
        }

        RetryPolicy::WaitRetry(Duration::from_secs(1))
    }
}

/// A caching resolver for looking up Minecraft-related DNS records.
pub struct Resolver {
    cache: Mutex<LruCache<(String, u16), Option<SocketAddr>>>,
    resolver: TokioAsyncResolver,
}

impl Default for Resolver {
    fn default() -> Self {
        Self {
            cache: Mutex::new(LruCache::new(1024)),
            resolver: get_dns_resolver(),
        }
    }
}

impl Resolver {
    /// Attempt to lookup a host and port into a `SocketAddr`.
    ///
    /// It will retry multiple times if errors occur, then cache the result.
    pub async fn lookup(&self, host: String, port: u16) -> Option<SocketAddr> {
        let entry = (host, port);

        {
            let mut cache = self.cache.lock().await;
            if let Some(addr) = cache.get(&entry) {
                tracing::trace!("had cached socketaddr for {}:{}: {:?}", entry.0, port, addr);
                return addr.to_owned();
            }
        }

        let addr = FutureRetry::new(|| self.resolve(&entry.0, port), ResolverRetry::new(3))
            .await
            .map(|(addr, _attempts)| addr)
            .map_err(|(err, _attempts)| {
                tracing::error!("could not resolve host {:?}", err);
                err
            })
            .ok()
            .flatten();

        tracing::debug!("resolved {}:{}, {:?}", entry.0, port, addr);

        {
            let mut cache = self.cache.lock().await;
            cache.put(entry, addr);
        }

        addr
    }

    /// Attempt to resolve a host and port into a usable `SocketAddr`.
    ///
    /// It first attempts to resolve any potential SRV records then falls back to
    /// using the given host and port.
    async fn resolve(&self, host: &str, port: u16) -> Result<Option<SocketAddr>, ResolveError> {
        let records = self
            .resolve_srv(host)
            .await?
            .into_iter()
            .chain(vec![(host.to_owned(), port)]);

        for (host, port) in records {
            let ip = if let Ok(ip_addr) = host.parse::<IpAddr>() {
                tracing::trace!("host was ip");
                Some(ip_addr)
            } else {
                tracing::trace!("looking up ip for host");
                RESOLVES.inc();
                match self.resolver.lookup_ip(host).await {
                    Ok(ips) => {
                        let ips = ips.into_iter();

                        let mut rng = rand::thread_rng();
                        ips.choose(&mut rng)
                    }
                    Err(err) if matches!(err.kind(), ResolveErrorKind::NoRecordsFound { .. }) => {
                        None
                    }
                    Err(err) => return Err(err),
                }
            };

            if let Some(ip) = ip {
                tracing::debug!("found ip for host: {}", ip);
                return Ok(Some(SocketAddr::new(ip, port)));
            }
        }

        tracing::debug!("found no usable records");
        Ok(None)
    }

    /// Attempt to resolve SRV records for a given host. Returns any discovered
    /// targets and ports.
    async fn resolve_srv(&self, host: &str) -> Result<Vec<(String, u16)>, ResolveError> {
        let name = format!("_minecraft._tcp.{}", host);

        RESOLVES.inc();
        let records = match self.resolver.srv_lookup(name).await {
            Ok(records) => records,
            Err(err) if matches!(err.kind(), ResolveErrorKind::NoRecordsFound { .. }) => {
                return Ok(vec![])
            }
            Err(err) => return Err(err),
        };

        let records = records
            .into_iter()
            .map(|record| (record.target().to_string(), record.port()))
            .collect();

        tracing::trace!("discovered srv records: {:?}", records);
        Ok(records)
    }
}
