#!/usr/bin/env python3
"""Raster comparison for trusted Beautiful Mermaid reference SVG fixtures.

Resolve the pinned upstream CSS theme formulas to native RGB, omit the network
font import, and use the same local font fallback as the Rust renderer. Requires
rsvg-convert and ImageMagick on the development machine; neither ships in the lib.
"""
import argparse
import json
import math
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET

NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", NS)

def resolve_reference(path):
    root = ET.parse(path).getroot()
    variables = dict(re.findall(r"(--[\w-]+):([^;]+)", root.attrib.pop("style")))
    bg, fg = variables["--bg"], variables["--fg"]
    def mix(weight, foreground=fg):
        b = [int(bg[i:i+2], 16) for i in (1, 3, 5)]
        f = [int(foreground[i:i+2], 16) for i in (1, 3, 5)]
        return "#" + "".join(f"{math.floor(a*weight + z*(1-weight)+0.5):02x}" for a,z in zip(f,b))
    derived = {"--_text": fg, "--_group-fill": bg}
    for key, override, weight in [
        ("text-sec", "muted", .6), ("text-muted", "muted", .4),
        ("text-faint", None, .25), ("line", "line", .5),
        ("arrow", "accent", .85), ("node-fill", "surface", .03),
        ("node-stroke", "border", .2), ("group-hdr", None, .05),
        ("inner-stroke", None, .12), ("key-badge", None, .1),
    ]:
        derived["--_"+key] = variables.get("--"+override, mix(weight)) if override else mix(weight)
    variables.update(derived)
    styles = "\n".join(child.text or "" for child in root if child.tag == f"{{{NS}}}style")
    # XY charts put actual paint attributes in class rules, unlike graph SVGs.
    # Materialize exactly the pinned rules before removing browser CSS.
    for index, value in re.findall(r"--xychart-color-(\d+):\s*([^;]+);", styles):
        fallback = re.fullmatch(r"var\(--accent,\s*(#[0-9a-fA-F]{6})\)", value.strip())
        color = variables.get("--accent", fallback[1]) if fallback else value.strip()
        if not re.fullmatch(r"#[0-9a-fA-F]{6}", color):
            raise ValueError(f"Unsupported reference XY color {color!r}")
        variables["--xychart-color-" + index] = color
        variables["--xychart-bar-fill-" + index] = mix(.25, color)
    xy_rules = []
    for selectors, body in re.findall(r"([^{}]+)\{([^{}]+)\}", styles):
        props = dict(re.findall(r"([a-z-]+)\s*:\s*([^;]+);?", body))
        for selector in selectors.split(","):
            selector = selector.strip()
            if "xychart-" in selector and re.fullmatch(r"(?:[a-z]+)?(?:\.[a-zA-Z0-9_-]+)+", selector):
                tag, *classes = selector.split(".")
                xy_rules.append((tag, set(classes), props))
    for element in root.iter():
        classes = set(element.get("class", "").split())
        tag = element.tag.rsplit("}", 1)[-1]
        for expected_tag, required, props in xy_rules:
            if (not expected_tag or expected_tag == tag) and required <= classes:
                for key, value in props.items():
                    element.set(key, value.strip())
    for child in list(root):
        if child.tag == f"{{{NS}}}style":
            root.remove(child)
    root.set("font-family", "Inter, system-ui, sans-serif")
    for element in root.iter():
        if "mono" in element.get("class", "").split():
            element.set("font-family", "'JetBrains Mono', 'SF Mono', 'Fira Code', ui-monospace, monospace")
        for name,value in list(element.attrib.items()):
            element.set(name, re.sub(r"var\((--[\w-]+)\)", lambda m:variables[m[1]], value))
    root.insert(0,ET.Element(f"{{{NS}}}rect", {"width":"100%","height":"100%","fill":bg}))
    return root

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference",type=Path)
    parser.add_argument("native",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=True)
    reference = resolve_reference(args.reference)
    ref_path = args.output/"reference-static.svg"
    ET.ElementTree(reference).write(ref_path,encoding="unicode")
    native = ET.parse(args.native).getroot()
    sizes = {"reference": [reference.attrib[k] for k in ("width","height")],
             "native": [native.attrib[k] for k in ("width","height")]}
    for name,path in [("reference",ref_path),("native",args.native)]:
        subprocess.run(["rsvg-convert","-o",str(args.output/f"{name}.png"),str(path)],check=True)
    result=subprocess.run(["magick","compare","-metric","RMSE",str(args.output/"reference.png"),
        str(args.output/"native.png"),str(args.output/"difference.png")],capture_output=True,text=True)
    if result.returncode not in (0,1):
        raise RuntimeError(result.stderr)
    report={"dimensions":sizes,"rmse":result.stderr.strip(),"identical_pixels":result.returncode==0,
        "normalization":"Pinned upstream CSS mixes resolved to 8-bit sRGB; local Inter/system-ui/sans-serif fonts; background materialized as rect."}
    (args.output/"comparison.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report))

if __name__ == "__main__":
    main()
