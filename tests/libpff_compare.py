"""Independent validation with libpff (pypff): compare a source PST/OST with a file
OST/PST Studio wrote from it, folder by folder and message by message.

    pip install libpff-python
    python tests/libpff_compare.py <source> <output> [--json]

libpff is the reference implementation nearly every other PST tool wraps, and it shares
no code with this engine. If it reads the output and gets the same folders, messages
and properties out of both files, the conversion is sound at the level a reader can
judge. It does not prove Outlook will open the file; that is a separate, manual step
(see docs/OUTLOOK_COMPATIBILITY.md).

Exit code 0 when every comparison matches, 1 otherwise, 2 when libpff is unavailable.
"""
import json
import sys


def open_file(path):
    import os
    import pypff
    f = pypff.file()
    # libpff passes the name to Win32 with a \?\ prefix, which rejects forward slashes.
    f.open(os.path.normpath(os.path.abspath(path)))
    return f


def walk(folder, path, out, depth=0):
    if depth > 64:
        return
    name = folder.get_name() or ""
    here = f"{path}/{name}" if path else name
    msgs = []
    for i in range(folder.get_number_of_sub_messages()):
        m = folder.get_sub_message(i)
        props = {}
        try:
            rs = m.get_record_set(0) if m.get_number_of_record_sets() else None
            if rs is not None:
                for j in range(rs.get_number_of_entries()):
                    e = rs.get_entry(j)
                    try:
                        v = e.get_data()
                    except Exception:
                        v = None
                    props[f"{e.get_entry_type():04X}:{e.get_value_type():04X}"] = (
                        v.hex() if isinstance(v, (bytes, bytearray)) else str(v)
                    )
        except Exception as ex:  # a message libpff cannot fully read still counts
            props["_error"] = str(ex)
        try:
            subject = m.get_subject() or ""
        except Exception:
            subject = ""
        try:
            n_att = m.get_number_of_attachments()
        except Exception:
            n_att = -1
        msgs.append({
            "identifier": m.get_identifier(),
            "subject": subject,
            "attachments": n_att,
            "props": props,
        })
    out[here] = {
        "identifier": folder.get_identifier(),
        "messages": sorted(msgs, key=lambda m: m["identifier"]),
    }
    for i in range(folder.get_number_of_sub_folders()):
        walk(folder.get_sub_folder(i), here, out, depth + 1)


def survey(path):
    f = open_file(path)
    try:
        out = {}
        walk(f.get_root_folder(), "", out)
        return out
    finally:
        f.close()


def compare(a, b):
    problems = []
    for folder in sorted(set(a) | set(b)):
        if folder not in a:
            problems.append(f"folder only in output: {folder}")
            continue
        if folder not in b:
            problems.append(f"folder missing from output: {folder}")
            continue
        ma = {m["identifier"]: m for m in a[folder]["messages"]}
        mb = {m["identifier"]: m for m in b[folder]["messages"]}
        for nid in sorted(set(ma) | set(mb)):
            if nid not in mb:
                problems.append(f"{folder}: message 0x{nid:X} missing from output")
                continue
            if nid not in ma:
                problems.append(f"{folder}: message 0x{nid:X} only in output")
                continue
            x, y = ma[nid], mb[nid]
            if x["subject"] != y["subject"]:
                problems.append(f"{folder}: 0x{nid:X} subject differs: {x['subject']!r} vs {y['subject']!r}")
            if x["attachments"] != y["attachments"]:
                problems.append(f"{folder}: 0x{nid:X} attachment count differs: {x['attachments']} vs {y['attachments']}")
            for tag in sorted(set(x["props"]) | set(y["props"])):
                if x["props"].get(tag) != y["props"].get(tag):
                    problems.append(f"{folder}: 0x{nid:X} property {tag} differs")
    return problems


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    as_json = "--json" in sys.argv
    if len(args) != 2:
        print(__doc__)
        return 2
    try:
        import pypff  # noqa: F401
    except ImportError:
        print("libpff-python is not installed: pip install libpff-python")
        return 2
    src, out = args
    try:
        a = survey(src)
    except Exception as e:
        print(f"libpff could not read the source {src}: {e}")
        a = None
    try:
        b = survey(out)
    except Exception as e:
        print(f"libpff REFUSED the output {out}: {e}")
        return 1
    counts = lambda s: (len(s), sum(len(f["messages"]) for f in s.values()),
                        sum(len(m["props"]) for f in s.values() for m in f["messages"]))
    result = {
        "source": src,
        "output": out,
        "source_counts": counts(a) if a else None,
        "output_counts": counts(b),
        "problems": compare(a, b) if a else ["source unreadable by libpff; output read on its own"],
    }
    if as_json:
        print(json.dumps(result, indent=2))
    else:
        if a:
            fa, ma_, pa = result["source_counts"]
            print(f"  source  | {fa} folders, {ma_} messages, {pa} properties")
        fb, mb_, pb = result["output_counts"]
        print(f"  output  | {fb} folders, {mb_} messages, {pb} properties")
        if not result["problems"]:
            print("  Identical, id for id and byte for byte.")
        else:
            print(f"  {len(result['problems'])} difference(s):")
            for p in result["problems"][:50]:
                print(f"    - {p}")
    return 0 if not result["problems"] or a is None else 1


if __name__ == "__main__":
    sys.exit(main())
