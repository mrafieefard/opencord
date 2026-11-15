#!/usr/bin/env bash
# The update key's checks, run by the release job before anything is
# released (.github/workflows/cicd.yml, docs/updater.md).
#
#   update-key.sh match <pem-file> <public-key-base64>
#     The private key in <pem-file> (UPDATE_SIGNING_KEY) belongs to
#     <public-key-base64> (UPDATE_PUBLIC_KEY).
#   update-key.sh verify <public-key-base64> <file> <sig-file>
#     <sig-file>, base64 as release.json.sig is published, is a good
#     signature of <file> by <public-key-base64>.
#
# <public-key-base64> is base64 of the raw 32 bytes, or of the 44 bytes of
# DER `openssl pkey -pubout -outform DER` writes, as the launcher's build.rs
# takes it. The key is its last 32 bytes. A failed check prints an
# ::error:: line and exits 1. Nothing here prints key material.
set -euo pipefail

# SubjectPublicKeyInfo for Ed25519, before the 32 raw bytes.
der_prefix=302a300506032b6570032100
not_a_key="UPDATE_PUBLIC_KEY is not a base64 Ed25519 public key (raw 32 bytes or DER)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "::error::$1"
  exit 1
}

usage() {
  echo "usage: update-key.sh match <pem-file> <public-key-base64>" >&2
  echo "       update-key.sh verify <public-key-base64> <file> <sig-file>" >&2
  exit 2
}

size() { echo $(($(wc -c < "$1"))); }

starts_with_der_prefix() {
  [[ $(head -c 12 "$1" | od -An -tx1 | tr -d ' \n') == "$der_prefix" ]]
}

# The variable's 32 raw bytes, to $tmp/public.
public_key() {
  local text
  text=$(printf '%s' "$1" | tr -d '[:space:]')
  [[ -n "$text" ]] || fail "$not_a_key"
  printf '%s' "$text" | base64 -d > "$tmp/given" 2> /dev/null || fail "$not_a_key"
  case $(size "$tmp/given") in
    32) cp "$tmp/given" "$tmp/public" ;;
    44)
      starts_with_der_prefix "$tmp/given" || fail "$not_a_key"
      tail -c 32 "$tmp/given" > "$tmp/public"
      ;;
    *) fail "$not_a_key" ;;
  esac
}

match() {
  local pem=$1
  public_key "$2"
  openssl pkey -in "$pem" -pubout -outform DER > "$tmp/derived.der" 2> /dev/null \
    || fail "UPDATE_SIGNING_KEY is not a private key in PEM"
  if [[ $(size "$tmp/derived.der") != 44 ]] || ! starts_with_der_prefix "$tmp/derived.der"; then
    fail "UPDATE_SIGNING_KEY is not an Ed25519 key"
  fi
  tail -c 32 "$tmp/derived.der" > "$tmp/derived"
  cmp -s "$tmp/derived" "$tmp/public" || fail "UPDATE_PUBLIC_KEY doesn't match UPDATE_SIGNING_KEY"
}

verify() {
  local file=$2 sig=$3
  public_key "$1"
  # A PEM rebuilt from the variable alone, not from the secret.
  { printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; cat "$tmp/public"; } > "$tmp/public.der"
  openssl pkey -pubin -inform DER -in "$tmp/public.der" -out "$tmp/public.pem" 2> /dev/null \
    || fail "$not_a_key"
  tr -d '[:space:]' < "$sig" | base64 -d > "$tmp/sig" 2> /dev/null \
    || fail "$(basename "$sig") is not base64"
  openssl pkeyutl -verify -pubin -inkey "$tmp/public.pem" -rawin -in "$file" \
    -sigfile "$tmp/sig" > /dev/null 2>&1 \
    || fail "$(basename "$sig") doesn't verify against UPDATE_PUBLIC_KEY"
}

case "${1:-}" in
  match)
    [[ $# -eq 3 ]] || usage
    match "$2" "$3"
    ;;
  verify)
    [[ $# -eq 4 ]] || usage
    verify "$2" "$3" "$4"
    ;;
  *) usage ;;
esac
