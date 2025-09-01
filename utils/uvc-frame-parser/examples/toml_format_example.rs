/// 示例：展示如何使用 VideoInfo TOML 格式
use crab_uvc::{VideoFormat, UncompressedFormat};
use uvc_frame_parser::Parser;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // 创建一个示例的视频格式
    let video_format = VideoFormat::Uncompressed {
        width: 1920,
        height: 1080,
        frame_rate: 30,
        format_type: UncompressedFormat::Yuy2,
    };

    // 创建解析器
    let input_dir = PathBuf::from("frames");
    let output_dir = PathBuf::from("output");
    let parser = Parser::new(input_dir, output_dir).await;

    // 写入格式信息到 TOML 文件
    parser.write_format_info(&video_format).await?;

    // 读取格式信息
    match parser.read_format_info().await {
        Ok(video_info) => {
            println!("读取到的视频信息：");
            println!("  尺寸: {}x{}", video_info.width, video_info.height);
            println!("  帧率: {} fps", video_info.fps);
            println!("  像素格式: {:?}", video_info.pixel);
        }
        Err(e) => {
            println!("读取视频信息失败: {}", e);
        }
    }

    Ok(())
}
