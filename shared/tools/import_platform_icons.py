#!/usr/bin/env python3
"""Import the curated platform icon set as Compose vector resources."""

from __future__ import annotations

import re
import subprocess
import urllib.parse
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


MATERIAL_REVISION = "abd7f5c0e179c83f068c770650bd14ebac5d5a09"
FLUENT_REVISION = "3cc5907ff2b187df22c6c711e8d0a96da41debf8"
LUCIDE_REVISION = "4e2efb9350fac7dbcb94caae9ccac5654bdcc050"

ANDROID_NS = "http://schemas.android.com/apk/res/android"
ET.register_namespace("android", ANDROID_NS)


@dataclass(frozen=True)
class IconSource:
	name: str
	material: str
	fluent_directory: str
	fluent_file: str
	lucide: str


ICONS = (
	IconSource("add", "add", "Add", "add", "plus"),
	IconSource("arrow_back", "arrow_back", "Arrow Left", "arrow_left", "arrow-left"),
	IconSource("bell", "notifications", "Alert", "alert", "bell"),
	IconSource("bug", "bug_report", "Bug", "bug", "bug"),
	IconSource("check", "check", "Checkmark", "checkmark", "check"),
	IconSource("chevron_right", "chevron_right", "Chevron Right", "chevron_right", "chevron-right"),
	IconSource("close", "close", "Dismiss", "dismiss", "x"),
	IconSource("code", "code", "Code", "code", "code-xml"),
	IconSource("cloud_off", "cloud_off", "Cloud Off", "cloud_off", "cloud-off"),
	IconSource("delete", "delete", "Delete", "delete", "trash-2"),
	IconSource("document", "description", "Document", "document", "file-text"),
	IconSource("download", "download", "Arrow Download", "arrow_download", "download"),
	IconSource("file", "draft", "Document", "document", "file"),
	IconSource("folder", "folder", "Folder", "folder", "folder"),
	IconSource("globe", "language", "Globe", "globe", "globe"),
	IconSource("hand", "back_hand", "Hand Right", "hand_right", "hand"),
	IconSource("info", "info", "Info", "info", "info"),
	IconSource("lock", "lock", "Lock Closed", "lock_closed", "lock"),
	IconSource("megaphone", "campaign", "Megaphone", "megaphone", "megaphone"),
	IconSource("more_vertical", "more_vert", "More Vertical", "more_vertical", "ellipsis-vertical"),
	IconSource("moon", "dark_mode", "Weather Moon", "weather_moon", "moon"),
	IconSource("nfc", "nfc", "Tap Double", "tap_double", "nfc"),
	IconSource("qr_code", "qr_code_scanner", "QR Code", "qr_code", "qr-code"),
	IconSource("radio", "cell_tower", "Cellular Data 1", "cellular_data_1", "radio-tower"),
	IconSource("scan", "document_scanner", "Scan Type", "scan_type", "scan-line"),
	IconSource("send", "send", "Send", "send", "send"),
	IconSource("share", "share", "Share", "share", "share-2"),
	IconSource("sparkles", "auto_awesome", "Sparkle", "sparkle", "sparkles"),
	IconSource("settings", "settings", "Settings", "settings", "settings"),
	IconSource("shield", "shield", "Shield", "shield", "shield"),
	IconSource("shield_check", "verified_user", "Shield Checkmark", "shield_checkmark", "shield-check"),
	IconSource("stop_circle", "stop_circle", "Stop", "stop", "circle-stop"),
	IconSource("storage", "database", "Database", "database", "database"),
	IconSource("sun", "light_mode", "Weather Sunny", "weather_sunny", "sun"),
	IconSource("sync", "sync", "Arrow Sync", "arrow_sync", "refresh-cw"),
	IconSource("system_theme", "devices", "Desktop", "desktop", "monitor-cog"),
	IconSource("temporary", "schedule", "Clock", "clock", "timer-reset"),
	IconSource("total_storage", "data_usage", "Data Pie", "data_pie", "chart-pie"),
	IconSource("transfer_data", "inventory_2", "Archive", "archive", "package"),
	IconSource("user", "account_circle", "Person Circle", "person_circle", "circle-user-round"),
	IconSource("user_off", "person_off", "Person Prohibited", "person_prohibited", "user-round-x"),
)

ROOT = Path(__file__).resolve().parents[1]
DRAWABLES = ROOT / "src/commonMain/composeResources/drawable"
LICENSES = ROOT / "src/commonMain/composeResources/files/icon-licenses"


def fetch(url: str) -> bytes:
	return subprocess.run(
		["curl", "--fail", "--location", "--silent", "--show-error", url],
		check=True,
		capture_output=True,
	).stdout


def android_attr(name: str) -> str:
	return f"{{{ANDROID_NS}}}{name}"


def write_vector(name: str, family: str, vector: ET.Element) -> None:
	vector.insert(0, ET.Comment(f" {family} icon imported by shared/tools/import_platform_icons.py. "))
	ET.indent(vector, space="    ")
	data = ET.tostring(vector, encoding="unicode", xml_declaration=True)
	(DRAWABLES / f"icon_{family}_{name}.xml").write_text(data + "\n", encoding="utf-8")


def import_material(icon: IconSource) -> None:
	url = (
		"https://raw.githubusercontent.com/google/material-design-icons/"
		f"{MATERIAL_REVISION}/symbols/android/{icon.material}/materialsymbolsrounded/"
		f"{icon.material}_24px.xml"
	)
	vector = ET.fromstring(fetch(url))
	vector.attrib.pop(android_attr("tint"), None)
	for path in vector.iter("path"):
		path.set(android_attr("fillColor"), "#FF000000")
	write_vector(icon.name, "material", vector)


def import_fluent(icon: IconSource) -> None:
	directory = urllib.parse.quote(icon.fluent_directory)
	url = (
		"https://raw.githubusercontent.com/microsoft/fluentui-system-icons/"
		f"{FLUENT_REVISION}/assets/{directory}/SVG/"
		f"ic_fluent_{icon.fluent_file}_24_regular.svg"
	)
	svg = ET.fromstring(fetch(url))
	view_box = [float(value) for value in svg.attrib["viewBox"].split()]
	vector = vector_root(view_box[2], view_box[3])
	for element in svg.iter():
		if element.tag.rsplit("}", 1)[-1] != "path":
			continue
		path = ET.SubElement(vector, "path")
		path.set(android_attr("pathData"), element.attrib["d"])
		path.set(android_attr("fillColor"), "#FF000000")
		if element.attrib.get("fill-rule") == "evenodd":
			path.set(android_attr("fillType"), "evenOdd")
	write_vector(icon.name, "fluent", vector)


def clean_number(value: str | float) -> str:
	value = float(value)
	return str(int(value)) if value.is_integer() else str(value)


def points_path(points: str, close: bool) -> str:
	numbers = re.findall(r"-?\d+(?:\.\d+)?", points)
	pairs = list(zip(numbers[0::2], numbers[1::2]))
	commands = [f"M {x} {y}" if index == 0 else f"L {x} {y}" for index, (x, y) in enumerate(pairs)]
	if close:
		commands.append("Z")
	return " ".join(commands)


def primitive_path(element: ET.Element) -> str | None:
	tag = element.tag.rsplit("}", 1)[-1]
	attributes = element.attrib
	if tag == "path":
		return attributes["d"]
	if tag == "line":
		return f"M {attributes['x1']} {attributes['y1']} L {attributes['x2']} {attributes['y2']}"
	if tag == "polyline":
		return points_path(attributes["points"], close=False)
	if tag == "polygon":
		return points_path(attributes["points"], close=True)
	if tag in {"circle", "ellipse"}:
		cx = float(attributes["cx"])
		cy = float(attributes["cy"])
		rx = float(attributes.get("r", attributes.get("rx", "0")))
		ry = float(attributes.get("r", attributes.get("ry", "0")))
		return (
			f"M {clean_number(cx + rx)} {clean_number(cy)} "
			f"A {clean_number(rx)} {clean_number(ry)} 0 1 0 {clean_number(cx - rx)} {clean_number(cy)} "
			f"A {clean_number(rx)} {clean_number(ry)} 0 1 0 {clean_number(cx + rx)} {clean_number(cy)}"
		)
	if tag == "rect":
		x = float(attributes.get("x", "0"))
		y = float(attributes.get("y", "0"))
		width = float(attributes["width"])
		height = float(attributes["height"])
		radius = float(attributes.get("rx", "0"))
		if radius == 0:
			return f"M {clean_number(x)} {clean_number(y)} h {clean_number(width)} v {clean_number(height)} h -{clean_number(width)} Z"
		return (
			f"M {clean_number(x + radius)} {clean_number(y)} "
			f"H {clean_number(x + width - radius)} A {clean_number(radius)} {clean_number(radius)} 0 0 1 {clean_number(x + width)} {clean_number(y + radius)} "
			f"V {clean_number(y + height - radius)} A {clean_number(radius)} {clean_number(radius)} 0 0 1 {clean_number(x + width - radius)} {clean_number(y + height)} "
			f"H {clean_number(x + radius)} A {clean_number(radius)} {clean_number(radius)} 0 0 1 {clean_number(x)} {clean_number(y + height - radius)} "
			f"V {clean_number(y + radius)} A {clean_number(radius)} {clean_number(radius)} 0 0 1 {clean_number(x + radius)} {clean_number(y)} Z"
		)
	return None


def import_lucide(icon: IconSource) -> None:
	url = (
		"https://raw.githubusercontent.com/lucide-icons/lucide/"
		f"{LUCIDE_REVISION}/icons/{icon.lucide}.svg"
	)
	svg = ET.fromstring(fetch(url))
	view_box = [float(value) for value in svg.attrib["viewBox"].split()]
	vector = vector_root(view_box[2], view_box[3])
	for element in svg.iter():
		path_data = primitive_path(element)
		if path_data is None:
			continue
		path = ET.SubElement(vector, "path")
		path.set(android_attr("pathData"), path_data)
		path.set(android_attr("fillColor"), "#00000000")
		path.set(android_attr("strokeColor"), "#FF000000")
		path.set(android_attr("strokeWidth"), svg.attrib.get("stroke-width", "2"))
		path.set(android_attr("strokeLineCap"), svg.attrib.get("stroke-linecap", "round"))
		path.set(android_attr("strokeLineJoin"), svg.attrib.get("stroke-linejoin", "round"))
	write_vector(icon.name, "lucide", vector)


def vector_root(width: float, height: float) -> ET.Element:
	return ET.Element(
		"vector",
		{
			android_attr("width"): "24dp",
			android_attr("height"): "24dp",
			android_attr("viewportWidth"): clean_number(width),
			android_attr("viewportHeight"): clean_number(height),
		},
	)


def import_licenses() -> None:
	licenses = {
		"material-symbols-apache-2.0.txt": (
			"google/material-design-icons",
			MATERIAL_REVISION,
			"LICENSE",
		),
		"fluent-system-icons-mit.txt": (
			"microsoft/fluentui-system-icons",
			FLUENT_REVISION,
			"LICENSE",
		),
		"lucide-isc.txt": ("lucide-icons/lucide", LUCIDE_REVISION, "LICENSE"),
	}
	LICENSES.mkdir(parents=True, exist_ok=True)
	for filename, (repository, revision, source_file) in licenses.items():
		url = f"https://raw.githubusercontent.com/{repository}/{revision}/{source_file}"
		LICENSES.joinpath(filename).write_bytes(fetch(url))


def main() -> None:
	DRAWABLES.mkdir(parents=True, exist_ok=True)
	for icon in ICONS:
		import_material(icon)
		import_fluent(icon)
		import_lucide(icon)
	import_licenses()


if __name__ == "__main__":
	main()
