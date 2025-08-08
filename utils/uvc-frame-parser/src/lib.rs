use log::warn;
use tokio::fs;

pub struct Praser {}

impl Praser {
    pub async fn new() -> Self {
        // 创建输出目录
        let output_dir = "target/frames";
        if let Err(e) = fs::create_dir_all(output_dir).await {
            warn!("Failed to create output directory: {e:?}");
        }

        Self {}
    }
}
