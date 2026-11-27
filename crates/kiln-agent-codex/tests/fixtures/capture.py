#!/usr/bin/env python3
"""Drive `codex app-server` over JSON-RPC on stdio and record both
directions as JSONL, one `{"dir": "in"|"out", "msg": ...}` per line.

usage: capture.py <case> <out.jsonl>
  case: controls | turn | command | deny | edit | interrupt | resume | history
        | picture | thinking | subagent | mcp | login

"controls" sends no prompt, so it spends no usage: it lists the models and
reads the account and its rate limits, then starts a thread.
"turn" runs one tiny turn with reasoning summaries on.
"command" has the model run `echo kiln` while every command needs approval,
and accepts it; "deny" declines it instead.
"edit" has the model change one line of note.txt with its patch tool while
edits need approval, and accepts the change.
"interrupt" starts a turn that would run long and interrupts it.
"resume" runs a tiny turn in a first process, then resumes the thread in a
second one and runs another; the second process is recorded, and a "note"
record holds the first thread's id.
"history" runs a tiny turn, then lists the stored threads and reads that one
with its turns, as a reopened chat does.
"picture" sends a red square as a local image beside the text.
"thinking" asks a puzzle at high effort with summaries, so the turn has
several reasoning items around a command.
"subagent" has the model spawn one subagent that replies with a word, wait
for it, and report the word; the subagent's own thread streams on the same
connection, so the capture runs until the main thread's turn completes.
"mcp" gives the thread an MCP server named kiln, as Kiln gives its own,
with its tools approved, while every command needs approval; the model
calls its one tool, whose result holds text and a picture.
"login" sends no prompt: it starts a ChatGPT sign-in in a CODEX_HOME of its
own (`./codex-home`, so the usual login is not touched), prints the sign-in
URL and waits until the sign-in in the browser completes.

Run it in an empty directory outside the repository, from a plain terminal,
then turn the result into a fixture with scrub.py.
"""
import json, os, struct, subprocess, sys, zlib

STRIP = {"CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_ENTRYPOINT"}
MODEL = "gpt-6-luna"

case, out_path = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if k not in STRIP}
if case == "login":
    env["CODEX_HOME"] = os.path.abspath("codex-home")
    os.makedirs(env["CODEX_HOME"], exist_ok=True)
log = open(out_path, "w")


class Server:
    def __init__(self, recorded=True):
        self.proc = subprocess.Popen(["codex", "app-server"], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                     env=env, text=True, bufsize=1)
        self.recorded = recorded
        self.next_id = 0

    def write(self, obj):
        if self.recorded:
            log.write(json.dumps({"dir": "in", "msg": obj}) + "\n")
        self.proc.stdin.write(json.dumps(obj) + "\n")
        self.proc.stdin.flush()

    def read(self):
        line = self.proc.stdout.readline()
        if not line:
            raise SystemExit("app-server closed its output")
        msg = json.loads(line)
        if self.recorded:
            log.write(json.dumps({"dir": "out", "msg": msg}) + "\n")
        return msg

    def request(self, method, params, answer=None):
        """Sends a request and returns its result, answering server requests
        that come first with `answer(request)`."""
        self.next_id += 1
        ident = self.next_id
        self.write({"id": ident, "method": method, "params": params})
        while True:
            msg = self.read()
            if msg.get("id") == ident and "method" not in msg:
                return msg.get("result", msg.get("error"))
            self.serve(msg, answer)

    def serve(self, msg, answer):
        if "id" in msg and "method" in msg:
            result = answer(msg) if answer else {"decision": "decline"}
            self.write({"id": msg["id"], "result": result})

    def until(self, method, answer=None):
        while True:
            msg = self.read()
            self.serve(msg, answer)
            if msg.get("method") == method:
                return msg

    def start(self):
        self.request("initialize", {"clientInfo": {"name": "kiln", "title": "Kiln", "version": "0.0.0"}})
        self.write({"method": "initialized"})

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=30)


def text(words):
    return [{"type": "text", "text": words, "text_elements": []}]


def thread(server, **params):
    started = server.request("thread/start", {"cwd": os.getcwd(), "model": MODEL, **params})
    return started["thread"]["id"]


def turn(server, thread_id, items, answer=None, **params):
    server.request("turn/start", {"threadId": thread_id, "input": items, **params}, answer)
    return server.until("turn/completed", answer)


def accept(_request):
    return {"decision": "accept"}


def square_png(path, size=16):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    raw = b"".join(b"\x00" + b"\xff\x00\x00" * size for _ in range(size))
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
    with open(path, "wb") as file:
        file.write(png)


# An MCP server on stdio with one tool, `page_title`, that answers with a
# line of text and a red square.
TOOL_SERVER = r'''
import base64, json, sys
PNG = sys.argv[1]
def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    msg = json.loads(line)
    method, ident = msg.get("method"), msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": ident, "result": {
            "protocolVersion": msg["params"]["protocolVersion"],
            "capabilities": {"tools": {}}, "serverInfo": {"name": "kiln", "version": "0"}}})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": ident, "result": {"tools": [{
            "name": "page_title", "description": "The title of the page open in the browser.",
            "inputSchema": {"type": "object", "properties": {}}}]}})
    elif method == "tools/call":
        picture = base64.b64encode(open(PNG, "rb").read()).decode()
        send({"jsonrpc": "2.0", "id": ident, "result": {"content": [
            {"type": "text", "text": "The page is titled Kiln."},
            {"type": "image", "data": picture, "mimeType": "image/png"}]}})
    elif ident is not None:
        send({"jsonrpc": "2.0", "id": ident, "result": {}})
'''


def run():
    if case == "resume":
        first = Server(recorded=False)
        first.start()
        first_id = thread(first)
        turn(first, first_id, text("Remember the word marigold. Reply with the single word ok."))
        first.close()
        log.write(json.dumps({"dir": "note", "msg": {"first_thread": first_id}}) + "\n")
    server = Server()
    server.start()
    if case == "controls":
        server.request("model/list", {})
        server.request("account/read", {"refreshToken": False})
        server.request("account/rateLimits/read", None)
        thread(server)
    elif case == "login":
        started = server.request("account/login/start", {"type": "chatgpt"})
        print("Sign in at:", started.get("authUrl"), flush=True)
        server.until("account/login/completed")
        server.request("account/read", {"refreshToken": False})
    elif case == "turn":
        turn(server, thread(server), text("Reply with the single word ok."), summary="concise", effort="low")
    elif case in ("command", "deny"):
        answer = accept if case == "command" else None
        thread_id = thread(server, approvalPolicy="untrusted", sandbox="read-only")
        turn(server, thread_id, text("Run the shell command `echo kiln` and reply with its output."), answer)
    elif case == "edit":
        with open("note.txt", "w") as note:
            note.write("one\ntwo\nthree\n")
        thread_id = thread(server, approvalPolicy="untrusted", sandbox="workspace-write")
        turn(server, thread_id, text(
            "Without running any command, use apply_patch to change the line `two` in note.txt to "
            "`TWO`. Change nothing else."), accept)
    elif case == "interrupt":
        thread_id = thread(server)
        started = server.request("turn/start", {"threadId": thread_id, "input": text(
            "Count from 1 to 400, one number per line.")})
        server.until("item/agentMessage/delta")
        server.request("turn/interrupt", {"threadId": thread_id, "turnId": started["turn"]["id"]})
        server.until("turn/completed")
    elif case == "resume":
        server.request("thread/resume", {"threadId": first_id})
        turn(server, first_id, text("Which word did I ask you to remember? Reply with it alone."))
    elif case == "history":
        thread_id = thread(server)
        turn(server, thread_id, text("Reply with the single word ok."))
        server.request("thread/list", {"cwd": os.getcwd()})
        server.request("thread/read", {"threadId": thread_id, "includeTurns": True})
    elif case == "thinking":
        thread_id = thread(server, approvalPolicy="never", sandbox="read-only")
        turn(server, thread_id, text(
            "Think it through carefully: how many weekdays fall between 3 March 2031 and 17 April "
            "2031 inclusive? Then run `date +%Y` and reply with the count and the year."),
            accept, summary="auto", effort="high")
    elif case == "subagent":
        thread_id = thread(server)
        server.request("turn/start", {"threadId": thread_id, "input": text(
            "Spawn exactly one subagent with the prompt `Reply with the single word heron.` "
            "Wait for it, then reply with the word it gave.")})
        while True:
            done = server.until("turn/completed")
            if done["params"]["threadId"] == thread_id:
                break
    elif case == "picture":
        square_png("square.png")
        items = text("What color is this square? Reply with the color alone.")
        items.append({"type": "localImage", "path": os.path.abspath("square.png")})
        turn(server, thread(server), items)
    elif case == "mcp":
        square_png("square.png")
        with open("tool_server.py", "w") as file:
            file.write(TOOL_SERVER)
        config = {"mcp_servers": {"kiln": {
            "command": sys.executable,
            "args": [os.path.abspath("tool_server.py"), os.path.abspath("square.png")],
            "default_tools_approval_mode": "approve",
        }}}
        thread_id = thread(server, approvalPolicy="untrusted", sandbox="workspace-write", config=config)
        turn(server, thread_id, text(
            "Call the page_title tool of the kiln MCP server once, then reply with the title it "
            "gave and the color of the picture it returned."))
    else:
        raise SystemExit(f"unknown case {case}")
    server.close()


run()
