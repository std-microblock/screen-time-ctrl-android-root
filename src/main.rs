#![feature(random)]
use std::random;

use anyhow::Context;
use chrono::{Datelike, Timelike};
use fork::{daemon, Fork};
use serde::{Deserialize, Serialize};
mod android_dumpsys_usagestats_parser;

fn shell_exec(cmd: &str) -> String {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .expect("failed to execute process");

    String::from_utf8(output.stdout).unwrap()
}

fn shell_exec_stderr(cmd: &str) -> String {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .expect("failed to execute process");

    String::from_utf8(output.stderr).unwrap()
}

struct FocusedActivity {
    activity: String,
    package: String,
}

fn get_current_focused_activity() -> Option<FocusedActivity> {
    let cmd =
        "dumpsys window windows | grep -E 'mCurrentFocus|mFocusedApp|mInputMethodTarget|mSurface'";
    let output = shell_exec(cmd);

    let lines: Vec<&str> = output.split("\n").collect();
    /*
    output:
     mSurface=Surface(name=ScreenDecorOverlayBottom)/@0x8f5a317
     mSurface=Surface(name=ScreenDecorOverlay)/@0x745d0ed
     mSurface=Surface(name=NavigationBar0)/@0x1d076e
     mSurface=Surface(name=StatusBar)/@0xc6d3221
     mSurface=Surface(name=com.android.chrome/com.google.android.apps.chrome.Main)/@0x4647b7f
    */

    for line in lines {
        let inside_parentheses = line[6 + line.find("(name=")?..line.find(")")?].to_string();

        if inside_parentheses.contains("/") {
            let splited = inside_parentheses.split("/").collect::<Vec<&str>>();
            let package = splited[0];
            let activity = splited[1];
            return Some(FocusedActivity {
                activity: activity.trim().to_string(),
                package: package.trim().to_string(),
            });
        }
    }

    None
}

#[derive(Debug)]
struct AppUsageInfo {
    pub package: String,
    pub duration: i64,
}

fn get_app_usage_info() -> anyhow::Result<Vec<AppUsageInfo>> {
    let cmd = "dumpsys usagestats";
    let output = shell_exec(cmd);

    let events = android_dumpsys_usagestats_parser::parse(output.as_str())?;
    let mut app_usage_info_map = std::collections::HashMap::new();

    let mut activity_resume_map = std::collections::HashMap::new();

    for event in events.iter() {
        if event.event_type == "ACTIVITY_RESUMED" {
            activity_resume_map.insert(event.package.clone(), event.time.clone());
        } else if event.event_type == "ACTIVITY_PAUSED" {
            let package = &event.package;
            let end_time = &event.time;
            let start_time = activity_resume_map.remove(package);

            if let Some(start_time) = start_time {
                let mut start_time =
                    chrono::NaiveDateTime::parse_from_str(start_time.as_str(), "%Y-%m-%d %H:%M:%S")
                        .unwrap();

                // remove the record before 3:00AM of the current day
                let now = chrono::Local::now();
                let today = now.date_naive();
                let today_3am = today.and_hms_opt(3, 0, 0).context("hms opt failed")?;

                let mut end_time =
                    chrono::NaiveDateTime::parse_from_str(end_time.as_str(), "%Y-%m-%d %H:%M:%S")?;
                if today_3am < now.naive_local() && start_time < today_3am {
                    start_time = today_3am;
                    end_time = std::cmp::max(end_time, today_3am);
                }

                let duration = end_time.signed_duration_since(start_time).num_minutes();
                let app_usage_info = app_usage_info_map.entry(package).or_insert(0);
                *app_usage_info += duration;
            }
        }
    }

    let mut app_usage_info_vec = Vec::new();
    for (package, duration) in app_usage_info_map {
        app_usage_info_vec.push(AppUsageInfo {
            package: package.to_string(),
            duration,
        });
    }

    Ok(app_usage_info_vec)
}
#[derive(Debug, Serialize, Deserialize)]
struct AppUsageLimit {
    packages: Vec<String>,
    // in minutes
    max_duration_per_day: u32,
    // minutes, offset to the start of the day
    forbidden_time_ranges: Vec<(u32, u32)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    // minutes, offset to the start of the week
    free_time_range: (u32, u32),

    limits: Vec<AppUsageLimit>,

    sleep_time_range: Vec<(u32, u32)>,
}
static CONFIG_PATH: &str = "/data/media/0/limit_config.json";

fn run_main() {
    let default_config = Config {
        free_time_range: (0, 24 * 60),
        limits: vec![],
        sleep_time_range: vec![(0, 8 * 60)],
    };

    fn read_config() -> anyhow::Result<Config> {
        let config_str = std::fs::read_to_string(CONFIG_PATH)?;
        let config: Config = serde_json5::from_str(config_str.as_str())?;
        Ok(config)
    }

    match read_config() {
        Ok(config) => {
            println!("Running...{:?}\n\n", config);
            loop {
                let now = chrono::Local::now();
                let today = now.date_naive();
                let today_minutes = now.time().num_seconds_from_midnight() / 60;

                let today_weekday = today.weekday().num_days_from_monday();
                let today_weekday_minutes = today_weekday * 24 * 60 + today_minutes;

                println!("current time: {today}, today_mi: {today_minutes}, {today_weekday}, today_wdmi: {today_weekday_minutes}");

                // if in free time range, do not force stop
                if today_weekday_minutes >= config.free_time_range.0
                    && today_weekday_minutes <= config.free_time_range.1
                {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    continue;
                }

                let app_usage_info = get_app_usage_info().unwrap();
                if let Some(focused_activity) = get_current_focused_activity() {
                    println!("focused: {}", focused_activity.package);

                    let mut forbidden = false;

                    for app_limit in config
                        .limits
                        .iter()
                        .filter(|app_limit| app_limit.packages.contains(&focused_activity.package))
                    {
                        println!("matched limit: {:?}", app_limit);
                        let app_usage = app_usage_info
                            .iter()
                            .find(|app_usage| app_usage.package == focused_activity.package.clone());

                        if let Some(app_usage) = app_usage {
                            if app_usage.duration > app_limit.max_duration_per_day as i64 {
                                forbidden = true;
                            }
                        }

                        for (start, end) in app_limit.forbidden_time_ranges.iter() {
                            if today_minutes >= *start && today_minutes <= *end {
                                forbidden = true;
                            }
                        }
                    }

                    for (start, end) in config.sleep_time_range.iter() {
                        if today_minutes >= *start && today_minutes <= *end {
                            println!("sleep time, shutdown");
                            shell_exec("svc power shutdown");
                            std::thread::sleep(std::time::Duration::from_secs(5));
                            return;
                        }
                    }

                    if forbidden {
                        println!("forbidden: {}", focused_activity.package);
                        let cmd = format!("am force-stop {}", focused_activity.package);
                        shell_exec(cmd.as_str());

                        std::thread::sleep(std::time::Duration::from_secs(1));
                    } else {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                } else {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            }
        }
        Err(e) => {
            println!("Failed to read config: {:?}", e);
            let config_str = serde_json::to_string(&default_config).unwrap();
            std::fs::write(CONFIG_PATH, config_str).unwrap();
        }
    }
}

static startup_script: &str = r#"
#!/system/bin/sh
/data/local/tmp/limit_app &
"#;
static startup_script_path: &str = "/data/adb/service.d/limit_app.sh";

lazy_static::lazy_static! {
    static ref BINARY_DATA: Vec<u8> = {
        std::fs::read(std::env::current_exe().unwrap()).unwrap()
    };
}

fn reinstall() {
    // put startup script to /data/adb/service.d and copy binary to /data/local/tmp
    let cur_bin = std::env::current_exe().unwrap();

    std::fs::write(startup_script_path, startup_script).unwrap();
    shell_exec(format!("chmod 777 {}", startup_script_path).as_str());

    let _ = std::fs::write("/data/local/tmp/limit_app", BINARY_DATA.as_slice());
    shell_exec("chmod 777 /data/local/tmp/limit_app");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 {
        let tmp_dir = "/data/local/tmp";
        let cur_bin = std::env::current_exe().unwrap();
        shell_exec(format!("rm -rf {}/stc-f-*", tmp_dir).as_str());

        let mut fork_binary = || {
            let cur_bin = std::env::current_exe().unwrap();
            let random_name: String = (0..8)
                .map(|_| {
                    let idx = random::random::<u8>() % 62;
                    let c = if idx < 26 {
                        (b'a' + idx) as char
                    } else if idx < 52 {
                        (b'A' + idx - 26) as char
                    } else {
                        (b'0' + idx - 52) as char
                    };
                    c
                })
                .collect();

            let new_bin = format!("{}/stc-f-{}", tmp_dir, random_name);
            std::fs::copy(cur_bin, new_bin.as_str()).unwrap();
            shell_exec(format!("chmod 777 {}", new_bin).as_str());

            new_bin
        };

        let mut cmd = std::process::Command::new(fork_binary());
        let cmd = cmd.arg("main");
        let pid = cmd.spawn().unwrap().id();
        println!("pid: {}", pid);

        let mut cmd = std::process::Command::new(fork_binary());
        let cmd = cmd.arg("daemon1");
        let pid = cmd.spawn().unwrap().id();
        println!("pid2: {}", pid);

        let mut cmd = std::process::Command::new(fork_binary());
        let cmd = cmd.arg("daemon2").arg(pid.to_string());
        let pid = cmd.spawn().unwrap().id();
        println!("pid3: {}", pid);
    } else if args.len() == 2 {
        let typ = args[1].as_str();
        if typ == "main" {
            run_main();
        } else if typ == "daemon1" {
            loop {
                reinstall();
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        } else if typ == "daemon2" {
            let pid = args[2].parse::<i32>().unwrap();
            loop {
                let output = shell_exec_stderr(format!("kill -0 {pid}").as_str());
                if output.len() != 0 {
                    let mut cmd = std::process::Command::new("/data/local/tmp/limit_app");
                    let pid = cmd.spawn().unwrap().id();
                    println!("pid: {}", pid);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        } else if typ == "statistic" {
            let app_usage_info = get_app_usage_info().unwrap();
            for app_usage in app_usage_info {
                println!("{:?}", app_usage);
            }
        }
    }
}
