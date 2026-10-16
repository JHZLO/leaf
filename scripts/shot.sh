#!/bin/zsh
# Screenshots only a leaf window, never the rest of the screen.
#   scripts/shot.sh out.png [pid]
# With a pid, only that process's window, so parallel runs never catch each other's window.
set -eu
out=${1:?usage: scripts/shot.sh out.png [pid]}
pid=${2:-0}
id=$(swift - "$pid" <<'SWIFT'
import CoreGraphics
let pid = Int(CommandLine.arguments.last ?? "0") ?? 0
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for w in windows where (w[kCGWindowLayer as String] as? Int) == 0 {
    let owner = w[kCGWindowOwnerName as String] as? String
    let ownerPid = w[kCGWindowOwnerPID as String] as? Int ?? -1
    if pid != 0 ? ownerPid == pid : owner == "leaf" {
        print(w[kCGWindowNumber as String] as! Int)
        break
    }
}
SWIFT
)
[[ -n $id ]] || { echo "no leaf window on screen" >&2; exit 1; }
screencapture -x -o -l "$id" "$out"
echo "$out"
