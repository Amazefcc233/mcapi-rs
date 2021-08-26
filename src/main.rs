use std::{
    future::Future,
    net::SocketAddr,
    time::{Duration, Instant},
};

use actix_cors::Cors;
use actix_web::{
    get,
    http::header::{CacheControl, CacheDirective, ContentType},
    web, App, HttpResponse, HttpServer, Responder,
};
use lazy_static::lazy_static;
use prometheus::{register_histogram_vec, HistogramVec};
use redis::{AsyncCommands, Client as RedisClient};
use redlock::RedLock;
use tokio::time::timeout;
use tracing_actix_web::TracingLogger;

use resolver::Resolver;
use types::Error;

const TIMEOUT_DURATION: Duration = Duration::from_secs(5);
const MAX_AGE: u64 = 60 * 5;

mod image;
mod protocol;
mod resolver;
mod types;

lazy_static! {
    static ref UPDATE_DURATION: HistogramVec = register_histogram_vec!(
        "mcapi_update_duration_seconds",
        "Duration to update a server",
        &["method"]
    )
    .unwrap();
    static ref REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "mcapi_request_duration_seconds",
        "Total duration for a request",
        &["method"]
    )
    .unwrap();
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerRequest {
    #[serde(rename = "ip")]
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ServerImageRequest {
    #[serde(flatten)]
    pub server_request: ServerRequest,

    pub title: Option<String>,
    pub theme: Option<image::Theme>,
}

#[get("/server/status")]
async fn server_status(
    resolver: web::Data<Resolver>,
    redis: web::Data<RedisClient>,
    redlock: web::Data<RedLock>,
    web::Query(ServerRequest { host, port }): web::Query<ServerRequest>,
) -> impl Responder {
    let _timer = REQUEST_DURATION.with_label_values(&["ping"]).start_timer();

    let port = port.unwrap_or(25565);

    tracing::info!("attempting to get server status for {}:{}", host, port);

    let data = get_ping(&redis, &redlock, &resolver, host, port).await;

    HttpResponse::Ok()
        .insert_header(get_cache_control())
        .json(data)
}

#[get("/server/query")]
async fn server_query(
    resolver: web::Data<Resolver>,
    redis: web::Data<RedisClient>,
    redlock: web::Data<RedLock>,
    web::Query(ServerRequest { host, port }): web::Query<ServerRequest>,
) -> impl Responder {
    let _timer = REQUEST_DURATION.with_label_values(&["query"]).start_timer();

    let port = port.unwrap_or(25565);

    tracing::info!("attempting to get server query for {}:{}", host, port);

    let data = get_query(&redis, &redlock, &resolver, host, port).await;

    HttpResponse::Ok()
        .insert_header(get_cache_control())
        .json(data)
}

#[get("/server/image")]
async fn server_image(
    resolver: web::Data<Resolver>,
    redis: web::Data<RedisClient>,
    redlock: web::Data<RedLock>,
    web::Query(req): web::Query<ServerImageRequest>,
) -> impl Responder {
    let _timer = REQUEST_DURATION.with_label_values(&["image"]).start_timer();

    let port = req.server_request.port.unwrap_or(25565);

    tracing::info!(
        "attempting to get server image for {}:{}",
        req.server_request.host,
        port
    );

    let data = get_ping(
        &redis,
        &redlock,
        &resolver,
        req.server_request.host.clone(),
        port,
    )
    .await;

    let image = actix_web::rt::task::spawn_blocking(move || image::server_image(&req, data))
        .await
        .unwrap();

    HttpResponse::Ok()
        .insert_header(get_cache_control())
        .insert_header(ContentType::png())
        .body(image)
}

#[get("/server/icon")]
async fn server_icon(
    resolver: web::Data<Resolver>,
    redis: web::Data<RedisClient>,
    redlock: web::Data<RedLock>,
    web::Query(ServerRequest { host, port }): web::Query<ServerRequest>,
) -> impl Responder {
    let _timer = REQUEST_DURATION.with_label_values(&["icon"]).start_timer();

    let port = port.unwrap_or(25565);

    tracing::info!("attempting to get server icon for {}:{}", host, port);

    let data = get_ping(&redis, &redlock, &resolver, host.clone(), port).await;

    let icon = image::encode_png(image::server_icon(&data.favicon));

    HttpResponse::Ok()
        .insert_header(get_cache_control())
        .insert_header(ContentType::png())
        .body(icon)
}

#[get("/health")]
async fn health() -> impl Responder {
    "OK"
}

#[get("/metrics")]
async fn metrics() -> impl Responder {
    use prometheus::Encoder;

    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok().body(buffer)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    tracing::info!("starting mcapi-rs");

    let listen: SocketAddr = std::env::var("HTTP_HOST")
        .unwrap_or_else(|_err| "0.0.0.0:8080".to_string())
        .parse()
        .unwrap();

    tracing::info!("will listen on {}", listen);

    let redis_servers = std::env::var("REDIS_SERVER").unwrap();
    let redis_servers: Vec<_> = redis_servers.split(',').collect();

    let resolver = web::Data::new(Resolver::default());
    let redis = web::Data::new(RedisClient::open(redis_servers[0]).unwrap());
    let redlock = web::Data::new(RedLock::new(redis_servers));

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(["GET"])
            .allow_any_header()
            .max_age(86400);

        let scripts = actix_files::Files::new("/scripts", "./static/scripts").show_files_listing();
        let site = actix_files::Files::new("/site", "./static/site");

        App::new()
            .wrap(TracingLogger::default())
            .wrap(cors)
            .app_data(resolver.clone())
            .app_data(redis.clone())
            .app_data(redlock.clone())
            .service(server_status)
            .service(server_query)
            .service(server_image)
            .service(server_icon)
            .service(health)
            .service(metrics)
            .service(scripts)
            .service(site)
            .route(
                "/",
                web::get().to(|| -> HttpResponse {
                    HttpResponse::Ok().body(include_str!("../static/site/index.html"))
                }),
            )
    })
    .bind(listen)?
    .run()
    .await
}

/// Get standard cache-control directives.
fn get_cache_control() -> CacheControl {
    CacheControl(vec![
        CacheDirective::Public,
        CacheDirective::MaxAge(60),
        CacheDirective::MaxStale(60),
    ])
}

/// Get the current unix timestamp, as seconds.
fn unix_timestamp() -> u64 {
    let start = std::time::SystemTime::now();
    let since = start.duration_since(std::time::UNIX_EPOCH).unwrap();
    since.as_secs() as u64
}

/// Attempt to get data cached in Redis.
///
/// If the key cannot be found or is older than the max age, it will call the
/// function to calculate the value, then save that value into the same key.
///
/// It locks the key so the value should only be updated exactly once.
async fn get_cached_data<D, F, Fut>(
    redis: &RedisClient,
    locker: &RedLock,
    key: &str,
    max_age: u64,
    f: F,
) -> Result<D, Error>
where
    D: Clone + From<Error> + types::Metadata + serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<D, Error>>,
{
    let mut con = redis.get_async_connection().await?;

    // Check if we already have fresh data in cache. If we do, return that.
    if let Some(value) = con.get::<_, Option<Vec<u8>>>(key).await? {
        tracing::trace!("already had value for {} in cache", key);
        let data: D = serde_json::from_slice(&value)?;

        if data.updated_at() >= unix_timestamp() - max_age {
            tracing::trace!("data is fresh");
            return Ok(data);
        }
    }

    // Get exclusive lock to try and update this key.
    let lock_key = format!("lock:{}", key);
    tracing::debug!("wanting to compute new value, requesting lock {}", lock_key);

    let lock = loop {
        if let Some(lock) = locker
            .lock(lock_key.as_bytes(), TIMEOUT_DURATION.as_millis() as usize)
            .await
        {
            break lock;
        }
    };

    tracing::trace!("obtained lock {}", lock_key);

    // Make sure potential previous lock owner did not already refresh data.
    if let Some(value) = con.get::<_, Option<Vec<u8>>>(key).await? {
        let data: D = serde_json::from_slice(&value)?;

        if data.updated_at() >= unix_timestamp() - max_age {
            tracing::debug!("data was already updated");
            locker.unlock(&lock).await;
            return Ok(data);
        }
    }

    // Update data and store in cache.
    let now = Instant::now();
    let data = f().await.unwrap_or_else(D::from);
    let elapsed = now.elapsed();

    // Set when this request was completed and how long it took to complete.
    let data = data.set_times(unix_timestamp(), elapsed.as_nanos() as u64);

    UPDATE_DURATION
        .with_label_values(&[D::NAME])
        .observe(elapsed.as_secs_f64());

    let value = serde_json::to_vec(&data)?;
    con.set_ex::<_, _, ()>(key, value, 300).await?;

    locker.unlock(&lock).await;

    Ok(data)
}

/// Perform a server ping if not already cached, using default ages and
/// timeouts.
async fn get_ping(
    redis: &RedisClient,
    redlock: &RedLock,
    resolver: &Resolver,
    host: String,
    port: u16,
) -> types::ServerPing {
    get_cached_data(
        redis,
        redlock,
        &format!("ping:{}:{}", host, port),
        MAX_AGE,
        || async {
            let addr = resolver
                .lookup(host.clone(), port)
                .await
                .ok_or(Error::ResolveFailed)?;

            let data = timeout(TIMEOUT_DURATION, protocol::send_ping(addr, &host, port)).await??;

            Ok(types::ServerPing::from(data))
        },
    )
    .await
    .unwrap_or_else(From::from)
}

/// Perform a server query if not already cached, using default ages and
/// timeouts.
async fn get_query(
    redis: &RedisClient,
    redlock: &RedLock,
    resolver: &Resolver,
    host: String,
    port: u16,
) -> types::ServerQuery {
    get_cached_data(
        redis,
        redlock,
        &format!("query:{}:{}", host, port),
        MAX_AGE,
        || async {
            let addr = resolver
                .lookup(host.clone(), port)
                .await
                .ok_or(Error::ResolveFailed)?;

            let data = timeout(TIMEOUT_DURATION, protocol::send_query(addr)).await??;

            Ok(types::ServerQuery::from(data))
        },
    )
    .await
    .unwrap_or_else(From::from)
}
