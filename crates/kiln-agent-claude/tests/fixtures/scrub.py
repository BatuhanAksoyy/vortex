#!/usr/bin/env python3
"""Turn a raw capture (capture.py) into a committable fixture.

usage: scrub.py <raw.jsonl> <fixture.jsonl>

Drops lines that only carry the capturing user's environment (hooks, the full
command catalogue), shortens long lists and signatures, and replaces paths and
identifiers with stable placeholders so replays are deterministic.
"""
import json, re, sys

DROP_SYSTEM = {"hook_started", "hook_response", "commands_changed", "thinking_tokens", "status"}
KEEP_INIT = {"type", "subtype", "session_id", "cwd", "model", "permissionMode",
             "claude_code_version", "tools", "mcp_servers", "slash_commands", "uuid"}


def keep(record):
    msg = record["msg"]
    if not isinstance(msg, dict):
        return False
    return not (msg.get("type") == "system" and msg.get("subtype") in DROP_SYSTEM)


def shrink(msg):
    if msg.get("type") == "system" and msg.get("subtype") == "init":
        msg = {k: v for k, v in msg.items() if k in KEEP_INIT}
        msg["tools"] = ["Bash", "Edit", "Read", "Task", "Write"]
        msg["mcp_servers"] = []
        msg["slash_commands"] = ["compact", "review"]
    response = (msg.get("response") or {}).get("response")
    if isinstance(response, dict) and "commands" in response:
        msg["response"]["response"] = {"commands": [{"name": "compact", "description": "", "argumentHint": ""}]}
    return msg


class Placeholders:
    def __init__(self):
        self.ids = {}

    def stable(self, value, prefix):
        return self.ids.setdefault(value, f"{prefix}-{len(self.ids) + 1}")

    def scrub(self, text):
        text = re.sub(r'"signature": ?"[^"]*"', '"signature": "SIG"', text)
        text = re.sub(r'(?:/tmp|/home)/[^"\s]*?/work\b', "/work", text)
        text = re.sub(r"(?:/tmp|/home)/[A-Za-z0-9._/-]+", "/redacted", text)
        uuid = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
        return re.sub(uuid, lambda m: self.stable(m.group(0), "id"), text)


def main(raw_path, out_path):
    placeholders = Placeholders()
    with open(raw_path) as raw, open(out_path, "w") as out:
        for line in raw:
            record = json.loads(line)
            if not keep(record):
                continue
            record["msg"] = shrink(record["msg"])
            out.write(placeholders.scrub(json.dumps(record, ensure_ascii=False)) + "\n")


main(sys.argv[1], sys.argv[2])
