#!/usr/bin/env bash
# Starts a Dovecot with test mail for the IMAP tests:
#   tools/dovecot-test.sh && cd core && cargo test -p ozero-imap -p ozero-api -- --ignored
set -euo pipefail

dir="$(cd "$(dirname "$0")/../core/adapters/imap/tests/dovecot" && pwd)"
user=test@example.com

docker rm -f ozero-dovecot >/dev/null 2>&1 || true
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
doveadm mailbox create -u "$user" Sent Отчёты Empty
for i in 1 2 3; do
  printf 'From: =?UTF-8?B?0JDQvdC90LA=?= <anna@example.com>\r\nTo: %s\r\nSubject: Message %s\r\nDate: Wed, 14 Oct 2026 10:0%s:00 +0300\r\nMessage-ID: <%s@example.com>\r\n\r\nHello %s\r\n' \
    "$user" "$i" "$i" "$i" "$i" | doveadm save -u "$user"
done
doveadm flags add -u "$user" '\Seen \Flagged' mailbox INBOX uid 3
