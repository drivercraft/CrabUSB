use std::process::Command;

// 测试 Rust-native 视频转换功能
fn test_video_conversion() -> Result<(), Box<dyn std::error::Error>> {
    println!("测试 Rust-native 视频转换功能...");

    // 创建测试目录
    let test_dir = "test_frames";
    std::fs::create_dir_all(test_dir)?;

    // 生成几个测试图片 (使用 FFmpeg 生成彩色条纹测试图像)
    println!("生成测试图片...");
    for i in 0..10 {
        let output_path = format!("{}/frame_{:04}.png", test_dir, i);
        let args = vec![
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=640x480:rate=1",
            "-vframes",
            "1",
            "-t",
            "1",
            "-y",
            &output_path,
        ];

        let result = Command::new("ffmpeg").args(&args).output()?;

        if !result.status.success() {
            eprintln!(
                "生成测试图片失败: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            return Err("FFmpeg 生成测试图片失败".into());
        }
    }

    // 测试图片转视频功能
    println!("测试图片转视频...");
    let video_path = "test_output.mp4";
    let input_pattern = format!("{}/frame_%04d.png", test_dir);
    let args = vec![
        "-framerate",
        "30",
        "-i",
        &input_pattern,
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-y",
        video_path,
    ];

    let result = Command::new("ffmpeg").args(&args).output()?;

    if result.status.success() {
        println!("✅ 视频转换成功! 输出文件: {}", video_path);

        // 检查输出文件
        let metadata = std::fs::metadata(video_path)?;
        println!("视频文件大小: {} bytes", metadata.len());
    } else {
        eprintln!(
            "❌ 视频转换失败: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        return Err("视频转换失败".into());
    }

    // 清理测试文件
    println!("清理测试文件...");
    std::fs::remove_dir_all(test_dir)?;
    std::fs::remove_file(video_path)?;

    println!("✅ 所有测试通过!");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    test_video_conversion()
}
