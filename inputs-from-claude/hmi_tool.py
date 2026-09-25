#!/usr/bin/env python3
"""
hmi_tool.py — decode / patch Nextion .HMI project files based on the
reverse-engineered format in nextion_hmi_format.md.

Two operations:

  decode  <in.HMI>  <out.yaml>
      Dump every page/component/attribute to readable, diffable YAML.

  patch   <in.HMI>  <out.HMI>  --set PAGE:OBJNAME:ATTR=VALUE [--set ...]
      Rewrite one or more attribute values *in place*. Only works when the
      new value is the same byte-length as the old one (same string
      length, or an integer that fits the attribute's existing width) --
      see nextion_hmi_format.md section 4-6 for why, and for the
      validation loop you should run before trusting this on a real
      device.

Status: the decode path is solid. The patch path is implemented per the
documented record format but has NOT yet been confirmed against the real
Nextion Editor -- treat its output as "should work" until you've verified
one round-trip.
"""
import argparse
import string
import sys
import yaml

ALLOWED = set(string.ascii_letters + string.digits + "_-")
ALLOWED_BYTES = set(ord(c) for c in ALLOWED)
BLOCK = 0x80000


def find_payload_start(data: bytes) -> int:
    """Locate the first block, past the mirrored directory blocks, that's
    mostly non-zero. Generalizes beyond a hardcoded 0x700000 so this works
    on .HMI files with a different amount of reserved/erased space."""
    nblocks = (len(data) + BLOCK - 1) // BLOCK
    dir_block = data[0:BLOCK]
    for i in range(2, nblocks):
        blk = data[i * BLOCK:(i + 1) * BLOCK]
        if blk == dir_block:
            continue
        nonzero = sum(1 for b in blk if b != 0)
        if nonzero > 1000:  # arbitrary but well clear of noise
            return i * BLOCK
    raise ValueError("could not locate resource payload block")


def is_name_at(payload: bytes, off: int):
    if off + 16 > len(payload):
        return None
    window = payload[off:off + 16]
    k = 0
    while k < 16 and window[k] in ALLOWED_BYTES:
        k += 1
    if k == 0:
        return None
    if any(b != 0 for b in window[k:16]):
        return None
    return window[:k].decode("ascii")


def scan_records(payload: bytes):
    """Returns [(payload_offset, name), ...] for every attribute-record
    name found, in file order."""
    out = []
    off = 0
    n = len(payload)
    while off < n - 16:
        nm = is_name_at(payload, off)
        if nm:
            out.append((off, nm))
            off += 16
        else:
            off += 1
    return out


def decode_tail(tail: bytes, attr_name: str = ""):
    """Returns (kind, python_value, value_byte_len) for a record's tail
    (everything after the 16-byte name, up to the next record's name).

    Quirk: the 'txt' attribute is typed 0x12 (the same "raw bytes" type
    used for numeric fields like x/y/w/h) even though its bytes are the
    component's ASCII text, not a number. Everywhere else 0x12 really is
    an integer (id, colors, coordinates, ...). We special-case 'txt' by
    name so it round-trips as text instead of a meaningless big int.
    """
    if len(tail) < 4:
        return ("raw", tail.hex(), len(tail))
    type_byte = tail[-4]
    pad = tail[-3:]
    val = tail[:-4]
    if pad != b"\x00\x00\x00":
        return ("unknown-pad", tail.hex(), len(tail))
    if type_byte == 0x11:
        try:
            return ("str", val.decode("ascii"), len(val))
        except UnicodeDecodeError:
            return ("blob", val.hex(), len(val))  # see format doc caveat
    elif type_byte == 0x12:
        if attr_name == "txt":
            try:
                return ("str", val.decode("ascii"), len(val))
            except UnicodeDecodeError:
                return ("blob", val.hex(), len(val))
        return ("int", int.from_bytes(val, "little") if val else 0, len(val))
    else:
        return ("t0x%02x" % type_byte, val.hex(), len(val))


def decode_hmi(path: str):
    """Returns {'payload_start': int, 'pages': [ {objname, attrs, components: [...]} ]}"""
    data = open(path, "rb").read()
    payload_start = find_payload_start(data)
    payload = data[payload_start:]
    names = scan_records(payload)

    records = []
    for i, (p, nm) in enumerate(names):
        next_p = names[i + 1][0] if i + 1 < len(names) else len(payload)
        tail = payload[p + 16:next_p]
        kind, val, vlen = decode_tail(tail, attr_name=nm)
        records.append({
            "file_offset": payload_start + p,
            "name": nm,
            "kind": kind,
            "value": val,
            "value_len": vlen,
        })

    pages = []
    cur_page = None
    cur_comp = None
    for r in records:
        if r["name"] == "type":
            comp = {"_file_offset": r["file_offset"], "attrs": []}
            if r["value"] == "y":  # page container marker
                cur_page = {"objname": None, "attrs": [], "components": []}
                pages.append(cur_page)
                cur_comp = comp
                cur_page["attrs"] = comp["attrs"]
                cur_page["_container"] = comp
            else:
                if cur_page is None:  # malformed/unknown lead-in; start implicit page
                    cur_page = {"objname": None, "attrs": [], "components": []}
                    pages.append(cur_page)
                cur_comp = comp
                cur_page["components"].append(comp)
        if cur_comp is not None:
            cur_comp["attrs"].append({"name": r["name"], "kind": r["kind"],
                                       "value": r["value"], "file_offset": r["file_offset"],
                                       "value_len": r["value_len"]})
            if r["name"] == "objname" and cur_comp is cur_page.get("_container"):
                cur_page["objname"] = r["value"]
            elif r["name"] == "objname":
                cur_comp["objname"] = r["value"]

    for pg in pages:
        pg.pop("_container", None)

    return {"payload_start": payload_start, "pages": pages}


def to_readable(decoded):
    """Collapse the attrs list into a flat dict per component/page for a
    much more legible YAML dump. Keeps file_offset only on the component,
    not per-attribute, to stay readable -- use --raw if you need
    per-attribute offsets for patching by hand."""
    out_pages = []
    for pg in decoded["pages"]:
        flat_page = {"objname": pg["objname"]}
        for a in pg["attrs"]:
            flat_page[a["name"]] = a["value"]
        comps = []
        for c in pg["components"]:
            flat = {"_file_offset": c["_file_offset"]}
            for a in c["attrs"]:
                flat[a["name"]] = a["value"]
            comps.append(flat)
        flat_page["components"] = comps
        out_pages.append(flat_page)
    return {"payload_start": decoded["payload_start"], "pages": out_pages}


def cmd_decode(args):
    decoded = decode_hmi(args.input)
    readable = decoded if args.raw else to_readable(decoded)
    with open(args.output, "w") as f:
        yaml.safe_dump(readable, f, sort_keys=False, allow_unicode=True, width=100)
    n_pages = len(decoded["pages"])
    n_comps = sum(len(p["components"]) for p in decoded["pages"])
    print(f"decoded {n_pages} page(s), {n_comps} component(s) -> {args.output}")


def _find_attr_record(decoded, page_sel, objname_sel, attr_name):
    for pg in decoded["pages"]:
        if page_sel is not None and pg["objname"] != page_sel:
            continue
        targets = [pg] if objname_sel in (None, pg["objname"]) else []
        # also search components
        candidates = ([pg] if pg["objname"] == objname_sel else []) + \
                     [c for c in pg["components"] if c.get("objname") == objname_sel]
        for c in candidates:
            for a in c["attrs"]:
                if a["name"] == attr_name:
                    return a
    return None


def cmd_patch(args):
    data = bytearray(open(args.input, "rb").read())
    decoded = decode_hmi(args.input)

    for spec in args.set:
        try:
            selector, new_value = spec.split("=", 1)
            page_sel, objname_sel, attr_name = selector.split(":", 2)
        except ValueError:
            sys.exit(f"--set must look like PAGE:OBJNAME:ATTR=VALUE, got: {spec!r}")

        rec = _find_attr_record(decoded, page_sel, objname_sel, attr_name)
        if rec is None:
            sys.exit(f"could not find {page_sel}:{objname_sel}:{attr_name} in {args.input}")

        old_kind = rec["kind"]
        off = rec["file_offset"] + 16  # value starts right after the 16-byte name

        if old_kind == "str":
            old_len = len(rec["value"])
            new_bytes = new_value.encode("ascii")
            if len(new_bytes) != old_len:
                sys.exit(
                    f"{selector}: value length mismatch -- old {old_len} bytes "
                    f"({rec['value']!r}), new {len(new_bytes)} bytes ({new_value!r}). "
                    f"Pad/truncate to match; structural (length-changing) edits aren't "
                    f"supported yet -- see nextion_hmi_format.md section 5."
                )
            data[off:off + old_len] = new_bytes
        elif old_kind == "int":
            width = rec.get("value_len")
            if not width:
                sys.exit(f"{selector}: attribute has zero width in the source file, "
                          f"can't determine how many bytes to write")
            iv = int(new_value, 0)
            if iv < 0 or iv >= (1 << (8 * width)):
                sys.exit(f"{selector}: {iv} doesn't fit in the existing {width}-byte width")
            data[off:off + width] = iv.to_bytes(width, "little")
        else:
            sys.exit(f"{selector}: patching kind {old_kind!r} isn't supported yet")

        print(f"patched {selector} -> {new_value!r}")

    with open(args.output, "wb") as f:
        f.write(data)
    print(f"wrote {args.output}")
    print("NOTE: unverified against the real Nextion Editor -- see format doc section 6 "
          "before trusting this on real hardware.")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    d = sub.add_parser("decode", help="dump a .HMI to readable YAML")
    d.add_argument("input")
    d.add_argument("output")
    d.add_argument("--raw", action="store_true", help="keep full per-attribute offset/kind detail instead of the flattened view")
    d.set_defaults(func=cmd_decode)

    p = sub.add_parser("patch", help="rewrite attribute values in place (same-length only)")
    p.add_argument("input")
    p.add_argument("output")
    p.add_argument("--set", action="append", required=True,
                    help="PAGE:OBJNAME:ATTR=VALUE, repeatable")
    p.set_defaults(func=cmd_patch)

    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
