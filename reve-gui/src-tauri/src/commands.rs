use crate::utils;
use std::io::Write;
use std::process::Stdio;
///
use tauri::api::process::{Command, CommandEvent};
///

enum UpscaleTypes {
    General,
    Digital,
}

impl UpscaleTypes {
    /// Returns the model to be used in the upscale.
    fn upscale_type_as_str(&self) -> &str {
        match self {
            UpscaleTypes::General => "realesr-animevideov3",
            UpscaleTypes::Digital => "realesr-animevideov3",
        }
    }
}

#[tauri::command]
pub fn upscale_video(
    path: String,
    save_path: String,
    upscale_factor: String,
    upscale_type: String,
) -> Result<String, String> {
    let upscale_information = format!(
        "Upscaling image: {} with the following configuration:
        -> Save path: {}
        -> Upscale factor: {} ### NOT WORKING ATM ###
        -> Upscale type: {}\n",
        &path, &save_path, &upscale_factor, &upscale_type
    );
    println!("{}", &upscale_information);
    utils::write_log(&upscale_information);

    let output = Command::new("reve-cli.exe")
        .args(["-i", &path, "-s", &upscale_factor])
        .output()
        .expect("failed to execute process");

    if output.status.success() {
        utils::write_log(format!("Upscaling finished successfully: {:?}", &output.stderr).as_ref());
        Ok(String::from("Upscaling finished successfully"))
    } else {
        utils::write_log(format!("Upscaling failed: {:?}", &output.stderr).as_ref());
        Err(String::from("Upscaling failed"))
    }
}
