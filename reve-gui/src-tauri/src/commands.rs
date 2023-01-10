use crate::utils;
use reve_shared::*;
use std::io::Write;
use std::process::Stdio;
use tauri::api::process::{Command, CommandEvent};

#[tauri::command]
pub fn upscale_video(
    path: String,
    save_path: String,
    upscale_factor: u8,
    upscale_type: String,
    upscale_codec: String,
    segment_size: u32,
) -> Result<String, String> {
    let upscale_information = format!(
        "-> Video: {}\n-> Save path: {}\n-> Upscale factor: {}\n-> Upscale type: {}\n-> Upscale codec: {}\n-> Segment size: {}",
        &path, &save_path, &upscale_factor, &upscale_type, &upscale_codec, &segment_size
    );
    println!("{}", &upscale_information);
    utils::write_log(&upscale_information);

    let video = Video::new(&path, &save_path, segment_size, upscale_factor);

    for segment in &video.segments {
        println!("Segment index: {}", segment.index);
        println!("Segment size: {}", segment.size);
        update_progress_bar(segment.index as f64 / video.segments.len() as f64);
    }

    // print the number of segments
    println!("Number of segments: {}", video.segments.len());

    Ok("Upscaling finished!".to_string())
}

#[tauri::command]
// function to update the progress bar in the GUI
pub fn update_progress_bar(progress: f64) {
    println!("Progress: {}", progress);
}