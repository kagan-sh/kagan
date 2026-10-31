#!/bin/sh
# Print the open pull request for $CIRCLE_BRANCH as one line of JSON, or nothing
# when the branch has none. The repo is private, so the kagan-bot GitHub App signs in:
# KAGAN_BOT_APP_ID and KAGAN_BOT_KEY (the PEM private key, base64 on one line) are
# CircleCI project variables. The token it gets can only read pull requests.
set -eu

if [ -z "${KAGAN_BOT_APP_ID:-}" ] || [ -z "${KAGAN_BOT_KEY:-}" ]; then
  echo "KAGAN_BOT_APP_ID and KAGAN_BOT_KEY are not both set. Add them as CircleCI project variables." >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
umask 077

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

printf '%s' "$KAGAN_BOT_KEY" | openssl base64 -d -A > "$work/key.pem"
now=$(date +%s)
printf '%s.%s' \
  "$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)" \
  "$(printf '{"iat":%s,"exp":%s,"iss":%s}' $((now - 60)) $((now + 540)) "$KAGAN_BOT_APP_ID" | b64url)" > "$work/unsigned"
openssl dgst -sha256 -sign "$work/key.pem" -out "$work/sig" "$work/unsigned"
jwt="$(cat "$work/unsigned").$(b64url < "$work/sig")"

# Each curl runs on its own so a refusal fails the script; in a pipe only jq's status would count.
found=$(curl -fsS -H "Authorization: Bearer $jwt" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/${CIRCLE_PROJECT_USERNAME}/${CIRCLE_PROJECT_REPONAME}/installation")
installation=$(printf '%s' "$found" | jq -er .id)

minted=$(curl -fsS -X POST -H "Authorization: Bearer $jwt" -H "Accept: application/vnd.github+json" \
  -d "{\"repositories\":[\"${CIRCLE_PROJECT_REPONAME}\"],\"permissions\":{\"pull_requests\":\"read\"}}" \
  "https://api.github.com/app/installations/${installation}/access_tokens")
token=$(printf '%s' "$minted" | jq -er .token)

pulls=$(curl -fsS -H "Authorization: Bearer $token" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/${CIRCLE_PROJECT_USERNAME}/${CIRCLE_PROJECT_REPONAME}/pulls?state=open&head=${CIRCLE_PROJECT_USERNAME}:${CIRCLE_BRANCH}")
printf '%s' "$pulls" | jq -c '.[0] // empty'
