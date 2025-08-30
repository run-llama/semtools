use async_trait::async_trait;
use std::error::Error;

#[async_trait]
pub trait ParseBackend: Send + Sync {
    async fn parse(&self, files: Vec<String>) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;
    fn name(&self) -> &str;
    fn verbose(&self) -> bool;
}
