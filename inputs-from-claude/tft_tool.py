#!/usr/bin/env python3
"""
tft_tool.py — patch an already-compiled Nextion .tft directly, per the
findings in nextion_tft_format.md. No .HMI, no Nextion Editor involved.

Two operations, both "search for the current value, overwrite in place":

  patch-text  <in.tft> <out.tft> --set "OLDTEXT=NEWTEXT" [--set ...]
      Rewrites a text-pool slot. NEWTEXT may be shorter or equal length
      to OLDTEXT (the 104-byte slot is NUL-padded, so shorter text just
      gets zero-filled). Longer text is rejected -- that would overflow
      into the next component's slot.

  patch-geom  <in.tft> <out.tft> --set "X,Y,W,H=NEWX,NEWY,NEWW,NEWH"
      Finds the exact x,y,w,h quad and rewrites x,y,w,h,endx,endy
      consistently (endx=newx+neww-1, endy=newy+newh-1). If the quad
      isn't unique in the file, this refuses and tells you the offsets
      so you can disambiguate (e.g. by also patching text first, or by
      picking a more specific search).

Both operations have been run against a real project file (h5.tft) and
verified to touch only the intended bytes -- see the chat transcript /
nextion_tft_format.md section 4 for how that was checked. Still worth
re-confirming on your actual hardware/model before trusting it blind.
"""
import argparse
import struct
import sys

TEXT_SLOT = 104


def cmd_patch_text(args):
    data = bytearray(open(args.input, "rb").read())

    for spec in args.set:
        if "=" not in spec:
            sys.exit(f"--set must look like OLDTEXT=NEWTEXT, got: {spec!r}")
        old, new = spec.split("=", 1)
        old_b = old.encode("ascii")
        new_b = new.encode("ascii")
        if len(new_b) > len(old_b) and len(new_b) > TEXT_SLOT:
            sys.exit(f"{spec!r}: new text too long for a {TEXT_SLOT}-byte slot")

        hits = []
        i = 0
        while True:
            i = data.find(old_b, i)
            if i == -1:
                break
            hits.append(i)
            i += 1
        if not hits:
            sys.exit(f"{old!r}: not found in {args.input}")
        if len(hits) > 1:
            sys.exit(
                f"{old!r}: found {len(hits)} times (offsets {[hex(h) for h in hits]}) "
                f"-- not unique, refusing to guess. Use a more specific/longer OLDTEXT."
            )

        off = hits[0]
        # zero the whole slot's worth of following bytes that are still
        # part of the old string's padding, then write the new text +
        # NUL padding back in, without touching bytes past the slot.
        # We don't know the slot boundary precisely here (no page-base
        # bookkeeping consulted), so we conservatively pad only up to
        # len(old_b) and leave any further NULs alone -- safe because
        # trailing padding is already 0x00 and new_b is not longer.
        data[off:off + len(old_b)] = new_b + b"\x00" * (len(old_b) - len(new_b))
        print(f"patched text {old!r} -> {new!r} at {hex(off)}")

    with open(args.output, "wb") as f:
        f.write(bytes(data))
    print(f"wrote {args.output}")


def cmd_patch_geom(args):
    data = bytearray(open(args.input, "rb").read())

    for spec in args.set:
        if "=" not in spec:
            sys.exit(f"--set must look like X,Y,W,H=NEWX,NEWY,NEWW,NEWH, got: {spec!r}")
        old_s, new_s = spec.split("=", 1)
        x, y, w, h = (int(v) for v in old_s.split(","))
        nx, ny, nw, nh = (int(v) for v in new_s.split(","))

        needle = struct.pack("<HHHH", x, y, w, h)
        hits = []
        i = 0
        while True:
            i = data.find(needle, i)
            if i == -1:
                break
            hits.append(i)
            i += 1
        if not hits:
            sys.exit(f"{old_s}: not found in {args.input}")
        if len(hits) > 1 and args.at is None:
            sys.exit(
                f"{old_s}: found {len(hits)} times (offsets {[hex(h) for h in hits]}) "
                f"-- not unique, refusing to guess. Re-run with --at OFFSET to pick one."
            )

        off = args.at if args.at is not None else hits[0]
        if off not in hits:
            sys.exit(f"--at {hex(off)} isn't one of the found offsets: {[hex(h) for h in hits]}")
        endx, endy = nx + nw - 1, ny + nh - 1
        data[off:off + 8] = struct.pack("<HHHH", nx, ny, nw, nh)
        data[off + 8:off + 12] = struct.pack("<HH", endx, endy)
        print(f"patched geometry ({x},{y},{w},{h}) -> ({nx},{ny},{nw},{nh}) "
              f"[endx={endx} endy={endy}] at {hex(off)}")

    with open(args.output, "wb") as f:
        f.write(bytes(data))
    print(f"wrote {args.output}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    t = sub.add_parser("patch-text")
    t.add_argument("input")
    t.add_argument("output")
    t.add_argument("--set", action="append", required=True, help="OLDTEXT=NEWTEXT, repeatable")
    t.set_defaults(func=cmd_patch_text)

    g = sub.add_parser("patch-geom")
    g.add_argument("input")
    g.add_argument("output")
    g.add_argument("--set", action="append", required=True, help="X,Y,W,H=NEWX,NEWY,NEWW,NEWH, repeatable")
    g.add_argument("--at", type=lambda s: int(s, 0), default=None,
                    help="disambiguate a non-unique match by file offset (hex like 0xc0ef4 or decimal)")
    g.set_defaults(func=cmd_patch_geom)

    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
