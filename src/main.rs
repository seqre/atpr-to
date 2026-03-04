use axum::{routing::get, Router};
use lambda_http::{run, tracing, Error};

mod generated;

async fn index() -> &'static str {
    "atpr.to — AT Protocol URL Shortener"
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let app = Router::new().route("/", get(index));

    run(app).await
}

#[cfg(test)]
mod tests {
    use super::generated::to_atpr::link::Link;
    use jacquard_common::types::collection::Collection;

    #[test]
    fn test_link_record_serde_roundtrip() {
        let json = r#"{"url":"https://example.com/","createdAt":"2024-01-01T00:00:00Z"}"#;
        let record: Link = serde_json::from_str(json).unwrap();

        assert_eq!(record.url.as_ref(), "https://example.com/");

        let serialized = serde_json::to_string(&record).unwrap();
        let deserialized: Link = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.url.as_ref(), "https://example.com/");
    }

    #[test]
    fn test_link_record_builder() {
        let url = jacquard_common::types::string::Uri::new("https://example.com/").unwrap();
        let now = chrono::Utc::now().fixed_offset();
        let record = Link::new()
            .url(url)
            .created_at(now)
            .build();

        assert_eq!(record.url.as_ref(), "https://example.com/");
    }

    #[test]
    fn test_link_collection_nsid() {
        assert_eq!(<Link as Collection>::NSID, "to.atpr.link");
    }
}
