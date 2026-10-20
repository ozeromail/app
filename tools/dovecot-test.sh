#!/usr/bin/env bash
# Starts a Dovecot with test mail and a Mailpit SMTP server for the IMAP and SMTP tests:
#   tools/dovecot-test.sh && cd core && cargo test -p ozero-imap -p ozero-api -- --ignored
set -euo pipefail

dir="$(cd "$(dirname "$0")/../core/adapters/imap/tests/dovecot" && pwd)"
user=test@example.com

docker rm -f ozero-dovecot ozero-mailpit >/dev/null 2>&1 || true
# SMTP with STARTTLS on 31587 that takes any login; its HTTP API on 31825 lists what arrived.
docker run -d --name ozero-mailpit \
  -p 31587:1025 -p 31825:8025 \
  -v "$dir/tls.crt:/tls.crt:ro" -v "$dir/tls.key:/tls.key:ro" \
  -e MP_SMTP_TLS_CERT=/tls.crt -e MP_SMTP_TLS_KEY=/tls.key \
  -e MP_SMTP_REQUIRE_STARTTLS=true -e MP_SMTP_AUTH_ACCEPT_ANY=true \
  axllent/mailpit:v1.27.1 >/dev/null
docker run -d --name ozero-dovecot -e USER_PASSWORD=pass \
  -p 31993:31993 \
  -v "$dir/test.conf:/etc/dovecot/conf.d/test.conf:ro" \
  -v "$dir/tls.crt:/etc/dovecot/ssl/tls.crt:ro" \
  -v "$dir/tls.key:/etc/dovecot/ssl/tls.key:ro" \
  dovecot/dovecot:2.4.1 >/dev/null

ready=
for _ in $(seq 30); do
  docker exec ozero-dovecot doveadm mailbox list -u "$user" >/dev/null 2>&1 && ready=1 && break
  sleep 1
done
if [ -z "$ready" ]; then
  echo "Dovecot did not start:" >&2
  docker logs ozero-dovecot >&2
  exit 1
fi

doveadm() { docker exec -i ozero-dovecot doveadm "$@"; }
doveadm mailbox create -u "$user" Sent Отчёты Empty Archive Moves Changes Gone Waits Deletes
for i in 1 2 3; do
  printf 'From: =?UTF-8?B?0JDQvdC90LA=?= <anna@example.com>\r\nTo: %s\r\nSubject: Message %s\r\nDate: Wed, 14 Oct 2026 10:0%s:00 +0300\r\nMessage-ID: <%s@example.com>\r\n\r\nHello %s\r\n' \
    "$user" "$i" "$i" "$i" "$i" | doveadm save -u "$user"
done
doveadm flags add -u "$user" '\Seen \Flagged' mailbox INBOX uid 3
for i in 1 2; do
  printf 'From: anna@example.com\r\nSubject: Change %s\r\n\r\nHello\r\n' "$i" | doveadm save -u "$user" -m Changes
done
for i in 1 2 3 4; do
  printf 'From: anna@example.com\r\nSubject: Delete %s\r\n\r\nHello\r\n' "$i" | doveadm save -u "$user" -m Deletes
done
printf 'From: anna@example.com\r\nSubject: Move me\r\nList-Unsubscribe: <mailto:u@example.com>\r\n\r\nHello\r\n' | doveadm save -u "$user" -m Moves
