cross build --target aarch64-linux-android
adb shell rm /data/tmp/stca
adb push ./target/aarch64-linux-android/debug/screen-time-ctrl-android-root /data/tmp/stca
adb shell chmod 777 /data/tmp/stca
adb shell su -c /data/tmp/stca main