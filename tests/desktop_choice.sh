#!/usr/bin/env bash
# The desktop and login manager of the new system: which desktop the copy brought, what is installed and
# removed for another one (or none), which login manager starts, and how autologin is set up for each.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$HERE/resources/backend/backend_install.sh"

FUNCS="enable_service disable_service enable_service_checked find_shipped_desktop check_desktop_choice new_system_session
installed_package_names install_desktop_packages remove_present_packages apply_desktop set_lightdm_option
set_login_manager set_greetd_autologin set_autologin reset_live_autologin"
for f in $FUNCS; do
	eval "$(sed -n "/^$f()/,/^}/p" "$SCRIPT")"
	type "$f" >/dev/null 2>&1 || { echo "FAIL $f is not defined in the backend"; exit 1; }
done

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
fails=0
check() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got '$2', want '$3'"; fails=$((fails + 1)); fi; }

DISTRO_CONF_DEFAULT="$HERE/resources/backend/distro.conf"; DISTRO_CONF_OVERRIDE=/nonexistent
. "$HERE/resources/backend/distro_config.sh"
declare -A MIRRORS=([Default]="https://repo-default.voidlinux.org/")
TARGETDIR="$T/target"; INIT_SYSTEM=runit

# the live image and the target: a package set each, xbps calls recorded
LIVE=" "; INSTALLED=" "; CALLS=""; LOGS=""; DIED=""; REFUSE=""
declare -A OPTS
get_option() { echo "${OPTS[$1]:-}"; }
log_ui() { LOGS="$LOGS|$*"; }
die() { DIED="$1"; return 1; }
xbps-query() { case "$LIVE" in *" $1 "*) return 0 ;; esac; return 1; }
chroot() {
	shift
	case "$1" in
		xbps-query)
			if [ "$2" = -l ]; then for p in $INSTALLED; do echo "ii $p-1.0_1 x"; done; return 0; fi
			case "$INSTALLED" in *" $2 "*) return 0 ;; esac; return 1 ;;
		xbps-install)
			CALLS="$CALLS|$*"; local a skip=0
			[ "$2" = -C ] && SEEN_CONF="$(cat "$TARGETDIR$3"/*.conf 2>/dev/null)"
			for a in "${@:2}"; do
				if [ "$skip" = 1 ]; then skip=0; continue; fi
				case "$a" in -C) skip=1 ;; -*) ;; *) INSTALLED="$INSTALLED$a " ;; esac
			done ;;
		xbps-remove)
			local a
			for a in "${@:3}"; do case " $REFUSE " in *" $a "*) CALLS="$CALLS|refused $*"; return 1 ;; esac; done
			CALLS="$CALLS|$*"
			for a in "${@:3}"; do INSTALLED="${INSTALLED/ $a / }"; done ;;
		xbps-pkgdb) CALLS="$CALLS|$*" ;;
	esac
}
reset() {
	LIVE=" "; INSTALLED=" "; CALLS=""; LOGS=""; DIED=""; REFUSE=""; OPTS=([UPDATE]=1 [MIRROR]=Default [USERLOGIN]=gui)
	rm -rf "$TARGETDIR"; mkdir -p "$TARGETDIR/etc/runit/runsvdir/default"
	for s in lightdm gdm sddm cosmic-greeter cosmic-greeter-daemon; do mkdir -p "$TARGETDIR/etc/sv/$s"; done
}
xfce_iso() { LIVE=" xfce4-session xfce-core lightdm lightdm-gtk3-greeter xorg-minimal firefox "; INSTALLED="$LIVE"; }
enabled() { ls "$TARGETDIR/etc/runit/runsvdir/default" | tr '\n' ' '; }

# ---- which desktop the live image has
reset; xfce_iso
check "the Xfce image ships xfce" "$(find_shipped_desktop)" "xfce"
reset; LIVE=" gnome-shell gdm "
check "the GNOME image ships the first GNOME (voidlab), not its stock variant" "$(find_shipped_desktop)" "gnome"
reset
check "an image without a desktop ships none" "$(find_shipped_desktop)" ""

# ---- choices are checked before anything is touched
reset; xfce_iso; OPTS[DESKTOP]=kde; OPTS[UPDATE]=0; check_desktop_choice
check "offline, another desktop stops the install" "$DIED" "The kde desktop is not on the live image and needs an online mirror"
reset; xfce_iso; OPTS[DESKTOP]=xfce; OPTS[DISPLAYMANAGER]=sddm; OPTS[UPDATE]=0; check_desktop_choice
check "offline, a login manager the image lacks stops it" "$DIED" "The sddm login manager is not on the live image and needs an online mirror"
reset; xfce_iso; OPTS[DESKTOP]=none; OPTS[DISPLAYMANAGER]=lightdm; check_desktop_choice
check "a login manager without a desktop stops it" "$DIED" "A login manager needs a desktop"
reset; xfce_iso; OPTS[DESKTOP]=windows; check_desktop_choice
check "an unknown desktop stops it" "$DIED" "distro.conf defines no desktop windows"
reset; xfce_iso; OPTS[DESKTOP]=xfce; OPTS[DISPLAYMANAGER]=lightdm; OPTS[UPDATE]=0; check_desktop_choice
check "offline, the shipped desktop and its login manager pass" "$DIED|$SHIPPED_DESKTOP" "|xfce"

# ---- the desktop of the new system
reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=xfce; OPTS[DISPLAYMANAGER]=lightdm; apply_desktop >/dev/null
check "the shipped desktop with its login manager: nothing to do" "$CALLS" ""

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=kde; OPTS[DISPLAYMANAGER]=sddm; apply_desktop >/dev/null
check "KDE replaces Xfce: KDE and SDDM are installed (xorg-minimal is already there)" \
	"$(printf '%s' "$CALLS" | cut -d'|' -f2)" "xbps-install -C /tmp/voyage-xbps.d -Sy plasma-workspace kde-plasma sddm"
check "then Xfce and the unused LightDM go, Xorg (SDDM needs it) stays" \
	"$(printf '%s' "$CALLS" | cut -d'|' -f3)" "xbps-remove -Ry xfce4-session xfce-core lightdm lightdm-gtk3-greeter"
check "firefox stays when a desktop remains" "$(case "$INSTALLED" in *" firefox "*) echo kept ;; esac)" "kept"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=kde; OPTS[KEEPDESKTOP]=1; OPTS[DISPLAYMANAGER]=sddm; apply_desktop >/dev/null
check "keeping Xfce: KDE is added and nothing is removed" "$(printf '%s' "$CALLS" | grep -c xbps-remove)" "0"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=none; OPTS[DISPLAYMANAGER]=none; apply_desktop >/dev/null
check "no desktop: nothing is installed" "$(printf '%s' "$CALLS" | grep -c xbps-install)" "0"
check "and the desktop, login manager, Xorg and graphical applications are removed" "$CALLS" \
	"|xbps-remove -Ry xfce4-session xfce-core xorg-minimal lightdm lightdm-gtk3-greeter firefox"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=none; OPTS[DISPLAYMANAGER]=none; REFUSE="xorg-minimal"; apply_desktop >/dev/null
check "what xbps will not remove is kept and reported, the rest still goes" \
	"$(printf '%s' "$INSTALLED" | xargs)|$LOGS" "xorg-minimal||DESKTOP_REMOVE|Kept because other packages still need them: xorg-minimal"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=gnome-void; OPTS[DISPLAYMANAGER]=gdm; OPTS[NONFREE]=1; apply_desktop >/dev/null
check "stock GNOME comes from Void's repositories only" "$(printf '%s' "$CALLS" | cut -d'|' -f2)" \
	"xbps-install -C /tmp/voyage-xbps.d -Sy --ignore-conf-repos --repository=https://repo-default.voidlinux.org/current --repository=https://repo-default.voidlinux.org/current/nonfree gnome-shell gnome-core gdm"
check "and every package it brought is repolocked" "$(printf '%s' "$CALLS" | grep -o 'xbps-pkgdb -m repolock [a-z-]*' | awk '{print $4}' | sort | xargs)" "gdm gnome-core gnome-shell"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=cosmic; OPTS[DISPLAYMANAGER]=cosmic-greeter; apply_desktop >/dev/null
check "COSMIC from the configured repositories (voidlab carries it)" "$(printf '%s' "$CALLS" | cut -d'|' -f2)" \
	"xbps-install -C /tmp/voyage-xbps.d -Sy cosmic-session cosmic-desktop cosmic-greeter"

reset; xfce_iso; SHIPPED_DESKTOP=xfce; OPTS[DESKTOP]=kde; OPTS[DISPLAYMANAGER]=sddm; mkdir -p "$TARGETDIR/etc/xbps.d"
printf 'ignorepkg=base-system\nignorepkg=sddm\nignorepkg=AppStream\nrepository=https://example.org/repo\n' > "$TARGETDIR/etc/xbps.d/00-vessel.conf"
apply_desktop >/dev/null
check "the live image's ignorepkg lines do not apply to the desktop transaction" "$(printf '%s' "$SEEN_CONF" | grep -c ignorepkg)|$(printf '%s' "$SEEN_CONF" | grep -c repository=)" "0|1"
check "what the desktop brought is no longer ignored, the rest still is" "$(grep ignorepkg "$TARGETDIR/etc/xbps.d/00-vessel.conf" | xargs)" "ignorepkg=base-system ignorepkg=AppStream"
check "the temporary xbps.d is gone" "$([ -e "$TARGETDIR/tmp/voyage-xbps.d" ] && echo left || echo gone)" "gone"

# ---- the login manager that starts
reset; SHIPPED_DESKTOP=xfce; ln -s /etc/sv/lightdm "$TARGETDIR/etc/runit/runsvdir/default/lightdm"
OPTS[DESKTOP]=cosmic; OPTS[DISPLAYMANAGER]=cosmic-greeter; set_login_manager
check "the copied LightDM is switched off, the COSMIC greeter and its daemon on" "$(enabled)" "cosmic-greeter cosmic-greeter-daemon "
reset; SHIPPED_DESKTOP=xfce; ln -s /etc/sv/lightdm "$TARGETDIR/etc/runit/runsvdir/default/lightdm"
OPTS[DESKTOP]=none; OPTS[DISPLAYMANAGER]=none; set_login_manager
check "none: no login manager starts" "$(enabled)" ""
reset; SHIPPED_DESKTOP=xfce; mkdir -p "$TARGETDIR/etc/lightdm"
printf '[Seat:*]\nuser-session=xfce\n' > "$TARGETDIR/etc/lightdm/lightdm.conf"
OPTS[DESKTOP]=budgie; OPTS[DISPLAYMANAGER]=lightdm; set_login_manager
check "LightDM: the live image's user-session is replaced in lightdm.conf" "$(grep user-session "$TARGETDIR/etc/lightdm/lightdm.conf")" "user-session=budgie-desktop"
check "LightDM: and set in lightdm.conf.d" "$(grep -h user-session "$TARGETDIR"/etc/lightdm/lightdm.conf.d/*)" "user-session=budgie-desktop"
INIT_SYSTEM=dinit; reset; mkdir -p "$TARGETDIR/etc/dinit.d/boot.d"; touch "$TARGETDIR/etc/dinit.d/sddm"
ln -s /etc/dinit.d/gdm "$TARGETDIR/etc/dinit.d/boot.d/gdm"; OPTS[DESKTOP]=kde; OPTS[DISPLAYMANAGER]=sddm; set_login_manager
check "dinit: gdm off, sddm on" "$(ls "$TARGETDIR/etc/dinit.d/boot.d" | xargs)" "sddm"
INIT_SYSTEM=runit

# ---- autologin, per login manager
reset; OPTS[AUTOLOGIN]=1; OPTS[DESKTOP]=kde; OPTS[DISPLAYMANAGER]=sddm; set_autologin
check "SDDM logs into the chosen desktop's session" "$(grep Session= "$TARGETDIR/etc/sddm.conf.d/10-voyage-autologin.conf")" "Session=plasma.desktop"

reset; OPTS[AUTOLOGIN]=1; OPTS[DESKTOP]=cinnamon; OPTS[DISPLAYMANAGER]=lightdm; set_autologin
check "LightDM gets the user and the session" "$(grep -h autologin- "$TARGETDIR"/etc/lightdm/lightdm.conf.d/50-voyage-autologin.conf | xargs)" \
	"autologin-user=gui autologin-user-timeout=0 autologin-session=cinnamon"

reset; OPTS[AUTOLOGIN]=1; OPTS[DESKTOP]=cosmic; OPTS[DISPLAYMANAGER]=cosmic-greeter
mkdir -p "$TARGETDIR/etc/greetd" "$TARGETDIR/usr/share/wayland-sessions"
printf '[Desktop Entry]\nName=COSMIC\nExec=/usr/bin/start-cosmic\n' > "$TARGETDIR/usr/share/wayland-sessions/cosmic.desktop"
printf '[terminal]\nvt = "1"\n\n[default_session]\ncommand = "cosmic-greeter-start"\nuser = "cosmic-greeter"\n' > "$TARGETDIR/etc/greetd/cosmic-greeter.toml"
set_autologin
check "COSMIC greeter: greetd starts the session once at boot" \
	"$(sed -n '/^\[initial_session\]/,$p' "$TARGETDIR/etc/greetd/cosmic-greeter.toml" | xargs)" '[initial_session] command = /usr/bin/start-cosmic user = gui'
check "and the greeter stays the default session" "$(grep -c 'cosmic-greeter-start' "$TARGETDIR/etc/greetd/cosmic-greeter.toml")" "1"
set_autologin
check "set twice, it is still one initial session" "$(grep -c '^\[initial_session\]' "$TARGETDIR/etc/greetd/cosmic-greeter.toml")" "1"
reset_live_autologin anon
check "a live user's initial session is not mistaken for ours" "$(grep -c '^\[initial_session\]' "$TARGETDIR/etc/greetd/cosmic-greeter.toml")" "1"
reset_live_autologin gui
check "the live user's initial session is removed" "$(grep -c '^\[initial_session\]' "$TARGETDIR/etc/greetd/cosmic-greeter.toml")" "0"

reset; OPTS[AUTOLOGIN]=1; OPTS[DESKTOP]=none; OPTS[DISPLAYMANAGER]=none
check "no login manager: autologin is not configured" "$(set_autologin)" "Autologin not configured: none has no autologin support here."

# ---- contract: what the GUI writes into the conf file is what the backend reads
WRITER="$HERE/src/backend/install_runner.rs"
for key in DESKTOP KEEPDESKTOP DISPLAYMANAGER; do
	w=$(grep -c "\"$key\"," "$WRITER" 2>/dev/null); r=$(grep -c "get_option $key)" "$SCRIPT")
	check "$key is written by the GUI and read by the backend" "$([ "$w" -ge 1 ] && [ "$r" -ge 1 ] && echo both || echo "writer=$w reader=$r")" "both"
done

# ---- order: checked before the disks are touched, applied after updates, login manager after the user exists
line() { grep -n "$1" "$SCRIPT" | tail -1 | cut -d: -f1; }
[ "$(line '^check_desktop_choice$')" -lt "$(line '^create_filesystems$')" ] && echo "ok   the choice is checked before anything is created" || { echo "FAIL check order"; fails=$((fails + 1)); }
[ "$(line '^install_extra_software$')" -lt "$(line '^apply_desktop$')" ] && echo "ok   the desktop is installed after the system update" || { echo "FAIL apply order"; fails=$((fails + 1)); }
[ "$(line '^set_useraccount$')" -lt "$(line '^set_login_manager$')" ] && [ "$(line '^set_login_manager$')" -lt "$(line '^set_autologin$')" ] \
	&& echo "ok   login manager after the user, before autologin" || { echo "FAIL login manager order"; fails=$((fails + 1)); }

[ "$fails" -eq 0 ]
