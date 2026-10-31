//! Keeps builds from replacing themselves until the signed release pipeline exists.

use async_trait::async_trait;
use instantpad_application::message::Failure;
use instantpad_application::ports::AppUpdater;

pub struct LocalBuild;

#[async_trait]
impl AppUpdater for LocalBuild {
    fn current_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    fn is_release_build(&self) -> bool {
        false
    }

    async fn find_update(&self) -> Result<Option<String>, Failure> {
        Ok(None)
    }

    async fn download(&self, _progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        Ok(())
    }

    fn apply(&self, _restart: bool) -> Result<(), Failure> {
        Ok(())
    }
}
