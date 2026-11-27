"""Packs an unpacked `material-icon-theme` npm package into material-icons.json.

usage: python3 -I pack.py <package folder> > material-icons.json
"""

import json
import os
import sys

package = sys.argv[1]
with open(os.path.join(package, "dist", "material-icons.json")) as file:
    theme = json.load(file)

TABLES = {
    "files": "fileNames",
    "extensions": "fileExtensions",
    "folders": "folderNames",
    "open_folders": "folderNamesExpanded",
}


def tables(source):
    # Names with a path in them are left out: icons are looked up by name.
    return {
        ours: {k: v for k, v in sorted(source.get(theirs, {}).items()) if "/" not in k}
        for ours, theirs in TABLES.items()
    }


dark, light = tables(theme), tables(theme["light"])
used = {"file", "folder", "folder-open"}
for table in [*dark.values(), *light.values()]:
    used.update(table.values())
icons = {}
for name in sorted(used):
    path = os.path.basename(theme["iconDefinitions"][name]["iconPath"])
    with open(os.path.join(package, "icons", path)) as file:
        icons[name] = file.read()
json.dump({"dark": dark, "light": light, "icons": icons}, sys.stdout, indent=0, sort_keys=True)
