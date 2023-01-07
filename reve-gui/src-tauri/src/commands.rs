use crate::utils;
use reve_shared::*;
use std::io::Write;
use std::process::Stdio;
use tauri::api::process::{Command, CommandEvent};

#[tauri::command]
pub fn upscale_video(
    path: String,
    save_path: String,
    upscale_factor: String,
    upscale_type: String,
    upscale_codec: String,
) -> Result<String, String> {
    let upscale_information = format!(
        "-> Video: {}\n-> Save path: {}\n-> Upscale factor: {}\n-> Upscale type: {}\n-> Upscale codec: {}\n",
        &path, &save_path, &upscale_factor, &upscale_type, &upscale_codec
    );
    println!("{}", &upscale_information);
    utils::write_log(&upscale_information);

    // check if the executable exists
    if !utils::check_if_file_exists("reve-cli.exe".to_string()) {
        utils::write_log("Upscaling failed: reve-cli.exe not found");
        // log the command being executed
        utils::write_log(
            format!(
                "Command: reve-cli.exe -i {} -s {} -o {} -e {}",
                &path, &upscale_factor, &save_path, &upscale_codec
            )
            .as_ref(),
        );
        return Err(String::from("Upscaling failed: reve-cli.exe not found"));
    } else {
        utils::write_log("reve-cli.exe found");
        utils::write_log(
            format!(
                "Command: reve-cli.exe -i {} -s {} -o {} -e {}",
                &path, &upscale_factor, &save_path, &upscale_codec
            )
            .as_ref(),
        );
        let output = Command::new("reve-cli.exe")
            .args([
                "-i",
                &path,
                "-s",
                &upscale_factor,
                "-o",
                &save_path,
                "-e",
                &upscale_codec,
            ])
            .output()
            .expect("failed to execute process");
        if output.status.success() {
            utils::write_log(
                format!("Upscaling finished successfully: {:?}", &output.stderr).as_ref(),
            );
            Ok(String::from("Upscaling finished successfully"))
        } else {
            utils::write_log(format!("Upscaling failed: {:?}", &output.stderr).as_ref());
            Err(String::from("Upscaling failed"))
        }
    }
}
