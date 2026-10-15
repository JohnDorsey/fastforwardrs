import os
import sys

try:
    from PIL import Image, ImageChops
except ImportError:
    print("no PIL available")
    sys.exit(0)

here = os.path.dirname(os.path.abspath(__file__))
a_folder, b_folder = os.path.join(here, "out_s1"), os.path.join(here, "out_linux_s1")
for name in sorted(os.listdir(b_folder)):
    if name == "contact_sheet.png" or not os.path.exists(os.path.join(a_folder, name)):
        continue
    a = Image.open(os.path.join(a_folder, name)).convert("RGBA")
    b = Image.open(os.path.join(b_folder, name)).convert("RGBA")
    difference = ImageChops.difference(a, b)
    worst = max(extreme[1] for extreme in difference.getextrema())
    histogram = difference.convert("L").histogram()
    total = sum(histogram)
    over_one = sum(histogram[2:])
    print(f"{name:24s} WARP vs lavapipe: largest difference {worst:3d} of 255; pixels differing by more than 1 level: {over_one} of {total}")
