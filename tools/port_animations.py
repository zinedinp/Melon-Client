#!/usr/bin/env python3
"""Writes crates/melon-world/src/entity/animations/: vanilla's keyframe animation
definitions (client/animation/definitions/*.java) as Rust statics for
crates/melon-world/src/entity/keyframe.rs.

    tools/port_animations.py [path/to/mc-decomp/src/26.3/client]

Each `AnimationDefinition.Builder.withLength(..)[.looping()].addAnimation(
bone, new AnimationChannel(target, new Keyframe(..)...))` becomes a static
of the same name; keyframes with a separate pre and post target keep both.
The generated files are not formatted (`#[rustfmt::skip]`): one keyframe a
line, which keeps diffs against a new vanilla version readable.
"""
import os
import re
import sys

DEFAULT_ROOT = "/mnt/ssd/Programmieren/Pumpkin-Reference/mc-decomp/src/26.3/client"
ROOT = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_ROOT
SRC = os.path.join(ROOT, "net/minecraft/client/animation/definitions")
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                   "crates/melon-world/src/entity/animations")

INTERPOLATION = {"LINEAR": "Linear", "CATMULLROM": "CatmullRom"}
TARGET = {"POSITION": "Position", "ROTATION": "Rotation", "SCALE": "Scale"}


def balanced(text, start):
    """The text inside the parentheses that open at `start`, and the index
    after the closing one."""
    assert text[start] == "("
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1:i], i + 1
    raise ValueError("unbalanced")


def split_args(text):
    """Top-level comma separated arguments."""
    args, depth, current = [], 0, ""
    for ch in text:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            args.append(current.strip())
            current = ""
        else:
            current += ch
    if current.strip():
        args.append(current.strip())
    return args


def number(text):
    """A Java float or double literal as a Rust one."""
    text = text.strip().rstrip("fFdD")
    if re.fullmatch(r"-?\d+", text):
        text += ".0"
    return text


def vector(text):
    m = re.fullmatch(r"KeyframeAnimations\.(degreeVec|posVec|scaleVec)\((.*)\)", text, re.S)
    if not m:
        raise ValueError("unknown vector: " + text)
    func = {"degreeVec": "deg", "posVec": "pos", "scaleVec": "scl"}[m.group(1)]
    return "%s(%s)" % (func, ", ".join(number(a) for a in split_args(m.group(2))))


def interpolation(text):
    return INTERPOLATION[text.rsplit(".", 1)[1]]


def keyframe(args):
    if len(args) == 3:
        time, post, interp = args
        pre = None
    else:
        time, pre, post, interp = args
    interp = interpolation(interp)
    if pre is None or pre == post:
        ctor = "linear" if interp == "Linear" else "catmull_rom"
        return "Keyframe::%s(%s, %s)" % (ctor, number(time), vector(post))
    return "Keyframe::jump(%s, %s, %s, Interpolation::%s)" % (
        number(time), vector(pre), vector(post), interp)


def definitions(text):
    """(name, length, looping, [(bone, [(target, [keyframe])])])"""
    out = []
    for m in re.finditer(
            r"public static final AnimationDefinition (\w+)\s*=\s*"
            r"AnimationDefinition\.Builder\.withLength\(([^)]*)\)", text):
        name, length = m.group(1), number(m.group(2))
        end = text.index(".build()", m.end())
        body = text[m.end():end]
        looping = ".looping()" in body.split(".addAnimation", 1)[0]
        bones = []
        pos = 0
        while True:
            i = body.find(".addAnimation(", pos)
            if i < 0:
                break
            args, pos = balanced(body, i + len(".addAnimation"))
            bone, channel = split_args(args)[0], args[args.index("new AnimationChannel("):]
            bone = bone.strip('"')
            inner, _ = balanced(channel, len("new AnimationChannel"))
            parts = split_args(inner)
            target = TARGET[parts[0].rsplit(".", 1)[1]]
            keys = []
            for part in parts[1:]:
                assert part.startswith("new Keyframe(")
                kargs, _ = balanced(part, len("new Keyframe"))
                keys.append(keyframe(split_args(kargs)))
            # Channels of one bone stay together, in vanilla's order.
            if not bones or bones[-1][0] != bone:
                if any(b[0] == bone for b in bones):
                    raise ValueError("bone %s is split in %s" % (bone, name))
                bones.append((bone, []))
            bones[-1][1].append((target, keys))
        out.append((name, length, looping, bones))
    return out


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def write(module, class_name, defs):
    body = []
    for name, length, looping, bones in defs:
        body.append("#[rustfmt::skip]")
        body.append("pub static %s: AnimationDefinition = AnimationDefinition {" % name.upper())
        body.append("    length_in_seconds: %s," % length)
        body.append("    looping: %s," % ("true" if looping else "false"))
        body.append("    bones: &[")
        for bone, channels in bones:
            body.append("        Bone::new(\"%s\", &[" % bone)
            for target, keys in channels:
                body.append("            Channel::new(Target::%s, &[" % target)
                for key in keys:
                    body.append("                %s," % key)
                body.append("            ]),")
            body.append("        ]),")
        body.append("    ],")
        body.append("};")
        body.append("")
    text = "\n".join(body)
    # Only what the file uses, so none of it warns.
    names = ["AnimationDefinition", "Bone", "Channel"]
    if "Interpolation::" in text:
        names.append("Interpolation")
    names.append("Keyframe")
    names.append("Target")
    for func, alias in (("degree_vec", "deg"), ("pos_vec", "pos"), ("scale_vec", "scl")):
        if re.search(r"\b%s\(" % alias, text):
            names.append("%s as %s" % (func, alias))
    head = [
        "//! Generated by tools/port_animations.py from vanilla's `%s`. Do not edit." % class_name,
        "",
        "use crate::world::entity::keyframe::{%s};" % ", ".join(names),
        "",
    ]
    with open(os.path.join(OUT, module + ".rs"), "w", encoding="utf-8") as f:
        f.write("\n".join(head) + text)


def main():
    os.makedirs(OUT, exist_ok=True)
    modules = []
    for file in sorted(os.listdir(SRC)):
        if not file.endswith("Animation.java"):
            continue
        class_name = file[:-len(".java")]
        defs = definitions(open(os.path.join(SRC, file), encoding="utf-8").read())
        if not defs:
            continue
        module = snake(class_name[:-len("Animation")])
        write(module, class_name, defs)
        modules.append((module, class_name, [d[0].upper() for d in defs]))
    with open(os.path.join(OUT, "mod.rs"), "w", encoding="utf-8") as f:
        f.write("//! Vanilla's animation definitions, generated by tools/port_animations.py\n"
                "//! (client/animation/definitions). Bake one for a model with\n"
                "//! [`KeyframeAnimation::bake`](super::keyframe::KeyframeAnimation::bake).\n\n")
        for module, class_name, names in modules:
            f.write("/// `%s`: %s.\npub mod %s;\n" % (class_name, ", ".join(names), module))
        f.write("\n/// Every definition by its module and name, e.g. `bat::BAT_RESTING`: to find\n"
                "/// one from data, and for checks over all of them.\n"
                "pub static ALL: &[(&str, &super::keyframe::AnimationDefinition)] = &[\n")
        for module, class_name, names in modules:
            for name in names:
                f.write('    ("%s::%s", &%s::%s),\n' % (module, name, module, name))
        f.write("];\n")
    print("%d modules, %d definitions" % (len(modules), sum(len(m[2]) for m in modules)))


main()
