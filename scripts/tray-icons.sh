#!/usr/bin/env bash
# Render the status icon's images (d0139, page 31 item 1) into crates/otaq-desktop/icons/tray/, from the shapes below.
# The PNGs are committed; run this only when the artwork changes, and commit the result.
#
# - macos/<look>.png: template images (black with alpha; macOS tints them for light and dark menu bars). 36 px: the
#   menu bar shows a status item's image 18 pt high, so the 16 pt artwork sits in it with 1 pt around, at @2x (a 1x
#   display scales it down). The count is the status item's text, not in the image.
# - light/ and dark/ (Windows and Linux trays, for light and dark taskbars): <look>-16.png and <look>-32.png (100 % and
#   200 %), with the turquoise count badge lower right in needs-you-<n> and paused-needs-you-<n>, n = 1–9 and 9plus.
#
# Needs Google Chrome (headless, for the SVG and the badge's Geist digits). macOS only.

set -euo pipefail

root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
out="$root/crates/otaq-desktop/icons/tray"
font="$root/crates/otaq-desktop/ui/src/theme/fonts/Geist-Bold.ttf"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
if [[ ! -x "$chrome" ]]; then
    echo "tray-icons.sh: Google Chrome is missing (set CHROME to its binary)" >&2
    exit 1
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The shapes, in page 31's 24-unit box: the Command tent, its two columns, the pole and flag, the pause bars, the slash.
tent='M3.6 21.2v-7.2q4.8-2 8.4-8 3.6 6 8.4 8v7.2z'
columns='M9.4 13.2v8M14.6 13.2v8'
pole='M12 6.2V1.4'
flag='M12 .8h4q.6 0 .2.6l-.8 1 .8 1q.4.6-.2.6h-4z'
bars='M9.6 13.6v6.6M14.4 13.6v6.6'
slash='M3 3l18 18'
line='fill="none" stroke-linecap="round" stroke-linejoin="round"'

# The artwork of a look in `ink` (and `flag` and `bar` colours), as SVG in the 24-unit box. Cut-outs are real holes
# (a mask), so a template image and a coloured one show the bar through them.
art() {
    local look="$1" ink="$2" flagc="$3" barc="$4"
    case "$look" in
        plain)
            echo "<path d='$tent' stroke='$ink' stroke-width='1.9' $line/>
<path d='$columns' stroke='$ink' stroke-width='1.5' $line/>
<path d='$pole' stroke='$ink' stroke-width='1.5' $line/>
<path d='$flag' fill='$flagc'/>"
            ;;
        filled)
            echo "<mask id='cut' maskUnits='userSpaceOnUse' x='-2' y='-2' width='28' height='28'>
<rect x='-2' y='-2' width='28' height='28' fill='white'/><path d='$columns' stroke='black' stroke-width='1.5' $line/></mask>
<path d='$tent' fill='$ink' stroke='$ink' stroke-width='1.9' stroke-linejoin='round' mask='url(#cut)'/>
<path d='$pole' stroke='$ink' stroke-width='1.5' $line/>
<path d='$flag' fill='$flagc'/>"
            ;;
        paused)
            echo "<path d='$tent' stroke='$ink' stroke-width='1.9' $line/>
<path d='$bars' stroke='$barc' stroke-width='3.2' $line/>"
            ;;
        paused-filled)
            echo "<mask id='cut' maskUnits='userSpaceOnUse' x='-2' y='-2' width='28' height='28'>
<rect x='-2' y='-2' width='28' height='28' fill='white'/><path d='$bars' stroke='black' stroke-width='3.2' $line/></mask>
<path d='$tent' fill='$ink' stroke='$ink' stroke-width='1.9' stroke-linejoin='round' mask='url(#cut)'/>"
            ;;
        not-connected)
            echo "<mask id='cut' maskUnits='userSpaceOnUse' x='-2' y='-2' width='28' height='28'>
<rect x='-2' y='-2' width='28' height='28' fill='white'/><path d='$slash' stroke='black' stroke-width='4.2' $line/></mask>
<path d='$tent' stroke='$ink' stroke-width='1.9' $line mask='url(#cut)'/>
<path d='$slash' stroke='$ink' stroke-width='1.9' $line/>"
            ;;
    esac
}

# Page 31's badge in the 16 px box: a 9.6 px turquoise circle (a pill for 9+) in the lower right, the count in Geist
# Bold 8 px (6.7 px for 9+). Page 31 lets it run 1 px past the edge; here it ends at the edge, so nothing is cut off.
badge() {
    local count="$1" fill="$2" text="$3"
    if [[ "$count" == "9+" ]]; then
        echo "<rect x='5.4' y='6.4' width='10.6' height='9.6' rx='4.8' fill='$fill'/>
<text x='10.7' y='11.2' font-size='6.7' fill='$text' text-anchor='middle' dominant-baseline='central'>9+</text>"
    else
        echo "<circle cx='11.2' cy='11.2' r='4.8' fill='$fill'/>
<text x='11.2' y='11.2' font-size='8' fill='$text' text-anchor='middle' dominant-baseline='central'>$count</text>"
    fi
}

# Renders `svg` (in the 16 px box) to `file` at `px` square.
render() {
    local file="$1" px="$2" svg="$3"
    local page="$work/page.html"
    cat >"$page" <<EOF
<!doctype html><html><head><style>
@font-face { font-family: "Geist"; font-weight: 700; src: url("file://$font"); }
html, body { margin: 0; background: transparent; }
svg { display: block; font-family: "Geist"; font-weight: 700; }
</style></head><body><svg xmlns="http://www.w3.org/2000/svg" width="$px" height="$px" viewBox="0 0 16 16">$svg</svg></body></html>
EOF
    mkdir -p "$(dirname "$file")"
    "$chrome" --headless=new --user-data-dir="$work/profile" --disable-gpu --hide-scrollbars \
        --default-background-color=00000000 --force-device-scale-factor=1 --window-size="$px,$px" \
        --virtual-time-budget=1000 --screenshot="$file" "file://$page" >/dev/null 2>&1
}

# The 24-unit artwork in the 16 px box.
box() { echo "<g transform='scale(0.6666667)'>$1</g>"; }

rm -rf "$out"

# macOS: black templates, the 16 pt artwork centred in an 18 pt image.
for look in plain needs-you paused paused-needs-you not-connected; do
    case "$look" in
        needs-you) shape=filled ;;
        paused-needs-you) shape=paused-filled ;;
        *) shape="$look" ;;
    esac
    render "$out/macos/$look.png" 36 "<g transform='translate(0.8888889 0.8888889) scale(0.8888889)'>$(box "$(art "$shape" '#000' '#000' '#000')")</g>"
done

# Windows and Linux: page 31's colours. Light taskbar: ink #1B1B1B, flag #0A8F82, bars #976400, badge #087A6F / white.
# Dark taskbar: ink white, flag #5EE6D3, bars #F2BE4A, badge #2DD4BF / #0B1F1C. Not connected: grey #8A8F98 on both.
for bar in light dark; do
    if [[ "$bar" == light ]]; then
        ink='#1B1B1B' flagc='#0A8F82' barc='#976400' badgec='#087A6F' badget='#FFFFFF'
    else
        ink='#FFFFFF' flagc='#5EE6D3' barc='#F2BE4A' badgec='#2DD4BF' badget='#0B1F1C'
    fi
    for px in 16 32; do
        render "$out/$bar/plain-$px.png" "$px" "$(box "$(art plain "$ink" "$flagc" "$barc")")"
        render "$out/$bar/paused-$px.png" "$px" "$(box "$(art paused "$ink" "$flagc" "$barc")")"
        render "$out/$bar/not-connected-$px.png" "$px" "$(box "$(art not-connected '#8A8F98' "$flagc" "$barc")")"
        for count in 1 2 3 4 5 6 7 8 9 9+; do
            name="${count/+/plus}"
            render "$out/$bar/needs-you-$name-$px.png" "$px" \
                "$(box "$(art plain "$ink" "$flagc" "$barc")")$(badge "$count" "$badgec" "$badget")"
            render "$out/$bar/paused-needs-you-$name-$px.png" "$px" \
                "$(box "$(art paused "$ink" "$flagc" "$barc")")$(badge "$count" "$badgec" "$badget")"
        done
    done
done

echo "tray-icons.sh: $(find "$out" -name '*.png' | wc -l | tr -d ' ') images in ${out#"$root"/}"
