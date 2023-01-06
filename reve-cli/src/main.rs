use clap::Parser;
use clearscreen::clear;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reve_shared::*;
use rusqlite::{params, Connection};
use std::fs;
use std::fs::metadata;
use std::io::ErrorKind;
use std::path::Path;
use std::process::exit;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn main() {
    pre_work();
}
