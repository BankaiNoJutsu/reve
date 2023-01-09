use crate::utils;
use reve_shared::*;
use std::io::Write;
use std::process::Stdio;
use tauri::api::process::{Command, CommandEvent};
use tauri::Window;

#[tauri::command]
pub fn upscale_video(
    path: String,
    save_path: String,
    upscale_factor: String,
    upscale_type: String,
    upscale_codec: String,
    window: Window,
) -> Result<String, String> {
    let upscale_information = format!(
        "-> Video: {}\n-> Save path: {}\n-> Upscale factor: {}\n-> Upscale type: {}\n-> Upscale codec: {}\n",
        &path, &save_path, &upscale_factor, &upscale_type, &upscale_codec
    );
    println!("{}", &upscale_information);
    utils::write_log(&upscale_information);

    let segment_size: u32 = 1000;
    let upscale_factor: u8 = upscale_factor.parse().unwrap();

    // use Video::new to create a new Video object
    let mut video = Video::new(&path, &save_path, segment_size, upscale_factor);

    for segment in &video.segments {
        // export the frames of the segment and cout the number of frames in output folder
        let export_result = Video::export_segment(&video, segment.index as usize);
        if export_result.is_err() {
            utils::write_log(&format!("Failed to export segment {}.", segment.index));
            return Err(export_result.err().unwrap().to_string());
        } else {
            utils::write_log(&format!("Exported segment {}.", segment.index));
        }

        let upscale_result = Video::upscale_segment(&video, segment.index as usize);
        if upscale_result.is_err() {
            utils::write_log(&format!("Failed to upscale segment {}.", segment.index));
            return Err(upscale_result.err().unwrap().to_string());
        } else {
            utils::write_log(&format!("Upscaled segment {}.", segment.index));
        }

        // set the "status-bar" <v-progress-linear> max value to the number of segments

        // update "status-bar" <v-progress-linear> with segment index
    }
    Ok("Upscaling finished!".to_string())
}
