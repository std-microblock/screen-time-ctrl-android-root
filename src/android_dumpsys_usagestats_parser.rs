/*
user=0
  Last 24 hour events (timeRange="2024/11/23 01:59 – 2024/11/24 01:59" )
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=h839dp-nowidecg-lowdr-notouch-keyshidden-v34 flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=finger-v34 flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=night-v34 flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=keysexposed-v34 flags=0x0
    time="2024-11-23 10:44:56" type=SCREEN_INTERACTIVE package=android flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=v34 flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=widecg-highdr-v34 flags=0x0
    time="2024-11-23 10:44:56" type=ACTIVITY_RESUMED package=com.android.settings class=com.android.settings.FallbackHome instanceId=225113592 taskRootPackage=com.android.settings taskRootClass=com.android.settings.FallbackHome flags=0x0
    time="2024-11-23 10:44:56" type=CONFIGURATION_CHANGE package=android config=h815dp-v34 flags=0x0
    time="2024-11-23 10:44:56" type=KEYGUARD_SHOWN package=android flags=0x0
    time="2024-11-23 10:44:56" type=ACTIVITY_PAUSED package=com.android.settings class=com.android.settings.FallbackHome instanceId=225113592 taskRootPackage=com.android.settings taskRootClass=com.android.settings.FallbackHome flags=0x0
    time="2024-11-23 10:44:56" type=ACTIVITY_STOPPED package=com.android.settings class=com.android.settings.FallbackHome instanceId=225113592 taskRootPackage=com.android.settings taskRootClass=com.android.settings.FallbackHome flags=0x0
    time="2024-11-23 10:44:56" type=STANDBY_BUCKET_CHANGED package=com.bootleggers.shishufied.clockfont.alienleague standbyBucket=50 reason=t flags=0x0
    time="2024-11-23 10:44:56" type=STANDBY_BUCKET_CHANGED package=com.android.internal.display.cutout.emulation.noCutout standbyBucket=50 reason=t flags=0x0
    time="2024-11-23 10:44:56" type=STANDBY_BUCKET_CHANGED package=com.amazon.mShop.andro
     */

use anyhow::Context;

pub struct UsageStatsEvent {
    pub time: String,
    pub event_type: String,
    pub package: String,
}

fn parse_usagestats_event(line: &str) -> anyhow::Result<UsageStatsEvent> {
    let get_by_key = |start: &str, end: &str| -> Option<String> {
        let start_index = line.find(start)?;
        let end_index = line[start_index + start.len()..].find(end)?;
        Some(line[start_index + start.len()..start_index + start.len() + end_index].to_string())
    };
    let time = get_by_key("time=\"", "\"").context("Failed to get time")?;
    let event_type = get_by_key("type=", " ").context("Failed to get event_type")?;
    let package = get_by_key("package=", " ").context("Failed to get package")?;

    Ok(UsageStatsEvent {
        time,
        event_type,
        package,
    })
}

pub fn parse(output: &str) -> anyhow::Result<Vec<UsageStatsEvent>> {
    let mut events = Vec::new();
    // start from Last 24 hour events , end at the next line with timeRange=
    let lines = output.split("\n").collect::<Vec<&str>>();

    let lines_recent = lines
        .iter()
        .skip_while(|line| !line.contains("Last 24 hour events"))
        .skip(1)
        .take_while(|line| !line.contains("timeRange="));

    for line in lines_recent {
        if let Ok(event) = parse_usagestats_event(line) {
            events.push(event);
        }
    }

    Ok(events)
}
