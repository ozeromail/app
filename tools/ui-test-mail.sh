#!/usr/bin/env bash
# The test server of tools/dovecot-test.sh, with an inbox full enough to scroll, swipe and switch
# tabs in, for the Android UI tests: tools/ui-test-mail.sh && tools/android-ui-test.sh
set -euo pipefail

"$(dirname "$0")/dovecot-test.sh"

user=test@example.com
save() { docker exec -i ozero-dovecot doveadm save -u "$user"; }

# $1 from, $2 subject, $3 hours ago, $4 extra headers, $5 body.
letter() {
  printf 'From: %s\r\nTo: %s\r\nSubject: %s\r\nDate: %s\r\nMessage-ID: <ui-%s@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n%b\r\n%s\r\n' \
    "$1" "$user" "$2" "$(date -R -d "-$3 hours")" "$RANDOM$RANDOM" "$4" "$5" | save
}

people=("Anna Petrova <anna@example.com>" "Boris Ivanov <boris@example.com>" "Clara Mendes <clara@example.org>"
  "Dmitri Sokolov <dmitri@example.net>" "Eva Novak <eva@example.com>" "Farid Aliyev <farid@example.org>")
for i in $(seq 1 24); do
  from="${people[$((i % ${#people[@]}))]}"
  letter "$from" "People $i: plans for the weekend" "$i" "" "Hi! This is letter $i. Shall we meet on Saturday by the lake?"
done
for i in $(seq 1 8); do
  letter "Bank <noreply@bank.example>" "Notice $i: your statement is ready" "$((i * 3))" "" "Your statement $i is ready."
done
for i in $(seq 1 8); do
  letter "Weekly Digest <digest@news.example>" "Digest $i: stories of the week" "$((i * 5))" \
    "List-Unsubscribe: <mailto:unsubscribe@news.example>\r\n" "Ten stories worth reading this week."
done
# Read mail, so a swipe can mark it unread too.
docker exec ozero-dovecot doveadm flags add -u "$user" '\Seen' mailbox INBOX subject "People 2:"
