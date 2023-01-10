use clap::Parser;
use clearscreen::clear;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use path_clean::PathClean;
use rayon::prelude::*;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use serde_json::Value;
use std::env;
use std::fs;
use std::fs::metadata;
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::path::Path;
use std::process::exit;
use std::process::Output;
use std::process::{ChildStderr, Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::vec;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize)]
pub struct Segment {
    pub index: u32,
    pub size: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Video {
    pub path: String,
    pub output_path: String,
    pub segments: Vec<Segment>,
    pub frame_rate: f32,
    pub frame_count: u32,
    pub segment_size: u32,
    pub segment_count: u32,
    pub upscale_ratio: u8,
}

impl Video {
    pub fn new(path: &str, output_path: &str, segment_size: u32, upscale_ratio: u8) -> Video {
        let frame_count = get_frame_count(&path.to_string());
        let frame_rate = get_frame_rate(&path.to_string()).parse::<f32>().unwrap();

        let parts_num = (frame_count as f32 / segment_size as f32).ceil() as i32;
        let last_segment_size = get_last_segment_size(frame_count, segment_size);

        let mut segments = Vec::new();
        for i in 0..(parts_num - 1) {
            let frame_number = segment_size;
            segments.push(Segment {
                index: i as u32,
                size: frame_number as u32,
            });
        }
        segments.push(Segment {
            index: (parts_num - 1) as u32,
            size: last_segment_size as u32,
        });

        let segment_count = segments.len() as u32;

        Video {
            path: path.to_string(),
            output_path: output_path.to_string(),
            segments,
            frame_rate,
            frame_count,
            segment_size,
            segment_count,
            upscale_ratio,
        }
    }

    pub fn export_segment(&self, index: usize) -> Result<BufReader<ChildStderr>, Error> {
        let index_dir = format!("temp\\tmp_frames\\{}", index);
        fs::create_dir(&index_dir).unwrap();

        let output_path = format!("temp\\tmp_frames\\{}\\frame%08d.png", index);
        let start_time = if index == 0 {
            String::from("0")
        } else {
            ((index as u32 * self.segment_size - 1) as f32 / self.frame_rate).to_string()
        };
        let segments_index = if self.segments.len() == 1 { 0 } else { 1 };
        let stderr = Command::new("ffmpeg")
            .args([
                "-v",
                "verbose",
                "-ss",
                &start_time,
                "-i",
                &self.path.to_string(),
                "-qscale:v",
                "1",
                "-qmin",
                "1",
                "-qmax",
                "1",
                "-vsync",
                "0",
                "-vframes",
                &self.segments[segments_index].size.to_string(),
                &output_path,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .stderr
            .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;
        Ok(BufReader::new(stderr))
    }

    pub fn upscale_segment(&self, index: usize) -> Result<BufReader<ChildStderr>, Error> {
        let input_path = format!("temp\\tmp_frames\\{}", index);
        let output_path = format!("temp\\out_frames\\{}", index);
        fs::create_dir(&output_path).expect("could not create directory");

        let stderr = Command::new("realesrgan-ncnn-vulkan")
            .args([
                "-i",
                &input_path,
                "-o",
                &output_path,
                "-n",
                "realesr-animevideov3-x2",
                "-s",
                &self.upscale_ratio.to_string(),
                "-f",
                "png",
                "-v",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // get the output of the command
        let error = stderr
            .stderr
            .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;
        let output = stderr
            .stdout
            .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

        // read the output of the command
        let _output = BufReader::new(output);

        // for each line of the output that contains "done", print it
        for line in BufReader::new(_output).lines() {
            let line = line.unwrap();
            if line.contains("done") {
                println!("{}", line);
            }
        }
        Ok(BufReader::new(error))
    }

    // TODO: args builder for custom commands
    pub fn merge_segment(&self, args: Vec<&str>) -> Result<BufReader<ChildStderr>, Error> {
        let mut stderr = Command::new("ffmpeg");
        for arg in args {
            stderr.arg(arg);
        }
        let stderr = stderr
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .stderr
            .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

        Ok(BufReader::new(stderr))
    }

    pub fn concatenate_segments(&self) {
        let mut f_content = String::from("file 'video_parts\\0.mp4'");
        for segment_index in 1..self.segment_count {
            let video_part_path = format!("video_parts\\{}.mp4", segment_index);
            f_content = format!("{}\nfile '{}'", f_content, video_part_path);
        }
        fs::write("temp\\parts.txt", f_content).unwrap();

        Command::new("ffmpeg")
            .args([
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                "temp\\parts.txt",
                "-i",
                &self.path,
                "-map",
                "0:v",
                "-map",
                "1:a?",
                "-map",
                "1:s?",
                "-map_chapters",
                "1",
                "-c",
                "copy",
                &self.output_path,
            ])
            .output()
            .unwrap();
        fs::remove_file("temp\\parts.txt").unwrap();
    }
}

#[derive(Parser, Serialize, Deserialize, Debug)]
#[clap(name = "Real-ESRGAN Video Enhance",
author = "ONdraid <ondraid.png@gmail.com>",
about = "Real-ESRGAN video upscaler with resumability",
long_about = None)]
pub struct Args {
    /// input video path (mp4/mkv/...) or folder path (\\... or /... or C:\...)
    #[clap(short = 'i', long, value_parser = input_validation)]
    pub inputpath: String,

    // maximum resolution (480 by default)
    #[clap(short = 'r', long, value_parser = max_resolution_validation, default_value = "480")]
    pub resolution: Option<String>,

    // output video extension format (mp4 by default)
    #[clap(short = 'f', long, value_parser = format_validation, default_value = "mp4")]
    pub format: String,

    // model name (realesr-animevideov3-x2 by default)
    #[clap(short = 'm', long, value_parser = model_validation, default_value = "realesr-animevideov3")]
    pub model: String,

    /// upscale ratio (2, 3, 4)
    #[clap(short = 's', long, value_parser = clap::value_parser!(u8).range(2..5), default_value_t = 2)]
    pub scale: u8,

    /// segment size (in frames)
    #[clap(short = 'P', long = "parts", value_parser, default_value_t = 1000)]
    pub segmentsize: u32,

    /// video constant rate factor (crf: 51-0)
    #[clap(short = 'c', long = "crf", value_parser = clap::value_parser!(u8).range(0..52), default_value_t = 15)]
    pub crf: u8,

    /// video encoding preset
    #[clap(short = 'p', long, value_parser = preset_validation, default_value = "slow")]
    pub preset: String,

    /// codec encoding parameters (libsvt_hevc, libsvtav1, libx265)
    #[clap(
        short = 'e',
        long = "encoder",
        value_parser = codec_validation,
        default_value = "libx265"
    )]
    pub codec: String,

    /// x265 encoding parameters
    #[clap(
        short = 'x',
        long,
        value_parser,
        default_value = "psy-rd=2:aq-strength=1:deblock=0,0:bframes=8"
    )]
    pub x265params: String,

    // (Optional) output video path (file.mp4/mkv/...)
    #[clap(short = 'o', long, value_parser = output_validation)]
    pub outputpath: Option<String>,
}

fn input_validation(s: &str) -> Result<String, String> {
    let p = Path::new(s);

    // if the path in p contains a double quote, remove it and everything after it
    if p.to_str().unwrap().contains("\"") {
        let mut s = p.to_str().unwrap().to_string();
        s.truncate(s.find("\"").unwrap());
        return Ok(s);
    }

    if p.is_dir() {
        return Ok(String::from_str(s).unwrap());
    }

    if !p.exists() {
        return Err(String::from_str("input path not found").unwrap());
    }

    match p.extension().unwrap().to_str().unwrap() {
        "mp4" | "mkv" | "avi" => Ok(s.to_string()),
        _ => Err(String::from_str("valid input formats: mp4/mkv/avi").unwrap()),
    }
}

pub fn output_validation(s: &str) -> Result<String, String> {
    let p = Path::new(s);

    if p.exists() {
        println!("{} already exists!", &s);
        exit(1);
    } else {
        match p.extension().unwrap().to_str().unwrap() {
            "mp4" | "mkv" | "avi" => Ok(s.to_string()),
            _ => Err(String::from_str("valid input formats: mp4/mkv/avi").unwrap()),
        }
    }
}

pub fn output_validation_dir(s: &str) -> Result<String, String> {
    let p = Path::new(s);

    if p.exists() {
        return Ok("already exists".to_string());
    } else {
        match p.extension().unwrap().to_str().unwrap() {
            "mp4" | "mkv" | "avi" => Ok(s.to_string()),
            _ => Err(String::from_str("valid input formats: mp4/mkv/avi").unwrap()),
        }
    }
}

fn format_validation(s: &str) -> Result<String, String> {
    match s {
        "mp4" | "mkv" | "avi" => Ok(s.to_string()),
        _ => Err(String::from_str("valid output formats: mp4/mkv/avi").unwrap()),
    }
}

fn model_validation(s: &str) -> Result<String, String> {
    match s {
        "realesr-animevideov3" => Ok(s.to_string()),
        _ => Err(String::from_str("valid: realesr-animevideov3").unwrap()),
    }
}

fn max_resolution_validation(s: &str) -> Result<String, String> {
    let validate = s.parse::<f64>().is_ok();
    match validate {
        true => Ok(s.to_string()),
        false => Err(String::from_str("valid resolution is numeric!").unwrap()),
    }
}

fn preset_validation(s: &str) -> Result<String, String> {
    match s {
        "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium" | "slow"
        | "slower" | "veryslow" => Ok(s.to_string()),
        _ => Err(String::from_str(
            "valid: ultrafast/superfast/veryfast/faster/fast/medium/slow/slower/veryslow",
        )
        .unwrap()),
    }
}

fn codec_validation(s: &str) -> Result<String, String> {
    match s {
        "libx265" | "libsvt_hevc" | "libsvtav1" => Ok(s.to_string()),
        _ => Err(String::from_str("valid: libx265/libsvt_hevc/libsvtav1").unwrap()),
    }
}

pub fn get_last_segment_size(frame_count: u32, segment_size: u32) -> u32 {
    let last_segment_size = (frame_count % segment_size) as u32;
    if last_segment_size == 0 {
        segment_size
    } else {
        last_segment_size - 1
    }
}

pub fn rebuild_temp(keep_args: bool) {
    let _ = fs::create_dir("temp");
    if !keep_args {
        println!("removing temp");
        fs::remove_dir_all("temp").expect("could not remove temp. try deleting manually");

        for dir in ["temp\\tmp_frames", "temp\\out_frames", "temp\\video_parts"] {
            println!("creating {}", dir);
            fs::create_dir_all(dir).unwrap();
        }
    } else {
        for dir in ["temp\\tmp_frames", "temp\\out_frames"] {
            println!("removing {}", dir);
            fs::remove_dir_all(dir)
                .unwrap_or_else(|_| panic!("could not remove {:?}. try deleting manually", dir));
            println!("creating {}", dir);
            fs::create_dir_all(dir).unwrap();
        }
        println!("removing parts.txt");
        let _ = fs::remove_file("temp\\parts.txt");
    }
}

pub fn add_to_db(
    files: Vec<String>,
    res: String,
    bar: ProgressBar,
) -> Result<(Vec<AtomicI32>, Arc<Mutex<Vec<std::string::String>>>)> {
    let count: AtomicI32 = AtomicI32::new(0);
    let db_count;
    let db_count_added: AtomicI32 = AtomicI32::new(0);
    let db_count_skipped: AtomicI32 = AtomicI32::new(0);
    let files_to_process: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let conn = Connection::open("reve.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS video_info (
                    id INTEGER PRIMARY KEY,
                    filename TEXT NOT NULL,
                    filepath TEXT NOT NULL,
                    width INTEGER NOT NULL,
                    height INTEGER NOT NULL,
                    duration REAL NOT NULL,
                    pixel_format TEXT NOT NULL,
                    display_aspect_ratio TEXT NOT NULL,
                    sample_aspect_ratio TEXT NOT NULL,
                    format TEXT NOT NULL,
                    size BIGINT NOT NULL,
                    folder_size BIGINT NOT NULL,
                    bitrate BIGINT NOT NULL,
                    codec TEXT NOT NULL,
                    resolution TEXT NOT NULL,
                    status TEXT NOT NULL,
                    hash TEXT NOT NULL
                  )",
        params![],
    )?;

    let filenames_skip = files.clone();
    let mut filenames = files;

    // get all items in db
    let mut stmt = conn.prepare("SELECT * FROM video_info")?;
    let mut rows = stmt
        .query_map(params![], |row| {
            Ok((
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
                row.get(12)?,
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
            ))
        })
        .unwrap();
    let mut db_items: Vec<(
        String,
        String,
        i32,
        i32,
        f64,
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        String,
    )> = Vec::new();
    while let Some(row) = rows.next() {
        let row = row.unwrap();
        db_items.push(row);
    }
    // get all items from filenames that are not in db
    let mut filenames_to_process: Vec<String> = Vec::new();
    for filename in filenames {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename {
                found = true;
                break;
            }
        }
        if !found {
            filenames_to_process.push(filename);
        }
    }

    // get all the items from filenames that are in db
    let mut filenames_to_skip: Vec<String> = Vec::new();
    for filename in filenames_skip {
        let real_filename = Path::new(&filename).file_name().unwrap().to_str().unwrap();
        let mut found = false;
        for item in &db_items {
            if item.0 == real_filename {
                found = true;
                break;
            }
        }
        if found {
            filenames_to_skip.push(filename);
        }
    }
    db_count = AtomicI32::new(filenames_to_skip.len() as i32);

    // print count for all items in filenames_to_process and return filenames with all items in db removed
    println!("Found {} files not in database", filenames_to_process.len());
    filenames = filenames_to_process.clone();

    bar.set_length(filenames.len() as u64);
    let conn = Arc::new(Mutex::new(Connection::open("reve.db")?));

    filenames.par_iter().for_each(|filename| {
        let real_filename = Path::new(filename).file_name().unwrap().to_str().unwrap();
        let conn = conn.clone();
        let conn = conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM video_info WHERE filename=?1").unwrap();
        let file_exists: bool = stmt.exists(params![real_filename]).unwrap();
        if !file_exists {
            let output = Command::new("ffprobe")
                .args([
                    "-i",
                    filename,
                    "-v",
                    "error",
                    "-select_streams",
                    "v",
                    "-show_entries",
                    "stream",
                    "-show_format",
                    "-show_data_hash",
                    "sha256",
                    "-show_streams",
                    "-of",
                    "json"
                ])
                .output()
                .expect("failed to execute process");
            let json_value: Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
            let json_str = json_value.to_string();
            if &json_str.len() >= &1 {
                let values: Value = json_value;
                let _width = values["streams"][0]["width"].as_i64().unwrap_or(0);
                let _height = values["streams"][0]["height"].as_i64().unwrap_or(0);
                let filepath = values["format"]["filename"].as_str().unwrap();
                let filename = Path::new(filepath).file_name().unwrap().to_str().unwrap();
                let size = values["format"]["size"].as_str().unwrap_or("0");
                let bitrate = values["format"]["bit_rate"].as_str().unwrap_or("0");
                let duration = values["format"]["duration"].as_str().unwrap_or("0.0");
                let format = values["format"]["format_name"].as_str().unwrap_or("NaN");
                let width = values["streams"][0]["width"].as_i64().unwrap_or(0);
                let height = values["streams"][0]["height"].as_i64().unwrap_or(0);
                let codec = values["streams"][0]["codec_name"].as_str().unwrap_or("NaN");
                let pix_fmt = values["streams"][0]["pix_fmt"].as_str().unwrap_or("NaN");
                let checksum = values["streams"][0]["extradata_hash"].as_str().unwrap_or("NaN");
                let dar = values["streams"][0]["display_aspect_ratio"].as_str().unwrap_or("NaN");
                let sar = values["streams"][0]["sample_aspect_ratio"].as_str().unwrap_or("NaN");

                // for each file in this folder and it's subfodlers, sum the size of the files
                let mut folder_size = 0;
                for entry in WalkDir::new(Path::new(filepath).parent().unwrap()) {
                    let entry = entry.unwrap();
                    let metadata = fs::metadata(entry.path());
                    folder_size += metadata.unwrap().len() as i64;
                }
                //println!("{}", folder_size);

                if height <= res.parse::<i64>().unwrap() {
                    conn.execute(
                        "INSERT INTO video_info (filename, filepath, width, height, duration, pixel_format, display_aspect_ratio, sample_aspect_ratio, format, size, folder_size, bitrate, codec, resolution, status, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                        params![filename, filepath, width, height, duration, pix_fmt, dar, sar, format, size, folder_size, bitrate, codec, res, "pending", checksum]
                    ).unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    db_count_added.fetch_add(1, Ordering::SeqCst);
                } else {
                    //db_count_skipped.fetch_add(1, Ordering::SeqCst);
                    conn.execute(
                        "INSERT INTO video_info (filename, filepath, width, height, duration, pixel_format, display_aspect_ratio, sample_aspect_ratio, format, size, folder_size, bitrate, codec, resolution, status, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                        params![filename, filepath, width, height, duration, pix_fmt, dar, sar, format, size, folder_size, bitrate, codec, res, "skipped", checksum]
                    ).unwrap();
                    count.fetch_add(1, Ordering::SeqCst);
                    db_count_added.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        // TODO check if all files in db then return only the ones that need to be processed
        let height = get_ffprobe_output(filename).unwrap();
        let height_value = height["streams"][0]["height"].as_i64().unwrap_or(0);
        if height_value <= res.parse::<i64>().unwrap() {
            files_to_process.lock().unwrap().push(filename.to_string());
        }

        bar.inc(1);
    });

    // return all the counters
    Ok((
        vec![count, db_count, db_count_added, db_count_skipped],
        files_to_process,
    ))
}

pub fn update_db_status(
    conn: &Connection,
    filepath: &str,
    status: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("UPDATE video_info SET status=?1 WHERE filepath=?2")?;
    stmt.execute(params![status, filepath])?;
    Ok(())
}

pub fn create_db_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS video_info (
            id INTEGER PRIMARY KEY,
            filename TEXT NOT NULL,
            filepath TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            duration TEXT NOT NULL,
            pixel_format TEXT NOT NULL,
            display_aspect_ratio TEXT NOT NULL,
            sample_aspect_ratio TEXT NOT NULL,
            format TEXT NOT NULL,
            size TEXT NOT NULL,
            folder_size INTEGER NOT NULL,
            bitrate TEXT NOT NULL,
            codec TEXT NOT NULL,
            resolution TEXT NOT NULL,
            status TEXT NOT NULL,
            hash TEXT NOT NULL
        )",
        params![],
    )?;
    Ok(())
}

pub fn get_ffprobe_output(filename: &str) -> Result<Value, String> {
    let output: Output = Command::new("ffprobe")
        .args([
            "-i",
            filename,
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream",
            "-show_format",
            "-show_data_hash",
            "sha256",
            "-show_streams",
            "-of",
            "json",
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let output_str = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let value: Value = from_str(&output_str).map_err(|e| e.to_string())?;
        Ok(value)
    } else {
        Err(String::from_utf8(output.stderr).unwrap_or_else(|e| e.to_string()))
    }
}

#[cfg(target_os = "linux")]
pub fn dev_shm_exists() -> Result<(), std::io::Error> {
    let path = "/dev/shm";
    let b: bool = Path::new(path).is_dir();

    if b == true {
        fs::create_dir_all("/dev/shm/tmp_frames")?;
        fs::create_dir_all("/dev/shm/out_frames")?;
        fs::create_dir_all("/dev/shm/video_parts")?;
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "dev/shm does not exist!",
        ))
    }
}

pub fn copy_streams_no_bin_data(
    video_input_path: &String,
    copy_input_path: &String,
    output_path: &String,
    //ffmpeg_args: &String,
) -> std::process::Output {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-y",
            "-i",
            video_input_path,
            "-i",
            copy_input_path,
            "-map",
            "0:v",
            "-map",
            "1",
            "-map",
            "-1:d",
            "-map",
            "-1:v",
            "-c",
            "copy",
            output_path,
        ])
        .output()
        .expect("failed to execute process")
}

pub fn copy_streams(
    video_input_path: &String,
    copy_input_path: &String,
    output_path: &String,
) -> std::process::Output {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-y",
            "-i",
            video_input_path,
            "-i",
            copy_input_path,
            "-map",
            "0:v",
            "-map",
            "1",
            "-map",
            "-1:v",
            "-c",
            "copy",
            output_path,
        ])
        .output()
        .expect("failed to execute process")
}

pub fn absolute_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .expect("could not get current path")
            .join(path)
    }
    .clean();

    absolute_path.into_os_string().into_string().unwrap()
}

pub fn walk_count(dir: &String) -> usize {
    let mut count = 0;
    for e in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if e.metadata().unwrap().is_file() {
            let filepath = e.path().display();
            let str_filepath = filepath.to_string();
            //println!("{}", filepath);
            let mime = find_mimetype(&str_filepath);
            if mime.to_string() == "VIDEO" {
                count = count + 1;
                //println!("{}", e.path().display());
            }
        }
    }
    println!("Found {} valid video files in folder!", count);
    return count;
}

pub fn walk_files(dir: &String) -> Vec<String> {
    let mut arr = vec![];
    let mut index = 0;

    for e in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if e.metadata().unwrap().is_file() {
            let filepath = e.path().display();
            let str_filepath = filepath.to_string();
            //println!("{}", filepath);
            let mime = find_mimetype(&str_filepath);
            if mime.to_string() == "VIDEO" {
                //println!("{}", e.path().display());
                arr.insert(index, e.path().display().to_string());
                index = index + 1;
            }
        }
    }
    return arr;
}

pub fn find_mimetype(filename: &String) -> String {
    let parts: Vec<&str> = filename.split('.').collect();

    let res = match parts.last() {
        Some(v) => match *v {
            "mkv" => "VIDEO",
            "avi" => "VIDEO",
            "mp4" => "VIDEO",
            "divx" => "VIDEO",
            "flv" => "VIDEO",
            "m4v" => "VIDEO",
            "mov" => "VIDEO",
            "ogv" => "VIDEO",
            "ts" => "VIDEO",
            "webm" => "VIDEO",
            "wmv" => "VIDEO",
            &_ => "OTHER",
        },
        None => "OTHER",
    };
    return res.to_string();
}

pub fn check_ffprobe_output_i8(data: &str, res: &str) -> Result<i8, Error> {
    let to_process;
    let values: Value = serde_json::from_str(data)?;
    let height = &values["streams"][0]["height"];
    let u8_height = height.as_i64().unwrap();
    let u8_res: i64 = res.parse().unwrap();

    if u8_res >= u8_height {
        to_process = 1;
    } else {
        to_process = 0;
    }

    return Ok(to_process);
}

pub fn get_frame_count(input_path: &String) -> u32 {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=nb_frames")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let r = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<u32>();
    match r {
        Err(_e) => 0,
        _ => r.unwrap(),
    }
}

pub fn get_frame_count_tag(input_path: &String) -> u32 {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream_tags=NUMBER_OF_FRAMES-eng")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let r = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<u32>();
    match r {
        Err(_e) => 0,
        _ => r.unwrap(),
    }
}

pub fn get_frame_count_duration(input_path: &String) -> u32 {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let r = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<f32>();
    match r {
        Err(_e) => 0,
        _ => (r.unwrap() * 25.0) as u32,
    }
}

pub fn get_display_aspect_ratio(input_path: &String) -> String {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=display_aspect_ratio")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");
    let r = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse::<String>();
    match r {
        Err(_e) => "0".to_owned(),
        _ => r.unwrap(),
    }
}

pub fn get_frame_rate(input_path: &String) -> String {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=avg_frame_rate")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let temp_output = output.clone();
    let raw_framerate = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    let split_framerate = raw_framerate.split("/");
    let vec_framerate: Vec<&str> = split_framerate.collect();
    let frames: f32 = vec_framerate[0].parse().unwrap();
    let seconds: f32 = vec_framerate[1].parse().unwrap();
    return (frames / seconds).to_string();
}

pub fn get_bin_data(input_path: &String) -> String {
    let output = Command::new("ffprobe")
        .arg("-i")
        .arg(input_path)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("d")
        .arg("-show_entries")
        .arg("stream=index")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .output()
        .expect("failed to execute process");

    let temp_output = output.clone();
    let bin_data = String::from_utf8(temp_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    return bin_data;
}

pub fn export_frames(
    input_path: &String,
    output_path: &String,
    start_time: &String,
    frame_number: &u32,
    progress_bar: ProgressBar,
) -> Result<(), Error> {
    let stderr = Command::new("ffmpeg")
        .args([
            "-v",
            "verbose",
            "-ss",
            start_time,
            "-i",
            input_path,
            "-qscale:v",
            "1",
            "-qmin",
            "1",
            "-qmax",
            "1",
            "-vsync",
            "0",
            "-vframes",
            &frame_number.to_string(),
            output_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    let reader = BufReader::new(stderr);
    let mut count: i32 = -1;

    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.contains("AVIOContext"))
        .for_each(|_| {
            count += 1;
            progress_bar.set_position(count as u64);
        });

    Ok(())
}

pub fn upscale_frames(
    input_path: &String,
    output_path: &String,
    scale: &String,
    model: &String,
    progress_bar: ProgressBar,
    total_progress_bar: ProgressBar,
    mut frame_position: u64,
) -> Result<u64, Error> {
    let final_model = format!("{}-x{}", model, scale);
    #[cfg(target_os = "linux")]
    let stderr = Command::new("./realesrgan-ncnn-vulkan")
        .args([
            "-i",
            input_path,
            "-o",
            output_path,
            "-n",
            &final_model,
            "-s",
            scale,
            "-f",
            "png",
            "-v",
        ])
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    #[cfg(target_os = "windows")]
    let stderr = Command::new("realesrgan-ncnn-vulkan")
        .args([
            "-i",
            input_path,
            "-o",
            output_path,
            "-n",
            &final_model,
            "-s",
            scale,
            "-f",
            "png",
            "-v",
        ])
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    let reader = BufReader::new(stderr);
    let mut count = 0;

    total_progress_bar.set_position(frame_position);
    //println!("{}", frame_position);

    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.contains("done"))
        .for_each(|_| {
            count += 1;
            frame_position += 1;
            progress_bar.set_position(count);
            total_progress_bar.set_position(frame_position);
        });

    Ok(u64::from(total_progress_bar.position()))
}

// 2022-05-23 17:47 27cffd1
// https://github.com/AnimMouse/ffmpeg-autobuild/releases/download/m-2022-05-23-17-47/ffmpeg-27cffd1-ff31946-win64-nonfree.7z
pub fn merge_frames(
    input_path: &String,
    output_path: &String,
    codec: &String,
    frame_rate: &String,
    crf: &String,
    preset: &String,
    x265_params: &String,
    progress_bar: ProgressBar,
) -> Result<(), Error> {
    let stderr = Command::new("ffmpeg")
        .args([
            "-v",
            "verbose",
            "-f",
            "image2",
            "-framerate",
            &format!("{}/1", frame_rate),
            "-i",
            input_path,
            "-c:v",
            codec,
            "-pix_fmt",
            "yuv420p10le",
            "-crf",
            crf,
            "-preset",
            preset,
            "-x265-params",
            x265_params,
            output_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    let reader = BufReader::new(stderr);
    let mut count = 0;

    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.contains("AVIOContext"))
        .for_each(|_| {
            count += 1;
            progress_bar.set_position(count);
        });
    Ok(())
}

// 2022-03-28 07:12 c2d1597
// https://github.com/AnimMouse/ffmpeg-autobuild/releases/download/m-2022-03-28-07-12/ffmpeg-c2d1597-651202b-win64-nonfree.7z
pub fn merge_frames_svt_hevc(
    input_path: &String,
    output_path: &String,
    codec: &String,
    frame_rate: &String,
    crf: &String,
    progress_bar: ProgressBar,
) -> Result<(), Error> {
    let stderr = Command::new("ffmpeg")
        .args([
            "-v",
            "verbose",
            "-f",
            "image2",
            "-framerate",
            &format!("{}/1", frame_rate),
            "-i",
            input_path,
            "-c:v",
            codec,
            "-rc",
            "0",
            "-qp",
            crf,
            "-tune",
            "0",
            "-pix_fmt",
            "yuv420p10le",
            "-crf",
            crf,
            output_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    let reader = BufReader::new(stderr);
    let mut count = 0;

    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.contains("AVIOContext"))
        .for_each(|_| {
            count += 1;
            progress_bar.set_position(count);
        });

    Ok(())
}

pub fn merge_frames_svt_av1(
    input_path: &String,
    output_path: &String,
    codec: &String,
    frame_rate: &String,
    crf: &String,
    progress_bar: ProgressBar,
) -> Result<(), Error> {
    let stderr = Command::new("ffmpeg")
        .args([
            "-v",
            "verbose",
            "-f",
            "image2",
            "-framerate",
            &format!("{}/1", frame_rate),
            "-i",
            input_path,
            "-c:v",
            codec,
            "-pix_fmt",
            "yuv420p10le",
            "-crf",
            crf,
            output_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .stderr
        .ok_or_else(|| Error::new(ErrorKind::Other, "Could not capture standard output."))?;

    let reader = BufReader::new(stderr);
    let mut count = 0;

    reader
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| line.contains("AVIOContext"))
        .for_each(|_| {
            count += 1;
            progress_bar.set_position(count);
        });

    Ok(())
}

pub fn merge_video_parts_dar(
    input_path: &String,
    output_path: &String,
    dar: &String,
) -> std::process::Output {
    Command::new("ffmpeg")
        .args([
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            input_path,
            "-aspect",
            dar,
            "-c",
            "copy",
            output_path,
        ])
        .output()
        .expect("failed to execute process")
}

pub fn merge_video_parts(input_path: &String, output_path: &String) -> std::process::Output {
    Command::new("ffmpeg")
        .args([
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            input_path,
            "-c",
            "copy",
            output_path,
        ])
        .output()
        .expect("failed to execute process")
}

pub fn prepare() {
    let main_now = Instant::now();

    let mut args;
    args = Args::parse();

    let temp_args;
    temp_args = Args::parse();

    #[cfg(target_os = "linux")]
    match dev_shm_exists() {
        Err(e) => {
            println!("{:?}", e);
            exit(1);
        }
        _ => (),
    };

    let mut output_path: String = "".to_string();
    let mut done_output: String = "".to_string();
    let mut current_file_count = 0;
    let mut total_files: i32;
    let resolution = temp_args.resolution.unwrap().parse::<u32>().unwrap();

    let md = metadata(Path::new(&args.inputpath)).unwrap();
    // Check if input is a directory, if yes, check how many video files are in it, and process the ones that are smaller than the given resolution
    if md.is_dir() {
        let mut count;
        let db_count;
        let db_count_added;
        let db_count_skipped;
        let walk_count: u64 = walk_count(&args.inputpath) as u64;
        let files_bar = ProgressBar::new(walk_count);
        let files_style = "[file][{elapsed_precise}] [{wide_bar:.green/white}] {percent}% {pos:>7}/{len:7} analyzed files       eta: {eta:<7}";
        files_bar.set_style(
            ProgressStyle::default_bar()
                .template(files_style)
                .unwrap()
                .progress_chars("#>-"),
        );

        let vector_files = walk_files(&args.inputpath);
        let mut vector_files_to_process_frames_count: Vec<u64> = Vec::new();

        let result = add_to_db(
            vector_files.clone(),
            // if some args.resolution is given, use it, if not, use 0
            resolution.clone().to_string(),
            files_bar.clone(),
        )
        .unwrap();
        // get the counters from the add_to_db function
        let counters = result.0;

        let to_process = result.1;
        // get the vector of files to process
        let mut vector_files_to_process = to_process.lock().unwrap().clone();

        // count, db_count, db_count_added, db_count_skipped
        count = format!("{:?}", counters[0]).parse::<i32>().unwrap();
        db_count = format!("{:?}", counters[1]).parse::<u64>().unwrap();
        db_count_added = format!("{:?}", counters[2]).parse::<u64>().unwrap();
        db_count_skipped = format!("{:?}", counters[3]).parse::<u64>().unwrap();

        if vector_files_to_process.len() == 0 {
            // get all the files from the database that contain input_path's folder parent in column filepath and status 'processing' in status column and add them to the vector_files_to_process
            let conn = Connection::open("reve.db").unwrap();
            let input = args.inputpath.clone();
            let mut stmt = conn
                .prepare("SELECT * FROM video_info WHERE status = 'processing' AND filepath LIKE ?")
                .unwrap();
            let mut rows = stmt.query(&[&format!("%{}%", input)]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                vector_files_to_process.push(row.get(2).unwrap());
            }
            // get all the files from the database that contain input_path's folder parent in column filepath and status 'pending' in status column and add them to the vector_files_to_process
            let conn = Connection::open("reve.db").unwrap();
            let input = args.inputpath.clone();
            let mut stmt = conn
                .prepare("SELECT * FROM video_info WHERE status = 'pending' AND filepath LIKE ?")
                .unwrap();
            let mut rows = stmt.query(&[&format!("%{}%", input)]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                vector_files_to_process.push(row.get(2).unwrap());
            }
        }

        if count == 0 && vector_files_to_process.len() != 0 {
            count = vector_files_to_process.len() as i32;
            current_file_count = db_count - vector_files_to_process.len() as u64;
        }

        let frame_count_bar = ProgressBar::new(vector_files_to_process.len() as u64);
        let frame_count_style = "[frm_cnt][{elapsed_precise}] [{wide_bar:.green/white}] {percent}% {pos:>7}/{len:7} counted frames       eta: {eta:<7}";
        frame_count_bar.set_style(
            ProgressStyle::default_bar()
                .template(frame_count_style)
                .unwrap()
                .progress_chars("#>-"),
        );

        files_bar.finish_and_clear();
        println!("Added {} files to the database ({} already present, {} skipped due to max resolution being {}p)", db_count_added, db_count, db_count_skipped, resolution);
        println!(
            "Upscaling {} files (Due to max height resolution: {}p)",
            count, resolution
        );

        let total_frames = vector_files_to_process.clone();
        let mut current_frame_count: u64 = 0;
        for file in total_frames.clone() {
            current_frame_count += u64::from(get_frame_count(&file));
            vector_files_to_process_frames_count.push(current_frame_count);
            frame_count_bar.inc(1);
        }

        if current_frame_count == 0 {
            vector_files_to_process_frames_count.clear();
            if vector_files_to_process_frames_count.is_empty() {
                for file in total_frames.clone() {
                    current_frame_count += u64::from(get_frame_count_tag(&file));
                    vector_files_to_process_frames_count.push(current_frame_count);
                }
            }
        }

        // current_frame_count = 0; then get the frame count by dividing the duration by the fps
        if current_frame_count == 0 {
            vector_files_to_process_frames_count.clear();
            if vector_files_to_process_frames_count.is_empty() {
                for file in total_frames.clone() {
                    current_frame_count += u64::from(get_frame_count_duration(&file));
                    vector_files_to_process_frames_count.push(current_frame_count);
                }
            }
        }

        let total_frames_count = current_frame_count;

        for file in vector_files_to_process.clone() {
            let dar = get_display_aspect_ratio(&file).to_string();
            current_file_count = current_file_count + 1;
            total_files = vector_files_to_process.len() as i32;
            args.inputpath = file.clone();
            rebuild_temp(true);

            if args.outputpath.is_none() {
                let path = Path::new(&args.inputpath);
                let filename_ext = &args.format;
                let filename_no_ext = path.file_stem().unwrap().to_string_lossy();
                let filename_codec = &args.codec;
                let directory = absolute_path(path.parent().unwrap());
                #[cfg(target_os = "windows")]
                let directory_path = format!("{}{}", directory.trim_end_matches("."), "\\");
                #[cfg(target_os = "linux")]
                let directory_path = format!("{}{}", directory.trim_end_matches("."), "/");
                output_path = format!(
                    "{}{}.{}.{}",
                    directory_path, filename_no_ext, filename_codec, filename_ext
                );
                done_output = format!("{}.{}.{}", filename_no_ext, filename_codec, filename_ext);
                match output_validation_dir(&output_path) {
                    Err(e) => {
                        println!("{:?}", e);
                        exit(1);
                    }
                    Ok(s) => {
                        if s.contains("already exists") {
                            println!("{} already exists, skipping", done_output);
                            continue;
                        }
                    }
                }
            }
            if args.outputpath.is_some() {
                let str_outputpath = &args
                    .outputpath
                    .as_deref()
                    .unwrap_or("default string")
                    .to_owned();
                let path = Path::new(&str_outputpath);
                let filename = path.file_name().unwrap().to_string_lossy();

                output_path = absolute_path(filename.to_string());
                done_output = filename.to_string();
                match output_validation_dir(&output_path) {
                    Err(e) => {
                        println!("{:?}", e);
                        exit(1);
                    }
                    Ok(s) => {
                        if s.contains("already exists") {
                            println!("{} already exists, skipping", done_output);
                            continue;
                        }
                    }
                }
            }

            args.inputpath = absolute_path(file.clone());

            println!(
                "Processing file {} of {} ({}):",
                current_file_count, total_files, done_output
            );
            println!("Input path: {}", args.inputpath);
            println!("Output path: {}", output_path);
            println!("total_frames_count: {}", total_frames_count);
            println!(
                "vector_files_to_process_frames_count: {:?}",
                vector_files_to_process_frames_count
            );
            //exit(1);

            // update status in sqlite database 'reve.db' to processing for this file where filepaths match the current file
            let conn = Connection::open("reve.db").unwrap();
            conn.execute(
                "UPDATE video_info SET status = 'processing' WHERE filepath = ?",
                &[&args.inputpath],
            )
            .unwrap();

            process(
                &args,
                dar.clone(),
                current_file_count as i32,
                total_files,
                done_output.clone(),
                output_path.clone(),
                total_frames_count.clone(),
                vector_files_to_process_frames_count.clone(),
            );

            // Validation
            {
                let in_extension = Path::new(&args.inputpath).extension().unwrap();
                let out_extension = Path::new(&output_path).extension().unwrap();

                if in_extension == "mkv" && out_extension != "mkv" {
                    clear().unwrap();
                    println!(
                        "{} Invalid value {} for '{}': mkv file can only be exported as mkv file\n\nFor more information try {}",
                        "error:".to_string().bright_red(),
                        format!("\"{}\"", args.inputpath).yellow(),
                        "--outputpath <OUTPUTPATH>".to_string().yellow(),
                        "--help".to_string().green()
                    );
                    std::process::exit(1);
                }
            }
        }
        let elapsed = main_now.elapsed();
        let seconds = elapsed.as_secs() % 60;
        let minutes = (elapsed.as_secs() / 60) % 60;
        let hours = (elapsed.as_secs() / 60) / 60;
        println!(
            "done {} files in {}h:{}m:{}s",
            count, hours, minutes, seconds
        );
    }

    #[cfg(target_os = "windows")]
    let folder_args = "\\";
    #[cfg(target_os = "linux")]
    let folder_args = "/";

    if md.is_file() {
        let dar = get_display_aspect_ratio(&args.inputpath).to_string();
        current_file_count = 1;
        let mut total_frames_count = u64::from(get_frame_count(&args.inputpath));
        if total_frames_count == 0 {
            total_frames_count = u64::from(get_frame_count_tag(&args.inputpath));
        }
        let directory = Path::new(&args.inputpath)
            .parent()
            .unwrap()
            .to_str()
            .unwrap();
        if args.outputpath.is_none() {
            let path = Path::new(&args.inputpath);
            let filename_ext = &args.format;
            let filename_no_ext = path.file_stem().unwrap().to_string_lossy();
            let filename_codec = &args.codec;
            output_path = format!(
                "{}{}{}.{}.{}",
                directory, folder_args, filename_no_ext, filename_codec, filename_ext
            );
            done_output = format!("{}.{}.{}", filename_no_ext, filename_codec, filename_ext);
        }
        if args.outputpath.is_some() {
            let str_outputpath = &args
                .outputpath
                .as_deref()
                .unwrap_or("default string")
                .to_owned();
            let path = Path::new(&str_outputpath);
            let filename = path.file_name().unwrap().to_string_lossy();
            output_path = path.to_str().unwrap().to_string();
            done_output = filename.to_string();
        }
        match output_validation(&output_path) {
            Err(e) => println!("{:?}", e),
            _ => (),
        }
        clear().expect("failed to clear screen");
        total_files = 1;

        let temp_vector = vec![total_frames_count];

        let ffprobe_output = Command::new("ffprobe")
            .args([
                "-i",
                args.inputpath.as_str(),
                "-v",
                "error",
                "-select_streams",
                "v",
                "-show_entries",
                "stream",
                "-show_format",
                "-show_data_hash",
                "sha256",
                "-show_streams",
                "-of",
                "json",
            ])
            .output()
            .unwrap();
        //.\ffprobe.exe -i '\\192.168.1.99\Data\Animes\Agent AIKa\Saison 2\Agent AIKa - S02E03 - Trial 3 Deep Blue Girl.mkv' -v error -select_streams v -show_entries stream -show_format -show_data_hash sha256 -show_streams -of json
        let json_output = std::str::from_utf8(&ffprobe_output.stdout[..]).unwrap();
        let height = check_ffprobe_output_i8(json_output, &resolution.to_string());
        if height.unwrap() == 1 {
            process(
                &args,
                dar,
                current_file_count as i32,
                total_files,
                done_output,
                output_path.clone(),
                total_frames_count,
                temp_vector,
            );
        } else {
            println!(
                "{} is bigger than {}p",
                args.inputpath,
                resolution.to_string()
            );
            println!("Set argument -r to a higher value");
            exit(1);
        }

        // Validation
        {
            let in_extension = Path::new(&args.inputpath).extension().unwrap();
            let out_extension = Path::new(&output_path).extension().unwrap();

            if in_extension == "mkv" && out_extension != "mkv" {
                clear().unwrap();
                println!(
                    "{} Invalid value {} for '{}': mkv file can only be exported as mkv file\n\nFor more information try {}",
                    "error:".to_string().bright_red(),
                    format!("\"{}\"", args.inputpath).yellow(),
                    "--outputpath <OUTPUTPATH>".to_string().yellow(),
                    "--help".to_string().green()
                );
                std::process::exit(1);
            }
        }
    }
}

pub fn process(
    args: &Args,
    dar: String,
    current_file_count: i32,
    total_files: i32,
    done_output: String,
    output_path: String,
    total_frames_count: u64,
    vector_files_to_process_frames_count: Vec<u64>,
) {
    let work_now = Instant::now();

    /*     // print all arguments given to function work
    if args.verbose {
        println!("Arguments given to function work:");
        println!("dar: {}", dar);
        println!("current_file_count: {}", current_file_count);
        println!("total_files: {}", total_files);
        println!("done_output: {}", done_output);
        println!("output_path: {}", output_path);
        println!("total_frames_count: {}", total_frames_count);
        println!(
            "vector_files_to_process_frames_count: {:?}",
            vector_files_to_process_frames_count
        );
    }
    exit(1); */

    #[cfg(target_os = "windows")]
    let args_path = Path::new("temp\\args.temp");
    #[cfg(target_os = "windows")]
    let video_parts_path = "temp\\video_parts\\";
    #[cfg(target_os = "windows")]
    let temp_video_path = format!("temp\\temp.{}", &args.format);
    #[cfg(target_os = "windows")]
    let txt_list_path = "temp\\parts.txt";

    #[cfg(target_os = "linux")]
    let args_path = Path::new("/dev/shm/args.temp");
    #[cfg(target_os = "linux")]
    let video_parts_path = "/dev/shm/video_parts/";
    #[cfg(target_os = "linux")]
    let temp_video_path = format!("/dev/shm/temp.{}", &args.format);
    #[cfg(target_os = "linux")]
    let txt_list_path = "/dev/shm/parts.txt";

    if Path::new(&args_path).exists() {
        //Check if previous file is used, if yes, continue upscale without asking
        let old_args_json = fs::read_to_string(&args_path).expect("Unable to read file");
        let old_args: Args = serde_json::from_str(&old_args_json).unwrap();
        let previous_file = Path::new(&old_args.inputpath);
        let md = fs::metadata(&args.inputpath).unwrap();

        // Check if same file is used as previous upscale and if yes, resume
        if args.inputpath == previous_file.to_string_lossy()
            && args.model == old_args.model
            && args.scale == old_args.scale
        {
            if md.is_file() {
                println!(
                    "Same file! '{}' Resuming...",
                    previous_file.file_name().unwrap().to_str().unwrap()
                );
                // Resume upscale
                rebuild_temp(true);
                clear().expect("failed to clear screen");
                println!("{}", "resuming upscale".to_string().green());
            }
        } else {
            // Remove and start new
            rebuild_temp(false);
            match fs::remove_file(&txt_list_path) {
                Ok(()) => "ok",
                Err(_e) if _e.kind() == ErrorKind::NotFound => "not found",
                Err(_e) => "other",
            };
            match fs::remove_file(&temp_video_path) {
                Ok(()) => "ok",
                Err(_e) if _e.kind() == ErrorKind::NotFound => "not found",
                Err(_e) => "other",
            };

            let serialized_args = serde_json::to_string(&args).unwrap();

            let md = metadata(&args.inputpath).unwrap();
            if md.is_file() {
                fs::write(&args_path, serialized_args).expect("Unable to write file");
            }
            clear().expect("failed to clear screen");
            println!(
                "{}",
                "deleted all temporary files, parsing console input"
                    .to_string()
                    .green()
            );
        }
    } else {
        // Remove and start new
        rebuild_temp(false);
        match fs::remove_file(&txt_list_path) {
            Ok(()) => "ok",
            Err(_e) if _e.kind() == ErrorKind::NotFound => "not found",
            Err(_e) => "other",
        };
        match fs::remove_file(&temp_video_path) {
            Ok(()) => "ok",
            Err(_e) if _e.kind() == ErrorKind::NotFound => "not found",
            Err(_e) => "other",
        };

        let serialized_args = serde_json::to_string(&args).unwrap();

        let md = metadata(&args.inputpath).unwrap();
        if md.is_file() {
            fs::write(&args_path, serialized_args).expect("Unable to write file");
        }
        clear().expect("failed to clear screen");
        println!(
            "{}",
            "deleted all temporary files, parsing console input"
                .to_string()
                .green()
        );
    }

    // check if files are marked as processing in database that is not the current file and set status as 'pending'
    let conn = Connection::open("reve.db").unwrap();
    create_db_table(&conn);
    conn.execute(
        "UPDATE video_info SET status = 'pending' WHERE status = 'processing' AND filepath != ?1",
        params![args.inputpath],
    )
    .unwrap();

    let mut frame_position;
    let filename = Path::new(&args.inputpath)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();

    let mut total_frame_count = get_frame_count(&args.inputpath);

    if total_frame_count == 0 {
        total_frame_count = get_frame_count_tag(&args.inputpath);
    }

    if total_frame_count == 0 {
        total_frame_count = get_frame_count_duration(&args.inputpath);
    }

    let original_frame_rate = get_frame_rate(&args.inputpath);

    // Calculate steps
    let parts_num = (total_frame_count as f32 / args.segmentsize as f32).ceil() as i32;
    let last_part_size = (total_frame_count % args.segmentsize) as u32;
    let last_part_size = if last_part_size == 0 {
        args.segmentsize
    } else {
        last_part_size
    };

    let _codec = args.codec.clone();
    clear().expect("failed to clear screen");
    println!(
        "{}",
        format!(
            "{}/{}, {}, total segments: {}, last segment size: {}, codec: {} (ctrl+c to exit)",
            current_file_count,
            total_files,
            filename.green(),
            parts_num,
            last_part_size,
            _codec
        )
        .yellow()
    );

    {
        let mut unprocessed_indexes = Vec::new();
        for i in 0..parts_num {
            #[cfg(target_os = "linux")]
            let n = format!("{}/{}.{}", video_parts_path, i, &args.format);
            #[cfg(target_os = "windows")]
            let n = format!("{}\\{}.{}", video_parts_path, i, &args.format);
            let p = Path::new(&n);
            let frame_number = if i + 1 == parts_num {
                last_part_size
            } else {
                args.segmentsize
            };
            if !p.exists() {
                unprocessed_indexes.push(Segment {
                    index: i as u32,
                    size: frame_number as u32,
                });
            } else {
                let mut c = get_frame_count(&p.display().to_string());
                if c == 0 {
                    c = get_frame_count_tag(&p.display().to_string());
                }
                if c != frame_number {
                    fs::remove_file(p).expect("could not remove invalid part, maybe in use?");
                    println!("removed invalid segment file [{}] with {} frame size", i, c);
                    unprocessed_indexes.push(Segment {
                        index: i as u32,
                        size: frame_number as u32,
                    });
                }
            }
        }

        let count;
        if current_file_count == 1 {
            count = total_frames_count;
        } else {
            count = total_frames_count
                - vector_files_to_process_frames_count[(current_file_count - 2) as usize];
        }
        frame_position = (total_frames_count - count as u64)
            + (parts_num as usize - unprocessed_indexes.len()) as u64 * args.segmentsize as u64;

        let mut export_handle = thread::spawn(move || {});
        let mut merge_handle = thread::spawn(move || {});
        let total_frames_style = "[fram][{elapsed_precise}] [{wide_bar:.green/white}] {pos:>7}/{len:7} total frames             eta: {eta:<7}";
        let info_style = "[info][{elapsed_precise}] [{wide_bar:.green/white}] {pos:>7}/{len:7} processed segments       eta: {eta:<7}";
        let expo_style = "[expo][{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} exporting segment        {per_sec:<12}";
        let upsc_style = "[upsc][{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} upscaling segment        {per_sec:<12}";
        let merg_style = "[merg][{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} merging segment          {per_sec:<12}";
        let _alt_style = "[]{elapsed}] {wide_bar:.cyan/blue} {spinner} {percent}% {human_len:>7}/{human_len:7} {per_sec} {eta}";

        let m = MultiProgress::new();
        let pb = m.add(ProgressBar::new(parts_num as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(info_style)
                .unwrap()
                .progress_chars("#>-"),
        );
        let mut last_pb = pb.clone();

        //let progress_bar = m.insert_after(&last_pb, ProgressBar::new(total_files as u64));
        let progress_bar_frames =
            m.insert_after(&last_pb, ProgressBar::new(total_frames_count as u64));
        progress_bar_frames.set_style(
            ProgressStyle::default_bar()
                .template(total_frames_style)
                .unwrap()
                .progress_chars("#>-"),
        );
        progress_bar_frames.set_position(frame_position as u64);

        last_pb = progress_bar_frames.clone();

        // Initial export
        if !unprocessed_indexes.is_empty() {
            let index = unprocessed_indexes[0].index;
            let _inpt = &args.inputpath.clone();
            #[cfg(target_os = "linux")]
            let _outpt = format!("/dev/shm/tmp_frames/{}/frame%08d.png", index);
            #[cfg(target_os = "windows")]
            let _outpt = format!("temp\\tmp_frames\\{}\\frame%08d.png", index);
            let _start_time = if index == 0 {
                String::from("0")
            } else {
                ((index * args.segmentsize - 1) as f32
                    / original_frame_rate.parse::<f32>().unwrap())
                .to_string()
            };
            #[cfg(target_os = "linux")]
            let _index_dir = format!("/dev/shm/tmp_frames/{}", index);
            #[cfg(target_os = "windows")]
            let _index_dir = format!("temp\\tmp_frames\\{}", index);
            let _frame_number = unprocessed_indexes[0].size;

            let progress_bar = m.insert_after(&last_pb, ProgressBar::new(_frame_number as u64));
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template(expo_style)
                    .unwrap()
                    .progress_chars("#>-"),
            );
            last_pb = progress_bar.clone();

            fs::create_dir(&_index_dir).expect("could not create directory");

            // TODO LINUX: /dev/shm to export the frames
            // https://github.com/PauMAVA/cargo-ramdisk
            // Windows doesn't really have something native like a ramdisk sadly
            export_frames(
                &args.inputpath,
                &_outpt,
                &_start_time,
                &(_frame_number as u32),
                progress_bar,
            )
            .unwrap();
            m.clear().unwrap();
        }

        for _ in 0..unprocessed_indexes.len() {
            let segment = &unprocessed_indexes[0];
            export_handle.join().unwrap();
            if unprocessed_indexes.len() != 1 {
                let index = unprocessed_indexes[1].index;
                let _inpt = args.inputpath.clone();
                #[cfg(target_os = "linux")]
                let _outpt = format!("/dev/shm/tmp_frames/{}/frame%08d.png", index);
                #[cfg(target_os = "windows")]
                let _outpt = format!("temp\\tmp_frames\\{}\\frame%08d.png", index);
                let _start_time = ((index * args.segmentsize - 1) as f32
                    / original_frame_rate.parse::<f32>().unwrap())
                .to_string();
                #[cfg(target_os = "linux")]
                let _index_dir = format!("/dev/shm/tmp_frames/{}", index);
                #[cfg(target_os = "windows")]
                let _index_dir = format!("temp\\tmp_frames\\{}", index);
                let _frame_number = unprocessed_indexes[1].size;

                let progress_bar = m.insert_after(&last_pb, ProgressBar::new(_frame_number as u64));
                progress_bar.set_style(
                    ProgressStyle::default_bar()
                        .template(expo_style)
                        .unwrap()
                        .progress_chars("#>-"),
                );
                last_pb = progress_bar.clone();

                export_handle = thread::spawn(move || {
                    fs::create_dir(&_index_dir).expect("could not create directory");
                    export_frames(
                        &_inpt,
                        &_outpt,
                        &_start_time,
                        &(_frame_number as u32),
                        progress_bar,
                    )
                    .unwrap();
                });
            } else {
                export_handle = thread::spawn(move || {});
            }

            #[cfg(target_os = "linux")]
            let inpt_dir = format!("/dev/shm/tmp_frames/{}", segment.index);
            #[cfg(target_os = "linux")]
            let outpt_dir = format!("/dev/shm/out_frames/{}", segment.index);
            #[cfg(target_os = "windows")]
            let inpt_dir = format!("temp\\tmp_frames\\{}", segment.index);
            #[cfg(target_os = "windows")]
            let outpt_dir = format!("temp\\out_frames\\{}", segment.index);

            fs::create_dir(&outpt_dir).expect("could not create directory");

            let frame_number = unprocessed_indexes[0].size;

            let progress_bar = m.insert_after(&last_pb, ProgressBar::new(frame_number as u64));
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template(upsc_style)
                    .unwrap()
                    .progress_chars("#>-"),
            );
            last_pb = progress_bar.clone();

            frame_position = upscale_frames(
                &inpt_dir,
                &outpt_dir,
                &args.scale.to_string(),
                &args.model,
                progress_bar,
                progress_bar_frames.clone(),
                frame_position,
            )
            .expect("could not upscale frames");

            merge_handle.join().unwrap();

            let _codec = args.codec.clone();
            #[cfg(target_os = "linux")]
            let _inpt = format!("/dev/shm/out_frames/{}/frame%08d.png", segment.index);
            #[cfg(target_os = "linux")]
            let _outpt = format!("/dev/shm/video_parts/{}.{}", segment.index, &args.format);
            #[cfg(target_os = "windows")]
            let _inpt = format!("temp\\out_frames\\{}\\frame%08d.png", segment.index);
            #[cfg(target_os = "windows")]
            let _outpt = format!("temp\\video_parts\\{}.{}", segment.index, &args.format);
            let _frmrt = original_frame_rate.clone();
            let _crf = args.crf.clone().to_string();
            let _preset = args.preset.clone();
            let _x265_params = args.x265params.clone();
            let _extension = args.format.clone();

            let progress_bar = m.insert_after(&last_pb, ProgressBar::new(frame_number as u64));
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template(merg_style)
                    .unwrap()
                    .progress_chars("#>-"),
            );
            last_pb = progress_bar.clone();

            merge_handle = thread::spawn(move || {
                // 2022-03-28 07:12 c2d1597
                // https://github.com/AnimMouse/ffmpeg-autobuild/releases/download/m-2022-03-28-07-12/ffmpeg-c2d1597-651202b-win64-nonfree.7z
                fs::remove_dir_all(&inpt_dir).unwrap();
                if &_codec == "libsvt_hevc" {
                    merge_frames_svt_hevc(&_inpt, &_outpt, &_codec, &_frmrt, &_crf, progress_bar)
                        .unwrap();
                    fs::remove_dir_all(&outpt_dir).unwrap();
                } else if &_codec == "libsvtav1" {
                    merge_frames_svt_av1(&_inpt, &_outpt, &_codec, &_frmrt, &_crf, progress_bar)
                        .unwrap();
                    fs::remove_dir_all(&outpt_dir).unwrap();
                } else if &_codec == "libx265" {
                    merge_frames(
                        &_inpt,
                        &_outpt,
                        &_codec,
                        &_frmrt,
                        &_crf,
                        &_preset,
                        &_x265_params,
                        progress_bar,
                    )
                    .unwrap();
                    fs::remove_dir_all(&outpt_dir).unwrap();
                }
            });

            unprocessed_indexes.remove(0);
            pb.set_position((parts_num - unprocessed_indexes.len() as i32 - 1) as u64);
        }
        merge_handle.join().unwrap();
        m.clear().unwrap();
    }

    // Merge video parts
    let choosen_extension = &args.format;
    #[cfg(target_os = "linux")]
    let mut f_content = format!("file 'video_parts/0.{}'", choosen_extension);
    #[cfg(target_os = "windows")]
    let mut f_content = format!("file 'video_parts\\0.{}'", choosen_extension);

    for part_number in 1..parts_num {
        #[cfg(target_os = "linux")]
        let video_part_path = format!("video_parts/{}.{}", part_number, choosen_extension);
        #[cfg(target_os = "windows")]
        let video_part_path = format!("video_parts\\{}.{}", part_number, choosen_extension);
        f_content = format!("{}\nfile '{}'", f_content, video_part_path);
    }

    fs::write(txt_list_path, f_content).expect("Unable to write file");

    println!("merging video segments");
    {
        let mut count = 0;
        let p = Path::new(&temp_video_path);
        loop {
            thread::sleep(Duration::from_secs(1));
            if count == 5 {
                panic!("could not merge segments")
            } else if p.exists() {
                if fs::File::open(p).unwrap().metadata().unwrap().len() == 0 {
                    count += 1;
                } else {
                    break;
                }
            } else {
                if dar == "0" || dar == "N/A" {
                    merge_video_parts(&txt_list_path.to_string(), &temp_video_path.to_string());
                } else {
                    merge_video_parts_dar(
                        &txt_list_path.to_string(),
                        &temp_video_path.to_string(),
                        &dar,
                    );
                }
                count += 1;
            }
        }
    }

    //Check if there is invalid bin data in the input file
    let bin_data = get_bin_data(&args.inputpath);
    if bin_data != "" {
        println!("invalid data at index: {}, skipping this one", bin_data);
        println!("copying streams");
        copy_streams_no_bin_data(&temp_video_path.to_string(), &args.inputpath, &output_path);
    } else {
        println!("copying streams");
        copy_streams(&temp_video_path.to_string(), &args.inputpath, &output_path);
    }

    //Check if file has been copied successfully to output path, if so, update database
    let p = Path::new(&output_path);
    if p.exists() {
        if fs::File::open(p).unwrap().metadata().unwrap().len() == 0 {
            panic!("failed to copy streams");
        }
        // update sqlite database "reve.db" entry with status "done" using update_db_status function;
        let conn = Connection::open("reve.db").unwrap();
        let db_status = update_db_status(&conn, &args.inputpath, "done");
        match db_status {
            Ok(_) => println!("updated database"),
            Err(e) => println!("failed to update database: {}", e),
        }
    } else {
        panic!("failed to copy streams");
    }

    clear().expect("failed to clear screen");
    let elapsed = work_now.elapsed();
    let seconds = elapsed.as_secs() % 60;
    let minutes = (elapsed.as_secs() / 60) % 60;
    let hours = (elapsed.as_secs() / 60) / 60;

    let ancestors = Path::new(&args.inputpath).file_name().unwrap();
    println!(
        "done {:?} to {:?} in {}h:{}m:{}s",
        ancestors, done_output, hours, minutes, seconds
    );
}
