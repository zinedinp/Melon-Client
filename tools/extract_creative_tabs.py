#!/usr/bin/env python3
"""Vanilla's creative tabs (`CreativeModeTabs.bootstrap`) as data.

The tab contents are Java code in the decompiled client, not a data file.
This reads `CreativeModeTabs.java` and resolves every `Items.X` (and the
colour/copper item families) to its registry id, checked against Pumpkin's
item list. Items made at runtime from registries (potions, enchanted books,
goat horns, fireworks, ominous bottles, paintings, light levels...) are kept
as generator entries that Melon expands itself.

Writes `crates/melon-world/src/creative_tabs.json`. Re-run after a version bump:

    python3 tools/extract_creative_tabs.py [decomp-src-dir] [pumpkin-items.json]
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
    "/mnt/ssd/Programmieren/Pumpkin-Reference/mc-decomp/src/26.3/client/net/minecraft"
)
ITEMS_JSON = Path(sys.argv[2]) if len(sys.argv) > 2 else (
    ROOT.parent / "Pumpkin/Pumpkin/assets/items.json"
)
OUT = ROOT / "crates/melon-world/src/creative_tabs.json"

# `ColorCollection.forEach` and the tab code's `gameplayColorOrder`.
COLORS = ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray",
          "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black"]
GAMEPLAY_COLORS = ["white", "light_gray", "gray", "black", "brown", "red", "orange", "yellow",
                   "lime", "green", "cyan", "light_blue", "blue", "purple", "magenta", "pink"]
# `WeatheringCopperCollection`: weathering then waxed, each in state order.
COPPER_STATES = ["unaffected", "exposed", "weathered", "oxidized"]
COPPER_PREFIX = {
    "weathering": ["", "exposed_", "weathered_", "oxidized_"],
    "waxed": ["waxed_", "waxed_exposed_", "waxed_weathered_", "waxed_oxidized_"],
}

known_items = set(json.load(open(ITEMS_JSON)))


def camel_to_snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


# --- Field name -> registry id -----------------------------------------------

def read(rel):
    return (SRC / rel).read_text()


block_item_ids = {}  # BlockItemIds field -> item name, or ("color"|"copper", base)
for m in re.finditer(r'BlockItemId (\w+) = BlockItemId\.create\("([^"]+)"(?:, "([^"]+)")?\)',
                     read("references/BlockItemIds.java")):
    block_item_ids[m.group(1)] = m.group(3) or m.group(2)
for m in re.finditer(r'ColorCollection<BlockItemId> (\w+) = createSimpleColored\("([^"]+)"\)',
                     read("references/BlockItemIds.java")):
    block_item_ids[m.group(1)] = ("color", m.group(2))
for m in re.finditer(r'WeatheringCopperCollection<BlockItemId> (\w+) = createSimpleCopper\("([^"]+)"\)',
                     read("references/BlockItemIds.java")):
    block_item_ids[m.group(1)] = ("copper", m.group(2))
# `COPPER_BLOCK_SPECIAL_NAMES`: "copper_block" unaffected, "copper" otherwise.
block_item_ids["COPPER_BLOCK"] = ("copper_special", None)

item_ids = {}
for m in re.finditer(r'ResourceKey<Item> (\w+) = create\("([^"]+)"\)', read("references/ItemIds.java")):
    item_ids[m.group(1)] = m.group(2)
for m in re.finditer(r'ColorCollection<ResourceKey<Item>> (\w+) = createSimpleColored\("([^"]+)"\)',
                     read("references/ItemIds.java")):
    item_ids[m.group(1)] = ("color", m.group(2))

items_java = read("world/item/Items.java")
# Each `Items` field's declaration up to the next field.
fields = {}
for m in re.finditer(r"public static final (\w+(?:<Item>)?) (\w+) = (.*?);\n(?=    public|    private|    static|})",
                     items_java, re.S):
    fields[m.group(2)] = m.group(3)


def family(field):
    """`("color"|"copper"|"copper_special", base)` for a collection field."""
    decl = fields.get(field, "")
    ref = re.search(r"(BlockItemIds|ItemIds)\.(\w+)", decl)
    if ref:
        table = block_item_ids if ref.group(1) == "BlockItemIds" else item_ids
        value = table.get(ref.group(2))
        if isinstance(value, tuple):
            return value
    return None


def family_members(field, part=None, state=None, color=None):
    kind = family(field)
    if kind is None:
        raise SystemExit(f"unknown item family {field}")
    if kind[0] == "color":
        colors = [color] if color else COLORS
        return [f"{c}_{kind[1]}" for c in colors]
    parts = [part] if part else ["weathering", "waxed"]
    out = []
    for p in parts:
        for i, prefix in enumerate(COPPER_PREFIX[p]):
            if state and COPPER_STATES[i] != state:
                continue
            if kind[0] == "copper_special":
                base = "copper_block" if i == 0 else "copper"
            else:
                base = kind[1]
            out.append(prefix + base)
    return out


def item_id(field):
    """The registry id of a plain `Items.FIELD`."""
    guess = field.lower()
    if guess in known_items:
        return guess
    decl = fields.get(field, "")
    ref = re.search(r"(BlockItemIds|ItemIds)\.(\w+)", decl)
    if ref:
        table = block_item_ids if ref.group(1) == "BlockItemIds" else item_ids
        value = table.get(ref.group(2))
        if isinstance(value, str):
            return value
    # `createSpawnEgg(EntityTypeIds.X)`, `createMusicDisc(...)`...
    m = re.search(r"Ids\.(\w+)", decl)
    raise SystemExit(f"cannot resolve Items.{field} ({decl[:80]!r}, {m and m.group(0)})")


SELECTOR = r"((?:\.\w+\(\))*)"


def selected(field, selectors):
    """`Items.WOOL.white()`, `Items.LIGHTNING_ROD.waxed().unaffected()`."""
    calls = re.findall(r"\.(\w+)\(\)", selectors)
    part = state = color = None
    for call in calls:
        snake = camel_to_snake(call)
        if snake in ("weathering", "waxed"):
            part = snake
        elif snake in COPPER_STATES:
            state = snake
        elif snake in COLORS:
            color = snake
        else:
            raise SystemExit(f"unknown selector {call} on {field}")
    return family_members(field, part, state, color)


# --- The tabs ------------------------------------------------------------------

tabs_java = read("world/item/CreativeModeTabs.java")
copper_families = re.findall(r"output\.accept\(Items\.(\w+)\)",
                             re.search(r"copperBlockFamilies\(.*?\{(.*?)\n    \}", tabs_java, re.S).group(1))

blocks = re.split(r"\n        (?:return )?Registry\.register\(", tabs_java)[1:]
tabs = []
for block in blocks:
    key = re.search(r"^\s*registry,\s*(\w+),", block).group(1)
    head = re.search(r"CreativeModeTab\.builder\(CreativeModeTab\.Row\.(\w+), (\d+)\)", block)
    title = re.search(r'\.title\(Component\.translatable\("([^"]+)"\)\)', block).group(1)
    icon_m = re.search(r"\.icon\(\(\) -> new ItemStack\((?:Blocks|Items)\.(\w+)" + SELECTOR + r"\)\)", block)
    icon = selected(icon_m.group(1), icon_m.group(2))[0] if icon_m.group(2) else item_id(icon_m.group(1))
    kind = re.search(r"\.type\(CreativeModeTab\.Type\.(\w+)\)", block)
    tab = {
        "id": key.lower(),
        "row": head.group(1).lower(),
        "column": int(head.group(2)),
        "title": title,
        "icon": icon,
        "type": (kind.group(1) if kind else "CATEGORY").lower(),
        "aligned_right": ".alignedRight()" in block,
        "show_title": ".hideTitle()" not in block,
        "scrollbar": ".noScrollBar()" not in block,
        "items": [],
    }
    background = re.search(r"\.backgroundTexture\((\w+)\)", block)
    tab["background"] = {"INVENTORY_BACKGROUND": "tab_inventory",
                         "SEARCH_BACKGROUND": "tab_item_search"}.get(
        background.group(1) if background else "", "tab_items")

    entries = tab["items"]

    def push(name, search_only=False):
        if name not in known_items:
            raise SystemExit(f"{name} is not an item ({key})")
        entries.append({"item": name, "search_only": True} if search_only else name)

    body = block.split(".displayItems", 1)[1] if ".displayItems" in block else ""
    body = body.split(".build()", 1)[0]
    for line in body.splitlines():
        line = line.strip()
        search_only = "SEARCH_TAB_ONLY" in line
        parent_only = "PARENT_TAB_ONLY" in line
        m = re.match(r"\w+\.accept\(Items\.(\w+)" + SELECTOR + r"(?:, CreativeModeTab\.TabVisibility\.\w+)?\);$", line)
        if m:
            names = selected(m.group(1), m.group(2)) if m.group(2) else [item_id(m.group(1))]
            for name in names:
                push(name, search_only)
            continue
        m = re.match(r"Items\.(\w+)" + SELECTOR + r"\.forEach\(", line)
        if m:
            for name in selected(m.group(1), m.group(2)) if m.group(2) else family_members(m.group(1)):
                push(name)
            continue
        m = re.match(r"registerColoredItems\(\w+, gameplayColorOrder, Items\.(\w+)\);", line)
        if m:
            for color in GAMEPLAY_COLORS:
                push(family_members(m.group(1), color=color)[0])
            continue
        m = re.match(r"copperBlockFamilies\(family -> family\.(\w+)\(\)\.forEach", line)
        if m:
            for fam in copper_families:
                for name in family_members(fam, part=m.group(1)):
                    push(name)
            continue
        generators = {
            "generatePotionEffectTypes": "potions",
            "generateEnchantmentBookTypesOnlyMaxLevel": "enchanted_books_max",
            "generateEnchantmentBookTypesAllLevels": "enchanted_books_all",
            "generateInstrumentTypes": "goat_horns",
            "generateSuspiciousStews": "suspicious_stews",
            "generateOminousBottles": "ominous_bottles",
            "generateFireworksAllDurations": "fireworks",
            "generatePresetPaintings": "paintings",
            "Raid.getOminousBannerInstance": "ominous_banner",
            "TestBlock.setModeOnStack": "test_blocks",
            "LightBlock.setLightOnStack": "light_blocks",
        }
        for call, generator in generators.items():
            if call in line and not line.startswith("private"):
                entry = {"generator": generator}
                if generator == "potions":
                    # The item is an argument a line or two below; read it
                    # from the call's whole text.
                    pass
                if search_only:
                    entry["search_only"] = True
                if parent_only:
                    entry["parent_only"] = True
                entries.append(entry)
                break
    # Potion generators name their item in the following lines.
    potion_items = re.findall(r"generatePotionEffectTypes\(\s*\w+,\s*\w+,\s*Items\.(\w+)", body)
    potion_entries = [e for e in entries if isinstance(e, dict) and e.get("generator") == "potions"]
    for entry, field in zip(potion_entries, potion_items):
        entry["item"] = item_id(field)
    for entry in entries:
        if isinstance(entry, dict) and entry.get("generator") in ("enchanted_books_max", "enchanted_books_all"):
            entry.setdefault("parent_only" if entry["generator"].endswith("max") else "search_only", True)
    if "parameters.hasPermissions()" in body:
        tab["operator_only"] = True
    tabs.append(tab)

order = {"top": 0, "bottom": 1}
tabs.sort(key=lambda t: (order[t["row"]], t["column"]))
OUT.write_text(json.dumps({"tabs": tabs}, indent=1) + "\n")
total = sum(len(t["items"]) for t in tabs)
print(f"{len(tabs)} tabs, {total} entries -> {OUT}")
for t in tabs:
    print(f"  {t['id']:<22} {t['type']:<9} {len(t['items'])}")
