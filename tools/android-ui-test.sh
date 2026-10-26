#!/usr/bin/env bash
# Runs the Android UI tests on the emulator adb sees, against the server of tools/ui-test-mail.sh.
# The films (a JSON of positions and a contact sheet per gesture) land in android/ui-films/.
set -uo pipefail

cd "$(dirname "$0")/../android"
# The test server's certificate is from the test CA, which the emulator trusts as a user-added
# one, as a person would add it. Needs an emulator image that allows root, as google_apis does.
ca=../core/adapters/imap/tests/dovecot/ca.crt
added="/data/misc/user/0/cacerts-added/$(openssl x509 -subject_hash_old -noout -in "$ca").0"
adb root >/dev/null
adb wait-for-device
adb shell mkdir -p /data/misc/user/0/cacerts-added
adb push "$ca" "$added" >/dev/null
adb shell chown system:system "$added"
adb shell chmod 644 "$added"
# The app reaches the test server on the emulator's own localhost, which the certificate names.
adb reverse tcp:31993 tcp:31993
adb reverse tcp:31587 tcp:31587
./gradlew :app:connectedDebugAndroidTest -Pozero.abis=x86_64 "$@"
status=$?
rm -rf ui-films
mkdir -p ui-films
find app/build/outputs/connected_android_test_additional_output -path '*films/*' -type f -exec cp {} ui-films/ \; 2>/dev/null
find app/build/outputs/androidTest-results/connected -name '*.xml' -exec cp {} ui-films/ \; 2>/dev/null
# A device gone offline would keep adb waiting until the job times out, and the report with it.
timeout 60 adb logcat -d > ui-films/logcat.txt
exit $status
