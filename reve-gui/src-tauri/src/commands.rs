use crate::utils;
use std::io::Write;
use std::process::Stdio;
use tauri::api::process::{Command, CommandEvent};
use reve_shared::*;

#[tauri::command]
pub fn upscale_video(
    path: String,
    save_path: String,
    upscale_factor: String,
    upscale_type: String,
) -> Result<String, String> {
    let upscale_information = format!(
        "-> Video: {}\n-> Save path: {}\n-> Upscale factor: {}\n-> Upscale type: {}\n",
        &path, &save_path, &upscale_factor, &upscale_type
    );
    println!("{}", &upscale_information);
    utils::write_log(&upscale_information);

    // generate Args with the given arguments
/*     let args = Args {
        inputpath: path,
        resolution: String::from(""),
        format: String::from(""),
        scale: upscale_factor,
        segmentsize: String::from(""),
        crf: String::from(""),
        preset: String::from(""),
        codec: String::from(""),
        x265params: String::from(""),
        outputpath: save_path,
    }; */
    //let Args { inputpath, resolution, format, scale, segmentsize, crf, preset, codec, x265params, outputpath };
    //pre_work();

    // check if the executable exists
    if !utils::check_if_file_exists("reve-cli.exe".to_string()) {
        utils::write_log("Upscaling failed: reve-cli.exe not found");
        // log the command being executed
        utils::write_log(
            format!("Command: reve-cli.exe -i {} -s {} -o {}", &path, &upscale_factor, &save_path).as_ref(),
        );
        return Err(String::from("Upscaling failed: reve-cli.exe not found"));
    } else {
        utils::write_log("reve-cli.exe found");
        utils::write_log(
            format!("Command: reve-cli.exe -i {} -s {} -o {}", &path, &upscale_factor, &save_path).as_ref(),
        );
        let output = Command::new("reve-cli.exe")
            .args(["-i", &path, "-s", &upscale_factor, "-o", &save_path])
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
