#!/usr/bin/env python3
"""Turn a raw capture (capture.py) into a committable fixture.

usage: scrub.py <raw.jsonl> <fixture.jsonl>

Run it from the directory the capture ran in. Replaces that directory with
`/work`, the home directory with `~`, and the account's email, user,
account, installation and host names with stable placeholders, and keeps
only the first three models of a model list.
"""
import getpass, json, os, re, socket, sys

KEPT_MODELS = 3


def shrink(record):
    msg = record["msg"]
    result = msg.get("result") if isinstance(msg, dict) else None
    if isinstance(result, dict) and isinstance(result.get("data"), list) and result["data"] \
            and "supportedReasoningEfforts" in result["data"][0]:
        result["data"] = result["data"][:KEPT_MODELS]
    return record


def main():
    raw, fixture = sys.argv[1], sys.argv[2]
    work = os.getcwd()
    home = os.path.expanduser("~")
    lines = [json.dumps(shrink(json.loads(line))) for line in open(raw)]
    text = "\n".join(lines) + "\n"
    # The capture directory under /tmp is also reported by its real path.
    for path in sorted({work, os.path.realpath(work), work.removeprefix("/private")}, key=len, reverse=True):
        text = text.replace(path, "/work")
    text = text.replace(home, "~")
    text = re.sub(r"[\w.+-]+@[\w-]+(\.[\w-]+)+", "person.example", text)
    text = re.sub(r'"(installationId|chatgptAccountId|accountId)": "[^"]+"', r'"\1": "00000000-0000-0000-0000-000000000000"', text)
    text = re.sub(r'"userId": "[^"]+"', '"userId": "user-0"', text)
    for host in {socket.gethostname(), socket.gethostname().split(".")[0] + ".local"}:
        text = text.replace(host, "host.local")
    text = text.replace(getpass.getuser(), "user")
    with open(fixture, "w") as out:
        out.write(text)


main()
