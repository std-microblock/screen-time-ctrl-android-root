The real strong screen time limit, with no app, requires magisk.

## Usage

```sh
# push config
adb push <path-to-your-config> /data/media/0/limit_config.json
# install the limit_app
adb push screen-time-ctrl-android-root /data/tmp/limit_app
adb shell su -c /data/tmp/limit_app &
```

## Config example

```json
{
    "free_time_range": [
        // offset by the start of a week, during the time do not limit anything
        0, 100
    ],
    "limits": [
        // rule for test
        {
            "package": [
                // packages applying the rule
                "mark.via"
            ],
            // the app can be used for 2 minutes per day
            "max_duration_per_day": 2,
            "forbidden_time_ranges": [
                // forbit use during 00:30 ~ 01:00 each day
                [30, 60]
            ]
        },
    ],
    "sleep_time_range": [
        // shutdown during 11:30 PM to 12:00 PM 
        [
            1410,
            1460
        ]
    ]
}
```