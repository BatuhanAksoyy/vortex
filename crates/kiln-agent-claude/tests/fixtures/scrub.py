#!/usr/bin/env python3
"""Turn a raw capture (capture.py) into a committable fixture.

usage: scrub.py <raw.jsonl> <fixture.jsonl>

Drops lines that only carry the capturing user's environment (hooks, the full
command catalogue), shortens long lists and signatures, and replaces paths and
identifiers with stable placeholders so replays are deterministic.
"""
import getpass, json, re, sys

DROP_SYSTEM = {"hook_started", "hook_response", "thinking_tokens"}
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
    if msg.get("type") == "system" and msg.get("subtype") == "commands_changed":
        msg["commands"] = sample_commands(msg["commands"])
    response = (msg.get("response") or {}).get("response")
    if isinstance(response, dict) and "commands" in response:
        msg["response"]["response"] = initialize_reply(response)
    if isinstance(response, dict) and "totalTokens" in response:
        msg["response"]["response"] = {k: response[k] for k in CONTEXT_KEEP if k in response}
    if isinstance(response, dict) and "rate_limits" in response:
        msg["response"]["response"] = usage_reply(response)
    return msg


CONTEXT_KEEP = ("totalTokens", "maxTokens", "percentage", "model")


def usage_reply(response):
    """Keeps the two windows Kiln reads; the rest is account detail."""
    limits = response.get("rate_limits") or {}
    window_keep = ("utilization", "resets_at", "locked_reason")
    return {
        "subscription_type": response.get("subscription_type"),
        "rate_limits_available": response.get("rate_limits_available"),
        "rate_limits": {name: {k: (limits.get(name) or {}).get(k) for k in window_keep}
                        for name in ("five_hour", "seven_day")},
    }


SAMPLE_COMMANDS = ("compact", "code-review", "effort", "clear")


def sample_commands(commands):
    """The full catalogue lists the capturing user's skills and plugins; a few
    built-in commands show its shape."""
    return [c for c in commands if c.get("name") in SAMPLE_COMMANDS]


def initialize_reply(response):
    """Keeps what Kiln reads; the account is replaced, the rest describes the
    capturing machine."""
    return {
        "commands": sample_commands(response.get("commands", [])),
        "models": [m for m in response.get("models", []) if m.get("value") in ("default", "sonnet", "haiku")],
        "account": {"email": "user.redacted", "organization": "Redacted Org",
                    "subscriptionType": response.get("account", {}).get("subscriptionType"),
                    "apiProvider": "firstParty"},
        "current_permission_mode": response.get("current_permission_mode"),
        "claude_code_version": response.get("claude_code_version"),
    }


class Placeholders:
    def __init__(self):
        self.ids = {}

    def stable(self, value, prefix):
        return self.ids.setdefault(value, f"{prefix}-{len(self.ids) + 1}")

    def scrub(self, text):
        text = re.sub(r'"signature": ?"[^"]*"', '"signature": "SIG"', text)
        text = re.sub(r'(?:/tmp|/home)/[^"\s]*?/work\b', "/work", text)
        text = re.sub(r"(?:/tmp|/home)/[A-Za-z0-9._/-]+", "/redacted", text)
        # Streamed tool input splits paths across deltas, out of reach of the
        # path patterns above; the user name can still show in a fragment.
        text = text.replace(getpass.getuser(), "user")
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
