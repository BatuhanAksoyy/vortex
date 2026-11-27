#!/usr/bin/env python3
"""Generate the Codex app-server protocol types from the CLI's own schema.

usage: generate.py [schema-dir]

Without a directory it runs `codex app-server generate-json-schema` itself.
It keeps only the definitions Vortex uses (ROOTS and what they refer to),
writes them to `schema/protocol.json`, so a new CLI version shows as a diff
there, and writes the Rust types to `src/protocol/`, split into files the
quality gate accepts. Never edit the output by hand; change this script.

A union the generator cannot map onto serde cleanly becomes
`serde_json::Value`, so an odd shape never breaks the build or a message.
"""
import json, keyword, re, subprocess, sys, tempfile
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent
OUT = CRATE / "src" / "protocol"
MAX_LINES = 360

ROOTS = """
ErrorNotification ThreadStartedNotification ThreadStatusChangedNotification
ThreadNameUpdatedNotification ThreadTokenUsageUpdatedNotification
TurnStartedNotification TurnCompletedNotification TurnDiffUpdatedNotification
TurnPlanUpdatedNotification ItemStartedNotification ItemCompletedNotification
AgentMessageDeltaNotification CommandExecutionOutputDeltaNotification
FileChangeOutputDeltaNotification ServerRequestResolvedNotification
McpToolCallProgressNotification AccountUpdatedNotification
AccountRateLimitsUpdatedNotification ReasoningSummaryTextDeltaNotification
ReasoningTextDeltaNotification ContextCompactedNotification
ModelReroutedNotification WarningNotification ConfigWarningNotification
AccountLoginCompletedNotification
CommandExecutionRequestApprovalParams CommandExecutionRequestApprovalResponse
FileChangeRequestApprovalParams FileChangeRequestApprovalResponse
ToolRequestUserInputParams ToolRequestUserInputResponse
PermissionsRequestApprovalParams PermissionsRequestApprovalResponse
InitializeParams InitializeResponse ThreadStartParams ThreadStartResponse
ThreadResumeParams ThreadResumeResponse ThreadListParams ThreadListResponse
ThreadReadParams ThreadReadResponse TurnStartParams TurnStartResponse
TurnInterruptParams TurnInterruptResponse ModelListParams ModelListResponse
GetAccountParams GetAccountResponse GetAccountRateLimitsResponse
LoginAccountParams LoginAccountResponse CancelLoginAccountParams
CancelLoginAccountResponse
""".split()

RUST_KEYWORDS = set(keyword.kwlist) | {
    "type", "match", "ref", "mod", "move", "fn", "impl", "loop", "self",
    "super", "crate", "use", "where", "struct", "enum", "trait", "dyn",
    "abstract", "final", "override", "macro", "priv", "unsafe", "box",
}


def load(schema_dir):
    bundle = json.loads((Path(schema_dir) / "codex_app_server_protocol.schemas.json").read_text())
    top = bundle["definitions"]
    v2 = top.pop("v2")
    # Names in both are the same type, but for `RequestId`, where v2's is
    # the one every message uses.
    return {**top, **v2}


def refs(schema):
    text = json.dumps(schema)
    return re.findall(r'"\$ref": "#/definitions/(?:v2/)?([^"]+)"', text)


def closure(defs):
    seen, stack = {}, list(ROOTS)
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        seen[name] = defs[name]
        stack.extend(refs(defs[name]))
    return dict(sorted(seen.items()))


def strip(schema):
    """The schema without what does not shape a type, refs made flat."""
    if isinstance(schema, list):
        return [strip(item) for item in schema]
    if not isinstance(schema, dict):
        return schema
    kept = {}
    for key, value in schema.items():
        if key in ("description", "$schema", "minimum", "minLength"):
            continue
        if key == "$ref":
            value = "#/definitions/" + value.split("/")[-1]
        if key == "properties":
            # Property names, which may well be `description` too.
            kept[key] = {name: strip(prop) for name, prop in value.items()}
            continue
        kept[key] = value if key in ("enum", "required") else strip(value)
    return kept


# ---- naming ----------------------------------------------------------------

def pascal(text):
    words = re.split(r"[^A-Za-z0-9]+", text)
    out = "".join(word[:1].upper() + word[1:] for word in words if word)
    if not out or out[0].isdigit():
        out = "V" + out
    return out


def snake(text):
    text = re.sub(r"[^A-Za-z0-9]+", "_", text)
    text = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", text)
    text = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", text).lower().strip("_")
    if not text or text[0].isdigit():
        text = "v_" + text
    return "r#" + text if text in RUST_KEYWORDS else text


# ---- mapping ---------------------------------------------------------------

class Gen:
    def __init__(self, defs):
        self.defs = defs
        self.items = {}  # name -> (code lines, names it refers to)
        self.direct = {}  # name -> names held without indirection
        self.fields = {}  # struct name -> its field types

    def ref_name(self, schema):
        return schema["$ref"].split("/")[-1]

    def nullable(self, schema):
        """The schema without its null alternative, and whether it had one."""
        kinds = schema.get("type")
        if isinstance(kinds, list) and "null" in kinds:
            rest = [k for k in kinds if k != "null"]
            return {**schema, "type": rest[0] if len(rest) == 1 else rest}, True
        for key in ("anyOf", "oneOf"):
            alts = schema.get(key)
            if alts and any(a.get("type") == "null" for a in alts):
                rest = [a for a in alts if a.get("type") != "null"]
                if len(rest) == 1:
                    return rest[0], True
                return {**schema, key: rest}, True
        return schema, False

    def rust_type(self, schema, owner, hint, used, direct=True):
        """The Rust type for `schema` inside item `owner`; inline objects and
        enums become items of their own named after `hint`."""
        if schema is True or schema == {}:
            return "Value"
        schema, null = self.nullable(schema)
        inner = self.plain_type(schema, owner, hint, used, direct)
        return f"Option<{inner}>" if null else inner

    def plain_type(self, schema, owner, hint, used, direct):
        if "$ref" in schema:
            name = self.ref_name(schema)
            used.add(name)
            if direct:
                self.direct.setdefault(owner, set()).add(name)
            return name
        if "allOf" in schema and len(schema["allOf"]) == 1:
            return self.plain_type(schema["allOf"][0], owner, hint, used, direct)
        kind = schema.get("type")
        if kind == "string" and "enum" not in schema:
            return "String"
        if kind == "boolean":
            return "bool"
        if kind == "integer":
            return "u64" if schema.get("format", "").startswith("uint") or schema.get("minimum") == 0 else "i64"
        if kind == "number":
            return "f64"
        if kind == "array":
            items = schema.get("items", True)
            return f"Vec<{self.rust_type(items, owner, hint + 'Item', used, False)}>"
        if kind == "object" and "properties" not in schema:
            extra = schema.get("additionalProperties", True)
            if extra is True or extra == {}:
                return "Map<String, Value>"
            return f"BTreeMap<String, {self.rust_type(extra, owner, hint + 'Value', used, False)}>"
        if kind == "object" or "enum" in schema or "oneOf" in schema or "anyOf" in schema:
            name = self.unique(hint)
            self.item(name, schema)
            used.add(name)
            if direct:
                self.direct.setdefault(owner, set()).add(name)
            return name
        return "Value"

    def unique(self, name):
        base, n = name, 2
        while name in self.items or (name in self.defs and name != base):
            name, n = f"{base}{n}", n + 1
        return name

    def item(self, name, schema):
        if name in self.items:
            return
        self.items[name] = None  # claimed while it is built
        schema, _ = self.nullable(schema)
        used = set()
        if "allOf" in schema and len(schema["allOf"]) == 1:
            schema = schema["allOf"][0]
        if "$ref" in schema or schema.get("type") in ("string", "integer", "number", "boolean", "array") and "enum" not in schema:
            code = [f"pub type {name} = {self.rust_type(schema, name, name, used)};"]
        elif "enum" in schema:
            code = self.string_enum(name, schema["enum"])
        elif "oneOf" in schema or "anyOf" in schema:
            code = self.union(name, schema, used)
        elif "properties" in schema:
            code = self.struct(name, schema, used)
        else:
            code = [f"pub type {name} = {self.plain_type(schema, name, name, used, True)};"]
        self.items[name] = (code, used)

    def string_enum(self, name, values):
        if not all(isinstance(v, str) for v in values):
            return [f"pub type {name} = Value;"]
        lines = ["#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]",
                 f"pub enum {name} {{"]
        for value in values:
            variant = pascal(value)
            if variant != value:
                lines.append(f'    #[serde(rename = "{value}")]')
            lines.append(f"    {variant},")
        lines += ["    /// A value a newer CLI added.", "    #[serde(other)]", "    Unrecognized,", "}"]
        return lines

    def struct(self, name, schema, used, skip=None):
        required = set(schema.get("required", []))
        lines = ["#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]",
                 f"pub struct {name} {{"]
        props = [(k, v) for k, v in schema["properties"].items() if k != skip]
        self.fields[name] = types = []
        for key, prop in props:
            field = snake(key)
            ty = self.rust_type(prop, name, name + pascal(key), used)
            attrs = []
            if field.removeprefix("r#") != key:
                attrs.append(f'rename = "{key}"')
            if key not in required:
                if not ty.startswith("Option<"):
                    ty = f"Option<{ty}>"
                attrs.append('default, skip_serializing_if = "Option::is_none"')
            elif ty.startswith("Option<"):
                attrs.append("default")
            types.append(ty)
            if attrs:
                lines.append(f"    #[serde({', '.join(attrs)})]")
            lines.append(f"    pub {field}: {ty},")
        lines.append("}")
        if not props:
            lines = lines[:1] + [f"pub struct {name} {{}}"]
        return lines

    @staticmethod
    def merged(alt):
        """An object whose `anyOf` lists other ways to fill it, such as a
        picture by URL or by file id, as one object with each way's fields
        optional."""
        options = alt.get("anyOf")
        if alt.get("type") != "object" or not options or not all("properties" in o for o in options):
            return alt
        props = dict(alt.get("properties", {}))
        for option in options:
            props.update(option["properties"])
        return {k: v for k, v in alt.items() if k != "anyOf"} | {"properties": props}

    def union(self, name, schema, used):
        alts = [self.merged(alt) for alt in schema.get("oneOf") or schema.get("anyOf")]
        tag = self.discriminant(alts)
        if tag:
            return self.tagged(name, alts, tag, used)
        if all(self.is_word(a) or self.is_wrapper(a) for a in alts):
            return self.external(name, alts, used)
        if all("$ref" in a or a.get("type") in ("string", "integer", "number", "boolean") and "enum" not in a for a in alts):
            return self.untagged(name, alts, used)
        return [f"pub type {name} = Value;"]

    def discriminant(self, alts):
        for key in ("type", "mode", "kind", "method"):
            values = [a.get("properties", {}).get(key, {}).get("enum") for a in alts]
            if all(v and len(v) == 1 and isinstance(v[0], str) for v in values):
                if len({v[0] for v in values}) == len(values) and not any("anyOf" in a or "oneOf" in a for a in alts):
                    return key
        return None

    def tagged(self, name, alts, tag, used):
        lines = ["#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]",
                 f'#[serde(tag = "{tag}")]', f"pub enum {name} {{"]
        for alt in alts:
            value = alt["properties"][tag]["enum"][0]
            variant = pascal(value)
            if variant != value:
                lines.append(f'    #[serde(rename = "{value}")]')
            fields = [k for k in alt["properties"] if k != tag]
            if not fields:
                lines.append(f"    {variant},")
                continue
            body = self.unique(name + variant)
            self.items[body] = None
            inner = set()
            self.items[body] = (self.struct(body, alt, inner, skip=tag), inner)
            used.add(body)
            # Boxed: the kinds differ much in size, and a box also ends any
            # recursion through them.
            lines.append(f"    {variant}(Box<{body}>),")
        lines += ["    /// A kind a newer CLI added.", "    #[serde(other)]", "    Unrecognized,", "}"]
        return lines

    @staticmethod
    def is_word(alt):
        return alt.get("type") == "string" and isinstance(alt.get("enum"), list)

    @staticmethod
    def is_wrapper(alt):
        props = alt.get("properties", {})
        return alt.get("type") == "object" and len(props) == 1 and alt.get("required") == list(props)

    def external(self, name, alts, used):
        lines = ["#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]", f"pub enum {name} {{"]
        for alt in alts:
            if self.is_word(alt):
                for value in alt["enum"]:
                    variant = pascal(value)
                    if variant != value:
                        lines.append(f'    #[serde(rename = "{value}")]')
                    lines.append(f"    {variant},")
                continue
            (key, prop), = alt["properties"].items()
            variant = pascal(key)
            if variant != key:
                lines.append(f'    #[serde(rename = "{key}")]')
            lines.append(f"    {variant}({self.rust_type(prop, name, name + variant, used)}),")
        lines.append("}")
        return lines

    def untagged(self, name, alts, used):
        lines = ["#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]",
                 "#[serde(untagged)]", f"pub enum {name} {{"]
        for alt in alts:
            ty = self.rust_type(alt, name, name, used)
            variant = pascal(ty.split("<")[0])
            variant = {"I64": "Integer", "U64": "Integer", "F64": "Number", "Bool": "Boolean"}.get(variant, variant)
            lines.append(f"    {variant}({ty}),")
        lines.append("}")
        return lines

    def derive_default(self):
        """Gives `Default` to each struct whose fields all have it, so a
        request can name only what it sets."""
        plain = re.compile(r"(Option|Vec|Map|BTreeMap)<|String$|bool$|[iuf]64$|Value$")
        has = set()
        while True:
            more = {name for name, types in self.fields.items()
                    if name not in has and all(plain.match(t) or t in has for t in types)}
            if not more:
                break
            has |= more
        for name in has:
            code = self.items[name][0]
            code[0] = code[0].replace("Debug, PartialEq", "Debug, Default, PartialEq")

    # ---- recursion -----------------------------------------------------

    def box_cycles(self):
        """Boxes a directly held field that leads back to its own item, which
        would otherwise have no finite size."""
        def reaches(start, goal):
            seen, stack = set(), [start]
            while stack:
                node = stack.pop()
                if node == goal:
                    return True
                if node in seen:
                    continue
                seen.add(node)
                stack.extend(self.direct.get(node, ()))
            return False
        for name, (code, used) in self.items.items():
            for target in list(self.direct.get(name, ())):
                if reaches(target, name):
                    code[:] = [re.sub(rf"(: |\(|Option<){target}\b(?!<)", rf"\1Box<{target}>", line) for line in code]
                    self.direct[name].discard(target)


# ---- output ----------------------------------------------------------------

HEADER = "// Generated by schema/generate.py from Codex CLI {version}; do not edit.\n"


def write(gen, version, budget):
    names = sorted(gen.items)
    chunks, chunk, size = [], [], 0
    for name in names:
        lines = len(gen.items[name][0]) + 1
        if chunk and size + lines > budget:
            chunks.append(chunk)
            chunk, size = [], 0
        chunk.append(name)
        size += lines
    chunks.append(chunk)
    for old in OUT.glob("*.rs"):
        old.unlink()
    OUT.mkdir(parents=True, exist_ok=True)
    home = {name: f"part{i + 1:02}" for i, chunk in enumerate(chunks) for name in chunk}
    mods = []
    for i, chunk in enumerate(chunks):
        module = f"part{i + 1:02}"
        mods.append((module, chunk))
        body = "\n".join("\n".join(gen.items[name][0]) + "\n" for name in chunk)
        used = set().union(*(gen.items[name][1] for name in chunk)) - set(chunk)
        imports = []
        if used:
            imports.append("use super::{" + ", ".join(sorted(used)) + "};")
        std = [t for t in ("BTreeMap",) if re.search(rf"\b{t}<", body)]
        if std:
            imports.insert(0, "use std::collections::BTreeMap;\n")
        serde_names = [n for n in ("Map", "Value") if re.search(rf"\b{n}\b", body)]
        imports.append("use serde::{Deserialize, Serialize};")
        if serde_names:
            imports.append("use serde_json::{" + ", ".join(serde_names) + "};")
        text = HEADER.format(version=version) + "\n" + "\n".join(imports) + "\n\n" + body
        (OUT / f"{module}.rs").write_text(text)
    lines = [HEADER.format(version=version), "//! The Codex app-server protocol types Vortex uses.\n"]
    lines += [f"mod {module};" for module, _ in mods] + [""]
    lines += [f"pub use {module}::{{{', '.join(chunk)}}};" for module, chunk in mods]
    (OUT / "mod.rs").write_text("\n".join(lines) + "\n")


def main():
    version = subprocess.run(["codex", "--version"], capture_output=True, text=True).stdout.split()[-1]
    with tempfile.TemporaryDirectory() as tmp:
        schema_dir = sys.argv[1] if len(sys.argv) > 1 else tmp
        if len(sys.argv) < 2:
            subprocess.run(["codex", "app-server", "generate-json-schema", "--out", tmp], check=True)
        defs = closure(load(schema_dir))
    defs = {name: strip(schema) for name, schema in defs.items()}
    pruned = {"codexVersion": version, "definitions": defs}
    (CRATE / "schema" / "protocol.json").write_text(json.dumps(pruned, indent=1, sort_keys=True) + "\n")
    gen = Gen(defs)
    for name, schema in defs.items():
        gen.item(name, schema)
    gen.box_cycles()
    gen.derive_default()
    # rustfmt lengthens items by an amount hard to foresee, so the files are
    # split again, smaller, until each one fits once formatted.
    budget = MAX_LINES
    while True:
        write(gen, version, budget)
        subprocess.run(["rustfmt", "--edition", "2024", *map(str, OUT.glob("*.rs"))], check=True)
        if all(len(path.read_text().splitlines()) <= MAX_LINES for path in OUT.glob("*.rs")):
            break
        budget -= 20


if __name__ == "__main__":
    main()
