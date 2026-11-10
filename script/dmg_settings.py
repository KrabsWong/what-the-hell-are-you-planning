"""Finder layout for the Planning installer, rendered without Apple Events."""
from pathlib import Path

staging = Path(defines["staging"])
format = "UDZO"
files = [str(staging / "Planning.app")]
symlinks = {"Applications": "/Applications"}
icon = str(staging / ".VolumeIcon.icns")
background = str(staging / ".background/install.tiff")
# Balance the icon row with bottom artwork and optional Finder path/status bars.
window_rect = ((160, 120), (640, 450))
icon_locations = {"Planning.app": (220, 190), "Applications": (420, 190)}
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
default_view = "icon-view"
arrange_by = None
grid_spacing = 64
icon_size = 80
text_size = 13
label_pos = "bottom"
show_icon_preview = False
