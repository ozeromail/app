#!/usr/bin/env bash
# Richer test mail for the UX tour (TourTest): a conversation, HTML, attachments, an invite,
# long text, mail in Sent, Drafts, Archive, Trash and Spam, and a second account.
set -euo pipefail
user=test@example.com
second=work@example.org
save() { docker exec -i ozero-dovecot doveadm save -u "${2:-$user}" ${1:+-m "$1"}; }
when() { date -R -d "-$1 minutes"; }

docker exec ozero-dovecot doveadm mailbox create -u "$user" Junk Receipts Travel 2>/dev/null || true

# A conversation of three, newest on top of the inbox.
printf 'From: Anna Petrova <anna@example.com>\r\nTo: %s\r\nSubject: Trip to the lake\r\nDate: %s\r\nMessage-ID: <trip-1@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nShall we go to the lake on Saturday? I can drive. We could leave at nine and be back by evening.\r\n' "$user" "$(when 50)" | save
printf 'From: Test <%s>\r\nTo: Anna Petrova <anna@example.com>\r\nSubject: Re: Trip to the lake\r\nDate: %s\r\nMessage-ID: <trip-2@example.com>\r\nIn-Reply-To: <trip-1@example.com>\r\nReferences: <trip-1@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nYes, nine is fine. I will bring sandwiches.\r\n\r\nOn Saturday, Anna Petrova wrote:\r\n> Shall we go to the lake on Saturday? I can drive.\r\n' "$user" "$(when 40)" | save Sent
printf 'From: Anna Petrova <anna@example.com>\r\nTo: %s\r\nCc: Boris Ivanov <boris@example.com>, Clara Mendes <clara@example.org>\r\nSubject: Re: Trip to the lake\r\nDate: %s\r\nMessage-ID: <trip-3@example.com>\r\nIn-Reply-To: <trip-2@example.com>\r\nReferences: <trip-1@example.com> <trip-2@example.com>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nGreat. Boris and Clara are coming too. See you at nine by the bakery.\r\n\r\n> Yes, nine is fine. I will bring sandwiches.\r\n' "$user" "$(when 5)" | save

# Attachments: a PDF and a picture.
pdf=$(printf '%%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj 2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj 3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%%%EOF\n' | base64 -w0)
png=$(python3 -c "import base64,zlib,struct
w,h=64,40
raw=b''.join(b'\x00'+bytes([40+x*3%200,120,200])*w for x in range(h))
def c(t,d):return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d))
print(base64.b64encode(b'\x89PNG\r\n\x1a\n'+c(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+c(b'IDAT',zlib.compress(raw))+c(b'IEND',b'')).decode())")
printf 'From: Boris Ivanov <boris@example.com>\r\nTo: %s\r\nSubject: Photos and the plan\r\nDate: %s\r\nMessage-ID: <files@example.com>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary="b1"\r\n\r\n--b1\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHere is the plan and a photo from last year.\r\n--b1\r\nContent-Type: application/pdf; name="plan.pdf"\r\nContent-Disposition: attachment; filename="plan.pdf"\r\nContent-Transfer-Encoding: base64\r\n\r\n%s\r\n--b1\r\nContent-Type: image/png; name="lake.png"\r\nContent-Disposition: attachment; filename="lake.png"\r\nContent-Transfer-Encoding: base64\r\n\r\n%s\r\n--b1--\r\n' "$user" "$(when 20)" "$pdf" "$png" | save

# An invitation.
printf 'From: Olga Smirnova <olga@example.org>\r\nTo: %s\r\nSubject: Invitation: Planning at 11:00\r\nDate: %s\r\nMessage-ID: <invite@example.org>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary="b2"\r\n\r\n--b2\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlanning for the next month.\r\n--b2\r\nContent-Type: text/calendar; charset=utf-8; method=REQUEST\r\n\r\nBEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:tour-1\r\nSUMMARY:Planning\r\nDTSTART:%s\r\nDTEND:%s\r\nLOCATION:Room 4\r\nORGANIZER;CN=Olga Smirnova:mailto:olga@example.org\r\nATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:%s\r\nATTENDEE;PARTSTAT=ACCEPTED:mailto:ivan@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n--b2--\r\n' \
  "$user" "$(when 30)" "$(date -u -d 'tomorrow 08:00' +%Y%m%dT%H%M%SZ)" "$(date -u -d 'tomorrow 09:00' +%Y%m%dT%H%M%SZ)" "$user" | save

# A long letter in Russian.
long=$(for i in $(seq 1 12); do printf 'Абзац %s. Озеро было тихим, и над водой стоял туман. Мы шли вдоль берега и говорили о том, как прошло лето, что получилось и что нет.\r\n\r\n' "$i"; done)
printf 'From: =?UTF-8?B?0JTQvNC40YLRgNC40Lkg0KHQvtC60L7Qu9C+0LI=?= <dmitri@example.net>\r\nTo: %s\r\nSubject: =?UTF-8?B?0J/QuNGB0YzQvNC+INC40Lcg0L/QvtC10LfQtNC60Lg=?=\r\nDate: %s\r\nMessage-ID: <long@example.net>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n%s\r\n' "$user" "$(when 35)" "$long" | save

# An HTML newsletter with a remote image.
printf 'From: Lake Weekly <news@lake.example>\r\nTo: %s\r\nSubject: This week by the water\r\nDate: %s\r\nMessage-ID: <html@lake.example>\r\nList-Unsubscribe: <mailto:unsubscribe@lake.example>\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body style="margin:0;font-family:Georgia,serif"><table width="600" align="center" style="background:#f4efe6"><tr><td style="padding:24px"><h1 style="color:#1d3557">This week by the water</h1><img src="https://example.com/banner.jpg" width="552" height="200" alt="Banner"><p>Five quiet places to swim, a recipe for fish soup and the weather for the weekend.</p><a href="https://example.com" style="display:inline-block;background:#2f8cff;color:#fff;padding:12px 20px;border-radius:6px;text-decoration:none">Read more</a></td></tr></table></body></html>\r\n' "$user" "$(when 45)" | save

# Flagged, Archive, Trash, Spam, Drafts.
printf 'From: Eva Novak <eva@example.com>\r\nTo: %s\r\nSubject: Keys for the cabin\r\nDate: %s\r\nMessage-ID: <flag@example.com>\r\n\r\nThe keys are under the blue pot.\r\n' "$user" "$(when 60)" | save
docker exec ozero-dovecot doveadm flags add -u "$user" '\Flagged' mailbox INBOX subject "Keys for the cabin"
for i in 1 2 3; do
  printf 'From: Shop <orders@shop.example>\r\nTo: %s\r\nSubject: Order %s shipped\r\nDate: %s\r\nMessage-ID: <arch-%s@shop.example>\r\n\r\nYour order is on its way.\r\n' "$user" "$i" "$(when $((i * 1500)))" "$i" | save Archive
done
printf 'From: Old Friend <old@example.com>\r\nTo: %s\r\nSubject: Old photos\r\nDate: %s\r\nMessage-ID: <trash@example.com>\r\n\r\nFound some old photos.\r\n' "$user" "$(when 3000)" | save Trash
printf 'From: Prize Desk <win@prize.example>\r\nTo: %s\r\nSubject: You have won a boat\r\nDate: %s\r\nMessage-ID: <spam@prize.example>\r\n\r\nClaim it now.\r\n' "$user" "$(when 100)" | save Junk
printf 'From: Test <%s>\r\nTo: Farid Aliyev <farid@example.org>\r\nSubject: Fishing gear\r\nDate: %s\r\nMessage-ID: <draft@example.com>\r\n\r\nDo you still have the spare rod?\r\n' "$user" "$(when 70)" | save Drafts
docker exec ozero-dovecot doveadm flags add -u "$user" '\Draft' mailbox Drafts all

# The second account, if the server takes any user.
second_account() {
docker exec ozero-dovecot doveadm mailbox create -u "$second" Sent Drafts Archive Trash Junk 2>/dev/null || true
for i in 1 2 3; do
  printf 'From: Team Lead <lead@company.example>\r\nTo: %s\r\nSubject: Weekly report %s\r\nDate: %s\r\nMessage-ID: <work-%s@company.example>\r\n\r\nPlease send the report by Friday.\r\n' "$second" "$i" "$(when $((i * 90)))" "$i" | save "" "$second"
done
}
second_account || echo "no second account"
echo "tour mail ready"
