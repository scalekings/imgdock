mod config;
mod handlers;
mod models;

use actix_cors::Cors;
use actix_web::{middleware::Logger, web, App, HttpServer};
use aws_sdk_s3::Client as S3Client;
use fred::prelude::*;
use mongodb::Client as MongoClient;

use config::Config;
use handlers::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    
    // Load .env
    dotenvy::dotenv().ok();

    let config = Config::from_env();
    let port = config.port;

    log::info!("🔌 Connecting to services...");

    // ============ S3 Client (R2) ============
    let s3_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(&config.r2_endpoint)
        .credentials_provider(aws_credential_types::Credentials::new(
            &config.r2_access_key,
            &config.r2_secret_key,
            None,
            None,
            "r2",
        ))
        .region(aws_config::Region::new("auto"))
        .load()
        .await;

    let s3 = S3Client::new(&s3_config);

    // ============ MongoDB ============
    let mongo = MongoClient::with_uri_str(&config.mongo_uri)
        .await
        .expect("❌ MongoDB connection failed");

    let db = mongo.database("imgdock");
    let collection = db.collection::<mongodb::bson::Document>("i");

    mongo
        .database("admin")
        .run_command(mongodb::bson::doc! { "ping": 1 })
        .await
        .expect("❌ MongoDB ping failed");
    log::info!("✓ MongoDB connected");

    // ============ Redis (fred) ============
    let redis_config = RedisConfig::from_url(&config.redis_url)
        .expect("❌ Invalid Redis URL");

    let redis_client = RedisClient::new(redis_config, None, None, None);
    redis_client.connect();
    redis_client
        .wait_for_connect()
        .await
        .expect("❌ Redis connection failed");

    redis_client
        .ping::<String>()
        .await
        .expect("❌ Redis ping failed");
    log::info!("✓ Redis connected");

    // ============ Shared State ============
    let state = web::Data::new(AppState {
        config,
        s3,
        db: collection,
        redis: redis_client,
    });

    // ============ HTTP Server ============
    log::info!("🚀 Ready on 0.0.0.0:{}", port);

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST", "PUT", "OPTIONS"])
            .allowed_headers(vec!["Content-Type"])
            .max_age(3600);

        App::new()
            .wrap(Logger::default()) // Enable request logging
            .wrap(cors)
            .app_data(state.clone())
            .route("/transfer", web::post().to(handlers::create_transfer))
            .route(
                "/transfer/{id}/done",
                web::post().to(handlers::complete_transfer),
            )
            .route("/i/{id}", web::get().to(handlers::get_image))
            .route("/health", web::get().to(handlers::health))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
