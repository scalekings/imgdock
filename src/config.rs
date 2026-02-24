use std::env;

pub struct Config {
    pub r2_endpoint: String,
    pub r2_bucket: String,
    pub r2_access_key: String,
    pub r2_secret_key: String,
    pub r2_public_domain: String,
    pub mongo_uri: String,
    pub redis_url: String,
    pub port: u16,
    pub max_size: u64,
    pub max_size_mb: u64,
}

impl Config {
    pub fn from_env() -> Self {
        let max_size_mb = env::var("MAX_SIZE_MB")
            .unwrap_or_else(|_| "99".into())
            .parse::<u64>()
            .unwrap_or(99);

        Self {
            r2_endpoint: env::var("R2_ENDPOINT").expect("R2_ENDPOINT required"),
            r2_bucket: env::var("R2_BUCKET").expect("R2_BUCKET required"),
            r2_access_key: env::var("R2_ACCESS_KEY").expect("R2_ACCESS_KEY required"),
            r2_secret_key: env::var("R2_SECRET_KEY").expect("R2_SECRET_KEY required"),
            r2_public_domain: env::var("R2_PUBLIC_DOMAIN").expect("R2_PUBLIC_DOMAIN required"),
            mongo_uri: env::var("MONGO_URI").expect("MONGO_URI required"),
            redis_url: env::var("REDIS_URL").expect("REDIS_URL required"),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .unwrap_or(3000),
            max_size: max_size_mb * 1024 * 1024,
            max_size_mb,
        }
    }
}
