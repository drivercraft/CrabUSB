#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![cfg(not(target_os = "none"))]

use clap::{Arg, Command};
use crab_uvc::{UncompressedFormat, VideoFormat, VideoFormatType};
use env_logger;
use log::{error, info, warn};
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use uvc_frame_parser::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = Command::new("uvc-frame-parser")
        .version("1.0")
        .author("CrabUSB Team")
        .about("Parse UVC frame data from serial logs and convert to images/video")
        .arg(
            Arg::new("log-file")
                .short('l')
                .long("log-file")
                .value_name("FILE")
                .help("Serial log file containing frame data")
                .required(true),
        )
        .arg(
            Arg::new("output-dir")
                .short('o')
                .long("output-dir")
                .value_name("DIR")
                .help("Output directory for processed images")
                .required(true),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_name("FORMAT")
                .help("Output format: jpg, png, video")
                .default_value("jpg"),
        )
        .get_matches();

    let log_file = matches.get_one::<String>("log-file").unwrap();
    let output_dir = PathBuf::from(matches.get_one::<String>("output-dir").unwrap());
    let output_format = matches.get_one::<String>("format").unwrap();

    info!("Parsing log file: {}", log_file);
    info!("Output directory: {:?}", output_dir);
    info!("Output format: {}", output_format);

    // 解析串口日志文件
    let (video_format, frame_data) = parse_serial_log(log_file).await?;

    info!("Parsed video format: {:?}", video_format);
    info!("Frame data size: {} bytes", frame_data.len());

    // 创建临时目录用于存储原始帧数据
    let temp_dir = std::env::temp_dir().join("uvc_frame_parser");
    tokio::fs::create_dir_all(&temp_dir).await?;

    let parser = Parser::new(temp_dir.clone(), output_dir.clone()).await;

    // 保存帧数据到临时文件
    let frame_file = temp_dir.join("frame_000000.raw");
    tokio::fs::write(&frame_file, &frame_data).await?;

    match output_format.as_str() {
        "jpg" | "jpeg" => {
            info!("Converting to JPEG image...");
            parser.convert_raw_to_images(&[0], &video_format).await?;
        }
        "png" => {
            info!("Converting to PNG image...");
            parser.convert_raw_to_images(&[0], &video_format).await?;
        }
        "video" => {
            info!("Creating video...");
            parser.write_format_info(&video_format).await?;
            parser
                .create_video_from_frames(&[0], 30.0, &video_format)
                .await?;
        }
        _ => {
            error!("Unsupported format: {}", output_format);
            return Err("Unsupported format".into());
        }
    }

    // 清理临时文件
    if let Err(e) = tokio::fs::remove_dir_all(&temp_dir).await {
        warn!("Failed to clean up temp directory: {}", e);
    }

    info!("Processing completed successfully!");
    Ok(())
}

/// 解析串口日志文件，提取视频格式信息和帧数据
async fn parse_serial_log(
    log_file: &str,
) -> Result<(VideoFormat, Vec<u8>), Box<dyn std::error::Error>> {
    let file = File::open(log_file)?;
    let reader = BufReader::new(file);

    let mut video_format: Option<VideoFormat> = None;
    let mut frame_data = Vec::new();
    let mut in_video_format = false;
    let mut in_frame_data = false;
    let mut frame_size: Option<usize> = None;

    for line_result in reader.lines() {
        let line = line_result?;
        // 去除ANSI彩色码和时间戳，只保留实际消息
        let cleaned_line = strip_ansi_and_timestamp(&line);
        let trimmed = cleaned_line.trim();

        // 解析视频格式信息
        if trimmed.contains("VIDEO_FORMAT_START") {
            in_video_format = true;
            continue;
        }
        if trimmed.contains("VIDEO_FORMAT_END") {
            in_video_format = false;
            continue;
        }
        if in_video_format && trimmed.starts_with("VIDEO_FORMAT:") {
            video_format = Some(parse_video_format_from_log(trimmed)?);
            continue;
        }

        // 解析帧数据
        if trimmed.contains("FRAME_DATA_START") {
            in_frame_data = true;
            continue;
        }
        if trimmed.contains("FRAME_DATA_END") {
            in_frame_data = false;
            break;
        }
        if in_frame_data {
            if trimmed.starts_with("FRAME_SIZE:") {
                if let Some(size_str) = trimmed.strip_prefix("FRAME_SIZE:").map(|s| s.trim()) {
                    frame_size = Some(size_str.parse()?);
                }
            } else if trimmed.starts_with("CHUNK_") {
                // 解析十六进制数据块
                if let Some(colon_pos) = trimmed.find(':') {
                    let hex_data = &trimmed[colon_pos + 1..].trim();
                    let chunk_bytes = hex_to_bytes(hex_data)?;
                    frame_data.extend_from_slice(&chunk_bytes);
                }
            }
        }
    }

    let format = video_format.ok_or("No video format found in log")?;

    if let Some(expected_size) = frame_size {
        if frame_data.len() != expected_size {
            warn!(
                "Frame data size mismatch: expected {}, got {}",
                expected_size,
                frame_data.len()
            );
        }
    }

    Ok((format, frame_data))
}

/// 从日志行解析VideoFormat
fn parse_video_format_from_log(line: &str) -> Result<VideoFormat, Box<dyn std::error::Error>> {
    // 简单的字符串解析，匹配日志中的Debug格式
    // 例如: "VIDEO_FORMAT: Mjpeg { width: 640, height: 480, frame_rate: 30 }"

    if line.contains("Mjpeg") {
        let width = extract_field_value(line, "width")?;
        let height = extract_field_value(line, "height")?;
        let frame_rate = extract_field_value(line, "frame_rate").unwrap_or(30);

        Ok(VideoFormat {
            width: width as u16,
            height: height as u16,
            frame_rate,
            format_type: VideoFormatType::Mjpeg,
        })
    } else if line.contains("Uncompressed") {
        let width = extract_field_value(line, "width")?;
        let height = extract_field_value(line, "height")?;
        let frame_rate = extract_field_value(line, "frame_rate").unwrap_or(30);

        // 默认使用YUY2格式，实际项目中可能需要更详细的解析
        let format_type = if line.contains("Yuy2") {
            UncompressedFormat::Yuy2
        } else if line.contains("Nv12") {
            UncompressedFormat::Nv12
        } else if line.contains("Rgb24") {
            UncompressedFormat::Rgb24
        } else if line.contains("Rgb32") {
            UncompressedFormat::Rgb32
        } else {
            UncompressedFormat::Yuy2 // 默认
        };

        Ok(VideoFormat {
            width: width as u16,
            height: height as u16,
            frame_rate,
            format_type: VideoFormatType::Uncompressed(format_type),
        })
    } else if line.contains("H264") {
        let width = extract_field_value(line, "width")?;
        let height = extract_field_value(line, "height")?;
        let frame_rate = extract_field_value(line, "frame_rate").unwrap_or(30);

        Ok(VideoFormat {
            width: width as u16,
            height: height as u16,
            frame_rate,
            format_type: VideoFormatType::H264,
        })
    } else {
        Err("Unsupported video format in log".into())
    }
}

/// 从字符串中提取字段值
fn extract_field_value(text: &str, field: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let pattern = format!("{}: ", field);
    if let Some(start) = text.find(&pattern) {
        let value_start = start + pattern.len();
        let value_end = text[value_start..]
            .find(|c: char| c == ',' || c == ' ' || c == '}')
            .map(|pos| value_start + pos)
            .unwrap_or(text.len());

        let value_str = &text[value_start..value_end].trim();
        Ok(value_str.parse()?)
    } else {
        Err(format!("Field '{}' not found", field).into())
    }
}

/// 将十六进制字符串转换为字节数组
fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let hex_clean = hex_str.replace(" ", "");
    if hex_clean.len() % 2 != 0 {
        return Err("Invalid hex string length".into());
    }

    let mut bytes = Vec::new();
    for chunk in hex_clean.as_bytes().chunks(2) {
        let hex_byte = std::str::from_utf8(chunk)?;
        let byte = u8::from_str_radix(hex_byte, 16)?;
        bytes.push(byte);
    }

    Ok(bytes)
}

fn strip_ansi_and_timestamp(line: &str) -> String {
    // 去除ANSI转义码
    let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let no_ansi = ansi_regex.replace_all(line, "");

    // 去除emoji和时间戳前缀 (如 "💡 36.624s    [test::tests:142]")
    let timestamp_regex = Regex::new(r"^[^\[]*\[[^\]]+\]\s*").unwrap();
    let cleaned = timestamp_regex.replace(&no_ansi, "");

    cleaned.to_string()
}
