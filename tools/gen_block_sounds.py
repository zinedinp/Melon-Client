#!/usr/bin/env python3
"""Writes crates/melon-audio/src/block_sounds/data.rs: each block's sound type, from the
decompiled vanilla sources (Blocks.java, SoundType.java, SoundEvents.java).

    tools/gen_block_sounds.py [path/to/mc-decomp/src/26.3/client]

A block takes the sound type named in its definition (`.sound(SoundType.X)`,
or passed to a helper such as `logProperties`); a helper that sets one itself
gives it to every block that calls it; stairs, slabs, walls and `ofFullCopy`
blocks take their base block's; the rest is `STONE`, vanilla's default.
"""
import os
import re
import sys

DEFAULT_ROOT = "/mnt/ssd/Programmieren/Pumpkin-Reference/mc-decomp/src/26.3/client"
ROOT = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_ROOT
MC = os.path.join(ROOT, "net/minecraft")
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                   "crates/melon-audio/src/block_sounds/data.rs")


def read(*parts):
    return open(os.path.join(MC, *parts), encoding="utf-8").read()


def sound_events():
    text = read("sounds", "SoundEvents.java")
    return dict(re.findall(r'SoundEvent (\w+) = register\w*\("([^"]+)"', text))


def sound_types(events):
    text = read("world", "level", "block", "SoundType.java")
    types = {}
    for name, args in re.findall(
        r"public static final SoundType (\w+) = new SoundType\((.*?)\);", text, re.S
    ):
        parts = [a.strip() for a in args.split(",")]
        volume, pitch = float(parts[0].rstrip("F")), float(parts[1].rstrip("F"))
        consts = [p.removeprefix("SoundEvents.") for p in parts[2:7]]
        # `SoundEvents.EMPTY` is the event "intentionally_empty": no sound.
        names = ["" if events.get(c) == "intentionally_empty" else events.get(c, "") for c in consts]
        types[name] = (volume, pitch, *names)
    return types


def block_segments(text):
    starts = [(m.start(), m.group(1))
              for m in re.finditer(r"    public static final Block (\w+) =", text)]
    segments = {}
    for i, (pos, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(text)
        segments[name] = text[pos:end]
    return segments


COLORS = ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray",
          "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black"]
WEATHERING = ["waxed_", "exposed_", "weathered_", "oxidized_"]


def collection_segments(text):
    """Colour and copper families (`ColorCollection`, `WeatheringCopperCollection`)
    registered as one field: field name -> its text."""
    starts = [(m.start(), m.group(1)) for m in re.finditer(
        r"    public static final (?:Color|WeatheringCopper)Collection<Block> (\w+) =", text)]
    segments = {}
    for i, (pos, name) in enumerate(starts):
        end = text.find(";\n    public static", pos)
        segments[name] = text[pos:end if end > 0 else len(text)]
    return segments


def family_of(name):
    """A block made by a collection, down to the collection's own name:
    `lime_wool_slab` -> `wool_slab`, `waxed_exposed_cut_copper` -> `cut_copper`."""
    for color in COLORS:
        if name.startswith(color + "_"):
            return name[len(color) + 1:]
    changed = True
    while changed:
        changed = False
        for prefix in WEATHERING:
            if name.startswith(prefix):
                name, changed = name[len(prefix):], True
    return {"copper": "copper_block"}.get(name, name)


def helper_sounds(text):
    """Helper methods that set a sound themselves: name -> SoundType."""
    helpers = {}
    for m in re.finditer(r"    (?:private )?static [\w.<>]+ (\w+)\([^)]*\) \{", text):
        start = m.end()
        body = text[start:text.find("\n    }", start)]
        found = re.search(r"SoundType\.(\w+)", body)
        if found:
            helpers[m.group(1)] = found.group(1)
    return helpers


COPY = [
    r"of(?:Full|Legacy)Copy\(\s*(?:Blocks\.)?(\w+)",
    r"register(?:Legacy)?Stair\(\s*[\w.]+,\s*(?:Blocks\.)?(\w+)",
    r"registerSlab\(\s*[\w.]+,\s*(?:Blocks\.)?(\w+)",
    r"registerWall\(\s*[\w.]+,\s*(?:Blocks\.)?(\w+)",
    r"wallVariant\(\s*(?:Blocks\.)?(\w+)",
]


def resolve(segments, helpers):
    chosen, base = {}, {}
    for name, seg in segments.items():
        explicit = re.search(r"SoundType\.(\w+)", seg)
        if explicit:
            chosen[name] = explicit.group(1)
            continue
        called = next((h for h in helpers if re.search(rf"\b{h}\(", seg)), None)
        if called:
            chosen[name] = helpers[called]
            continue
        for pattern in COPY:
            m = re.search(pattern, seg)
            if m and m.group(1) in segments:
                base[name] = m.group(1)
                break
        else:
            chosen[name] = "STONE"

    def of(name, depth=0):
        if name in chosen:
            return chosen[name]
        if depth > 8 or name not in base:
            return "STONE"
        return of(base[name], depth + 1)

    return {name: of(name) for name in segments}


def family_sounds(text, helpers, assigned):
    """The sound of every collection field, by its name without `dyed_`."""
    fields = collection_segments(text)
    sounds, bases = {}, {}
    for name, seg in fields.items():
        key = name.lower().removeprefix("dyed_")
        explicit = re.search(r"SoundType\.(\w+)", seg)
        helper = next((h for h in helpers if re.search(rf"\b{h}\(", seg)), None)
        base = re.search(r"register(?:Stair|Slab|Wall)\([^,]+,\s*(\w+)\.pick", seg) or re.search(
            r"of(?:Full|Legacy)Copy\(\s*(\w+)", seg)
        if explicit:
            sounds[key] = explicit.group(1)
        elif helper:
            sounds[key] = helpers[helper]
        elif base:
            bases[key] = base.group(1)
        else:
            sounds[key] = "STONE"
    # A base is another collection, or an ordinary block field.
    for _ in range(len(bases) + 1):
        for key, base in bases.items():
            target = base.lower().removeprefix("dyed_")
            sounds[key] = sounds.get(target) or assigned.get(base, "STONE")
    return sounds


def blocks_json_names():
    """The block names the game knows (Pumpkin's data), for the blocks the
    decompiled source makes in loops."""
    import json
    path = os.environ.get("MELON_BLOCKS_JSON",
                          "/mnt/ssd/Programmieren/Pumpkin/Pumpkin/assets/blocks.json")
    try:
        return [b["name"] for b in json.load(open(path))["blocks"]]
    except OSError:
        return []


def main():
    events = sound_events()
    types = sound_types(events)
    blocks = block_segments(read("world", "level", "block", "Blocks.java"))
    helpers = helper_sounds(read("world", "level", "block", "Blocks.java"))
    assigned = resolve(blocks, helpers)
    families = family_sounds(read("world", "level", "block", "Blocks.java"), helpers, assigned)
    names = blocks_json_names()
    for name in names:
        if name.upper() not in assigned and family_of(name) in families:
            assigned[name.upper()] = families[family_of(name)]

    used = sorted({t for t in assigned.values() if t != "STONE"} | {"STONE"})
    index = {name: i for i, name in enumerate(used)}
    lines = [
        "//! Generated by tools/gen_block_sounds.py from the decompiled vanilla",
        "//! sources (`Blocks.java`, `SoundType.java`, `SoundEvents.java`). Do not",
        "//! edit by hand.",
        "",
        "use super::SoundType;",
        "",
        "#[rustfmt::skip]",
        "pub static SOUND_TYPES: &[SoundType] = &[",
    ]
    for name in used:
        volume, pitch, brk, step, place, hit, fall = types[name]
        lines.append(
            f'    SoundType {{ name: "{name.lower()}", volume: {volume!r}, pitch: {pitch!r}, '
            f'break_sound: "{brk}", step_sound: "{step}", place_sound: "{place}", '
            f'hit_sound: "{hit}", fall_sound: "{fall}" }},'
        )
    lines += ["];", "", "/// Block name and index into `SOUND_TYPES`, by name; a block not here is", "/// `STONE` (index 0 is not assumed: see `DEFAULT`).",
              f"pub const DEFAULT: usize = {index['STONE']};", "", "#[rustfmt::skip]", "pub static BLOCKS: &[(&str, u8)] = &["]
    for name in sorted(n.lower() for n, t in assigned.items() if t != "STONE"):
        lines.append(f'    ("{name}", {index[assigned[name.upper()]]}),')
    lines.append("];")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    open(OUT, "w").write("\n".join(lines) + "\n")
    print(f"{len(assigned)} Bloecke, {len(used)} Sound-Typen, {sum(1 for t in assigned.values() if t != 'STONE')} Eintraege")
    for probe in ("STONE", "GRASS_BLOCK", "DIRT", "OAK_PLANKS", "OAK_STAIRS", "OAK_LOG", "OAK_LEAVES",
                  "GLASS", "SAND", "IRON_BLOCK", "WHITE_WOOL", "COBBLESTONE_STAIRS", "SNOW", "WATER",
                  "LIME_WOOL_SLAB", "RED_CONCRETE", "BLUE_STAINED_GLASS_PANE", "WAXED_EXPOSED_CUT_COPPER",
                  "COPPER_BLOCK", "BLACK_CARPET", "WHITE_BED", "PINK_CANDLE", "ORANGE_GLAZED_TERRACOTTA"):
        print(f"  {probe}: {assigned.get(probe)}")


main()
