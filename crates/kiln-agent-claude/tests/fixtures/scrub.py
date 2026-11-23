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
        msg["response"]["response"] = context_reply(response)
    if isinstance(response, dict) and "rate_limits" in response:
        msg["response"]["response"] = usage_reply(response)
    if isinstance(response, dict) and is_skills_menu(response):
        msg["response"]["response"] = skills_reply(response)
    if isinstance(response, dict) and "automaticUrl" in response:
        msg["response"]["response"] = {k: SIGN_IN_URL for k in ("manualUrl", "automaticUrl")}
    if isinstance(response, dict) and "account" in response and "commands" not in response:
        msg["response"]["response"] = {**response, "account": redact_account(response["account"])}
    return msg


# The real URLs carry a one-time PKCE challenge and state.
SIGN_IN_URL = "https://claude.com/cai/oauth/authorize?redacted"


def redact_account(account):
    """Keeps which fields are present and the plan; hides who signed in."""
    redacted = {"email": "user.redacted", "organization": "Redacted Org"}
    return {k: redacted.get(k, v) for k, v in (account or {}).items()}


def is_skills_menu(response):
    rows = response.get("skills")
    return isinstance(rows, list) and all(isinstance(r, dict) and "state" in r for r in rows)


def skills_reply(response):
    """Keeps the fixture's own project skills. The other rows name the
    capturing user's skills and plugins; one row of each other source stays,
    renamed, to show its shape (a plugin's lock, a synced skill's handles)."""
    rows, seen = [], set()
    for row in response["skills"]:
        source = row.get("source")
        if source == "project":
            rows.append(row)
        elif source not in seen:
            seen.add(source)
            rows.append(example_skill(row, len(seen)))
    return {"skills": rows}


def example_skill(row, number):
    short = f"example-{number}"
    handles = {}
    if "unqualified_name" in row.get("handles", {}):
        handles["unqualified_name"] = short
    if "aliases" in row.get("handles", {}):
        handles["aliases"] = [short]
    return {**row, "name": f"example-source:{short}", "display_name": short,
            "description": "An example skill.", "handles": handles}


CONTEXT_KEEP = ("totalTokens", "maxTokens", "percentage", "model")
CATEGORY_KEEP = ("id", "name", "tokens", "kind")


def context_reply(response):
    """Keeps the totals and the category breakdown. The rest lists the
    capturing user's agents, skills, memory files and MCP tools, with paths."""
    reply = {k: response[k] for k in CONTEXT_KEEP if k in response}
    reply["categories"] = [{k: c[k] for k in CATEGORY_KEEP if k in c}
                           for c in response.get("categories", [])]
    return reply


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
        "account": redact_account(response.get("account")),
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
        roots = r"(?:/private)?(?:/tmp|/var|/home|/Users)"
        text = re.sub(roots + r'/[^"\s]*?/work\b', "/work", text)
        text = re.sub(roots + r"/[A-Za-z0-9._/-]+", "/redacted", text)
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
