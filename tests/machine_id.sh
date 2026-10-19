#!/usr/bin/env bash
# copy_rootfs copies the live system's machine-id; set_machine_id must replace it with a fresh one
# that /etc and dbus share.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$HERE/resources/backend/backend_install.sh"

eval "$(sed -n '/^set_machine_id()/,/^}/p' "$SCRIPT")"
type set_machine_id >/dev/null 2>&1 || { echo "FAIL set_machine_id is not defined in the backend"; exit 1; }

W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT
TARGETDIR="$W/target"
mkdir -p "$TARGETDIR/etc" "$TARGETDIR/var/lib/dbus"
live=139b00c21f796e011ba8871c6ad69682
echo "$live" > "$TARGETDIR/etc/machine-id"
echo "$live" > "$TARGETDIR/var/lib/dbus/machine-id"

fails=0
check() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi; }

set_machine_id
id="$(cat "$TARGETDIR/etc/machine-id")"
check "new id differs from the live one" "$([ "$id" != "$live" ] && echo yes)" "yes"
check "id is 32 lowercase hex digits" "$(grep -cxE '[0-9a-f]{32}' "$TARGETDIR/etc/machine-id")" "1"
check "dbus id links to /etc/machine-id" "$(readlink "$TARGETDIR/var/lib/dbus/machine-id")" "/etc/machine-id"

[ "$fails" = 0 ] && echo "all passed" || exit 1
